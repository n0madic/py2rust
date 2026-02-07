// List storage-strategy analysis for code generation.

use super::super::*;
use super::walk::{
    walk_storage_expr_tree, walk_storage_stmt_events, StorageExprCallbacks, StorageStmtEvent,
};

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
        walk_storage_stmt_events(
            stmts,
            ListUseContext::Value,
            ListUseContext::Escape,
            &mut |event| match event {
                StorageStmtEvent::Let { name, value } => {
                    self.note_list_storage_assignment(name, value, shared_globals, storage);
                    // Alias assignment: let x = y
                    if let ExprKind::Name(src) = &value.kind {
                        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                            self.mark_list_shared(src, storage);
                            self.mark_list_shared(name, storage);
                        }
                    }
                }
                StorageStmtEvent::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_list_storage_assignment(name, value, shared_globals, storage);
                        if let ExprKind::Name(src) = &value.kind {
                            if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                                self.mark_list_shared(src, storage);
                                self.mark_list_shared(name, storage);
                            }
                        }
                    }
                }
                StorageStmtEvent::Expr { expr, ctx } => {
                    self.collect_list_storage_in_expr(expr, ctx, shared_globals, storage);
                }
            },
        );
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
        let mut visitor = ListStorageExprVisitor {
            codegen: self,
            shared_globals,
            storage,
        };
        walk_storage_expr_tree(&mut visitor, expr, ctx);
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

/// Adapter that applies the shared expression walker to list storage rules.
struct ListStorageExprVisitor<'codegen, 'ctx, 'src> {
    codegen: &'codegen Codegen<'src>,
    shared_globals: &'ctx HashSet<String>,
    storage: &'ctx mut HashMap<String, ListStorage>,
}

impl<'codegen, 'ctx, 'src> StorageExprCallbacks<ListUseContext>
    for ListStorageExprVisitor<'codegen, 'ctx, 'src>
{
    fn value_ctx(&self) -> ListUseContext {
        ListUseContext::Value
    }

    fn escape_ctx(&self) -> ListUseContext {
        ListUseContext::Escape
    }

    fn call_is_safe(&self, func: &Expr) -> bool {
        self.codegen.call_is_list_safe(func)
    }

    fn visit_expr(&mut self, expr: &Expr, ctx: ListUseContext) {
        if matches!(ctx, ListUseContext::Escape) && matches!(expr.ty.as_ref(), Some(Type::List(_)))
        {
            if let ExprKind::Name(name) = &expr.kind {
                self.codegen.mark_list_shared(name, self.storage);
            }
        }
        match &expr.kind {
            ExprKind::Compare {
                op: CmpOp::Is | CmpOp::IsNot,
                left,
                right,
            } => {
                self.codegen.mark_identity_list_operand(left, self.storage);
                self.codegen.mark_identity_list_operand(right, self.storage);
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
                        self.codegen.mark_identity_list_operand(prev, self.storage);
                        self.codegen.mark_identity_list_operand(cmp, self.storage);
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
            .collect_list_storage_in_stmts(stmts, self.shared_globals, self.storage);
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
