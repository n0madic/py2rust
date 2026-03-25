// Global-sharing analysis for code generation.

use super::super::*;
use super::walk::{walk_assign_target_names, walk_stmt_tree};
use std::collections::HashMap;

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
        walk_stmt_tree(
            std::slice::from_ref(stmt),
            &mut |current| match &current.kind {
                StmtKind::Let { name, .. } => {
                    vars.insert(name.clone());
                }
                StmtKind::Assign { target, .. } => {
                    walk_assign_target_names(target, &mut |name| {
                        vars.insert(name.to_string());
                    });
                }
                StmtKind::For { target, .. } => {
                    for name in target.names() {
                        vars.insert(name.to_string());
                    }
                }
                StmtKind::Match { cases, .. } => {
                    for case in cases {
                        for binding in &case.bindings {
                            vars.insert(binding.clone());
                        }
                    }
                }
                StmtKind::Try { handlers, .. } => {
                    for handler in handlers {
                        if let Some(name) = &handler.name {
                            vars.insert(name.clone());
                        }
                    }
                }
                StmtKind::Class { def } => {
                    vars.insert(def.name.clone());
                }
                StmtKind::If { .. }
                | StmtKind::While { .. }
                | StmtKind::Return { .. }
                | StmtKind::Expr(_)
                | StmtKind::Assert { .. }
                | StmtKind::Raise { .. }
                | StmtKind::Import { .. }
                | StmtKind::ImportFrom { .. }
                | StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Delete { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
            },
        );
    }

    /// Collect local/global declarations for a statement list within a scope.
    fn collect_scope_locals(
        &self,
        stmts: &[Stmt],
        locals: &mut HashSet<String>,
        globals: &mut HashSet<String>,
    ) {
        walk_stmt_tree(stmts, &mut |stmt| match &stmt.kind {
            StmtKind::Let { name, .. } => {
                if !globals.contains(name) {
                    locals.insert(name.clone());
                }
            }
            StmtKind::Assign { target, .. } => {
                walk_assign_target_names(target, &mut |name| {
                    if !globals.contains(name) {
                        locals.insert(name.to_string());
                    }
                });
            }
            StmtKind::For { target, .. } => {
                for name in target.names() {
                    if !globals.contains(name) {
                        locals.insert(name.to_string());
                    }
                }
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    for binding in &case.bindings {
                        if !globals.contains(binding) {
                            locals.insert(binding.clone());
                        }
                    }
                }
            }
            StmtKind::Try { handlers, .. } => {
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        if !globals.contains(name) {
                            locals.insert(name.clone());
                        }
                    }
                }
            }
            StmtKind::Global { names } => {
                for name in names {
                    globals.insert(name.clone());
                    // Global declarations override any prior local binding.
                    locals.remove(name);
                }
            }
            StmtKind::Class { def } => {
                if !globals.contains(&def.name) {
                    locals.insert(def.name.clone());
                }
            }
            StmtKind::Nonlocal { names } => {
                for name in names {
                    // Nonlocal declarations remove the local binding in this scope.
                    locals.remove(name);
                }
            }
            StmtKind::If { .. }
            | StmtKind::While { .. }
            | StmtKind::Return { .. }
            | StmtKind::Expr(_)
            | StmtKind::Assert { .. }
            | StmtKind::Raise { .. }
            | StmtKind::Import { .. }
            | StmtKind::ImportFrom { .. }
            | StmtKind::Delete { .. }
            | StmtKind::Break
            | StmtKind::Continue => {}
        });
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
                StmtKind::Delete { target } => match target.as_ref() {
                    AssignTarget::Attr { value, .. } => {
                        self.collect_used_globals_in_expr(
                            value,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                    AssignTarget::Index { value, index } => {
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
                    AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                        for item in items {
                            if let AssignTarget::Attr { value, .. } = item {
                                self.collect_used_globals_in_expr(
                                    value,
                                    locals,
                                    outers,
                                    globals,
                                    module_vars,
                                    used,
                                );
                            } else if let AssignTarget::Index { value, index } = item {
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
                        }
                    }
                    AssignTarget::Starred(inner) => {
                        if let AssignTarget::Attr { value, .. } = inner.as_ref() {
                            self.collect_used_globals_in_expr(
                                value,
                                locals,
                                outers,
                                globals,
                                module_vars,
                                used,
                            );
                        } else if let AssignTarget::Index { value, index } = inner.as_ref() {
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
                    }
                    AssignTarget::Name(_) => {}
                },
                StmtKind::Class { def } => {
                    for attr in &def.class_attrs {
                        self.collect_used_globals_in_expr(
                            &attr.value,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                    for method in &def.methods {
                        let mut method_locals: HashSet<String> =
                            method.params.iter().map(|p| p.name.clone()).collect();
                        let mut method_globals = HashSet::new();
                        self.collect_scope_locals(
                            &method.body,
                            &mut method_locals,
                            &mut method_globals,
                        );
                        let mut method_outers = outers.clone();
                        method_outers.extend(locals.iter().cloned());
                        self.collect_used_globals_in_stmts(
                            &method.body,
                            &method_locals,
                            &method_outers,
                            &method_globals,
                            module_vars,
                            used,
                        );
                    }
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
            ExprKind::Yield { value } => {
                if let Some(value) = value {
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
                for entry in items {
                    match entry {
                        DictEntry::Item { key, value } => {
                            self.collect_used_globals_in_expr(
                                key,
                                locals,
                                outers,
                                globals,
                                module_vars,
                                used,
                            );
                            self.collect_used_globals_in_expr(
                                value,
                                locals,
                                outers,
                                globals,
                                module_vars,
                                used,
                            );
                        }
                        DictEntry::Unpack { value } => {
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
            ExprKind::Lambda { params, body, .. } => {
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

    /// Detect shared globals that are assigned exactly once and have a scalar
    /// (Copy) type. These can use `OnceLock<T>` without a Mutex wrapper, avoiding
    /// lock overhead on every read in hot loops.
    pub(crate) fn collect_readonly_globals(
        &self,
        program: &Program,
        shared_globals: &HashSet<String>,
    ) -> HashSet<String> {
        let mut write_counts: HashMap<String, usize> = HashMap::new();

        // Count assignments at module top level.
        for item in &program.items {
            if let Item::Stmt(stmt) = item {
                Self::count_global_writes(stmt, shared_globals, &mut write_counts);
            }
        }

        // Count assignments inside functions that use `global` declarations.
        for item in &program.items {
            match item {
                Item::Function(func) => {
                    let explicit = Self::get_explicit_globals(&func.body);
                    Self::count_global_writes_in_func(&func.body, &explicit, &mut write_counts);
                }
                Item::Class(class_def) => {
                    for method in &class_def.methods {
                        let explicit = Self::get_explicit_globals(&method.body);
                        Self::count_global_writes_in_func(
                            &method.body,
                            &explicit,
                            &mut write_counts,
                        );
                    }
                }
                Item::Stmt(_) | Item::Union(_) => {}
            }
        }

        // A shared global is readonly if written exactly once and has a scalar type.
        // Scalar types are Copy in Rust (i64, f64, bool), so reads are cheap.
        shared_globals
            .iter()
            .filter(|name| {
                write_counts.get(*name).copied().unwrap_or(0) == 1
                    && self
                        .ctx
                        .globals
                        .get(*name)
                        .is_some_and(Type::is_numeric)
            })
            .cloned()
            .collect()
    }

    /// Count all assignments to shared global names within a statement tree.
    fn count_global_writes(
        stmt: &Stmt,
        shared_globals: &HashSet<String>,
        counts: &mut HashMap<String, usize>,
    ) {
        walk_stmt_tree(std::slice::from_ref(stmt), &mut |s| match &s.kind {
            StmtKind::Let { name, .. } => {
                if shared_globals.contains(name) {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
            }
            StmtKind::Assign { target, .. } => {
                walk_assign_target_names(target, &mut |name| {
                    if shared_globals.contains(name) {
                        *counts.entry(name.to_string()).or_insert(0) += 1;
                    }
                });
            }
            StmtKind::For { target, .. } => {
                for name in target.names() {
                    if shared_globals.contains(name) {
                        *counts.entry(name.to_string()).or_insert(0) += 1;
                    }
                }
            }
            _ => {}
        });
    }

    /// Count assignments to explicitly `global`-declared names within a function body.
    fn count_global_writes_in_func(
        stmts: &[Stmt],
        explicit_globals: &HashSet<String>,
        counts: &mut HashMap<String, usize>,
    ) {
        if explicit_globals.is_empty() {
            return;
        }
        walk_stmt_tree(stmts, &mut |s| match &s.kind {
            StmtKind::Assign { target, .. } => {
                walk_assign_target_names(target, &mut |name| {
                    if explicit_globals.contains(name) {
                        *counts.entry(name.to_string()).or_insert(0) += 1;
                    }
                });
            }
            StmtKind::For { target, .. } => {
                for name in target.names() {
                    if explicit_globals.contains(name) {
                        *counts.entry(name.to_string()).or_insert(0) += 1;
                    }
                }
            }
            _ => {}
        });
    }

    /// Collect names from `global` declarations in a function body.
    fn get_explicit_globals(stmts: &[Stmt]) -> HashSet<String> {
        let mut globals = HashSet::new();
        walk_stmt_tree(stmts, &mut |s| {
            if let StmtKind::Global { names } = &s.kind {
                globals.extend(names.iter().cloned());
            }
        });
        globals
    }
}
