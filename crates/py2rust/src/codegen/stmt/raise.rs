// Raise statement emission.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit a raise statement, with support for bare re-raise.
    pub(super) fn emit_raise_stmt(
        &mut self,
        exc: Option<&Expr>,
        cause: Option<&Expr>,
        span: Span,
    ) -> Result<(), CompileError> {
        // Check for unsupported exception chaining.
        if cause.is_some() {
            return Err(self.error(
                span,
                "Exception chaining (raise ... from ...) is not supported",
            ));
        }

        if let Some(exc_expr) = exc {
            // Check if it's exception constructor call.
            if let ExprKind::Call {
                func,
                args,
                keywords,
            } = &exc_expr.kind
            {
                if let ExprKind::Name(exc_name) = &func.kind {
                    let msg = if !keywords.is_empty() {
                        return Err(self.error(
                            span,
                            "Keyword arguments are not supported in raise constructors",
                        ));
                    } else if !args.is_empty() {
                        self.gen_expr(&args[0])?
                    } else {
                        "String::new()".to_string()
                    };

                    self.push_line(&format!("return Err(PyError::{}({}));", exc_name, msg));
                    return Ok(());
                }
            }

            let exc_code = self.gen_expr(exc_expr)?;
            self.push_line(&format!("return Err({});", exc_code));
        } else {
            // Re-raise - use captured exception.
            self.push_line("return Err(_current_exception);");
        }

        Ok(())
    }
}
