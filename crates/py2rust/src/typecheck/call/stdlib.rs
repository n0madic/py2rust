// Stdlib registry call validation.

use super::super::*;
use crate::callspec::validate_call_shape;
use crate::stdlib::registry::{StdlibMethodId, StdlibMethodSpec};

impl<'a> TypeChecker<'a> {
    pub(super) fn check_stdlib_call(
        &mut self,
        spec: &StdlibMethodSpec,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        span: Span,
    ) -> Result<Type, CompileError> {
        let keyword_names: Vec<Option<&str>> =
            keywords.iter().map(|kw| kw.name.as_deref()).collect();
        if let Err(shape_err) = validate_call_shape(
            &spec.callable_name(),
            spec.shape,
            args.len(),
            &keyword_names,
        ) {
            return Err(self.error(span, shape_err.message()));
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

    /// Return the static type of a stdlib method call.
    fn stdlib_method_return_type(method_id: StdlibMethodId) -> Type {
        match method_id {
            StdlibMethodId::OsRemove => Type::None,
            StdlibMethodId::SysExit => Type::None,
        }
    }
}
