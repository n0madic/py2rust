// Stdlib registry call lowering.

use super::super::*;
use crate::callspec::validate_call_shape;
use crate::stdlib::registry::StdlibMethodSpec;

impl<'a> Codegen<'a> {
    /// Emit a stdlib call resolved by registry metadata.
    pub(super) fn gen_stdlib_call(
        &mut self,
        span: Span,
        spec: &StdlibMethodSpec,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        let kw_names = crate::callspec::keyword_names(keywords);
        if let Err(shape_err) = validate_call_shape(
            &spec.callable_name(),
            spec.shape,
            args.len(),
            &kw_names,
        ) {
            return Err(self.error(span, shape_err.message()));
        }
        (spec.codegen_handler)(self, args, keywords)
    }
}
