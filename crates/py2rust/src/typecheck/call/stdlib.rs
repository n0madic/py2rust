// Stdlib registry call validation.

use super::super::*;
use crate::stdlib::registry::{StdlibMethodId, StdlibMethodSpec};

impl<'a> TypeChecker<'a> {
    pub(super) fn check_stdlib_call(
        &mut self,
        spec: &StdlibMethodSpec,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        span: Span,
    ) -> Result<Type, CompileError> {
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

        // Validate argument types for each registered method.
        match spec.method_id {
            StdlibMethodId::OsRemove => {
                let path_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&path_ty, &Type::Str, span)?;
            }
            StdlibMethodId::SysExit => {
                if args.len() == 1 {
                    let code_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                    self.ensure_assignable(&code_ty, &Type::Int, span)?;
                }
            }
        }

        Ok(Self::stdlib_method_return_type(spec.method_id))
    }

    /// Render a stable "expects N argument(s)" diagnostic for stdlib calls.
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

    /// Return the static type of a stdlib method call.
    fn stdlib_method_return_type(method_id: StdlibMethodId) -> Type {
        match method_id {
            StdlibMethodId::OsRemove => Type::None,
            StdlibMethodId::SysExit => Type::None,
        }
    }
}
