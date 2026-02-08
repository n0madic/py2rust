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

    /// Determine the storage strategy for a list variable by name.
    pub(crate) fn list_storage_for_name(&self, name: &str) -> ListStorage {
        if self.is_global(name) {
            return ListStorage::Shared;
        }
        if self.current_function.is_some() {
            if let Some(map) = self.local_list_storage.as_ref() {
                if let Some(storage) = map.get(name) {
                    return *storage;
                }
            }
            return ListStorage::Shared;
        }
        if let Some(storage) = self.main_list_storage.get(name) {
            return *storage;
        }
        ListStorage::Shared
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
        ListStorage::Shared
    }

    /// Wrap a list expression with the configured storage strategy.
    pub(crate) fn wrap_list_storage_expr(&self, expr: &str, storage: ListStorage) -> String {
        match storage {
            ListStorage::Local => expr.to_string(),
            ListStorage::Shared => format!("Arc::new(Mutex::new({}))", expr),
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
            return DictStorage::Shared;
        }
        if self.current_function.is_some() {
            if let Some(map) = self.local_dict_storage.as_ref() {
                if let Some(storage) = map.get(name) {
                    return *storage;
                }
            }
            return DictStorage::Shared;
        }
        if let Some(storage) = self.main_dict_storage.get(name) {
            return *storage;
        }
        DictStorage::Shared
    }

    /// Resolve storage strategy for a dict expression (defaults to Shared).
    pub(crate) fn dict_storage_for_expr(&self, expr: &Expr) -> DictStorage {
        if matches!(expr.ty.as_ref(), Some(Type::Dict(_, _))) {
            if let ExprKind::Name(name) = &expr.kind {
                return if matches!(self.dict_storage_for_name(name), DictStorage::Local) {
                    DictStorage::Local
                } else {
                    DictStorage::Shared
                };
            }
        }
        DictStorage::Shared
    }

    /// Wrap a dict expression with the configured storage strategy.
    pub(crate) fn wrap_dict_storage_expr(&self, expr: &str, storage: DictStorage) -> String {
        match storage {
            DictStorage::Local => expr.to_string(),
            DictStorage::Shared => format!("Arc::new(Mutex::new({}))", expr),
        }
    }

    // Dict storage for temporaries currently defaults to Shared; add tracking if needed.

    pub(crate) fn global_name(&self, name: &str) -> String {
        format!("__GLOBAL_{}", name.to_uppercase())
    }

    pub(crate) fn global_lock_expr(&self, name: &str) -> String {
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

    /// Check if a local name should be stored as Rc<RefCell<_>>.
    pub(crate) fn is_cell_local(&self, name: &str) -> bool {
        self.cell_locals
            .as_ref()
            .is_some_and(|names| names.contains(name))
    }

    /// Clone shared/owned values to preserve Python assignment semantics.
    pub(crate) fn maybe_clone_list_expr(
        &self,
        expr: String,
        value_expr: &Expr,
        expected_ty: Option<&Type>,
    ) -> String {
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

    /// Locate a method definition on a class by name.
    pub(crate) fn method_def(&self, class_name: &str, method_name: &str) -> Option<&Function> {
        self.class_defs
            .get(class_name)
            .and_then(|def| def.methods.iter().find(|m| m.name == method_name))
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
/// - Method calls that mutate: Some methods (if any) mutate their receiver
pub(crate) fn collect_assign_counts(stmts: &[Stmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    fn visit_expr(expr: &Expr, counts: &mut HashMap<String, usize>) {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Attr { value, attr } = &func.kind {
                    // Mutating collection methods require the receiver to be `mut`.
                    if matches!(
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
                    ) {
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
                visit_expr(func, counts);
                for arg in args {
                    visit_expr(arg, counts);
                }
                for kw in keywords {
                    visit_expr(&kw.value, counts);
                }
            }
            ExprKind::Starred { value } => visit_expr(value, counts),
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    visit_expr(value, counts);
                }
            }
            ExprKind::Attr { value, .. } => visit_expr(value, counts),
            ExprKind::Binary { left, right, .. } => {
                visit_expr(left, counts);
                visit_expr(right, counts);
            }
            ExprKind::Unary { expr, .. } => visit_expr(expr, counts),
            ExprKind::Compare { left, right, .. } => {
                visit_expr(left, counts);
                visit_expr(right, counts);
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                visit_expr(left, counts);
                for cmp in comparators {
                    visit_expr(cmp, counts);
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for v in values {
                    visit_expr(v, counts);
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    visit_expr(item, counts);
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    visit_expr(k, counts);
                    visit_expr(v, counts);
                }
            }
            ExprKind::Index { value, index } => {
                visit_expr(value, counts);
                visit_expr(index, counts);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                visit_expr(value, counts);
                if let Some(s) = start {
                    visit_expr(s, counts);
                }
                if let Some(e) = end {
                    visit_expr(e, counts);
                }
                if let Some(st) = step.as_deref() {
                    visit_expr(st, counts);
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                visit_expr(elt, counts);
                visit_expr(iter, counts);
                for cond in ifs {
                    visit_expr(cond, counts);
                }
            }
            ExprKind::UnionCtor { inner, .. } => visit_expr(inner, counts),
            ExprKind::Lambda { body, .. } => visit_expr(body, counts),
            ExprKind::IfExpr { test, body, orelse } => {
                visit_expr(test, counts);
                visit_expr(body, counts);
                visit_expr(orelse, counts);
            }
            ExprKind::Block { stmts } => {
                for stmt in stmts {
                    visit_stmt(stmt, counts);
                }
            }
            ExprKind::Name(_) | ExprKind::Literal(_) => {}
        }
    }

    fn visit_stmt(stmt: &Stmt, counts: &mut HashMap<String, usize>) {
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
                visit_expr(value, counts);
            }
            StmtKind::Assign { target, value } => {
                record_target(target, counts);
                visit_expr(value, counts);
            }
            StmtKind::Delete { target } => {
                // `del x[i]`, `del d[k]`, and `del obj.prop` mutate the receiver.
                record_target(target, counts);
            }
            StmtKind::If { test, body, orelse } => {
                visit_expr(test, counts);
                for stmt in body {
                    visit_stmt(stmt, counts);
                }
                for stmt in orelse {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::While { test, body } => {
                visit_expr(test, counts);
                for stmt in body {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::For { iter, body, .. } => {
                // Iterator bindings consumed via `.by_ref()` need mutable locals.
                if let ExprKind::Name(name) = &iter.kind {
                    if matches!(iter.ty.as_ref(), Some(Type::Iterator(_))) {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                visit_expr(iter, counts);
                for stmt in body {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::Match { subject, cases } => {
                visit_expr(subject, counts);
                for case in cases {
                    for stmt in &case.body {
                        visit_stmt(stmt, counts);
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
                    visit_stmt(stmt, counts);
                }
                for handler in handlers {
                    for stmt in &handler.body {
                        visit_stmt(stmt, counts);
                    }
                }
                for stmt in orelse {
                    visit_stmt(stmt, counts);
                }
                for stmt in finalbody {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::Expr(expr) => {
                visit_expr(expr, counts);
            }
            StmtKind::Assert { test, msg } => {
                visit_expr(test, counts);
                if let Some(msg) = msg {
                    visit_expr(msg, counts);
                }
            }
            StmtKind::Return { value: Some(expr) } => {
                visit_expr(expr, counts);
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(expr) = exc {
                    visit_expr(expr, counts);
                }
                if let Some(expr) = cause {
                    visit_expr(expr, counts);
                }
            }
            StmtKind::Nonlocal { .. } => {}
            _ => {}
        }
    }
    for stmt in stmts {
        visit_stmt(stmt, &mut counts);
    }
    counts
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
