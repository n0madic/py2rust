use super::*;

/// Scope management for type checking.
///
/// Python has complex scoping rules, especially around global variables:
/// 1. Variables can be read from global scope without declaration
/// 2. Assignment creates a local variable (shadows global)
/// 3. `global x` declaration makes `x` refer to global scope
/// 4. `global x` must appear before any use of `x` in that function
///
/// We track:
/// - Local scopes (function/block variables)
/// - Global variables and which functions have declared them global
/// - Whether a global was used before being declared (error)
///
/// Why this complexity?
/// - Python allows reading globals without declaration
/// - But assignment without `global` creates a local
/// - This affects code generation (need to use global variable access)

impl<'a> TypeChecker<'a> {
    /// Update a variable's type based on an expression.
    ///
    /// Used for type narrowing and inference. If we assign to a variable,
    /// we can improve our knowledge of its type.
    pub(super) fn maybe_update_from_expr(&mut self, expr: &Expr, ty: &Type) {
        if let ExprKind::Name(name) = &expr.kind {
            self.set_var_type(name, ty.clone());
        }
    }

    /// Check if we're currently inside a function.
    ///
    /// Used to determine if global declarations are allowed and
    /// how to handle variable assignments.
    pub(super) fn in_function(&self) -> bool {
        !self.global_scopes.is_empty()
    }

    /// Check if a variable has been declared global in the current function.
    ///
    /// Returns false if not in a function or if no global declaration.
    pub(super) fn is_declared_global(&self, name: &str) -> bool {
        self.global_scopes
            .last()
            .map(|scope| scope.declared.contains(name))
            .unwrap_or(false)
    }

    /// Record that a global variable is being used.
    ///
    /// We track this to detect errors where `global x` appears
    /// after `x` has already been referenced.
    ///
    /// Example error case:
    /// def f():
    ///     print(x)  # Use before declaration
    ///     global x  # Error: must come first
    pub(super) fn note_global_use(&mut self, name: &str, span: Span) {
        if !self.in_function() {
            return;
        }
        if !self.ctx.globals.contains_key(name) {
            return;
        }
        if self.is_declared_global(name) {
            return;
        }
        if let Some(scope) = self.global_scopes.last_mut() {
            scope
                .used_before_decl
                .entry(name.to_string())
                .or_insert(span);
        }
    }

    /// Process a `global` declaration.
    ///
    /// Validates:
    /// 1. The global variable actually exists at module scope
    /// 2. It hasn't been used before in this function
    ///
    /// After this, assignments to this name will modify the global.
    pub(super) fn declare_global(&mut self, name: &str, span: Span) -> Result<(), CompileError> {
        if !self.ctx.globals.contains_key(name) {
            return Err(self.error(
                span,
                format!("global name `{name}` is not defined at module scope"),
            ));
        }
        if let Some(scope) = self.global_scopes.last_mut() {
            if scope.used_before_decl.contains_key(name) {
                return Err(self.error(
                    span,
                    format!("global `{name}` must appear before first use"),
                ));
            }
            scope.declared.insert(name.to_string());
        }
        Ok(())
    }

    /// Update a variable's type (for type narrowing/inference).
    ///
    /// Searches from innermost to outermost scope.
    /// If the variable is global (or declared global), updates global context.
    ///
    /// Uses merge_types to combine with existing type knowledge.
    pub(super) fn set_var_type(&mut self, name: &str, ty: Type) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                *existing = Self::merge_types(existing.clone(), ty);
                return;
            }
        }
        if self.ctx.globals.contains_key(name)
            && (!self.in_function() || self.is_declared_global(name))
        {
            let merged = Self::merge_types(
                self.ctx.globals.get(name).cloned().unwrap_or(Type::Unknown),
                ty,
            );
            self.ctx.globals.insert(name.to_string(), merged);
        }
    }

    /// Insert a new variable in the current scope.
    ///
    /// Only creates a new binding, doesn't update existing ones.
    /// Used for variable declarations and function parameters.
    pub(super) fn insert_var(
        &mut self,
        name: &str,
        ty: Type,
        span: Span,
    ) -> Result<(), CompileError> {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
            Ok(())
        } else {
            Err(self.error(span, "No scope available"))
        }
    }

    /// Look up a variable's type.
    ///
    /// Searches local scopes from innermost to outermost,
    /// then checks global context.
    /// Returns None if not found (caller handles error).
    pub(super) fn lookup_var(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        self.ctx.globals.get(name).cloned()
    }

    /// Look up a variable's type in local scopes only (ignores globals).
    pub(super) fn lookup_local_var(&self, name: &str) -> Option<Type> {
        let start = if self.in_function() {
            self.function_scopes.last().copied().unwrap_or(0)
        } else {
            0
        };
        for (idx, scope) in self.scopes.iter().enumerate().rev() {
            if idx < start {
                break;
            }
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }
}
