use super::*;

/// Utility functions for code generation.
///
/// These are small helper functions that are used throughout codegen:
/// - Scope queries (is_global, local_var_type)
/// - Name generation (global_name, new_tmp)
/// - Output formatting (push_line)
/// - Error creation
///
/// Also includes mutation analysis for determining when variables need `mut`.
impl<'a> Codegen<'a> {
    /// Check if a variable name refers to a global variable.
    ///
    /// A name is global if:
    /// 1. It's NOT in the current function's local variable map, AND
    /// 2. It IS in the shared globals set for this program
    ///
    /// This is used to determine if we need to emit the mutex wrapper
    /// access pattern for globals.
    pub(crate) fn is_global(&self, name: &str) -> bool {
        if let Some(vars) = self.local_vars.as_ref() {
            if vars.contains_key(name) {
                return false;
            }
        }
        self.shared_globals.contains(name)
    }

    /// Look up a list element type hint for a name in the current scope.
    pub(crate) fn list_elem_type_for_name(&self, name: &str) -> Option<&Type> {
        if self.current_function.is_some() {
            self.inferred_list_elems
                .as_ref()
                .and_then(|map| map.get(name))
        } else {
            self.main_list_elems.get(name)
        }
    }

    /// Look up a dict key/value type hint for a name in the current scope.
    pub(crate) fn dict_kv_type_for_name(&self, name: &str) -> Option<&(Type, Type)> {
        if self.current_function.is_some() {
            self.inferred_dict_kv.as_ref().and_then(|map| map.get(name))
        } else {
            self.main_dict_kv.get(name)
        }
    }

    /// Determine the storage strategy for a list variable by name.
    pub(crate) fn list_storage_for_name(&self, name: &str) -> ListStorage {
        if self.is_global(name) {
            return ListStorage::SharedSync;
        }
        if self.current_function.is_some() {
            if let Some(map) = self.local_list_storage.as_ref() {
                if let Some(storage) = map.get(name) {
                    return *storage;
                }
            }
            return ListStorage::SharedCell;
        }
        if let Some(storage) = self.main_list_storage.get(name) {
            return *storage;
        }
        ListStorage::SharedSync
    }

    /// Check if a list variable is stored locally as Vec<T>.
    pub(crate) fn is_local_list_name(&self, name: &str) -> bool {
        matches!(self.list_storage_for_name(name), ListStorage::Local)
    }

    /// Resolve storage strategy for a list expression (defaults to Shared).
    pub(crate) fn list_storage_for_expr(&self, expr: &Expr) -> ListStorage {
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            if let ExprKind::Name(name) = &expr.kind {
                return self.list_storage_for_name(name);
            }
        }
        // Fresh-return flag forces Local for anonymous list expressions
        // (literals, comprehensions, constructors) but not named variables.
        if self.force_local_list_storage {
            return ListStorage::Local;
        }
        ListStorage::SharedCell
    }

    /// Wrap a list expression with the configured storage strategy.
    pub(crate) fn wrap_list_storage_expr(&self, expr: &str, storage: ListStorage) -> String {
        match storage {
            ListStorage::Local => expr.to_string(),
            ListStorage::SharedCell => format!("Arc::new(Mutex::new({}))", expr),
            ListStorage::SharedSync => format!("Arc::new(Mutex::new({}))", expr),
        }
    }

    /// Record list storage for a generated temporary name.
    pub(crate) fn set_list_storage_for_temp(&mut self, name: &str, storage: ListStorage) {
        if self.current_function.is_some() {
            if let Some(map) = self.local_list_storage.as_mut() {
                map.insert(name.to_string(), storage);
            }
        } else {
            self.main_list_storage.insert(name.to_string(), storage);
        }
    }

    /// Determine the storage strategy for a dict variable by name.
    pub(crate) fn dict_storage_for_name(&self, name: &str) -> DictStorage {
        if self.is_global(name) {
            return DictStorage::SharedSync;
        }
        if self.current_function.is_some() {
            if let Some(map) = self.local_dict_storage.as_ref() {
                if let Some(storage) = map.get(name) {
                    return *storage;
                }
            }
            return DictStorage::SharedCell;
        }
        if let Some(storage) = self.main_dict_storage.get(name) {
            return *storage;
        }
        DictStorage::SharedSync
    }

    /// Resolve storage strategy for a dict expression (defaults to Shared).
    pub(crate) fn dict_storage_for_expr(&self, expr: &Expr) -> DictStorage {
        if matches!(expr.ty.as_ref(), Some(Type::Dict(_, _))) {
            if let ExprKind::Name(name) = &expr.kind {
                return self.dict_storage_for_name(name);
            }
        }
        DictStorage::SharedCell
    }

    /// Wrap a dict expression with the configured storage strategy.
    pub(crate) fn wrap_dict_storage_expr(&self, expr: &str, storage: DictStorage) -> String {
        match storage {
            DictStorage::Local => expr.to_string(),
            DictStorage::SharedCell => format!("Arc::new(Mutex::new({}))", expr),
            DictStorage::SharedSync => format!("Arc::new(Mutex::new({}))", expr),
        }
    }

    /// Record dict storage for a generated temporary or local name.
    pub(crate) fn set_dict_storage_for_temp(&mut self, name: &str, storage: DictStorage) {
        if self.current_function.is_some() {
            if let Some(map) = self.local_dict_storage.as_mut() {
                map.insert(name.to_string(), storage);
            }
        } else {
            self.main_dict_storage.insert(name.to_string(), storage);
        }
    }

    pub(crate) fn global_name(&self, name: &str) -> String {
        format!("__GLOBAL_{}", name.to_uppercase())
    }

    pub(crate) fn global_lock_expr(&self, name: &str) -> String {
        if self.readonly_globals.contains(name) {
            // Write-once scalar globals skip the Mutex — direct OnceLock access.
            return format!(
                "{}.get().expect(\"global not initialized\")",
                self.global_name(name)
            );
        }
        // Use expect to surface clear panic messages if globals are misused.
        format!(
            "{}.get().expect(\"global not initialized\").lock().expect(\"global mutex poisoned\")",
            self.global_name(name)
        )
    }

    /// Look up a class attribute global name if it exists.
    pub(crate) fn class_attr_global(&self, class_name: &str, attr: &str) -> Option<&str> {
        self.ctx
            .classes
            .get(class_name)
            .and_then(|info| info.class_attrs.get(attr))
            .map(|info| info.global_name.as_str())
    }

    /// Look up a class property getter/setter info.
    pub(crate) fn class_property(&self, class_name: &str, attr: &str) -> Option<&PropertyInfo> {
        self.ctx
            .classes
            .get(class_name)
            .and_then(|info| info.properties.get(attr))
    }

    /// Look up the most recent override expression for a global name.
    pub(crate) fn global_override(&self, name: &str) -> Option<&str> {
        self.global_overrides
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, expr)| expr.as_str())
    }

    /// Temporarily replace a global name with a provided expression while generating code.
    pub(crate) fn with_global_override<T>(
        &mut self,
        name: &str,
        replacement: String,
        f: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        self.global_overrides.push((name.to_string(), replacement));
        let result = f(self);
        self.global_overrides.pop();
        result
    }

    /// Look up the most recent override expression for a local name.
    pub(crate) fn name_override(&self, name: &str) -> Option<&str> {
        self.name_overrides
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, expr)| expr.as_str())
    }

    /// Temporarily replace a local name with a provided expression while generating code.
    pub(crate) fn with_name_override<T>(
        &mut self,
        name: &str,
        replacement: String,
        f: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        self.name_overrides.push((name.to_string(), replacement));
        let result = f(self);
        self.name_overrides.pop();
        result
    }

    /// Check if a name is declared nonlocal in the current scope.
    pub(crate) fn is_nonlocal_decl(&self, name: &str) -> bool {
        self.nonlocal_decls
            .as_ref()
            .is_some_and(|names| names.contains(name))
    }

    /// Return the backing field for a deletable int property when pattern-matched.
    ///
    /// Supported shape:
    /// - getter body is a single `return self.<field>`
    /// - deleter body contains `del self.<field>`
    /// - `<field>` type is `int`
    pub(crate) fn deletable_property_backing_int_field(
        &self,
        class_name: &str,
        prop_name: &str,
    ) -> Option<String> {
        let class_def = self.class_defs.get(class_name)?;
        let mut getter_name: Option<String> = None;
        let mut deleter_name: Option<String> = None;
        for prop in class_def.properties.iter().filter(|p| p.name == prop_name) {
            if !prop.getter.is_empty() {
                getter_name = Some(prop.getter.clone());
            }
            if let Some(deleter) = &prop.deleter {
                deleter_name = Some(deleter.clone());
            }
        }
        let getter_name = getter_name?;
        let deleter_name = deleter_name?;
        let getter = class_def.methods.iter().find(|m| m.name == getter_name)?;
        let field = Self::getter_returns_self_field(getter)?;
        let deleter = class_def.methods.iter().find(|m| m.name == deleter_name)?;
        if !deleter
            .body
            .iter()
            .any(|stmt| Self::stmt_deletes_self_field(stmt, field.as_str()))
        {
            return None;
        }
        let field_ty = self
            .ctx
            .classes
            .get(class_name)
            .and_then(|info| info.fields.get(&field))?;
        if matches!(field_ty, Type::Int) {
            Some(field)
        } else {
            None
        }
    }

    fn getter_returns_self_field(func: &Function) -> Option<String> {
        if func.body.len() != 1 {
            return None;
        }
        let StmtKind::Return { value: Some(expr) } = &func.body[0].kind else {
            return None;
        };
        let ExprKind::Attr { value, attr } = &expr.kind else {
            return None;
        };
        if matches!(&value.kind, ExprKind::Name(name) if name == "self") {
            Some(attr.clone())
        } else {
            None
        }
    }

    fn stmt_deletes_self_field(stmt: &Stmt, field: &str) -> bool {
        match &stmt.kind {
            StmtKind::Delete { target } => {
                if let AssignTarget::Attr { value, attr } = target.as_ref() {
                    matches!(&value.kind, ExprKind::Name(name) if name == "self") && attr == field
                } else {
                    false
                }
            }
            StmtKind::If { body, orelse, .. } => {
                body.iter().any(|s| Self::stmt_deletes_self_field(s, field))
                    || orelse
                        .iter()
                        .any(|s| Self::stmt_deletes_self_field(s, field))
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                body.iter().any(|s| Self::stmt_deletes_self_field(s, field))
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                body.iter().any(|s| Self::stmt_deletes_self_field(s, field))
                    || handlers.iter().any(|h| {
                        h.body
                            .iter()
                            .any(|s| Self::stmt_deletes_self_field(s, field))
                    })
                    || orelse
                        .iter()
                        .any(|s| Self::stmt_deletes_self_field(s, field))
                    || finalbody
                        .iter()
                        .any(|s| Self::stmt_deletes_self_field(s, field))
            }
            StmtKind::Match { cases, .. } => cases.iter().any(|c| {
                c.body
                    .iter()
                    .any(|s| Self::stmt_deletes_self_field(s, field))
            }),
            _ => false,
        }
    }

    /// Check if a local name should be stored as Rc<RefCell<_>>.
    pub(crate) fn is_cell_local(&self, name: &str) -> bool {
        self.cell_locals
            .as_ref()
            .is_some_and(|names| names.contains(name))
    }

    /// Clone shared/owned values to preserve Python assignment semantics.
    ///
    /// When `already_cloned` is true, the expression has already been cloned
    /// (e.g., from an Optional unwrap path) and should not be cloned again.
    pub(crate) fn maybe_clone_list_expr(
        &self,
        expr: String,
        value_expr: &Expr,
        expected_ty: Option<&Type>,
        already_cloned: bool,
    ) -> String {
        if already_cloned {
            return expr;
        }
        let ty = match expected_ty {
            // When expected type is unknown (for example wide inline union annotations),
            // fall back to the expression type to preserve Python copy semantics.
            Some(Type::Unknown) | None => value_expr.ty.as_ref(),
            Some(other) => Some(other),
        };
        // Clone only when reading from an existing binding; temporaries can move safely.
        let needs_binding_clone =
            matches!(value_expr.kind, ExprKind::Name(_) | ExprKind::Attr { .. });
        if needs_binding_clone
            && matches!(
                ty,
                Some(Type::List(_)) | Some(Type::Dict(_, _)) | Some(Type::Str) | Some(Type::Bytes)
            )
        {
            // Detect already-cloned expressions to avoid `.clone().clone()`.
            if expr.ends_with(".clone()") {
                return expr;
            }
            return format!("{}.clone()", expr);
        }
        expr
    }

    /// Compute the global name used for default argument storage.
    pub(crate) fn default_global_name(
        &self,
        class_name: Option<&str>,
        func_name: &str,
        param_name: &str,
    ) -> String {
        if let Some(class_name) = class_name {
            format!("__default_{}_{}_{}", class_name, func_name, param_name)
        } else {
            format!("__default_{}_{}", func_name, param_name)
        }
    }

    /// Return true when a global name stores a lowered default argument value.
    pub(crate) fn is_default_global_name(&self, name: &str) -> bool {
        name.starts_with("__default_")
    }

    /// Compute the thread-local cache name used for mutable default list/dict args.
    pub(crate) fn default_cache_name(&self, name: &str) -> String {
        format!("__DEFAULT_CACHE_{}", name.to_uppercase())
    }

    /// Locate a method definition on a class by name.
    pub(crate) fn method_def(&self, class_name: &str, method_name: &str) -> Option<&Function> {
        // Method signatures in typecheck are inheritance-aware, so codegen lookup
        // must walk base classes too when a method is inherited but not redefined.
        let mut current = Some(class_name);
        while let Some(name) = current {
            if let Some(def) = self.class_defs.get(name) {
                if let Some(method) = def.methods.iter().find(|m| m.name == method_name) {
                    return Some(method);
                }
            }
            current = self
                .ctx
                .classes
                .get(name)
                .and_then(|info| info.base.as_deref());
        }
        None
    }

    /// Check if a class is the same as or a subclass of another class.
    pub(crate) fn is_subclass_of(&self, class_name: &str, target: &str) -> bool {
        let mut current = Some(class_name);
        while let Some(name) = current {
            if name == target {
                return true;
            }
            current = self
                .ctx
                .classes
                .get(name)
                .and_then(|info| info.base.as_deref());
        }
        false
    }

    pub(crate) fn new_tmp(&mut self) -> String {
        let name = format!("_tmp{}", self.tmp_counter);
        self.tmp_counter += 1;
        name
    }

    pub(crate) fn push_line(&mut self, line: &str) {
        if !line.is_empty() {
            self.out.push_str(&"    ".repeat(self.indent));
            self.out.push_str(line);
        }
        self.out.push('\n');
    }

    pub(crate) fn error(&self, span: Span, msg: impl Into<String>) -> CompileError {
        CompileError::new(msg, span, self.source, self.filename)
    }
}

/// Count how many times each variable is assigned/mutated in a statement block.
///
/// Why this matters:
/// Rust requires variables to be declared `mut` if they'll be reassigned.
/// Python doesn't have this distinction - all variables are implicitly mutable.
///
/// We analyze the code to determine which variables are assigned more than once
/// (or mutated by operations like `next(iter)`), and emit `let mut` for those.
///
/// This function returns a map of variable name -> assignment count.
/// If count > 1 (or there's a mutation), we emit `mut`.
///
/// Special cases tracked:
/// - Regular assignments: `x = value`
/// - next() calls: `next(iterator)` mutates the iterator
/// - Index assignments: `list[i] = value` mutates the list
/// - Method calls that mutate: built-in collection/file methods plus any user-defined
///   method that the caller determines takes `&mut self` (via `user_method_is_mutating`).
fn collect_assign_counts_impl<'stmt, I, F>(
    stmts: I,
    user_method_is_mutating: F,
) -> HashMap<String, usize>
where
    I: IntoIterator<Item = &'stmt Stmt>,
    F: Fn(/*class_name:*/ &str, /*method_name:*/ &str) -> bool,
{
    let mut counts = HashMap::new();
    // Inner helper: visit an expression and record all mutation-inducing sub-expressions.
    // `umf` is the type-informed predicate for user-defined methods.
    fn visit_expr(
        expr: &Expr,
        counts: &mut HashMap<String, usize>,
        umf: &dyn Fn(&str, &str) -> bool,
    ) {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Attr { value, attr } = &func.kind {
                    // Mutating built-in collection/file methods require the receiver to be `mut`.
                    let builtin_mutating = matches!(
                        attr.as_str(),
                        "append"
                            | "extend"
                            | "pop"
                            | "insert"
                            | "clear"
                            | "reverse"
                            | "sort"
                            | "add"
                            | "remove"
                            | "setdefault"
                            // File methods advance cursor or flush state and need mutable bindings.
                            | "read"
                            | "readline"
                            | "readlines"
                            | "write"
                            | "close"
                            | "__enter__"
                            | "__exit__"
                    );
                    // For user-defined methods on typed receivers, consult the HIR-derived predicate.
                    let is_user_mutating = !builtin_mutating
                        && matches!(value.ty.as_ref(), Some(Type::Custom(_)))
                        && {
                            if let (Some(Type::Custom(cn)), ExprKind::Name(_)) =
                                (value.ty.as_ref(), &value.kind)
                            {
                                umf(cn.as_str(), attr.as_str())
                            } else {
                                false
                            }
                        };
                    if builtin_mutating || is_user_mutating {
                        if let ExprKind::Name(name) = &value.kind {
                            *counts.entry(name.clone()).or_insert(0) += 1;
                        }
                    }
                }
                if let ExprKind::Name(name) = &func.kind {
                    if name == "next" {
                        if let Some(ExprKind::Name(arg_name)) = args.first().map(|arg| &arg.kind) {
                            *counts.entry(arg_name.clone()).or_insert(0) += 1;
                        }
                    }
                }
                visit_expr(func, counts, umf);
                for arg in args {
                    visit_expr(arg, counts, umf);
                }
                for kw in keywords {
                    visit_expr(&kw.value, counts, umf);
                }
            }
            ExprKind::Starred { value } => visit_expr(value, counts, umf),
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    visit_expr(value, counts, umf);
                }
            }
            ExprKind::Attr { value, .. } => visit_expr(value, counts, umf),
            ExprKind::Binary { left, right, .. } => {
                visit_expr(left, counts, umf);
                visit_expr(right, counts, umf);
            }
            ExprKind::Unary { expr, .. } => visit_expr(expr, counts, umf),
            ExprKind::Compare { left, right, .. } => {
                visit_expr(left, counts, umf);
                visit_expr(right, counts, umf);
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                visit_expr(left, counts, umf);
                for cmp in comparators {
                    visit_expr(cmp, counts, umf);
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for v in values {
                    visit_expr(v, counts, umf);
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    visit_expr(item, counts, umf);
                }
            }
            ExprKind::Dict(items) => {
                for entry in items {
                    match entry {
                        DictEntry::Item { key, value } => {
                            visit_expr(key, counts, umf);
                            visit_expr(value, counts, umf);
                        }
                        DictEntry::Unpack { value } => {
                            visit_expr(value, counts, umf);
                        }
                    }
                }
            }
            ExprKind::Index { value, index } => {
                visit_expr(value, counts, umf);
                visit_expr(index, counts, umf);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                visit_expr(value, counts, umf);
                if let Some(s) = start {
                    visit_expr(s, counts, umf);
                }
                if let Some(e) = end {
                    visit_expr(e, counts, umf);
                }
                if let Some(st) = step.as_deref() {
                    visit_expr(st, counts, umf);
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                visit_expr(elt, counts, umf);
                visit_expr(iter, counts, umf);
                for cond in ifs {
                    visit_expr(cond, counts, umf);
                }
            }
            ExprKind::UnionCtor { inner, .. } => visit_expr(inner, counts, umf),
            ExprKind::Lambda { body, .. } => visit_expr(body, counts, umf),
            ExprKind::IfExpr { test, body, orelse } => {
                visit_expr(test, counts, umf);
                visit_expr(body, counts, umf);
                visit_expr(orelse, counts, umf);
            }
            ExprKind::Block { stmts } => {
                for stmt in stmts {
                    visit_stmt(stmt, counts, umf);
                }
            }
            ExprKind::Name(_) | ExprKind::Literal(_) => {}
        }
    }

    fn visit_stmt(
        stmt: &Stmt,
        counts: &mut HashMap<String, usize>,
        umf: &dyn Fn(&str, &str) -> bool,
    ) {
        fn record_target(target: &AssignTarget, counts: &mut HashMap<String, usize>) {
            match target {
                AssignTarget::Name(name) => {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
                AssignTarget::Attr { value, .. } => {
                    if let ExprKind::Name(name) = &value.kind {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                AssignTarget::Index { value, .. } => {
                    if let ExprKind::Name(name) = &value.kind {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    // Unpacking assigns to each leaf target.
                    for item in items {
                        record_target(item, counts);
                    }
                }
                AssignTarget::Starred(inner) => {
                    // Starred unpacking also binds/mutates the wrapped target.
                    record_target(inner, counts);
                }
            }
        }

        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                *counts.entry(name.clone()).or_insert(0) += 1;
                visit_expr(value, counts, umf);
            }
            StmtKind::Assign { target, value } => {
                record_target(target, counts);
                visit_expr(value, counts, umf);
            }
            StmtKind::Delete { target } => {
                // `del x[i]`, `del d[k]`, and `del obj.prop` mutate the receiver.
                record_target(target, counts);
            }
            StmtKind::If { test, body, orelse } => {
                visit_expr(test, counts, umf);
                for stmt in body {
                    visit_stmt(stmt, counts, umf);
                }
                for stmt in orelse {
                    visit_stmt(stmt, counts, umf);
                }
            }
            StmtKind::While { test, body } => {
                visit_expr(test, counts, umf);
                for stmt in body {
                    visit_stmt(stmt, counts, umf);
                }
            }
            StmtKind::For { iter, body, .. } => {
                // Iterator bindings consumed via `.by_ref()` need mutable locals.
                if let ExprKind::Name(name) = &iter.kind {
                    if matches!(iter.ty.as_ref(), Some(Type::Iterator(_))) {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                visit_expr(iter, counts, umf);
                for stmt in body {
                    visit_stmt(stmt, counts, umf);
                }
            }
            StmtKind::Match { subject, cases } => {
                visit_expr(subject, counts, umf);
                for case in cases {
                    for stmt in &case.body {
                        visit_stmt(stmt, counts, umf);
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
                    visit_stmt(stmt, counts, umf);
                }
                for handler in handlers {
                    for stmt in &handler.body {
                        visit_stmt(stmt, counts, umf);
                    }
                }
                for stmt in orelse {
                    visit_stmt(stmt, counts, umf);
                }
                for stmt in finalbody {
                    visit_stmt(stmt, counts, umf);
                }
            }
            StmtKind::Expr(expr) => {
                visit_expr(expr, counts, umf);
            }
            StmtKind::Assert { test, msg } => {
                visit_expr(test, counts, umf);
                if let Some(msg) = msg {
                    visit_expr(msg, counts, umf);
                }
            }
            StmtKind::Return { value: Some(expr) } => {
                visit_expr(expr, counts, umf);
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(expr) = exc {
                    visit_expr(expr, counts, umf);
                }
                if let Some(expr) = cause {
                    visit_expr(expr, counts, umf);
                }
            }
            StmtKind::Nonlocal { .. } => {}
            _ => {}
        }
    }
    for stmt in stmts {
        visit_stmt(stmt, &mut counts, &user_method_is_mutating);
    }
    counts
}

pub(crate) fn collect_assign_counts(
    stmts: &[Stmt],
    user_method_is_mutating: impl Fn(&str, &str) -> bool,
) -> HashMap<String, usize> {
    collect_assign_counts_impl(stmts.iter(), user_method_is_mutating)
}

/// Count assignments for a top-level statement list held by reference.
///
/// This avoids cloning top-level `Stmt` values when only immutable traversal is needed.
pub(crate) fn collect_assign_counts_for_stmt_refs(
    stmts: &[&Stmt],
    user_method_is_mutating: impl Fn(&str, &str) -> bool,
) -> HashMap<String, usize> {
    collect_assign_counts_impl(stmts.iter().copied(), user_method_is_mutating)
}

/// Check if a variable needs `mut` based on its assignment count.
///
/// Returns `"mut "` if the variable is assigned more than once (needs mutability),
/// or `""` if it's assigned at most once (no mutability needed).
pub(crate) fn mut_kw_for_name(name: &str, mut_counts: &HashMap<String, usize>) -> &'static str {
    if name == "_" {
        return "";
    }
    if mut_counts.get(name).copied().unwrap_or(0) > 1 {
        "mut "
    } else {
        ""
    }
}

/// Check whether a function parameter needs `mut`.
///
/// Parameters have an implicit initial binding in Rust signatures, so any
/// assignment in the function body requires `mut` (count >= 1).
pub(crate) fn mut_kw_for_param(name: &str, mut_counts: &HashMap<String, usize>) -> &'static str {
    if name == "_" {
        return "";
    }
    if mut_counts.get(name).copied().unwrap_or(0) >= 1 {
        "mut "
    } else {
        ""
    }
}
