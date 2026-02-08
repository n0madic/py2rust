use super::*;

impl<'a> TypeChecker<'a> {
    /// Type check `try/except/else/finally` statement branches.
    pub(super) fn check_try_stmt(
        &mut self,
        body: &mut Vec<Stmt>,
        handlers: &mut Vec<ExceptHandler>,
        orelse: &mut Vec<Stmt>,
        finalbody: &mut Vec<Stmt>,
        expected_ret: Option<&TypeRef>,
    ) -> Result<(), CompileError> {
        // Check try body.
        for stmt in body {
            self.check_stmt(stmt, expected_ret)?;
        }

        // Check exception handlers.
        for handler in handlers {
            self.check_except_handler(handler, expected_ret)?;
        }

        // Check else and finally clauses.
        for stmt in orelse {
            self.check_stmt(stmt, expected_ret)?;
        }
        for stmt in finalbody {
            self.check_stmt(stmt, expected_ret)?;
        }

        Ok(())
    }

    /// Type check `raise` statements including optional exception chaining.
    pub(super) fn check_raise_stmt(
        &mut self,
        exc: &mut Option<Expr>,
        cause: &mut Option<Expr>,
        span: Span,
    ) -> Result<(), CompileError> {
        if let Some(exc_expr) = exc {
            // Special handling for built-in and custom exception constructors.
            if let ExprKind::Call {
                func,
                args,
                keywords: _,
            } = &mut exc_expr.kind
            {
                if let ExprKind::Name(exc_name) = &func.kind {
                    if self.resolve_exception_variant_name(exc_name).is_some() {
                        // Validate arguments (should be string message).
                        if !args.is_empty() {
                            self.check_expr(&mut args[0], Some(&Type::Str))?;
                        }
                        // Set the exception type.
                        exc_expr.ty = Some(Type::Exception(exc_name.clone()));
                    } else {
                        // Not a built-in, check normally.
                        self.check_expr(exc_expr, None)?;
                    }
                } else {
                    self.check_expr(exc_expr, None)?;
                }
            } else {
                self.check_expr(exc_expr, None)?;
            }

            let exc_ty = exc_expr
                .ty
                .as_ref()
                .ok_or_else(|| self.error(span, "Exception type unknown"))?;
            self.validate_exception_type(exc_ty, span)?;

            if let Some(cause_expr) = cause {
                // Similar handling for cause.
                if let ExprKind::Call {
                    func,
                    args,
                    keywords: _,
                } = &mut cause_expr.kind
                {
                    if let ExprKind::Name(exc_name) = &func.kind {
                        if self.resolve_exception_variant_name(exc_name).is_some() {
                            if !args.is_empty() {
                                self.check_expr(&mut args[0], Some(&Type::Str))?;
                            }
                            cause_expr.ty = Some(Type::Exception(exc_name.clone()));
                        } else {
                            self.check_expr(cause_expr, None)?;
                        }
                    } else {
                        self.check_expr(cause_expr, None)?;
                    }
                } else {
                    self.check_expr(cause_expr, None)?;
                }

                let cause_ty = cause_expr
                    .ty
                    .as_ref()
                    .ok_or_else(|| self.error(span, "Cause type unknown"))?;
                if !matches!(cause_ty, Type::None) {
                    self.validate_exception_type(cause_ty, span)?;
                }
            }
        } else {
            // Re-raise: must be in except handler.
            if self.except_handler_depth == 0 {
                return Err(self.error(span, "Re-raise not allowed outside except handler"));
            }
        }

        Ok(())
    }

    /// Type check one `except` handler and bind its exception name (if present).
    fn check_except_handler(
        &mut self,
        handler: &mut ExceptHandler,
        expected_return: Option<&TypeRef>,
    ) -> Result<(), CompileError> {
        if let Some(exc_types) = &handler.exc_types {
            for exc_type_name in exc_types {
                self.validate_exception_name(exc_type_name, handler.span)?;
            }
        }

        // Bind exception to name if present.
        if let Some(name) = &handler.name {
            // Specific handlers bind the payload message (`String`).
            // Catch-all handlers bind the full `PyError` object for re-raise flows.
            let bound_ty = match &handler.exc_types {
                None => Type::Exception("PyError".to_string()),
                Some(exc_types)
                    if exc_types
                        .iter()
                        .any(|exc_type| exc_type.as_str() == "Exception") =>
                {
                    Type::Exception("PyError".to_string())
                }
                Some(_) => Type::Str,
            };
            self.insert_var(name, bound_ty, handler.span)?;
        }

        self.except_handler_depth += 1;
        for stmt in &mut handler.body {
            self.check_stmt(stmt, expected_return)?;
        }
        self.except_handler_depth -= 1;

        Ok(())
    }

    /// Ensure an expression type can be used as a Python exception object.
    fn validate_exception_type(&self, ty: &Type, span: Span) -> Result<(), CompileError> {
        match ty {
            Type::Exception(_) => Ok(()),
            Type::Custom(name) if self.resolve_exception_variant_name(name).is_some() => Ok(()),
            _ => Err(self.error(span, "Invalid exception type")),
        }
    }

    /// Validate an exception type name in an `except SomeError` clause.
    fn validate_exception_name(&self, name: &str, span: Span) -> Result<(), CompileError> {
        if self.resolve_exception_variant_name(name).is_some() {
            Ok(())
        } else {
            Err(self.error(span, format!("Unknown exception type: {}", name)))
        }
    }
}
