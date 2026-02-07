// Dict storage-strategy analysis for code generation.

use super::super::*;
use super::walk::walk_stmt_tree;

impl<'a> Codegen<'a> {
    /// Compute dict storage strategy for a statement list.
    pub(in crate::codegen) fn collect_dict_storage_for_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
    ) -> HashMap<String, DictStorage> {
        let mut storage = HashMap::new();
        self.collect_dict_storage_in_stmts(stmts, shared_globals, &mut storage);
        storage
    }

    /// Walk statements and record whether dict locals can remain as HashMap.
    fn collect_dict_storage_in_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        walk_stmt_tree(stmts, &mut |stmt| match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                self.note_dict_storage_assignment(name, value, shared_globals, storage);
                // Alias assignment: let x = y
                if let ExprKind::Name(src) = &value.kind {
                    if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
                        self.mark_dict_shared(src, storage);
                        self.mark_dict_shared(name, storage);
                    }
                }
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            StmtKind::Assign { target, value } => {
                if let AssignTarget::Name(name) = target {
                    self.note_dict_storage_assignment(name, value, shared_globals, storage);
                    if let ExprKind::Name(src) = &value.kind {
                        if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
                            self.mark_dict_shared(src, storage);
                            self.mark_dict_shared(name, storage);
                        }
                    }
                }
                // Assigning a dict into a container is an escape.
                let ctx = match target {
                    AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {
                        DictUseContext::Escape
                    }
                    _ => DictUseContext::Value,
                };
                self.collect_dict_storage_in_expr(value, ctx, shared_globals, storage);
            }
            StmtKind::Return { value } => {
                if let Some(expr) = value {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            StmtKind::If { test, .. } | StmtKind::While { test, .. } => {
                self.collect_dict_storage_in_expr(
                    test,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            StmtKind::For { iter, .. } => {
                self.collect_dict_storage_in_expr(
                    iter,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            StmtKind::Expr(expr) => {
                self.collect_dict_storage_in_expr(
                    expr,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            StmtKind::Assert { test, msg } => {
                self.collect_dict_storage_in_expr(
                    test,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                if let Some(expr) = msg {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            StmtKind::Match { subject, .. } => {
                self.collect_dict_storage_in_expr(
                    subject,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(expr) = exc {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = cause {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
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

    /// Record a dict assignment and decide if it can stay local.
    fn note_dict_storage_assignment(
        &self,
        name: &str,
        value: &Expr,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        if shared_globals.contains(name) {
            self.mark_dict_shared(name, storage);
            return;
        }
        if !matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
            return;
        }
        if self.is_fresh_dict_expr(value) {
            self.mark_dict_local_if_absent(name, storage);
        } else {
            self.mark_dict_shared(name, storage);
        }
    }

    /// Determine if an expression creates a fresh dict value.
    fn is_fresh_dict_expr(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Dict(_) => true,
            ExprKind::Call { func, .. } => matches!(
                func.kind,
                ExprKind::Name(ref name) if name == "dict"
            ),
            _ => false,
        }
    }

    /// Record dict usage inside expressions, marking escapes conservatively.
    fn collect_dict_storage_in_expr(
        &self,
        expr: &Expr,
        ctx: DictUseContext,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        match &expr.kind {
            ExprKind::Name(name) => {
                if matches!(ctx, DictUseContext::Escape)
                    && matches!(expr.ty.as_ref(), Some(Type::Dict(_, _)))
                {
                    self.mark_dict_shared(name, storage);
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                let safe = self.call_is_dict_safe(func);
                match &func.kind {
                    ExprKind::Attr { value, .. } => {
                        self.collect_dict_storage_in_expr(
                            value,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    _ => {
                        self.collect_dict_storage_in_expr(
                            func,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                let arg_ctx = if safe {
                    DictUseContext::Value
                } else {
                    DictUseContext::Escape
                };
                for arg in args {
                    self.collect_dict_storage_in_expr(arg, arg_ctx, shared_globals, storage);
                }
                for kw in keywords {
                    self.collect_dict_storage_in_expr(&kw.value, arg_ctx, shared_globals, storage);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    self.collect_dict_storage_in_expr(
                        value,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Attr { value, .. } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_dict_storage_in_expr(
                    left,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    right,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Unary { expr, .. } => {
                self.collect_dict_storage_in_expr(
                    expr,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Compare { op, left, right } => {
                if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                    self.mark_identity_dict_operand(left, storage);
                    self.mark_identity_dict_operand(right, storage);
                }
                self.collect_dict_storage_in_expr(
                    left,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    right,
                    DictUseContext::Value,
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
                        self.mark_identity_dict_operand(prev, storage);
                        self.mark_identity_dict_operand(cmp, storage);
                    }
                    prev = cmp;
                }
                self.collect_dict_storage_in_expr(
                    left,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cmp in comparators {
                    self.collect_dict_storage_in_expr(
                        cmp,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for val in values {
                    self.collect_dict_storage_in_expr(
                        val,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_dict_storage_in_expr(
                        item,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_dict_storage_in_expr(
                        k,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                    self.collect_dict_storage_in_expr(
                        v,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    index,
                    DictUseContext::Value,
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
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                if let Some(expr) = start {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = end {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = step {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.collect_dict_storage_in_expr(
                    iter,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    elt,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_dict_storage_in_expr(
                        cond,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_dict_storage_in_expr(
                    iter,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    elt,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_dict_storage_in_expr(
                        cond,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Lambda { body, .. } => {
                // Lambdas can escape; treat captured dict uses as shared.
                self.collect_dict_storage_in_expr(
                    body,
                    DictUseContext::Escape,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_dict_storage_in_expr(
                    test,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    body,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    orelse,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Block { stmts } => {
                self.collect_dict_storage_in_stmts(stmts, shared_globals, storage);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_dict_storage_in_expr(
                    inner,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Literal(_) => {}
        }
    }

    /// Mark dict operands used in identity comparisons as shared.
    fn mark_identity_dict_operand(&self, expr: &Expr, storage: &mut HashMap<String, DictStorage>) {
        if matches!(expr.ty.as_ref(), Some(Type::Dict(_, _))) {
            if let ExprKind::Name(name) = &expr.kind {
                self.mark_dict_shared(name, storage);
            }
        }
    }

    /// Mark a dict variable as shared.
    fn mark_dict_shared(&self, name: &str, storage: &mut HashMap<String, DictStorage>) {
        storage.insert(name.to_string(), DictStorage::Shared);
    }

    /// Mark a dict variable as local if it hasn't already been forced shared.
    fn mark_dict_local_if_absent(&self, name: &str, storage: &mut HashMap<String, DictStorage>) {
        storage
            .entry(name.to_string())
            .or_insert(DictStorage::Local);
    }

    /// Decide whether a call is safe to treat dict arguments as non-escaping.
    fn call_is_dict_safe(&self, func: &Expr) -> bool {
        if let ExprKind::Name(name) = &func.kind {
            return matches!(
                name.as_str(),
                "len"
                    | "print"
                    | "all"
                    | "any"
                    | "min"
                    | "max"
                    | "sum"
                    | "dict"
                    | "list"
                    | "tuple"
                    | "set"
            );
        }
        false
    }
}

/// Dict usage context for storage analysis.
#[derive(Copy, Clone, PartialEq, Eq)]
enum DictUseContext {
    /// Regular evaluation; dict does not escape.
    Value,
    /// Dict value escapes and must be shared.
    Escape,
}
