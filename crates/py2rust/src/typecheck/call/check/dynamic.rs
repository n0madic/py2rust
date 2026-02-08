// Dynamic callable target type checking.

use super::super::super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_call_dynamic(
        &mut self,
        func: &mut Expr,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        span: Span,
    ) -> Result<Type, CompileError> {
        let callable_ty = self.check_expr(func, None)?;
        if let Type::Lambda { params, ret, .. } = callable_ty {
            if !keywords.is_empty() {
                return Err(self.error(
                    span,
                    "Keyword arguments are not supported for this callable",
                ));
            }
            if args.len() != params.len() {
                return Err(self.error(span, "Argument count mismatch"));
            }
            for (arg, expected) in args.iter_mut().zip(params.iter()) {
                let arg_ty = self.check_expr(arg, Some(expected))?;
                if !matches!(expected, Type::Unknown) {
                    self.ensure_assignable(&arg_ty, expected, span)?;
                }
            }
            Ok(*ret)
        } else {
            Err(self.error(span, "Unsupported call target"))
        }
    }
}
