// List storage-strategy analysis for code generation.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Collect list storage strategies for a block of statements.
    ///
    /// This analysis is conservative: if a list can escape or be aliased, it
    /// is marked Shared and emitted as Arc<Mutex<Vec<T>>>. Only non-escaping
    /// lists initialized from fresh literals/comprehensions are marked Local.
    pub(in crate::codegen) fn collect_list_storage_for_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
    ) -> HashMap<String, ListStorage> {
        let mut storage = HashMap::new();
        self.collect_list_storage_in_stmts(stmts, shared_globals, &mut storage);
        storage
    }

    /// Walk statements and record whether list locals can remain as Vec<T>.
    fn collect_list_storage_in_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    self.note_list_storage_assignment(name, value, shared_globals, storage);
                    // Alias assignment: let x = y
                    if let ExprKind::Name(src) = &value.kind {
                        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                            self.mark_list_shared(src, storage);
                            self.mark_list_shared(name, storage);
                        }
                    }
                    self.collect_list_storage_in_expr(
                        value,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_list_storage_assignment(name, value, shared_globals, storage);
                        if let ExprKind::Name(src) = &value.kind {
                            if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                                self.mark_list_shared(src, storage);
                                self.mark_list_shared(name, storage);
                            }
                        }
                    }
                    // Assigning a list into a container is an escape.
                    let ctx = match target {
                        AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {
                            ListUseContext::Escape
                        }
                        _ => ListUseContext::Value,
                    };
                    self.collect_list_storage_in_expr(value, ctx, shared_globals, storage);
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Escape,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_list_storage_in_expr(
                        test,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                    self.collect_list_storage_in_stmts(orelse, shared_globals, storage);
                }
                StmtKind::While { test, body } => {
                    self.collect_list_storage_in_expr(
                        test,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_list_storage_in_expr(
                        iter,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                }
                StmtKind::Expr(expr) => {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_list_storage_in_expr(
                        test,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    if let Some(expr) = msg {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_list_storage_in_expr(
                        subject,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    for case in cases {
                        self.collect_list_storage_in_stmts(&case.body, shared_globals, storage);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                    for handler in handlers {
                        self.collect_list_storage_in_stmts(&handler.body, shared_globals, storage);
                    }
                    self.collect_list_storage_in_stmts(orelse, shared_globals, storage);
                    self.collect_list_storage_in_stmts(finalbody, shared_globals, storage);
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    if let Some(expr) = cause {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
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

    /// Record a list assignment and decide if it can stay local.
    fn note_list_storage_assignment(
        &self,
        name: &str,
        value: &Expr,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        if shared_globals.contains(name) {
            self.mark_list_shared(name, storage);
            return;
        }
        if !matches!(value.ty.as_ref(), Some(Type::List(_))) {
            return;
        }
        if self.is_fresh_list_expr(value) {
            self.mark_list_local_if_absent(name, storage);
        } else {
            self.mark_list_shared(name, storage);
        }
    }

    /// Determine if an expression creates a fresh list value.
    fn is_fresh_list_expr(&self, expr: &Expr) -> bool {
        matches!(expr.kind, ExprKind::List(_) | ExprKind::ListComp { .. })
    }

    /// Record list usage inside expressions, marking escapes conservatively.
    fn collect_list_storage_in_expr(
        &self,
        expr: &Expr,
        ctx: ListUseContext,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        match &expr.kind {
            ExprKind::Name(name) => {
                if matches!(ctx, ListUseContext::Escape)
                    && matches!(expr.ty.as_ref(), Some(Type::List(_)))
                {
                    self.mark_list_shared(name, storage);
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                let safe = self.call_is_list_safe(func);
                match &func.kind {
                    ExprKind::Attr { value, .. } => {
                        self.collect_list_storage_in_expr(
                            value,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    _ => {
                        self.collect_list_storage_in_expr(
                            func,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                let arg_ctx = if safe {
                    ListUseContext::Value
                } else {
                    ListUseContext::Escape
                };
                for arg in args {
                    self.collect_list_storage_in_expr(arg, arg_ctx, shared_globals, storage);
                }
                for kw in keywords {
                    self.collect_list_storage_in_expr(&kw.value, arg_ctx, shared_globals, storage);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    self.collect_list_storage_in_expr(
                        value,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Attr { value, .. } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_list_storage_in_expr(
                    left,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    right,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Unary { expr, .. } => {
                self.collect_list_storage_in_expr(
                    expr,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Compare { op, left, right } => {
                if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                    self.mark_identity_list_operand(left, storage);
                    self.mark_identity_list_operand(right, storage);
                }
                self.collect_list_storage_in_expr(
                    left,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    right,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::CompareChain {
                left,
                ops,
                comparators,
                ..
            } => {
                let mut prev = left.as_ref();
                for (op, cmp) in ops.iter().zip(comparators.iter()) {
                    if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                        self.mark_identity_list_operand(prev, storage);
                        self.mark_identity_list_operand(cmp, storage);
                    }
                    prev = cmp;
                }
                self.collect_list_storage_in_expr(
                    left,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cmp in comparators {
                    self.collect_list_storage_in_expr(
                        cmp,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for val in values {
                    self.collect_list_storage_in_expr(
                        val,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_list_storage_in_expr(
                        item,
                        ListUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_list_storage_in_expr(
                        k,
                        ListUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_expr(
                        v,
                        ListUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    index,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                if let Some(expr) = start {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = end {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = step {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.collect_list_storage_in_expr(
                    iter,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    elt,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_list_storage_in_expr(
                        cond,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_list_storage_in_expr(
                    iter,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    elt,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_list_storage_in_expr(
                        cond,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Lambda { body, .. } => {
                // Lambdas can escape; treat captured list uses as shared.
                self.collect_list_storage_in_expr(
                    body,
                    ListUseContext::Escape,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_list_storage_in_expr(
                    test,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    body,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    orelse,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Block { stmts } => {
                self.collect_list_storage_in_stmts(stmts, shared_globals, storage);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_list_storage_in_expr(
                    inner,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Literal(_) => {}
        }
    }

    /// Mark list operands used in identity comparisons as shared.
    fn mark_identity_list_operand(&self, expr: &Expr, storage: &mut HashMap<String, ListStorage>) {
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            if let ExprKind::Name(name) = &expr.kind {
                self.mark_list_shared(name, storage);
            }
        }
    }

    /// Mark a list variable as shared.
    fn mark_list_shared(&self, name: &str, storage: &mut HashMap<String, ListStorage>) {
        storage.insert(name.to_string(), ListStorage::Shared);
    }

    /// Mark a list variable as local if it hasn't already been forced shared.
    fn mark_list_local_if_absent(&self, name: &str, storage: &mut HashMap<String, ListStorage>) {
        storage
            .entry(name.to_string())
            .or_insert(ListStorage::Local);
    }

    /// Decide whether a call is safe to treat list arguments as non-escaping.
    fn call_is_list_safe(&self, func: &Expr) -> bool {
        if let ExprKind::Name(name) = &func.kind {
            return matches!(
                name.as_str(),
                "len"
                    | "print"
                    | "enumerate"
                    | "zip"
                    | "map"
                    | "filter"
                    | "reversed"
                    | "all"
                    | "any"
                    | "min"
                    | "max"
                    | "sum"
                    | "list"
                    | "tuple"
                    | "set"
            );
        }
        false
    }
}

/// List usage context for storage analysis.
#[derive(Copy, Clone, PartialEq, Eq)]
enum ListUseContext {
    /// Regular evaluation; list does not escape.
    Value,
    /// List value escapes and must be shared.
    Escape,
}
