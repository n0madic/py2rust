#![forbid(unsafe_code)]
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
    pub emit_hir: bool,
    pub emit_types: bool,
    pub pretty: bool,
}

#[derive(Debug)]
pub struct CompileOutput {
    pub rust: String,
    pub hir: Option<String>,
    pub types: Option<String>,
    pub warnings: Vec<Warning>,
}

pub fn compile(
    source: &str,
    filename: &str,
    opts: &CompileOptions,
) -> Result<CompileOutput, miette::Report> {
    let suite = ast::Suite::parse(source, filename).map_err(|err| {
        let span = Span::new(0, 0);
        CompileError::new(err.to_string(), span, source, filename)
    })?;

    let mut program = Lowerer::new(source, filename).lower(&suite)?;
    rename_user_main(&mut program);
    let mut checker = TypeChecker::new(&program, source, filename)?;
    let ctx = checker.check_program(&mut program)?;
    let warnings = checker.take_warnings();

    let rust = Codegen::new(&ctx, source, filename).emit_program(&program)?;

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

fn rename_user_main(program: &mut hir::Program) {
    let has_user_main = program
        .items
        .iter()
        .any(|item| matches!(item, hir::Item::Function(func) if func.name == "main"));
    if !has_user_main {
        return;
    }
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
    for item in &mut program.items {
        if let hir::Item::Function(func) = item {
            if func.name == "main" {
                func.name = new_name.clone();
            }
        }
    }
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

fn rename_main_calls_in_expr(expr: &mut hir::Expr, new_name: &str) {
    match &mut expr.kind {
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
        hir::ExprKind::ListComp { elt, iter, ifs, .. } => {
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
        hir::ExprKind::Literal(_) | hir::ExprKind::Name(_) => {}
    }
}
