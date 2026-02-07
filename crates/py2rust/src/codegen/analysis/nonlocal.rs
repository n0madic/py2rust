// Nonlocal capture analysis for code generation.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Analyze nonlocal declarations in a scope and determine which locals
    /// must be stored in `Rc<RefCell<_>>` for inner mutations.
    pub(in crate::codegen) fn collect_nonlocal_info_for_stmts(
        &self,
        stmts: &[Stmt],
        params: &[String],
    ) -> NonlocalInfo {
        fn collect_declares(
            stmts: &[Stmt],
            nonlocals: &mut HashSet<String>,
            globals: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Nonlocal { names } => {
                        for name in names {
                            nonlocals.insert(name.clone());
                        }
                    }
                    StmtKind::Global { names } => {
                        for name in names {
                            globals.insert(name.clone());
                        }
                    }
                    StmtKind::If { body, orelse, .. } => {
                        collect_declares(body, nonlocals, globals);
                        collect_declares(orelse, nonlocals, globals);
                    }
                    StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                        collect_declares(body, nonlocals, globals);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            collect_declares(&case.body, nonlocals, globals);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        collect_declares(body, nonlocals, globals);
                        for handler in handlers {
                            collect_declares(&handler.body, nonlocals, globals);
                        }
                        collect_declares(orelse, nonlocals, globals);
                        collect_declares(finalbody, nonlocals, globals);
                    }
                    _ => {}
                }
            }
        }

        fn record_target(
            target: &AssignTarget,
            locals: &mut HashSet<String>,
            skip: &HashSet<String>,
        ) {
            match target {
                AssignTarget::Name(name) => {
                    if !skip.contains(name) {
                        locals.insert(name.clone());
                    }
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    for item in items {
                        record_target(item, locals, skip);
                    }
                }
                AssignTarget::Starred(inner) => record_target(inner, locals, skip),
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
            }
        }

        fn collect_local_defs(
            stmts: &[Stmt],
            locals: &mut HashSet<String>,
            skip: &HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Let { name, .. } => {
                        if !skip.contains(name) {
                            locals.insert(name.clone());
                        }
                    }
                    StmtKind::Assign { target, .. } => {
                        record_target(target, locals, skip);
                    }
                    StmtKind::For { target, body, .. } => {
                        for name in target.names() {
                            if !skip.contains(name) {
                                locals.insert(name.to_string());
                            }
                        }
                        collect_local_defs(body, locals, skip);
                    }
                    StmtKind::If { body, orelse, .. } => {
                        collect_local_defs(body, locals, skip);
                        collect_local_defs(orelse, locals, skip);
                    }
                    StmtKind::While { body, .. } => {
                        collect_local_defs(body, locals, skip);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            for binding in &case.bindings {
                                if !skip.contains(binding) {
                                    locals.insert(binding.clone());
                                }
                            }
                            collect_local_defs(&case.body, locals, skip);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        collect_local_defs(body, locals, skip);
                        for handler in handlers {
                            if let Some(name) = &handler.name {
                                if !skip.contains(name) {
                                    locals.insert(name.clone());
                                }
                            }
                            collect_local_defs(&handler.body, locals, skip);
                        }
                        collect_local_defs(orelse, locals, skip);
                        collect_local_defs(finalbody, locals, skip);
                    }
                    _ => {}
                }
            }
        }

        fn visit_expr_for_lambdas(
            this: &Codegen,
            expr: &Expr,
            locals: &HashSet<String>,
            nonlocals: &HashSet<String>,
            globals: &HashSet<String>,
            cell_locals: &mut HashSet<String>,
            unresolved: &mut HashSet<String>,
        ) {
            match &expr.kind {
                ExprKind::Lambda { params, body } => {
                    if let ExprKind::Block { stmts } = &body.kind {
                        let info = this.collect_nonlocal_info_for_stmts(stmts, params);
                        for name in info.unresolved {
                            if locals.contains(&name) {
                                cell_locals.insert(name);
                            } else {
                                unresolved.insert(name);
                            }
                        }
                    }
                }
                ExprKind::Call {
                    func,
                    args,
                    keywords,
                } => {
                    visit_expr_for_lambdas(
                        this,
                        func,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    for arg in args {
                        visit_expr_for_lambdas(
                            this,
                            arg,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    for kw in keywords {
                        visit_expr_for_lambdas(
                            this,
                            &kw.value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Starred { value } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Yield { value } => {
                    if let Some(value) = value {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Attr { value, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        left,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        right,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Unary { expr, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        expr,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::CompareChain {
                    left, comparators, ..
                } => {
                    visit_expr_for_lambdas(
                        this,
                        left,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    for cmp in comparators {
                        visit_expr_for_lambdas(
                            this,
                            cmp,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::BoolOp { values, .. } => {
                    for value in values {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                    for item in items {
                        visit_expr_for_lambdas(
                            this,
                            item,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Dict(items) => {
                    for (k, v) in items {
                        visit_expr_for_lambdas(
                            this,
                            k,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_expr_for_lambdas(
                            this,
                            v,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Index { value, index } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        index,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    if let Some(start) = start.as_deref() {
                        visit_expr_for_lambdas(
                            this,
                            start,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    if let Some(end) = end.as_deref() {
                        visit_expr_for_lambdas(
                            this,
                            end,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    if let Some(step) = step.as_deref() {
                        visit_expr_for_lambdas(
                            this,
                            step,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::ListComp { elt, iter, ifs, .. }
                | ExprKind::SetComp { elt, iter, ifs, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        elt,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        iter,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    for cond in ifs {
                        visit_expr_for_lambdas(
                            this,
                            cond,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::UnionCtor { inner, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        inner,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::IfExpr { test, body, orelse } => {
                    visit_expr_for_lambdas(
                        this,
                        test,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        body,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        orelse,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Block { stmts } => {
                    visit_stmts_for_lambdas(
                        this,
                        stmts,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Name(_) | ExprKind::Literal(_) => {}
            }
        }

        fn visit_stmts_for_lambdas(
            this: &Codegen,
            stmts: &[Stmt],
            locals: &HashSet<String>,
            nonlocals: &HashSet<String>,
            globals: &HashSet<String>,
            cell_locals: &mut HashSet<String>,
            unresolved: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Let { value, .. } => {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Assign { value, .. } => {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Return { value } => {
                        if let Some(expr) = value {
                            visit_expr_for_lambdas(
                                this,
                                expr,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::If { test, body, orelse } => {
                        visit_expr_for_lambdas(
                            this,
                            test,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            orelse,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::While { test, body } => {
                        visit_expr_for_lambdas(
                            this,
                            test,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::For { iter, body, .. } => {
                        visit_expr_for_lambdas(
                            this,
                            iter,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Expr(expr) => {
                        visit_expr_for_lambdas(
                            this,
                            expr,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Assert { test, msg } => {
                        visit_expr_for_lambdas(
                            this,
                            test,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        if let Some(msg) = msg {
                            visit_expr_for_lambdas(
                                this,
                                msg,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        visit_expr_for_lambdas(
                            this,
                            subject,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        for case in cases {
                            visit_stmts_for_lambdas(
                                this,
                                &case.body,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        for handler in handlers {
                            visit_stmts_for_lambdas(
                                this,
                                &handler.body,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                        visit_stmts_for_lambdas(
                            this,
                            orelse,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            finalbody,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Raise { exc, cause } => {
                        if let Some(expr) = exc {
                            visit_expr_for_lambdas(
                                this,
                                expr,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                        if let Some(expr) = cause {
                            visit_expr_for_lambdas(
                                this,
                                expr,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::Import { .. }
                    | StmtKind::ImportFrom { .. }
                    | StmtKind::Global { .. }
                    | StmtKind::Nonlocal { .. }
                    | StmtKind::Break
                    | StmtKind::Continue => {}
                }
            }
        }

        let mut nonlocal_decls = HashSet::new();
        let mut global_decls = HashSet::new();
        collect_declares(stmts, &mut nonlocal_decls, &mut global_decls);

        let mut local_defs: HashSet<String> = params.iter().cloned().collect();
        let mut skip: HashSet<String> = HashSet::new();
        for name in nonlocal_decls.iter().chain(global_decls.iter()) {
            skip.insert(name.clone());
        }
        collect_local_defs(stmts, &mut local_defs, &skip);

        let mut cell_locals = HashSet::new();
        let mut unresolved = HashSet::new();
        visit_stmts_for_lambdas(
            self,
            stmts,
            &local_defs,
            &nonlocal_decls,
            &global_decls,
            &mut cell_locals,
            &mut unresolved,
        );

        for name in nonlocal_decls.iter() {
            unresolved.insert(name.clone());
        }

        NonlocalInfo {
            nonlocal_decls,
            cell_locals,
            unresolved,
        }
    }
}

/// Nonlocal analysis result for a single scope.
#[derive(Default)]
pub(in crate::codegen) struct NonlocalInfo {
    pub(in crate::codegen) nonlocal_decls: HashSet<String>,
    pub(in crate::codegen) cell_locals: HashSet<String>,
    pub(in crate::codegen) unresolved: HashSet<String>,
}
