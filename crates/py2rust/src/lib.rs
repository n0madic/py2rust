#![forbid(unsafe_code)]
// Allow large Result types because our CompileError includes source text
#![allow(clippy::result_large_err)]

pub mod builtin;
pub mod call_bind;
pub mod callspec;
pub mod codegen;
pub mod container;
pub mod diagnostic;
pub mod hir;
pub mod hir_visit;
pub mod import_resolver;
pub mod lower;
pub mod span;
pub mod stdlib;
pub mod toolchain;
pub mod typecheck;
pub mod types;

use crate::codegen::Codegen;
use crate::diagnostic::{CompileError, Warning};
use crate::hir_visit::{ExprWalkerMut, StmtWalkerMut};
use crate::import_resolver::resolve_program_imports;
use crate::lower::Lowerer;
use crate::span::Span;
use crate::typecheck::TypeChecker;
use crate::types::Type;
use rustpython_parser::ast;
use rustpython_parser::Parse;

#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Emit debug HIR representation
    pub emit_hir: bool,
    /// Emit debug type information
    pub emit_types: bool,
    /// Run rustfmt on the generated code
    pub pretty: bool,
}

#[derive(Debug)]
pub struct CompileOutput {
    pub rust: String,
    pub hir: Option<String>,
    pub types: Option<String>,
    pub warnings: Vec<Warning>,
}

/// Main compilation pipeline: Python source → Rust source.
///
/// The pipeline consists of four phases:
/// 1. Parse: Python source → RustPython AST
/// 2. Lower: RustPython AST → HIR (High-level IR)
/// 3. TypeCheck: HIR → Typed HIR (fills in type information)
/// 4. Codegen: Typed HIR → Rust source code
///
/// Between phases 2 and 3, we perform `rename_user_main` to handle the special case
/// where the user defines a function named `main()`. Since we always generate a
/// Rust `fn main()` for top-level code, we need to rename the user's function to
/// avoid a collision (e.g., to `__py_main`).
pub fn compile(
    source: &str,
    filename: &str,
    opts: &CompileOptions,
) -> Result<CompileOutput, miette::Report> {
    // Phase 1: Parse Python source
    let suite = ast::Suite::parse(source, filename).map_err(|err| {
        let span = Span::new(0, 0);
        CompileError::new(err.to_string(), span, source, filename)
    })?;

    // Phase 2: Lower to HIR
    let program = Lowerer::new(source, filename).lower(&suite)?;

    // Resolve user-module/package imports into a merged HIR program.
    let mut program = resolve_program_imports(program, source, filename)?;

    // Handle user-defined `main()` function collision
    rename_user_main(&mut program);

    // Phase 3: Type check
    let mut checker = TypeChecker::new(&program, source, filename)?;
    let ctx = checker.check_program(&mut program)?;
    let mut warnings = checker.take_warnings();

    // Phase 3.5: Validate types - warn on remaining Unknown types
    validate_types(&program, source, filename, &mut warnings);

    // Phase 4: Generate Rust code
    let rust = Codegen::new(&ctx, source, filename).emit_program(&program)?;

    // Optional: Emit debug information
    let hir = if opts.emit_hir {
        Some(format!("{:#?}", program))
    } else {
        None
    };

    let types = if opts.emit_types {
        Some(format!("{:#?}", ctx))
    } else {
        None
    };

    Ok(CompileOutput {
        rust,
        hir,
        types,
        warnings,
    })
}

const MAX_RENAME_ATTEMPTS: usize = 1000;

/// Visitor that renames call sites from `main()` to a collision-safe replacement.
///
/// The traversal intentionally preserves the previous behavior:
/// - only call expressions are renamed,
/// - assignment targets are not traversed,
/// - comprehension `generators` are not traversed (legacy compatibility).
struct MainRenamer<'a> {
    new_name: &'a str,
}

impl ExprWalkerMut for MainRenamer<'_> {
    fn visit_call_mut(
        &mut self,
        func: &mut hir::Expr,
        args: &mut [hir::Expr],
        keywords: &mut [hir::KeywordArg],
    ) {
        // Rename only direct `main()` call sites, then continue walking children.
        if let hir::ExprKind::Name(name) = &mut func.kind {
            if name == "main" {
                *name = self.new_name.to_string();
            }
        }
        func.accept_mut(self);
        for arg in args {
            arg.accept_mut(self);
        }
        for kw in keywords {
            kw.value.accept_mut(self);
        }
    }

    fn visit_list_comp_mut(
        &mut self,
        elt: &mut hir::Expr,
        _target: &mut String,
        iter: &mut hir::Expr,
        ifs: &mut [hir::Expr],
        _generators: &mut [hir::CompClause],
    ) {
        // Keep legacy behavior: rename in the mirrored first clause only.
        elt.accept_mut(self);
        iter.accept_mut(self);
        for cond in ifs {
            cond.accept_mut(self);
        }
    }

    fn visit_set_comp_mut(
        &mut self,
        elt: &mut hir::Expr,
        _target: &mut String,
        iter: &mut hir::Expr,
        ifs: &mut [hir::Expr],
        _generators: &mut [hir::CompClause],
    ) {
        // Keep legacy behavior: rename in the mirrored first clause only.
        elt.accept_mut(self);
        iter.accept_mut(self);
        for cond in ifs {
            cond.accept_mut(self);
        }
    }

    fn visit_block_mut(&mut self, stmts: &mut [hir::Stmt]) {
        for stmt in stmts {
            stmt.accept_mut(self);
        }
    }
}

impl StmtWalkerMut for MainRenamer<'_> {
    fn visit_assign_mut(&mut self, _target: &mut hir::AssignTarget, value: &mut hir::Expr) {
        // Keep legacy behavior: walk the value only, not assignment targets.
        value.accept_mut(self);
    }

    fn visit_for_mut(
        &mut self,
        _target: &mut hir::ForTarget,
        iter: &mut hir::Expr,
        body: &mut [hir::Stmt],
    ) {
        // Keep legacy behavior: do not traverse the for-loop target.
        iter.accept_mut(self);
        for stmt in body {
            stmt.accept_mut(self);
        }
    }
}

/// Rename user-defined `main()` function to avoid collision with generated `fn main()`.
///
/// Why this is needed:
/// - We always generate a Rust `fn main()` to execute top-level Python statements
/// - If the user also defines `def main()`, we'd have a name collision
/// - We can't just skip generating `fn main()` because top-level statements need
///   somewhere to execute
///
/// How it works:
/// 1. Check if there's a user-defined function named "main"
/// 2. If yes, rename it to "__py_main" (or "__py_mainN" if that's also taken)
/// 3. Update all calls to `main()` to use the new name
///
/// This happens after lowering but before type checking so that type checking
/// sees the renamed function.
fn rename_user_main(program: &mut hir::Program) {
    let has_user_main = program
        .items
        .iter()
        .any(|item| matches!(item, hir::Item::Function(func) if func.name == "main"));
    if !has_user_main {
        return;
    }

    // Find an unused name for the renamed function
    let mut new_name = "__py_main".to_string();
    let mut suffix = 0;
    while program
        .items
        .iter()
        .any(|item| matches!(item, hir::Item::Function(func) if func.name == new_name))
    {
        suffix += 1;
        if suffix > MAX_RENAME_ATTEMPTS {
            // Safety: should never happen in practice, but prevents infinite loop
            panic!(
                "Unable to find unique name for user-defined main function after {} attempts",
                MAX_RENAME_ATTEMPTS
            );
        }
        new_name = format!("__py_main{suffix}");
    }

    // Rename the function definition
    for item in &mut program.items {
        if let hir::Item::Function(func) = item {
            if func.name == "main" {
                func.name = new_name.clone();
            }
        }
    }

    // Rename all call sites to use the selected function name.
    let mut renamer = MainRenamer {
        new_name: &new_name,
    };
    for item in &mut program.items {
        match item {
            hir::Item::Function(func) => {
                for stmt in &mut func.body {
                    stmt.accept_mut(&mut renamer);
                }
            }
            hir::Item::Class(class_def) => {
                for method in &mut class_def.methods {
                    for stmt in &mut method.body {
                        stmt.accept_mut(&mut renamer);
                    }
                }
            }
            hir::Item::Stmt(stmt) => {
                stmt.accept_mut(&mut renamer);
            }
            hir::Item::Union(_) => {}
        }
    }
}

/// Walk the HIR after type checking and emit warnings for remaining Unknown types.
///
/// This helps surface cases where type inference was insufficient, before they
/// cause confusing codegen errors. Only warns (does not error) to avoid breaking
/// programs that work today with Unknown fallbacks.
fn validate_types(
    program: &hir::Program,
    source: &str,
    filename: &str,
    warnings: &mut Vec<Warning>,
) {
    fn check_expr(expr: &hir::Expr, source: &str, filename: &str, warnings: &mut Vec<Warning>) {
        if matches!(expr.ty.as_ref(), Some(Type::Unknown)) {
            // Skip common false positives: name references and call results often
            // remain Unknown when the callee is a builtin with polymorphic returns.
            let is_noise = matches!(
                expr.kind,
                hir::ExprKind::Name(_) | hir::ExprKind::Call { .. } | hir::ExprKind::Lambda { .. }
            );
            if !is_noise {
                warnings.push(Warning::new(
                    "expression has unresolved type (Unknown)",
                    expr.span,
                    source,
                    filename,
                ));
            }
        }
    }

    fn check_stmts(stmts: &[hir::Stmt], source: &str, filename: &str, warnings: &mut Vec<Warning>) {
        for stmt in stmts {
            match &stmt.kind {
                hir::StmtKind::Let { value, .. } => check_expr(value, source, filename, warnings),
                hir::StmtKind::Assign { value, .. } => {
                    check_expr(value, source, filename, warnings)
                }
                hir::StmtKind::Expr(expr) => check_expr(expr, source, filename, warnings),
                hir::StmtKind::Return { value: Some(expr) } => {
                    check_expr(expr, source, filename, warnings)
                }
                hir::StmtKind::If { body, orelse, .. } => {
                    check_stmts(body, source, filename, warnings);
                    check_stmts(orelse, source, filename, warnings);
                }
                hir::StmtKind::While { body, .. } | hir::StmtKind::For { body, .. } => {
                    check_stmts(body, source, filename, warnings);
                }
                hir::StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    check_stmts(body, source, filename, warnings);
                    for handler in handlers {
                        check_stmts(&handler.body, source, filename, warnings);
                    }
                    check_stmts(orelse, source, filename, warnings);
                    check_stmts(finalbody, source, filename, warnings);
                }
                hir::StmtKind::Match { cases, .. } => {
                    for case in cases {
                        check_stmts(&case.body, source, filename, warnings);
                    }
                }
                _ => {}
            }
        }
    }

    for item in &program.items {
        match item {
            hir::Item::Function(func) => {
                check_stmts(&func.body, source, filename, warnings);
            }
            hir::Item::Class(class_def) => {
                for method in &class_def.methods {
                    check_stmts(&method.body, source, filename, warnings);
                }
            }
            hir::Item::Stmt(stmt) => {
                check_stmts(std::slice::from_ref(stmt), source, filename, warnings);
            }
            hir::Item::Union(_) => {}
        }
    }
}
