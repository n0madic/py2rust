// Stdlib registry call lowering.

use super::super::*;
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
        if !spec.allow_keywords && !keywords.is_empty() {
            return Err(self.error(
                span,
                format!(
                    "Keyword arguments are not supported for {}.{}()",
                    spec.module_name, spec.method_name
                ),
            ));
        }
        if args.len() < spec.min_args || args.len() > spec.max_args {
            return Err(self.error(
                span,
                Self::stdlib_arity_message(
                    spec.module_name,
                    spec.method_name,
                    spec.min_args,
                    spec.max_args,
                ),
            ));
        }
        (spec.codegen_handler)(self, args)
    }

    /// Render a stable arity diagnostic for stdlib calls in codegen validation.
    fn stdlib_arity_message(
        module_name: &str,
        method_name: &str,
        min_arity: usize,
        max_arity: usize,
    ) -> String {
        if min_arity == max_arity {
            if min_arity == 1 {
                return format!("{module_name}.{method_name}() expects one argument");
            }
            return format!("{module_name}.{method_name}() expects {min_arity} arguments");
        }
        if min_arity == 0 && max_arity == 1 {
            return format!("{module_name}.{method_name}() expects zero or one argument");
        }
        format!(
            "{module_name}.{method_name}() expects between {min_arity} and {max_arity} arguments"
        )
    }
}
