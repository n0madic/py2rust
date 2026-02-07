// Global-sharing analysis for code generation.

use super::super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn collect_shared_globals(&self, program: &Program) -> HashSet<String> {
        let module_vars = self.collect_module_vars(program);
        let mut used_by_functions = HashSet::new();

        for item in &program.items {
            match item {
                Item::Function(func) => {
                    self.collect_used_globals_in_function(
                        func,
                        &module_vars,
                        &mut used_by_functions,
                    );
                }
                Item::Class(class_def) => {
                    for method in &class_def.methods {
                        self.collect_used_globals_in_function(
                            method,
                            &module_vars,
                            &mut used_by_functions,
                        );
                    }
                }
                Item::Stmt(_) | Item::Union(_) => {}
            }
        }

        // Internal globals (defaults, class attrs) aren't module vars but must be emitted.
        for name in self.ctx.globals.keys() {
            if self
                .ctx
                .globals
                .get(name)
                .is_some_and(|ty| matches!(ty, Type::Module(_) | Type::StdlibFunction { .. }))
            {
                continue;
            }
            if !module_vars.contains(name) {
                used_by_functions.insert(name.clone());
            }
        }

        used_by_functions
    }

    /// Collect module-scope variable names from top-level statements.
    fn collect_module_vars(&self, program: &Program) -> HashSet<String> {
        let mut vars = HashSet::new();
        for item in &program.items {
            if let Item::Stmt(stmt) = item {
                self.collect_module_vars_from_stmt(stmt, &mut vars);
            }
        }
        vars
    }

    /// Walk a statement and record any names bound at module scope.
    fn collect_module_vars_from_stmt(&self, stmt: &Stmt, vars: &mut HashSet<String>) {
        fn record_target(target: &AssignTarget, vars: &mut HashSet<String>) {
            match target {
                AssignTarget::Name(name) => {
                    vars.insert(name.clone());
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    for item in items {
                        record_target(item, vars);
                    }
                }
                AssignTarget::Starred(inner) => record_target(inner, vars),
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
            }
        }

        match &stmt.kind {
            StmtKind::Let { name, .. } => {
                vars.insert(name.clone());
            }
            StmtKind::Assign { target, .. } => {
                record_target(target, vars);
            }
            StmtKind::For { target, body, .. } => {
                for name in target.names() {
                    vars.insert(name.to_string());
                }
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::If { body, orelse, .. } => {
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
                for stmt in orelse {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::While { body, .. } => {
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    for binding in &case.bindings {
                        vars.insert(binding.clone());
                    }
                    for stmt in &case.body {
                        self.collect_module_vars_from_stmt(stmt, vars);
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        vars.insert(name.clone());
                    }
                    for stmt in &handler.body {
                        self.collect_module_vars_from_stmt(stmt, vars);
                    }
                }
                for stmt in orelse {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
                for stmt in finalbody {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::Return { .. }
            | StmtKind::Import { .. }
            | StmtKind::ImportFrom { .. }
            | StmtKind::Global { .. }
            | StmtKind::Nonlocal { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Expr(_)
            | StmtKind::Assert { .. }
            | StmtKind::Raise { .. } => {}
        }
    }

    /// Collect local/global declarations for a statement list within a scope.
    fn collect_scope_locals(
        &self,
        stmts: &[Stmt],
        locals: &mut HashSet<String>,
        globals: &mut HashSet<String>,
    ) {
        fn record_target(
            target: &AssignTarget,
            locals: &mut HashSet<String>,
            globals: &HashSet<String>,
        ) {
            match target {
                AssignTarget::Name(name) => {
                    if !globals.contains(name) {
                        locals.insert(name.clone());
                    }
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    for item in items {
                        record_target(item, locals, globals);
                    }
                }
                AssignTarget::Starred(inner) => record_target(inner, locals, globals),
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
            }
        }

        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, .. } => {
                    if !globals.contains(name) {
                        locals.insert(name.clone());
                    }
                }
                StmtKind::Assign { target, .. } => {
                    record_target(target, locals, globals);
                }
                StmtKind::For { target, body, .. } => {
                    for name in target.names() {
                        if !globals.contains(name) {
                            locals.insert(name.to_string());
                        }
                    }
                    self.collect_scope_locals(body, locals, globals);
                }
                StmtKind::If { body, orelse, .. } => {
                    self.collect_scope_locals(body, locals, globals);
                    self.collect_scope_locals(orelse, locals, globals);
                }
                StmtKind::While { body, .. } => {
                    self.collect_scope_locals(body, locals, globals);
                }
                StmtKind::Match { cases, .. } => {
                    for case in cases {
                        for binding in &case.bindings {
                            if !globals.contains(binding) {
                                locals.insert(binding.clone());
                            }
                        }
                        self.collect_scope_locals(&case.body, locals, globals);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_scope_locals(body, locals, globals);
                    for handler in handlers {
                        if let Some(name) = &handler.name {
                            if !globals.contains(name) {
                                locals.insert(name.clone());
                            }
                        }
                        self.collect_scope_locals(&handler.body, locals, globals);
                    }
                    self.collect_scope_locals(orelse, locals, globals);
                    self.collect_scope_locals(finalbody, locals, globals);
                }
                StmtKind::Global { names } => {
                    for name in names {
                        globals.insert(name.clone());
                        // Global declarations override any prior local binding.
                        locals.remove(name);
                    }
                }
                StmtKind::Nonlocal { names } => {
                    for name in names {
                        // Nonlocal declarations remove the local binding in this scope.
                        locals.remove(name);
                    }
                }
                StmtKind::Return { .. }
                | StmtKind::Import { .. }
                | StmtKind::ImportFrom { .. }
                | StmtKind::Break
                | StmtKind::Continue
                | StmtKind::Expr(_)
                | StmtKind::Assert { .. }
                | StmtKind::Raise { .. } => {}
            }
        }
    }

    /// Find module globals referenced from a function or method body.
    fn collect_used_globals_in_function(
        &self,
        func: &Function,
        module_vars: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        let mut locals: HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();
        let mut globals = HashSet::new();
        self.collect_scope_locals(&func.body, &mut locals, &mut globals);

        // Explicit global declarations always require shared storage.
        for name in &globals {
            if module_vars.contains(name) {
                used.insert(name.clone());
            }
        }

        let outers = HashSet::new();
        self.collect_used_globals_in_stmts(
            &func.body,
            &locals,
            &outers,
            &globals,
            module_vars,
            used,
        );
    }

    /// Walk statements and record module globals used by expressions.
    fn collect_used_globals_in_stmts(
        &self,
        stmts: &[Stmt],
        locals: &HashSet<String>,
        outers: &HashSet<String>,
        globals: &HashSet<String>,
        module_vars: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { value, .. } => {
                    self.collect_used_globals_in_expr(
                        value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Assign { value, .. } => {
                    self.collect_used_globals_in_expr(
                        value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_used_globals_in_expr(
                        test,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        orelse,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::While { test, body } => {
                    self.collect_used_globals_in_expr(
                        test,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_used_globals_in_expr(
                        iter,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Expr(expr) => {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_used_globals_in_expr(
                        test,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    if let Some(expr) = msg {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_used_globals_in_expr(
                        subject,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    for case in cases {
                        self.collect_used_globals_in_stmts(
                            &case.body,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    for handler in handlers {
                        self.collect_used_globals_in_stmts(
                            &handler.body,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                    self.collect_used_globals_in_stmts(
                        orelse,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        finalbody,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                    if let Some(expr) = cause {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
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

    /// Walk an expression tree and record module globals used by name resolution.
    fn collect_used_globals_in_expr(
        &self,
        expr: &Expr,
        locals: &HashSet<String>,
        outers: &HashSet<String>,
        globals: &HashSet<String>,
        module_vars: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        match &expr.kind {
            ExprKind::Name(name) => {
                // Treat module-level names as globals when referenced from non-local scopes.
                if module_vars.contains(name)
                    && (globals.contains(name)
                        || (!locals.contains(name) && !outers.contains(name)))
                {
                    used.insert(name.clone());
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                self.collect_used_globals_in_expr(func, locals, outers, globals, module_vars, used);
                for arg in args {
                    self.collect_used_globals_in_expr(
                        arg,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                for kw in keywords {
                    self.collect_used_globals_in_expr(
                        &kw.value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::Starred { value } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Attr { value, .. } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_used_globals_in_expr(left, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(
                    right,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.collect_used_globals_in_expr(
                    inner,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Compare { left, right, .. } => {
                self.collect_used_globals_in_expr(left, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(
                    right,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.collect_used_globals_in_expr(left, locals, outers, globals, module_vars, used);
                for cmp in comparators {
                    self.collect_used_globals_in_expr(
                        cmp,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for value in values {
                    self.collect_used_globals_in_expr(
                        value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_used_globals_in_expr(
                        item,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_used_globals_in_expr(
                        k,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_expr(
                        v,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
                self.collect_used_globals_in_expr(
                    index,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
                if let Some(expr) = start {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                if let Some(expr) = end {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                if let Some(expr) = step.as_deref() {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
                generators,
            }
            | ExprKind::SetComp {
                elt,
                target,
                iter,
                ifs,
                generators,
            } => {
                // Comprehensions do not inherit `global` declarations.
                let empty_globals = HashSet::new();
                let mut comp_locals = HashSet::new();
                let mut comp_outers = outers.clone();
                comp_outers.extend(locals.iter().cloned());
                if generators.is_empty() {
                    // Backward-compatible single-clause representation.
                    self.collect_used_globals_in_expr(
                        iter,
                        &comp_locals,
                        &comp_outers,
                        &empty_globals,
                        module_vars,
                        used,
                    );
                    comp_locals.insert(target.clone());
                    for cond in ifs {
                        self.collect_used_globals_in_expr(
                            cond,
                            &comp_locals,
                            &comp_outers,
                            &empty_globals,
                            module_vars,
                            used,
                        );
                    }
                } else {
                    // Later generators/filters can reference earlier targets.
                    for clause in generators {
                        self.collect_used_globals_in_expr(
                            &clause.iter,
                            &comp_locals,
                            &comp_outers,
                            &empty_globals,
                            module_vars,
                            used,
                        );
                        comp_locals.insert(clause.target.clone());
                        for cond in &clause.ifs {
                            self.collect_used_globals_in_expr(
                                cond,
                                &comp_locals,
                                &comp_outers,
                                &empty_globals,
                                module_vars,
                                used,
                            );
                        }
                    }
                }
                self.collect_used_globals_in_expr(
                    elt,
                    &comp_locals,
                    &comp_outers,
                    &empty_globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Lambda { params, body } => {
                let mut lambda_locals: HashSet<String> = params.iter().cloned().collect();
                let mut lambda_globals = HashSet::new();
                if let ExprKind::Block { stmts } = &body.kind {
                    self.collect_scope_locals(stmts, &mut lambda_locals, &mut lambda_globals);
                }
                let mut lambda_outers = outers.clone();
                lambda_outers.extend(locals.iter().cloned());
                self.collect_used_globals_in_expr(
                    body,
                    &lambda_locals,
                    &lambda_outers,
                    &lambda_globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_used_globals_in_expr(test, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(body, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(
                    orelse,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Block { stmts } => {
                self.collect_used_globals_in_stmts(
                    stmts,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_used_globals_in_expr(
                    inner,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Literal(_) => {}
        }
    }
}
