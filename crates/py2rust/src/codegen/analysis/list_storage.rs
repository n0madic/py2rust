// List storage-strategy analysis for code generation.

use super::super::util::collect_assign_counts;
use super::super::{
    mark_local_if_absent, mark_shared_by_scope, mark_shared_cell, mark_shared_sync, promote_alias,
    *,
};
use super::walk::{
    walk_storage_expr_tree, walk_storage_stmt_events, StorageExprCallbacks, StorageStmtEvent,
};

impl<'a> Codegen<'a> {
    /// Collect list storage strategies for a block of statements.
    ///
    /// This analysis is conservative: if a list can escape or be aliased, it
    /// is marked shared (cell or sync). Only non-escaping lists initialized
    /// from fresh literals/comprehensions are marked Local.
    pub(in crate::codegen) fn collect_list_storage_for_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
    ) -> HashMap<String, ListStorage> {
        let mut storage = HashMap::new();
        self.collect_list_storage_in_stmts(stmts, shared_globals, &mut storage);
        storage
    }

    /// Collect list storage strategies from statement references without cloning.
    pub(in crate::codegen) fn collect_list_storage_for_stmt_refs(
        &self,
        stmts: &[&Stmt],
        shared_globals: &HashSet<String>,
    ) -> HashMap<String, ListStorage> {
        let mut storage = HashMap::new();
        for stmt in stmts {
            self.collect_list_storage_in_stmts(
                std::slice::from_ref(*stmt),
                shared_globals,
                &mut storage,
            );
        }
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
                            promote_alias(name, src, shared_globals, storage);
                        }
                    }
                }
                StorageStmtEvent::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_list_storage_assignment(name, value, shared_globals, storage);
                        if let ExprKind::Name(src) = &value.kind {
                            if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                                promote_alias(name, src, shared_globals, storage);
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
            mark_shared_sync(name, storage);
            return;
        }
        if !matches!(value.ty.as_ref(), Some(Type::List(_))) {
            return;
        }
        if self.is_fresh_list_expr(value) {
            mark_local_if_absent(name, storage);
        } else {
            mark_shared_cell(name, storage);
        }
    }

    /// Determine if an expression creates a fresh list value.
    fn is_fresh_list_expr(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::List(_) | ExprKind::ListComp { .. } => true,
            ExprKind::Binary {
                op: BinOp::Add,
                left,
                right,
            } => {
                matches!(left.ty.as_ref(), Some(Type::List(_)))
                    && matches!(right.ty.as_ref(), Some(Type::List(_)))
            }
            // List repetition: [x] * n or n * [x] creates a fresh list.
            ExprKind::Binary {
                op: BinOp::Mul,
                left,
                right,
            } => {
                (matches!(left.ty.as_ref(), Some(Type::List(_)))
                    && matches!(right.ty.as_ref(), Some(Type::Int)))
                    || (matches!(right.ty.as_ref(), Some(Type::List(_)))
                        && matches!(left.ty.as_ref(), Some(Type::Int)))
            }
            ExprKind::Slice { value, .. } => {
                matches!(expr.ty.as_ref(), Some(Type::List(_)))
                    && matches!(value.ty.as_ref(), Some(Type::List(_)))
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                // list()/tuple() constructors produce fresh lists.
                if keywords.is_empty()
                    && args.len() <= 1
                    && matches!(expr.ty.as_ref(), Some(Type::List(_)))
                    && matches!(&func.kind, ExprKind::Name(name) if name == "list" || name == "tuple")
                {
                    return true;
                }
                // Calls to functions known to return fresh lists.
                if let ExprKind::Name(name) = &func.kind {
                    if self.fresh_return_functions.contains(name) {
                        return true;
                    }
                }
                false
            }
            // TODO: Extend is_fresh_list_expr to recognize sorted(), reversed(), .copy()
            // as fresh list expressions. This requires aligning the codegen for these
            // builtins to respect the storage strategy (Local vs SharedCell) so the
            // generated wrapper type matches the expected variable type.
            _ => false,
        }
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
    fn mark_identity_list_operand(
        &self,
        expr: &Expr,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            if let ExprKind::Name(name) = &expr.kind {
                mark_shared_by_scope(name, shared_globals, storage);
            }
        }
    }

    /// Detect functions whose return type is `List` and all return statements
    /// produce a fresh list value (literal, comprehension, list concat, etc.).
    ///
    /// Uses a fixpoint loop so that transitive fresh-return calls are detected:
    /// if `softmax()` returns a fresh list and `attention()` returns `softmax(...)`,
    /// then `attention()` is also detected as a fresh-return function.
    pub(in crate::codegen) fn detect_fresh_return_functions(
        &self,
        program: &Program,
    ) -> HashSet<String> {
        let mut result = HashSet::new();
        // Collect candidate functions: those with List return type
        // (possibly wrapped in Result for throwing functions).
        let mut candidates: Vec<&Function> = Vec::new();
        for item in &program.items {
            if let Item::Function(func) = item {
                let ret_ty = self.ctx.functions.get(&func.name).map(|sig| &sig.ret);
                let inner_ret =
                    ret_ty.and_then(|t| t.unwrap_result().map(|(ok, _)| ok).or(Some(t)));
                if matches!(inner_ret, Some(Type::List(_))) {
                    candidates.push(func);
                }
            }
        }
        // Fixpoint loop: keep expanding until no new functions are added.
        // 2 passes suffice for typical call depths.
        for _ in 0..3 {
            let prev_len = result.len();
            for func in &candidates {
                if result.contains(&func.name) {
                    continue;
                }
                let returns = collect_return_exprs(&func.body);
                if returns.is_empty() {
                    continue;
                }
                if returns
                    .iter()
                    .all(|expr| self.is_fresh_list_expr_with_known(expr, &result))
                {
                    result.insert(func.name.clone());
                }
            }
            if result.len() == prev_len {
                break;
            }
        }
        result
    }

    /// Like `is_fresh_list_expr`, but also recognizes calls to already-known
    /// fresh-return functions for transitive detection.
    fn is_fresh_list_expr_with_known(&self, expr: &Expr, known_fresh: &HashSet<String>) -> bool {
        if self.is_fresh_list_expr(expr) {
            return true;
        }
        // Check if this is a call to a known fresh-return function.
        if let ExprKind::Call { func, .. } = &expr.kind {
            if let ExprKind::Name(name) = &func.kind {
                if known_fresh.contains(name) {
                    return true;
                }
            }
        }
        false
    }

    /// Decide whether a call is safe to treat list arguments as non-escaping.
    fn call_is_list_safe(&self, func: &Expr) -> bool {
        if let ExprKind::Name(name) = &func.kind {
            if matches!(
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
            ) {
                return true;
            }
            // User-defined functions where all list params are read-only
            // are safe to pass lists to without escaping.
            if let Some(readonly) = self.readonly_list_params.get(name.as_str()) {
                if let Some(sig) = self.ctx.functions.get(name.as_str()) {
                    let all_list_params_readonly = sig.params.iter().enumerate().all(|(i, ty)| {
                        !matches!(ty, Type::List(_))
                            || sig
                                .param_names
                                .get(i)
                                .is_some_and(|n| readonly.contains(n.as_str()))
                    });
                    if all_list_params_readonly {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Detect functions where list parameters are only read, never mutated.
    ///
    /// Returns a map from function name to the set of read-only list param names.
    /// Read-only list parameters can be emitted as `&[T]` instead of
    /// `Arc<Mutex<Vec<T>>>`, eliminating mutex overhead inside the function body.
    ///
    /// Uses a fixpoint loop so that transitive safe-call chains are detected:
    /// if `softmax(logits)` only reads `logits`, then `attention(x)` calling
    /// `softmax(x)` also has `x` as read-only.
    pub(in crate::codegen) fn detect_readonly_list_params(
        &self,
        program: &Program,
    ) -> HashMap<String, HashSet<String>> {
        let mut result: HashMap<String, HashSet<String>> = HashMap::new();

        // Collect candidate functions: those with at least one List parameter.
        struct Candidate<'a> {
            func: &'a Function,
            list_params: Vec<(usize, String)>, // (param index, param name)
        }
        let mut candidates: Vec<Candidate> = Vec::new();
        for item in &program.items {
            if let Item::Function(func) = item {
                if let Some(sig) = self.ctx.functions.get(&func.name) {
                    let list_params: Vec<(usize, String)> = sig
                        .params
                        .iter()
                        .enumerate()
                        .filter(|(_, ty)| matches!(ty, Type::List(_)))
                        .filter_map(|(i, _)| sig.param_names.get(i).map(|n| (i, n.clone())))
                        .collect();
                    if !list_params.is_empty() {
                        candidates.push(Candidate { func, list_params });
                    }
                }
            }
        }

        // Fixpoint loop: keep expanding until no new params are added.
        for _ in 0..3 {
            let prev_total: usize = result.values().map(|s| s.len()).sum();
            for cand in &candidates {
                let mut_counts = collect_assign_counts(&cand.func.body, |cn, m| {
                    self.user_method_is_mutating(cn, m)
                });
                let mut readonly_set: HashSet<String> = HashSet::new();
                for (_idx, param_name) in &cand.list_params {
                    // Skip if param is directly mutated (assigned, index-assigned,
                    // or mutating method called on it).
                    if mut_counts.get(param_name).copied().unwrap_or(0) > 0 {
                        continue;
                    }
                    // Check that the param is not passed as a mutable argument
                    // to any non-safe function call.
                    if !self.param_escapes_in_unsafe_call(param_name, &cand.func.body, &result) {
                        readonly_set.insert(param_name.clone());
                    }
                }
                if !readonly_set.is_empty() {
                    result.insert(cand.func.name.clone(), readonly_set);
                }
            }
            let new_total: usize = result.values().map(|s| s.len()).sum();
            if new_total == prev_total {
                break;
            }
        }
        result
    }

    /// Check if a list parameter is passed to any non-safe function call in the body.
    ///
    /// Returns `true` if the param escapes (is passed to a function where the
    /// corresponding parameter is NOT known to be read-only).
    fn param_escapes_in_unsafe_call(
        &self,
        param_name: &str,
        stmts: &[Stmt],
        known_readonly: &HashMap<String, HashSet<String>>,
    ) -> bool {
        for stmt in stmts {
            if self.param_escapes_in_stmt(param_name, &stmt.kind, known_readonly) {
                return true;
            }
        }
        false
    }

    /// Check a single statement for unsafe escapes of a list parameter.
    fn param_escapes_in_stmt(
        &self,
        param_name: &str,
        kind: &StmtKind,
        known_readonly: &HashMap<String, HashSet<String>>,
    ) -> bool {
        match kind {
            StmtKind::Expr(expr) => self.param_escapes_in_expr(param_name, expr, known_readonly),
            StmtKind::Let { value, .. } => {
                self.param_escapes_in_expr(param_name, value, known_readonly)
            }
            StmtKind::Assign { value, .. } => {
                self.param_escapes_in_expr(param_name, value, known_readonly)
            }
            StmtKind::Return { value: Some(expr) } => {
                self.param_escapes_in_expr(param_name, expr, known_readonly)
            }
            StmtKind::If { test, body, orelse } => {
                self.param_escapes_in_expr(param_name, test, known_readonly)
                    || self.param_escapes_in_unsafe_call(param_name, body, known_readonly)
                    || self.param_escapes_in_unsafe_call(param_name, orelse, known_readonly)
            }
            StmtKind::While { test, body } => {
                self.param_escapes_in_expr(param_name, test, known_readonly)
                    || self.param_escapes_in_unsafe_call(param_name, body, known_readonly)
            }
            StmtKind::For { iter, body, .. } => {
                self.param_escapes_in_expr(param_name, iter, known_readonly)
                    || self.param_escapes_in_unsafe_call(param_name, body, known_readonly)
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                self.param_escapes_in_unsafe_call(param_name, body, known_readonly)
                    || handlers.iter().any(|h| {
                        self.param_escapes_in_unsafe_call(param_name, &h.body, known_readonly)
                    })
                    || self.param_escapes_in_unsafe_call(param_name, orelse, known_readonly)
                    || self.param_escapes_in_unsafe_call(param_name, finalbody, known_readonly)
            }
            StmtKind::Match { subject, cases } => {
                self.param_escapes_in_expr(param_name, subject, known_readonly)
                    || cases.iter().any(|c| {
                        self.param_escapes_in_unsafe_call(param_name, &c.body, known_readonly)
                    })
            }
            StmtKind::Assert { test, msg } => {
                self.param_escapes_in_expr(param_name, test, known_readonly)
                    || msg
                        .as_ref()
                        .is_some_and(|m| self.param_escapes_in_expr(param_name, m, known_readonly))
            }
            StmtKind::Raise { exc, cause } => {
                exc.as_ref()
                    .is_some_and(|e| self.param_escapes_in_expr(param_name, e, known_readonly))
                    || cause
                        .as_ref()
                        .is_some_and(|c| self.param_escapes_in_expr(param_name, c, known_readonly))
            }
            _ => false,
        }
    }

    /// Check if a list parameter escapes in an expression via non-safe function calls.
    ///
    /// We only care about calls where the param is passed as an argument.
    /// Builtins like `len`, `enumerate`, `zip` are safe.
    /// User functions are safe if the corresponding param position is known read-only.
    fn param_escapes_in_expr(
        &self,
        param_name: &str,
        expr: &Expr,
        known_readonly: &HashMap<String, HashSet<String>>,
    ) -> bool {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                // Check if any argument is our param being passed to a non-safe call.
                let has_param_arg = args
                    .iter()
                    .any(|arg| matches!(&arg.kind, ExprKind::Name(n) if n == param_name));
                if has_param_arg {
                    // Check if the function is safe for list args.
                    if let ExprKind::Name(fname) = &func.kind {
                        let is_builtin_safe = matches!(
                            fname.as_str(),
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
                                | "sorted"
                                | "range"
                        );
                        if !is_builtin_safe {
                            // Check if it's a user function with the param position
                            // known to be read-only.
                            if let Some(readonly) = known_readonly.get(fname.as_str()) {
                                if let Some(sig) = self.ctx.functions.get(fname.as_str()) {
                                    // Check each arg position where our param appears.
                                    let safe = args.iter().enumerate().all(|(i, arg)| {
                                        if matches!(&arg.kind, ExprKind::Name(n) if n == param_name)
                                        {
                                            sig.param_names
                                                .get(i)
                                                .is_some_and(|n| readonly.contains(n.as_str()))
                                        } else {
                                            true
                                        }
                                    });
                                    if !safe {
                                        return true;
                                    }
                                } else {
                                    return true;
                                }
                            } else {
                                return true;
                            }
                        }
                    } else {
                        // Non-Name callable (attr call, etc.) — conservative: escape.
                        return true;
                    }
                }
                // Also check keyword values and nested expressions.
                self.param_escapes_in_expr(param_name, func, known_readonly)
                    || args.iter().any(|a| {
                        // Only recurse into non-Name args (Name args handled above).
                        if matches!(&a.kind, ExprKind::Name(_)) {
                            false
                        } else {
                            self.param_escapes_in_expr(param_name, a, known_readonly)
                        }
                    })
                    || keywords
                        .iter()
                        .any(|kw| self.param_escapes_in_expr(param_name, &kw.value, known_readonly))
            }
            // Recurse into sub-expressions.
            ExprKind::Binary { left, right, .. } => {
                self.param_escapes_in_expr(param_name, left, known_readonly)
                    || self.param_escapes_in_expr(param_name, right, known_readonly)
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.param_escapes_in_expr(param_name, inner, known_readonly)
            }
            ExprKind::Compare { left, right, .. } => {
                self.param_escapes_in_expr(param_name, left, known_readonly)
                    || self.param_escapes_in_expr(param_name, right, known_readonly)
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.param_escapes_in_expr(param_name, left, known_readonly)
                    || comparators
                        .iter()
                        .any(|c| self.param_escapes_in_expr(param_name, c, known_readonly))
            }
            ExprKind::BoolOp { values, .. } => values
                .iter()
                .any(|v| self.param_escapes_in_expr(param_name, v, known_readonly)),
            ExprKind::Index { value, index } => {
                self.param_escapes_in_expr(param_name, value, known_readonly)
                    || self.param_escapes_in_expr(param_name, index, known_readonly)
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.param_escapes_in_expr(param_name, value, known_readonly)
                    || start
                        .as_ref()
                        .is_some_and(|s| self.param_escapes_in_expr(param_name, s, known_readonly))
                    || end
                        .as_ref()
                        .is_some_and(|e| self.param_escapes_in_expr(param_name, e, known_readonly))
                    || step.as_deref().is_some_and(|st| {
                        self.param_escapes_in_expr(param_name, st, known_readonly)
                    })
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.param_escapes_in_expr(param_name, test, known_readonly)
                    || self.param_escapes_in_expr(param_name, body, known_readonly)
                    || self.param_escapes_in_expr(param_name, orelse, known_readonly)
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.param_escapes_in_expr(param_name, elt, known_readonly)
                    || self.param_escapes_in_expr(param_name, iter, known_readonly)
                    || ifs
                        .iter()
                        .any(|c| self.param_escapes_in_expr(param_name, c, known_readonly))
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => items
                .iter()
                .any(|item| self.param_escapes_in_expr(param_name, item, known_readonly)),
            ExprKind::Dict(entries) => entries.iter().any(|e| match e {
                DictEntry::Item { key, value } => {
                    self.param_escapes_in_expr(param_name, key, known_readonly)
                        || self.param_escapes_in_expr(param_name, value, known_readonly)
                }
                DictEntry::Unpack { value } => {
                    self.param_escapes_in_expr(param_name, value, known_readonly)
                }
            }),
            ExprKind::Attr { value, .. } => {
                self.param_escapes_in_expr(param_name, value, known_readonly)
            }
            ExprKind::Block { stmts } => {
                self.param_escapes_in_unsafe_call(param_name, stmts, known_readonly)
            }
            ExprKind::Lambda { body, .. } => {
                self.param_escapes_in_expr(param_name, body, known_readonly)
            }
            ExprKind::Starred { value } => {
                self.param_escapes_in_expr(param_name, value, known_readonly)
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.param_escapes_in_expr(param_name, inner, known_readonly)
            }
            // Name, Literal, Yield — no escape.
            _ => false,
        }
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
                mark_shared_by_scope(name, self.shared_globals, self.storage);
            }
        }
        match &expr.kind {
            ExprKind::Compare {
                op: CmpOp::Is | CmpOp::IsNot,
                left,
                right,
            } => {
                self.codegen
                    .mark_identity_list_operand(left, self.shared_globals, self.storage);
                self.codegen
                    .mark_identity_list_operand(right, self.shared_globals, self.storage);
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
                        self.codegen.mark_identity_list_operand(
                            prev,
                            self.shared_globals,
                            self.storage,
                        );
                        self.codegen.mark_identity_list_operand(
                            cmp,
                            self.shared_globals,
                            self.storage,
                        );
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

/// Recursively walk statement trees and collect all return value expressions.
fn collect_return_exprs(stmts: &[Stmt]) -> Vec<&Expr> {
    let mut result = Vec::new();
    for stmt in stmts {
        collect_return_exprs_inner(&stmt.kind, &mut result);
    }
    result
}

/// Walk a single statement kind to collect return expressions.
fn collect_return_exprs_inner<'a>(kind: &'a StmtKind, result: &mut Vec<&'a Expr>) {
    match kind {
        StmtKind::Return { value: Some(expr) } => {
            result.push(expr);
        }
        StmtKind::Return { value: None } => {}
        StmtKind::If { body, orelse, .. } => {
            for s in body {
                collect_return_exprs_inner(&s.kind, result);
            }
            for s in orelse {
                collect_return_exprs_inner(&s.kind, result);
            }
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
            for s in body {
                collect_return_exprs_inner(&s.kind, result);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            for s in body {
                collect_return_exprs_inner(&s.kind, result);
            }
            for handler in handlers {
                for s in &handler.body {
                    collect_return_exprs_inner(&s.kind, result);
                }
            }
            for s in orelse {
                collect_return_exprs_inner(&s.kind, result);
            }
            for s in finalbody {
                collect_return_exprs_inner(&s.kind, result);
            }
        }
        StmtKind::Match { cases, .. } => {
            for case in cases {
                for s in &case.body {
                    collect_return_exprs_inner(&s.kind, result);
                }
            }
        }
        _ => {}
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
