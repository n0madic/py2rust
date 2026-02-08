//! Shared walkers for scope-related codegen analyses.
//!
//! The analysis modules (`globals`, `nonlocal`) need consistent recursive
//! traversal for statements and assignment targets. Centralizing the traversal
//! prevents subtle drift between the passes.

use super::super::*;

/// Visit all names bound by an assignment target.
pub(super) fn walk_assign_target_names(target: &AssignTarget, visit: &mut impl FnMut(&str)) {
    match target {
        AssignTarget::Name(name) => visit(name),
        AssignTarget::Tuple(items) | AssignTarget::List(items) => {
            for item in items {
                walk_assign_target_names(item, visit);
            }
        }
        AssignTarget::Starred(inner) => walk_assign_target_names(inner, visit),
        AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
    }
}

/// Visit all expressions embedded inside an assignment target.
pub(super) fn walk_assign_target_exprs(target: &AssignTarget, visit: &mut impl FnMut(&Expr)) {
    match target {
        AssignTarget::Attr { value, .. } => visit(value),
        AssignTarget::Index { value, index } => {
            visit(value);
            visit(index);
        }
        AssignTarget::Tuple(items) | AssignTarget::List(items) => {
            for item in items {
                walk_assign_target_exprs(item, visit);
            }
        }
        AssignTarget::Starred(inner) => walk_assign_target_exprs(inner, visit),
        AssignTarget::Name(_) => {}
    }
}

/// Depth-first walk over a statement tree.
pub(super) fn walk_stmt_tree(stmts: &[Stmt], visit: &mut impl FnMut(&Stmt)) {
    for stmt in stmts {
        walk_stmt_tree_one(stmt, visit);
    }
}

/// Event stream used by container storage analyses while traversing statements.
pub(super) enum StorageStmtEvent<'a, Ctx> {
    /// A `let name = value` binding.
    Let { name: &'a str, value: &'a Expr },
    /// A plain assignment with its original target.
    Assign {
        target: &'a AssignTarget,
        value: &'a Expr,
    },
    /// Any expression that should be analyzed with a specific usage context.
    Expr { expr: &'a Expr, ctx: Ctx },
}

/// Walk statement trees and emit storage-analysis events.
///
/// The caller provides context values for regular expression evaluation
/// (`value_ctx`) and escape points (`escape_ctx`).
pub(super) fn walk_storage_stmt_events<Ctx: Copy>(
    stmts: &[Stmt],
    value_ctx: Ctx,
    escape_ctx: Ctx,
    visit: &mut impl FnMut(StorageStmtEvent<'_, Ctx>),
) {
    walk_stmt_tree(stmts, &mut |stmt| match &stmt.kind {
        StmtKind::Let { name, value, .. } => {
            visit(StorageStmtEvent::Let { name, value });
            visit(StorageStmtEvent::Expr {
                expr: value,
                ctx: value_ctx,
            });
        }
        StmtKind::Assign { target, value } => {
            visit(StorageStmtEvent::Assign { target, value });
            let ctx = match target {
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => escape_ctx,
                _ => value_ctx,
            };
            visit(StorageStmtEvent::Expr { expr: value, ctx });
        }
        StmtKind::Delete { target } => {
            walk_assign_target_exprs(target, &mut |expr| {
                visit(StorageStmtEvent::Expr {
                    expr,
                    ctx: value_ctx,
                });
            });
        }
        StmtKind::Return { value } => {
            if let Some(expr) = value {
                visit(StorageStmtEvent::Expr {
                    expr,
                    ctx: escape_ctx,
                });
            }
        }
        StmtKind::If { test, .. } | StmtKind::While { test, .. } => {
            visit(StorageStmtEvent::Expr {
                expr: test,
                ctx: value_ctx,
            });
        }
        StmtKind::For { iter, .. } => {
            visit(StorageStmtEvent::Expr {
                expr: iter,
                ctx: value_ctx,
            });
        }
        StmtKind::Expr(expr) => {
            visit(StorageStmtEvent::Expr {
                expr,
                ctx: value_ctx,
            });
        }
        StmtKind::Assert { test, msg } => {
            visit(StorageStmtEvent::Expr {
                expr: test,
                ctx: value_ctx,
            });
            if let Some(expr) = msg {
                visit(StorageStmtEvent::Expr {
                    expr,
                    ctx: value_ctx,
                });
            }
        }
        StmtKind::Match { subject, .. } => {
            visit(StorageStmtEvent::Expr {
                expr: subject,
                ctx: value_ctx,
            });
        }
        StmtKind::Raise { exc, cause } => {
            if let Some(expr) = exc {
                visit(StorageStmtEvent::Expr {
                    expr,
                    ctx: value_ctx,
                });
            }
            if let Some(expr) = cause {
                visit(StorageStmtEvent::Expr {
                    expr,
                    ctx: value_ctx,
                });
            }
        }
        StmtKind::Try { .. }
        | StmtKind::Import { .. }
        | StmtKind::ImportFrom { .. }
        | StmtKind::Global { .. }
        | StmtKind::Nonlocal { .. }
        | StmtKind::Break
        | StmtKind::Continue => {}
    });
}

/// Callbacks used by the shared expression walker for container storage passes.
pub(super) trait StorageExprCallbacks<Ctx: Copy> {
    /// Context used for regular expression evaluation.
    fn value_ctx(&self) -> Ctx;
    /// Context used when values escape (must be shared).
    fn escape_ctx(&self) -> Ctx;
    /// Return `true` if call arguments should be treated as non-escaping.
    fn call_is_safe(&self, func: &Expr) -> bool;
    /// Visit one expression node with its current context.
    fn visit_expr(&mut self, expr: &Expr, ctx: Ctx);
    /// Visit an embedded statement block expression.
    fn visit_block(&mut self, stmts: &[Stmt]);
}

/// Shared recursive expression traversal used by container storage analyses.
pub(super) fn walk_storage_expr_tree<Ctx: Copy, C: StorageExprCallbacks<Ctx>>(
    callbacks: &mut C,
    expr: &Expr,
    ctx: Ctx,
) {
    callbacks.visit_expr(expr, ctx);
    match &expr.kind {
        ExprKind::Name(_) | ExprKind::Literal(_) => {}
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            let value_ctx = callbacks.value_ctx();
            match &func.kind {
                ExprKind::Attr { value, .. } => {
                    walk_storage_expr_tree(callbacks, value, value_ctx);
                }
                _ => {
                    walk_storage_expr_tree(callbacks, func, value_ctx);
                }
            }
            let arg_ctx = if callbacks.call_is_safe(func) {
                value_ctx
            } else {
                callbacks.escape_ctx()
            };
            for arg in args {
                walk_storage_expr_tree(callbacks, arg, arg_ctx);
            }
            for kw in keywords {
                walk_storage_expr_tree(callbacks, &kw.value, arg_ctx);
            }
        }
        ExprKind::Starred { value } => {
            walk_storage_expr_tree(callbacks, value, callbacks.value_ctx());
        }
        ExprKind::Yield { value } => {
            if let Some(value) = value {
                walk_storage_expr_tree(callbacks, value, callbacks.value_ctx());
            }
        }
        ExprKind::Attr { value, .. } => {
            walk_storage_expr_tree(callbacks, value, callbacks.value_ctx());
        }
        ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
            walk_storage_expr_tree(callbacks, left, callbacks.value_ctx());
            walk_storage_expr_tree(callbacks, right, callbacks.value_ctx());
        }
        ExprKind::Unary { expr, .. } => {
            walk_storage_expr_tree(callbacks, expr, callbacks.value_ctx());
        }
        ExprKind::CompareChain {
            left, comparators, ..
        } => {
            walk_storage_expr_tree(callbacks, left, callbacks.value_ctx());
            for cmp in comparators {
                walk_storage_expr_tree(callbacks, cmp, callbacks.value_ctx());
            }
        }
        ExprKind::BoolOp { values, .. } => {
            for value in values {
                walk_storage_expr_tree(callbacks, value, callbacks.value_ctx());
            }
        }
        ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
            for item in items {
                walk_storage_expr_tree(callbacks, item, callbacks.escape_ctx());
            }
        }
        ExprKind::Dict(items) => {
            for (key, value) in items {
                walk_storage_expr_tree(callbacks, key, callbacks.escape_ctx());
                walk_storage_expr_tree(callbacks, value, callbacks.escape_ctx());
            }
        }
        ExprKind::Index { value, index } => {
            walk_storage_expr_tree(callbacks, value, callbacks.value_ctx());
            walk_storage_expr_tree(callbacks, index, callbacks.value_ctx());
        }
        ExprKind::Slice {
            value,
            start,
            end,
            step,
        } => {
            walk_storage_expr_tree(callbacks, value, callbacks.value_ctx());
            if let Some(start) = start {
                walk_storage_expr_tree(callbacks, start, callbacks.value_ctx());
            }
            if let Some(end) = end {
                walk_storage_expr_tree(callbacks, end, callbacks.value_ctx());
            }
            if let Some(step) = step {
                walk_storage_expr_tree(callbacks, step, callbacks.value_ctx());
            }
        }
        ExprKind::ListComp { elt, iter, ifs, .. } | ExprKind::SetComp { elt, iter, ifs, .. } => {
            walk_storage_expr_tree(callbacks, iter, callbacks.value_ctx());
            walk_storage_expr_tree(callbacks, elt, callbacks.value_ctx());
            for cond in ifs {
                walk_storage_expr_tree(callbacks, cond, callbacks.value_ctx());
            }
        }
        ExprKind::Lambda { body, .. } => {
            walk_storage_expr_tree(callbacks, body, callbacks.escape_ctx());
        }
        ExprKind::IfExpr { test, body, orelse } => {
            walk_storage_expr_tree(callbacks, test, callbacks.value_ctx());
            walk_storage_expr_tree(callbacks, body, callbacks.value_ctx());
            walk_storage_expr_tree(callbacks, orelse, callbacks.value_ctx());
        }
        ExprKind::Block { stmts } => callbacks.visit_block(stmts),
        ExprKind::UnionCtor { inner, .. } => {
            walk_storage_expr_tree(callbacks, inner, callbacks.value_ctx());
        }
    }
}

fn walk_stmt_tree_one(stmt: &Stmt, visit: &mut impl FnMut(&Stmt)) {
    visit(stmt);
    match &stmt.kind {
        StmtKind::If { body, orelse, .. } => {
            walk_stmt_tree(body, visit);
            walk_stmt_tree(orelse, visit);
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
            walk_stmt_tree(body, visit);
        }
        StmtKind::Match { cases, .. } => {
            for case in cases {
                walk_stmt_tree(&case.body, visit);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            walk_stmt_tree(body, visit);
            for handler in handlers {
                walk_stmt_tree(&handler.body, visit);
            }
            walk_stmt_tree(orelse, visit);
            walk_stmt_tree(finalbody, visit);
        }
        StmtKind::Let { .. }
        | StmtKind::Assign { .. }
        | StmtKind::Delete { .. }
        | StmtKind::Return { .. }
        | StmtKind::Expr(_)
        | StmtKind::Assert { .. }
        | StmtKind::Raise { .. }
        | StmtKind::Import { .. }
        | StmtKind::ImportFrom { .. }
        | StmtKind::Global { .. }
        | StmtKind::Nonlocal { .. }
        | StmtKind::Break
        | StmtKind::Continue => {}
    }
}
