// Dict storage-strategy analysis for code generation.

use super::super::*;
use super::walk::{
    walk_storage_expr_tree, walk_storage_stmt_events, StorageExprCallbacks, StorageStmtEvent,
};

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

    /// Walk statements and record whether dict locals can remain as local IndexMap.
    fn collect_dict_storage_in_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        walk_storage_stmt_events(
            stmts,
            DictUseContext::Value,
            DictUseContext::Escape,
            &mut |event| match event {
                StorageStmtEvent::Let { name, value } => {
                    self.note_dict_storage_assignment(name, value, shared_globals, storage);
                    // Alias assignment: let x = y
                    if let ExprKind::Name(src) = &value.kind {
                        if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
                            self.mark_dict_shared(src, storage);
                            self.mark_dict_shared(name, storage);
                        }
                    }
                }
                StorageStmtEvent::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_dict_storage_assignment(name, value, shared_globals, storage);
                        if let ExprKind::Name(src) = &value.kind {
                            if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
                                self.mark_dict_shared(src, storage);
                                self.mark_dict_shared(name, storage);
                            }
                        }
                    }
                }
                StorageStmtEvent::Expr { expr, ctx } => {
                    self.collect_dict_storage_in_expr(expr, ctx, shared_globals, storage);
                }
            },
        );
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
        let mut visitor = DictStorageExprVisitor {
            codegen: self,
            shared_globals,
            storage,
        };
        walk_storage_expr_tree(&mut visitor, expr, ctx);
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

/// Adapter that applies the shared expression walker to dict storage rules.
struct DictStorageExprVisitor<'codegen, 'ctx, 'src> {
    codegen: &'codegen Codegen<'src>,
    shared_globals: &'ctx HashSet<String>,
    storage: &'ctx mut HashMap<String, DictStorage>,
}

impl<'codegen, 'ctx, 'src> StorageExprCallbacks<DictUseContext>
    for DictStorageExprVisitor<'codegen, 'ctx, 'src>
{
    fn value_ctx(&self) -> DictUseContext {
        DictUseContext::Value
    }

    fn escape_ctx(&self) -> DictUseContext {
        DictUseContext::Escape
    }

    fn call_is_safe(&self, func: &Expr) -> bool {
        self.codegen.call_is_dict_safe(func)
    }

    fn visit_expr(&mut self, expr: &Expr, ctx: DictUseContext) {
        if matches!(ctx, DictUseContext::Escape)
            && matches!(expr.ty.as_ref(), Some(Type::Dict(_, _)))
        {
            if let ExprKind::Name(name) = &expr.kind {
                self.codegen.mark_dict_shared(name, self.storage);
            }
        }
        match &expr.kind {
            ExprKind::Compare {
                op: CmpOp::Is | CmpOp::IsNot,
                left,
                right,
            } => {
                self.codegen.mark_identity_dict_operand(left, self.storage);
                self.codegen.mark_identity_dict_operand(right, self.storage);
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
                        self.codegen.mark_identity_dict_operand(prev, self.storage);
                        self.codegen.mark_identity_dict_operand(cmp, self.storage);
                    }
                    prev = cmp;
                }
            }
            _ => {}
        }
    }

    fn visit_block(&mut self, stmts: &[Stmt]) {
        // Nested block expressions participate in the same storage map.
        self.codegen
            .collect_dict_storage_in_stmts(stmts, self.shared_globals, self.storage);
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
