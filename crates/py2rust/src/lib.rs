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
pub mod lower;
pub mod span;
pub mod stdlib;
pub mod toolchain;
pub mod typecheck;
pub mod types;

use crate::codegen::Codegen;
use crate::diagnostic::{CompileError, Warning};
use crate::hir_visit::{ExprVisitorMut, StmtVisitorMut};
use crate::lower::Lowerer;
use crate::span::Span;
use crate::typecheck::TypeChecker;
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
    let mut program = Lowerer::new(source, filename).lower(&suite)?;

    // Handle user-defined `main()` function collision
    rename_user_main(&mut program);

    // Phase 3: Type check
    let mut checker = TypeChecker::new(&program, source, filename)?;
    let ctx = checker.check_program(&mut program)?;
    let warnings = checker.take_warnings();

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

impl MainRenamer<'_> {
    /// Walk a list of statements in place.
    fn walk_stmts(&mut self, stmts: &mut [hir::Stmt]) {
        for stmt in stmts {
            stmt.accept_mut(self);
        }
    }

    /// Walk a list of expressions in place.
    fn walk_exprs(&mut self, exprs: &mut [hir::Expr]) {
        for expr in exprs {
            expr.accept_mut(self);
        }
    }
}

impl ExprVisitorMut<()> for MainRenamer<'_> {
    fn visit_literal_mut(&mut self, _lit: &mut hir::Literal) {}

    fn visit_name_mut(&mut self, _name: &mut String) {}

    fn visit_yield_mut(&mut self, value: &mut Option<Box<hir::Expr>>) {
        if let Some(value) = value {
            value.accept_mut(self);
        }
    }

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
        self.walk_exprs(args);
        for kw in keywords {
            kw.value.accept_mut(self);
        }
    }

    fn visit_starred_mut(&mut self, value: &mut hir::Expr) {
        value.accept_mut(self);
    }

    fn visit_attr_mut(&mut self, value: &mut hir::Expr, _attr: &mut String) {
        value.accept_mut(self);
    }

    fn visit_binary_mut(
        &mut self,
        _op: &mut hir::BinOp,
        left: &mut hir::Expr,
        right: &mut hir::Expr,
    ) {
        left.accept_mut(self);
        right.accept_mut(self);
    }

    fn visit_unary_mut(&mut self, _op: &mut hir::UnaryOp, inner: &mut hir::Expr) {
        inner.accept_mut(self);
    }

    fn visit_compare_mut(
        &mut self,
        _op: &mut hir::CmpOp,
        left: &mut hir::Expr,
        right: &mut hir::Expr,
    ) {
        left.accept_mut(self);
        right.accept_mut(self);
    }

    fn visit_compare_chain_mut(
        &mut self,
        left: &mut hir::Expr,
        _ops: &mut [hir::CmpOp],
        comparators: &mut [hir::Expr],
    ) {
        left.accept_mut(self);
        self.walk_exprs(comparators);
    }

    fn visit_bool_op_mut(&mut self, _op: &mut hir::BoolOp, values: &mut [hir::Expr]) {
        self.walk_exprs(values);
    }

    fn visit_list_mut(&mut self, items: &mut [hir::Expr]) {
        self.walk_exprs(items);
    }

    fn visit_tuple_mut(&mut self, items: &mut [hir::Expr]) {
        self.walk_exprs(items);
    }

    fn visit_dict_mut(&mut self, items: &mut [(hir::Expr, hir::Expr)]) {
        for (key, value) in items {
            key.accept_mut(self);
            value.accept_mut(self);
        }
    }

    fn visit_set_mut(&mut self, items: &mut [hir::Expr]) {
        self.walk_exprs(items);
    }

    fn visit_index_mut(&mut self, value: &mut hir::Expr, index: &mut hir::Expr) {
        value.accept_mut(self);
        index.accept_mut(self);
    }

    fn visit_slice_mut(
        &mut self,
        value: &mut hir::Expr,
        start: &mut Option<Box<hir::Expr>>,
        end: &mut Option<Box<hir::Expr>>,
        step: &mut Option<Box<hir::Expr>>,
    ) {
        value.accept_mut(self);
        if let Some(start) = start {
            start.accept_mut(self);
        }
        if let Some(end) = end {
            end.accept_mut(self);
        }
        if let Some(step) = step {
            step.accept_mut(self);
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
        self.walk_exprs(ifs);
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
        self.walk_exprs(ifs);
    }

    fn visit_union_ctor_mut(
        &mut self,
        _union: &mut String,
        _variant: &mut String,
        inner: &mut hir::Expr,
    ) {
        inner.accept_mut(self);
    }

    fn visit_lambda_mut(&mut self, _params: &mut [String], body: &mut hir::Expr) {
        body.accept_mut(self);
    }

    fn visit_if_expr_mut(
        &mut self,
        test: &mut hir::Expr,
        body: &mut hir::Expr,
        orelse: &mut hir::Expr,
    ) {
        test.accept_mut(self);
        body.accept_mut(self);
        orelse.accept_mut(self);
    }

    fn visit_block_mut(&mut self, stmts: &mut [hir::Stmt]) {
        self.walk_stmts(stmts);
    }
}

impl StmtVisitorMut<()> for MainRenamer<'_> {
    fn visit_let_mut(
        &mut self,
        _name: &mut String,
        _ann: &mut Option<types::TypeRef>,
        value: &mut hir::Expr,
    ) {
        value.accept_mut(self);
    }

    fn visit_assign_mut(&mut self, _target: &mut hir::AssignTarget, value: &mut hir::Expr) {
        // Keep legacy behavior: walk the value only, not assignment targets.
        value.accept_mut(self);
    }

    fn visit_return_mut(&mut self, value: &mut Option<hir::Expr>) {
        if let Some(value) = value {
            value.accept_mut(self);
        }
    }

    fn visit_if_mut(
        &mut self,
        test: &mut hir::Expr,
        body: &mut [hir::Stmt],
        orelse: &mut [hir::Stmt],
    ) {
        test.accept_mut(self);
        self.walk_stmts(body);
        self.walk_stmts(orelse);
    }

    fn visit_while_mut(&mut self, test: &mut hir::Expr, body: &mut [hir::Stmt]) {
        test.accept_mut(self);
        self.walk_stmts(body);
    }

    fn visit_for_mut(
        &mut self,
        _target: &mut hir::ForTarget,
        iter: &mut hir::Expr,
        body: &mut [hir::Stmt],
    ) {
        iter.accept_mut(self);
        self.walk_stmts(body);
    }

    fn visit_import_mut(&mut self, _names: &mut [hir::ImportBinding]) {}

    fn visit_import_from_mut(
        &mut self,
        _module: &mut String,
        _names: &mut [hir::ImportFromBinding],
    ) {
    }

    fn visit_global_mut(&mut self, _names: &mut [String]) {}

    fn visit_nonlocal_mut(&mut self, _names: &mut [String]) {}

    fn visit_break_mut(&mut self) {}

    fn visit_continue_mut(&mut self) {}

    fn visit_expr_stmt_mut(&mut self, expr: &mut hir::Expr) {
        expr.accept_mut(self);
    }

    fn visit_assert_mut(&mut self, test: &mut hir::Expr, msg: &mut Option<hir::Expr>) {
        test.accept_mut(self);
        if let Some(msg) = msg {
            msg.accept_mut(self);
        }
    }

    fn visit_match_mut(&mut self, subject: &mut hir::Expr, cases: &mut [hir::MatchCase]) {
        subject.accept_mut(self);
        for case in cases {
            self.walk_stmts(&mut case.body);
        }
    }

    fn visit_try_mut(
        &mut self,
        body: &mut [hir::Stmt],
        handlers: &mut [hir::ExceptHandler],
        orelse: &mut [hir::Stmt],
        finalbody: &mut [hir::Stmt],
    ) {
        self.walk_stmts(body);
        for handler in handlers {
            self.walk_stmts(&mut handler.body);
        }
        self.walk_stmts(orelse);
        self.walk_stmts(finalbody);
    }

    fn visit_raise_mut(&mut self, exc: &mut Option<hir::Expr>, cause: &mut Option<hir::Expr>) {
        if let Some(exc) = exc {
            exc.accept_mut(self);
        }
        if let Some(cause) = cause {
            cause.accept_mut(self);
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
