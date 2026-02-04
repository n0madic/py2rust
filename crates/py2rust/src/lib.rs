#![forbid(unsafe_code)]
// Allow large Result types because our CompileError includes source text
#![allow(clippy::result_large_err)]

pub mod codegen;
pub mod diagnostic;
pub mod hir;
pub mod lower;
pub mod span;
pub mod toolchain;
pub mod typecheck;
pub mod types;

use crate::codegen::Codegen;
use crate::diagnostic::{CompileError, Warning};
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

    // Rename all calls to main() throughout the program
    for item in &mut program.items {
        match item {
            hir::Item::Function(func) => {
                for stmt in &mut func.body {
                    rename_main_calls_in_stmt(stmt, &new_name);
                }
            }
            hir::Item::Class(class_def) => {
                for method in &mut class_def.methods {
                    for stmt in &mut method.body {
                        rename_main_calls_in_stmt(stmt, &new_name);
                    }
                }
            }
            hir::Item::Stmt(stmt) => {
                rename_main_calls_in_stmt(stmt.as_mut(), &new_name);
            }
            hir::Item::Union(_) => {}
        }
    }
}

/// Recursively rename calls to `main()` in a statement.
/// This is needed because the user's code might call their own `main()` function.
fn rename_main_calls_in_stmt(stmt: &mut hir::Stmt, new_name: &str) {
    match &mut stmt.kind {
        hir::StmtKind::Let { value, .. } => rename_main_calls_in_expr(value, new_name),
        hir::StmtKind::Assign { value, .. } => rename_main_calls_in_expr(value, new_name),
        hir::StmtKind::Return { value } => {
            if let Some(expr) = value {
                rename_main_calls_in_expr(expr, new_name);
            }
        }
        hir::StmtKind::If { test, body, orelse } => {
            rename_main_calls_in_expr(test, new_name);
            for stmt in body {
                rename_main_calls_in_stmt(stmt, new_name);
            }
            for stmt in orelse {
                rename_main_calls_in_stmt(stmt, new_name);
            }
        }
        hir::StmtKind::While { test, body } => {
            rename_main_calls_in_expr(test, new_name);
            for stmt in body {
                rename_main_calls_in_stmt(stmt, new_name);
            }
        }
        hir::StmtKind::For { iter, body, .. } => {
            rename_main_calls_in_expr(iter, new_name);
            for stmt in body {
                rename_main_calls_in_stmt(stmt, new_name);
            }
        }
        hir::StmtKind::Expr(expr) => rename_main_calls_in_expr(expr, new_name),
        hir::StmtKind::Assert { test, msg } => {
            rename_main_calls_in_expr(test, new_name);
            if let Some(msg) = msg {
                rename_main_calls_in_expr(msg, new_name);
            }
        }
        hir::StmtKind::Match { subject, cases } => {
            rename_main_calls_in_expr(subject, new_name);
            for case in cases {
                for stmt in &mut case.body {
                    rename_main_calls_in_stmt(stmt, new_name);
                }
            }
        }
        hir::StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            for stmt in body {
                rename_main_calls_in_stmt(stmt, new_name);
            }
            for handler in handlers {
                for stmt in &mut handler.body {
                    rename_main_calls_in_stmt(stmt, new_name);
                }
            }
            for stmt in orelse {
                rename_main_calls_in_stmt(stmt, new_name);
            }
            for stmt in finalbody {
                rename_main_calls_in_stmt(stmt, new_name);
            }
        }
        hir::StmtKind::Raise { exc, cause } => {
            if let Some(expr) = exc {
                rename_main_calls_in_expr(expr, new_name);
            }
            if let Some(expr) = cause {
                rename_main_calls_in_expr(expr, new_name);
            }
        }
        hir::StmtKind::Global { .. } => {}
        hir::StmtKind::Break | hir::StmtKind::Continue => {}
    }
}

/// Recursively rename calls to `main()` in an expression.
///
/// We need to traverse the entire expression tree because main() could be called
/// anywhere: in arguments to other functions, in binary operations, in list literals, etc.
fn rename_main_calls_in_expr(expr: &mut hir::Expr, new_name: &str) {
    match &mut expr.kind {
        // Special handling for Call: check if we're calling main()
        hir::ExprKind::Call { func, args } => {
            if let hir::ExprKind::Name(name) = &mut func.kind {
                if name == "main" {
                    *name = new_name.to_string();
                }
            }
            rename_main_calls_in_expr(func, new_name);
            for arg in args {
                rename_main_calls_in_expr(arg, new_name);
            }
        }
        hir::ExprKind::Attr { value, .. } => rename_main_calls_in_expr(value, new_name),
        hir::ExprKind::Binary { left, right, .. } => {
            rename_main_calls_in_expr(left, new_name);
            rename_main_calls_in_expr(right, new_name);
        }
        hir::ExprKind::Unary { expr: inner, .. } => rename_main_calls_in_expr(inner, new_name),
        hir::ExprKind::Compare { left, right, .. } => {
            rename_main_calls_in_expr(left, new_name);
            rename_main_calls_in_expr(right, new_name);
        }
        hir::ExprKind::BoolOp { values, .. } => {
            for v in values {
                rename_main_calls_in_expr(v, new_name);
            }
        }
        hir::ExprKind::List(items) | hir::ExprKind::Tuple(items) | hir::ExprKind::Set(items) => {
            for item in items {
                rename_main_calls_in_expr(item, new_name);
            }
        }
        hir::ExprKind::Dict(items) => {
            for (k, v) in items {
                rename_main_calls_in_expr(k, new_name);
                rename_main_calls_in_expr(v, new_name);
            }
        }
        hir::ExprKind::Index { value, index } => {
            rename_main_calls_in_expr(value, new_name);
            rename_main_calls_in_expr(index, new_name);
        }
        hir::ExprKind::Slice {
            value,
            start,
            end,
            step,
        } => {
            rename_main_calls_in_expr(value, new_name);
            if let Some(s) = start {
                rename_main_calls_in_expr(s, new_name);
            }
            if let Some(e) = end {
                rename_main_calls_in_expr(e, new_name);
            }
            if let Some(s) = step {
                rename_main_calls_in_expr(s, new_name);
            }
        }
        hir::ExprKind::ListComp { elt, iter, ifs, .. }
        | hir::ExprKind::SetComp { elt, iter, ifs, .. } => {
            rename_main_calls_in_expr(elt, new_name);
            rename_main_calls_in_expr(iter, new_name);
            for cond in ifs {
                rename_main_calls_in_expr(cond, new_name);
            }
        }
        hir::ExprKind::UnionCtor { inner, .. } => rename_main_calls_in_expr(inner, new_name),
        hir::ExprKind::Lambda { body, .. } => rename_main_calls_in_expr(body, new_name),
        hir::ExprKind::IfExpr { test, body, orelse } => {
            rename_main_calls_in_expr(test, new_name);
            rename_main_calls_in_expr(body, new_name);
            rename_main_calls_in_expr(orelse, new_name);
        }
        hir::ExprKind::Block { stmts } => {
            for stmt in stmts {
                rename_main_calls_in_stmt(stmt, new_name);
            }
        }
        // Literals and simple names don't contain calls
        hir::ExprKind::Literal(_) | hir::ExprKind::Name(_) => {}
    }
}
