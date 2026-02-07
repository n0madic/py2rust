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

        // Helper for keyword argument lookup after call-shape validation.
        let find_keyword_idx = |name: &str| {
            keywords
                .iter()
                .position(|kw| kw.name.as_deref() == Some(name))
        };

        // Validate argument types for each registered method.
        match spec.method_id {
            StdlibMethodId::OsRemove
            | StdlibMethodId::OsChdir
            | StdlibMethodId::OsMkdir
            | StdlibMethodId::OsListdir
            | StdlibMethodId::OsRmdir
            | StdlibMethodId::OsPathExists
            | StdlibMethodId::OsPathBasename
            | StdlibMethodId::OsPathDirname
            | StdlibMethodId::OsPathSplit
            | StdlibMethodId::OsPathIsDir
            | StdlibMethodId::OsPathIsFile
            | StdlibMethodId::OsPathAbspath => {
                let path_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&path_ty, &Type::Str, span)?;
            }
            StdlibMethodId::OsGetcwd => {}
            StdlibMethodId::OsRename | StdlibMethodId::OsReplace => {
                let src_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&src_ty, &Type::Str, span)?;
                let dst_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&dst_ty, &Type::Str, span)?;
            }
            StdlibMethodId::OsMakedirs => {
                let path_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&path_ty, &Type::Str, span)?;

                // CPython-style duplicate handling: positional + keyword for the same
                // semantic parameter is rejected.
                let exist_ok_idx = find_keyword_idx("exist_ok");
                if args.len() == 2 && exist_ok_idx.is_some() {
                    return Err(self.error(span, "Multiple values for keyword argument `exist_ok`"));
                }
                if args.len() == 2 {
                    let exist_ok_ty = self.check_expr(&mut args[1], Some(&Type::Bool))?;
                    self.ensure_assignable(&exist_ok_ty, &Type::Bool, span)?;
                }
                if let Some(idx) = exist_ok_idx {
                    let exist_ok_ty =
                        self.check_expr(&mut keywords[idx].value, Some(&Type::Bool))?;
                    self.ensure_assignable(&exist_ok_ty, &Type::Bool, span)?;
                }
            }
            StdlibMethodId::OsGetenv => {
                let key_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&key_ty, &Type::Str, span)?;

                let default_idx = find_keyword_idx("default");
                if args.len() == 2 && default_idx.is_some() {
                    return Err(self.error(span, "Multiple values for keyword argument `default`"));
                }

                // getenv default supports `str` and `None`; unknown remains permissive.
                if args.len() == 2 {
                    let default_ty = self.check_expr(&mut args[1], None)?;
                    if !matches!(default_ty, Type::Str | Type::None | Type::Unknown) {
                        return Err(self.error(span, "os.getenv() default must be str or None"));
                    }
                }
                if let Some(idx) = default_idx {
                    let default_ty = self.check_expr(&mut keywords[idx].value, None)?;
                    if !matches!(default_ty, Type::Str | Type::None | Type::Unknown) {
                        return Err(self.error(span, "os.getenv() default must be str or None"));
                    }
                }
            }
            StdlibMethodId::OsPathJoin => {
                for path in args {
                    let path_ty = self.check_expr(path, Some(&Type::Str))?;
                    self.ensure_assignable(&path_ty, &Type::Str, span)?;
                }
            }
            StdlibMethodId::SysExit => {
                if args.len() == 1 {
                    let code_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                    self.ensure_assignable(&code_ty, &Type::Int, span)?;
                }
            }
            StdlibMethodId::SysIntern => {
                let value_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&value_ty, &Type::Str, span)?;
            }
        }

        Ok(Self::stdlib_method_return_type(spec.method_id))
    }

    /// Return the static type of a stdlib method call.
    fn stdlib_method_return_type(method_id: StdlibMethodId) -> Type {
        match method_id {
            StdlibMethodId::OsRemove
            | StdlibMethodId::OsChdir
            | StdlibMethodId::OsMkdir
            | StdlibMethodId::OsRmdir
            | StdlibMethodId::OsRename
            | StdlibMethodId::OsReplace
            | StdlibMethodId::OsMakedirs => Type::None,
            StdlibMethodId::OsGetcwd
            | StdlibMethodId::OsPathJoin
            | StdlibMethodId::OsPathBasename
            | StdlibMethodId::OsPathDirname
            | StdlibMethodId::OsPathAbspath => Type::Str,
            StdlibMethodId::OsListdir => Type::List(Box::new(Type::Str)),
            StdlibMethodId::OsGetenv => Type::Option(Box::new(Type::Str)),
            StdlibMethodId::OsPathExists
            | StdlibMethodId::OsPathIsDir
            | StdlibMethodId::OsPathIsFile => Type::Bool,
            StdlibMethodId::OsPathSplit => Type::Tuple(vec![Type::Str, Type::Str]),
            StdlibMethodId::SysIntern => Type::Str,
            StdlibMethodId::SysExit => Type::None,
        }
    }
}
