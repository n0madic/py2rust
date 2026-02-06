// Main call-target dispatch and builtin/method call rules.

mod attr;
mod dynamic;
mod name;

use super::super::*;

impl<'a> TypeChecker<'a> {
    pub(in super::super) fn check_call(
        &mut self,
        func: &mut Expr,
        args: &mut Vec<Expr>,
        keywords: &mut [KeywordArg],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        match &mut func.kind {
            ExprKind::Name(_) => self.check_call_name(func, args, keywords, expected, span),
            ExprKind::Attr { value, attr } => {
                self.check_call_attr(value, attr, args, keywords, span)
            }
            _ => self.check_call_dynamic(func, args, keywords, span),
        }
    }
}
