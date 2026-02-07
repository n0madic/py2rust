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

/// Depth-first walk over a statement tree.
pub(super) fn walk_stmt_tree(stmts: &[Stmt], visit: &mut impl FnMut(&Stmt)) {
    for stmt in stmts {
        walk_stmt_tree_one(stmt, visit);
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
