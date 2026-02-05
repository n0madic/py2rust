// Lambda capture analysis helpers.

use super::super::*;
use std::collections::HashSet;

impl<'a> Codegen<'a> {
    /// Determine if a lambda captures variables from outer scope.
    ///
    /// Why this matters:
    /// - Lambdas that capture must use `move` keyword in Rust.
    /// - This transfers ownership of captured variables into the closure.
    /// - Without `move`, we'd get borrow checker errors when the lambda outlives
    ///   the scope that created it.
    ///
    /// This function analyzes the lambda body to detect references to variables
    /// defined in outer scopes (but not in the lambda's own parameters).
    pub(super) fn lambda_captures_outer(&self, name: &str, params: &[String], body: &Expr) -> bool {
        let outer_locals: HashSet<String> = match &self.local_vars {
            Some(vars) => vars.keys().cloned().collect(),
            None => return false,
        };
        if outer_locals.is_empty() {
            return false;
        }

        let mut local_defs: HashSet<String> = params.iter().cloned().collect();
        local_defs.insert(name.to_string());
        let mut global_names = HashSet::new();
        let mut nonlocal_names = HashSet::new();

        let stmts = match &body.kind {
            ExprKind::Block { stmts } => stmts,
            _ => return false,
        };

        fn collect_local_defs(
            stmts: &[Stmt],
            locals: &mut HashSet<String>,
            globals: &mut HashSet<String>,
            nonlocals: &mut HashSet<String>,
        ) {
            fn record_target(
                target: &AssignTarget,
                locals: &mut HashSet<String>,
                nonlocals: &HashSet<String>,
            ) {
                match target {
                    AssignTarget::Name(name) => {
                        if !nonlocals.contains(name) {
                            locals.insert(name.clone());
                        }
                    }
                    AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                        for item in items {
                            record_target(item, locals, nonlocals);
                        }
                    }
                    AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
                }
            }

            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Let { name, .. } => {
                        if !nonlocals.contains(name) {
                            locals.insert(name.clone());
                        }
                    }
                    StmtKind::Assign { target, .. } => {
                        record_target(target, locals, nonlocals);
                    }
                    StmtKind::For { target, body, .. } => {
                        for name in target.names() {
                            if !nonlocals.contains(name) {
                                locals.insert(name.to_string());
                            }
                        }
                        collect_local_defs(body, locals, globals, nonlocals);
                    }
                    StmtKind::If { body, orelse, .. } => {
                        collect_local_defs(body, locals, globals, nonlocals);
                        collect_local_defs(orelse, locals, globals, nonlocals);
                    }
                    StmtKind::While { body, .. } => {
                        collect_local_defs(body, locals, globals, nonlocals);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            for binding in &case.bindings {
                                if !nonlocals.contains(binding) {
                                    locals.insert(binding.clone());
                                }
                            }
                            collect_local_defs(&case.body, locals, globals, nonlocals);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        collect_local_defs(body, locals, globals, nonlocals);
                        for handler in handlers {
                            if let Some(name) = &handler.name {
                                if !nonlocals.contains(name) {
                                    locals.insert(name.clone());
                                }
                            }
                            collect_local_defs(&handler.body, locals, globals, nonlocals);
                        }
                        collect_local_defs(orelse, locals, globals, nonlocals);
                        collect_local_defs(finalbody, locals, globals, nonlocals);
                    }
                    StmtKind::Global { names } => {
                        for name in names {
                            globals.insert(name.clone());
                        }
                    }
                    StmtKind::Nonlocal { names } => {
                        for name in names {
                            nonlocals.insert(name.clone());
                        }
                    }
                    StmtKind::Expr(_)
                    | StmtKind::Assert { .. }
                    | StmtKind::Raise { .. }
                    | StmtKind::Return { .. } => {}
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
        }

        fn expr_uses_outer(
            expr: &Expr,
            locals: &HashSet<String>,
            globals: &HashSet<String>,
            outers: &HashSet<String>,
        ) -> bool {
            match &expr.kind {
                ExprKind::Name(name) => {
                    outers.contains(name) && !locals.contains(name) && !globals.contains(name)
                }
                ExprKind::Literal(_) => false,
                ExprKind::Lambda { .. } => false,
                ExprKind::Block { .. } => false,
                ExprKind::Call { func, args } => {
                    expr_uses_outer(func, locals, globals, outers)
                        || args
                            .iter()
                            .any(|arg| expr_uses_outer(arg, locals, globals, outers))
                }
                ExprKind::Attr { value, .. } => expr_uses_outer(value, locals, globals, outers),
                ExprKind::Binary { left, right, .. } => {
                    expr_uses_outer(left, locals, globals, outers)
                        || expr_uses_outer(right, locals, globals, outers)
                }
                ExprKind::Unary { expr, .. } => expr_uses_outer(expr, locals, globals, outers),
                ExprKind::Compare { left, right, .. } => {
                    expr_uses_outer(left, locals, globals, outers)
                        || expr_uses_outer(right, locals, globals, outers)
                }
                ExprKind::CompareChain {
                    left, comparators, ..
                } => {
                    if expr_uses_outer(left, locals, globals, outers) {
                        return true;
                    }
                    comparators
                        .iter()
                        .any(|cmp| expr_uses_outer(cmp, locals, globals, outers))
                }
                ExprKind::BoolOp { values, .. } => values
                    .iter()
                    .any(|v| expr_uses_outer(v, locals, globals, outers)),
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => items
                    .iter()
                    .any(|e| expr_uses_outer(e, locals, globals, outers)),
                ExprKind::Dict(items) => items.iter().any(|(k, v)| {
                    expr_uses_outer(k, locals, globals, outers)
                        || expr_uses_outer(v, locals, globals, outers)
                }),
                ExprKind::Index { value, index } => {
                    expr_uses_outer(value, locals, globals, outers)
                        || expr_uses_outer(index, locals, globals, outers)
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    expr_uses_outer(value, locals, globals, outers)
                        || start
                            .as_deref()
                            .is_some_and(|s| expr_uses_outer(s, locals, globals, outers))
                        || end
                            .as_deref()
                            .is_some_and(|e| expr_uses_outer(e, locals, globals, outers))
                        || step
                            .as_deref()
                            .is_some_and(|st| expr_uses_outer(st, locals, globals, outers))
                }
                ExprKind::ListComp { elt, iter, ifs, .. }
                | ExprKind::SetComp { elt, iter, ifs, .. } => {
                    expr_uses_outer(elt, locals, globals, outers)
                        || expr_uses_outer(iter, locals, globals, outers)
                        || ifs
                            .iter()
                            .any(|i| expr_uses_outer(i, locals, globals, outers))
                }
                ExprKind::UnionCtor { inner, .. } => {
                    expr_uses_outer(inner, locals, globals, outers)
                }
                ExprKind::IfExpr { test, body, orelse } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || expr_uses_outer(body, locals, globals, outers)
                        || expr_uses_outer(orelse, locals, globals, outers)
                }
            }
        }

        fn stmt_uses_outer(
            stmt: &Stmt,
            locals: &HashSet<String>,
            globals: &HashSet<String>,
            outers: &HashSet<String>,
        ) -> bool {
            match &stmt.kind {
                StmtKind::Let { value, .. } => expr_uses_outer(value, locals, globals, outers),
                StmtKind::Assign { value, .. } => expr_uses_outer(value, locals, globals, outers),
                StmtKind::Return { value } => value
                    .as_ref()
                    .is_some_and(|expr| expr_uses_outer(expr, locals, globals, outers)),
                StmtKind::If { test, body, orelse } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || body
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        || orelse
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::While { test, body } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || body
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::For { iter, body, .. } => {
                    expr_uses_outer(iter, locals, globals, outers)
                        || body
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::Expr(expr) => expr_uses_outer(expr, locals, globals, outers),
                StmtKind::Assert { test, msg } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || msg
                            .as_ref()
                            .is_some_and(|m| expr_uses_outer(m, locals, globals, outers))
                }
                StmtKind::Match { subject, cases } => {
                    expr_uses_outer(subject, locals, globals, outers)
                        || cases.iter().any(|c| {
                            c.body
                                .iter()
                                .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        })
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    body.iter()
                        .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        || handlers.iter().any(|h| {
                            h.body
                                .iter()
                                .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        })
                        || orelse
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        || finalbody
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::Raise { exc, cause } => {
                    exc.as_ref()
                        .is_some_and(|e| expr_uses_outer(e, locals, globals, outers))
                        || cause
                            .as_ref()
                            .is_some_and(|c| expr_uses_outer(c, locals, globals, outers))
                }
                StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => false,
            }
        }

        collect_local_defs(
            stmts,
            &mut local_defs,
            &mut global_names,
            &mut nonlocal_names,
        );
        stmts
            .iter()
            .any(|stmt| stmt_uses_outer(stmt, &local_defs, &global_names, &outer_locals))
    }
}
