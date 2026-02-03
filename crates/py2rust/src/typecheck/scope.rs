use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn maybe_update_from_expr(&mut self, expr: &Expr, ty: &Type) {
        if let ExprKind::Name(name) = &expr.kind {
            self.set_var_type(name, ty.clone());
        }
    }

    pub(super) fn in_function(&self) -> bool {
        !self.global_scopes.is_empty()
    }

    pub(super) fn is_declared_global(&self, name: &str) -> bool {
        self.global_scopes
            .last()
            .map(|scope| scope.declared.contains(name))
            .unwrap_or(false)
    }

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
    pub(super) fn insert_var(
        &mut self,
        name: &str,
        ty: Type,
        span: Span,
    ) -> Result<(), CompileError> {
        if self.in_function() && self.ctx.globals.contains_key(name) {
            return Err(self.error(
                span,
                format!("Local binding shadows global variable `{name}`"),
            ));
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
            Ok(())
        } else {
            Err(self.error(span, "No scope available"))
        }
    }

    pub(super) fn lookup_var(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        self.ctx.globals.get(name).cloned()
    }
}
