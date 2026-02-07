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
            StdlibMethodId::ReSearch | StdlibMethodId::ReMatch => {
                let pattern_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&pattern_ty, &Type::Str, span)?;
                let value_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&value_ty, &Type::Str, span)?;
            }
            StdlibMethodId::ReSub => {
                let pattern_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&pattern_ty, &Type::Str, span)?;
                let repl_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&repl_ty, &Type::Str, span)?;
                let value_ty = self.check_expr(&mut args[2], Some(&Type::Str))?;
                self.ensure_assignable(&value_ty, &Type::Str, span)?;
            }
            StdlibMethodId::JsonDumps => {
                // json.dumps accepts dynamic value shapes; preserve permissive typing.
                let _ = self.check_expr(&mut args[0], None)?;
            }
            StdlibMethodId::JsonLoads => {
                let text_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&text_ty, &Type::Str, span)?;
            }
            StdlibMethodId::JsonDump => {
                // json.dump accepts dynamic value shapes as the first argument.
                let _ = self.check_expr(&mut args[0], None)?;
                let file_ty = self.check_expr(
                    &mut args[1],
                    Some(&Type::Custom("__py2rust_file".to_string())),
                )?;
                self.ensure_assignable(
                    &file_ty,
                    &Type::Custom("__py2rust_file".to_string()),
                    span,
                )?;
            }
            StdlibMethodId::JsonLoad => {
                let file_ty = self.check_expr(
                    &mut args[0],
                    Some(&Type::Custom("__py2rust_file".to_string())),
                )?;
                self.ensure_assignable(
                    &file_ty,
                    &Type::Custom("__py2rust_file".to_string()),
                    span,
                )?;
            }
            StdlibMethodId::MathSqrt
            | StdlibMethodId::MathSin
            | StdlibMethodId::MathCos
            | StdlibMethodId::MathTan
            | StdlibMethodId::MathLog2
            | StdlibMethodId::MathLog10
            | StdlibMethodId::MathExp
            | StdlibMethodId::MathAsin
            | StdlibMethodId::MathAcos
            | StdlibMethodId::MathAtan
            | StdlibMethodId::MathSinh
            | StdlibMethodId::MathCosh
            | StdlibMethodId::MathTanh
            | StdlibMethodId::MathFabs
            | StdlibMethodId::MathDegrees
            | StdlibMethodId::MathRadians => {
                let value_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                self.ensure_assignable(&value_ty, &Type::Float, span)?;
            }
            StdlibMethodId::MathLog => {
                let value_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                self.ensure_assignable(&value_ty, &Type::Float, span)?;
                if args.len() == 2 {
                    let base_ty = self.check_expr(&mut args[1], Some(&Type::Float))?;
                    self.ensure_assignable(&base_ty, &Type::Float, span)?;
                }
            }
            StdlibMethodId::MathCeil | StdlibMethodId::MathFloor | StdlibMethodId::MathTrunc => {
                let value_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                self.ensure_assignable(&value_ty, &Type::Float, span)?;
            }
            StdlibMethodId::MathIsNan
            | StdlibMethodId::MathIsInf
            | StdlibMethodId::MathIsFinite => {
                let value_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                self.ensure_assignable(&value_ty, &Type::Float, span)?;
            }
            StdlibMethodId::MathAtan2
            | StdlibMethodId::MathFmod
            | StdlibMethodId::MathCopySign
            | StdlibMethodId::MathHypot
            | StdlibMethodId::MathPow => {
                let left_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                self.ensure_assignable(&left_ty, &Type::Float, span)?;
                let right_ty = self.check_expr(&mut args[1], Some(&Type::Float))?;
                self.ensure_assignable(&right_ty, &Type::Float, span)?;
            }
            StdlibMethodId::MathFactorial => {
                let value_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                self.ensure_assignable(&value_ty, &Type::Int, span)?;
            }
            StdlibMethodId::MathGcd
            | StdlibMethodId::MathLcm
            | StdlibMethodId::MathComb
            | StdlibMethodId::MathPerm => {
                let left_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                self.ensure_assignable(&left_ty, &Type::Int, span)?;
                let right_ty = self.check_expr(&mut args[1], Some(&Type::Int))?;
                self.ensure_assignable(&right_ty, &Type::Int, span)?;
            }
            StdlibMethodId::TimeTime
            | StdlibMethodId::TimeTimeNs
            | StdlibMethodId::TimeMonotonic
            | StdlibMethodId::TimeMonotonicNs
            | StdlibMethodId::TimePerfCounter
            | StdlibMethodId::TimePerfCounterNs
            | StdlibMethodId::TimeProcessTime
            | StdlibMethodId::TimeProcessTimeNs => {}
            StdlibMethodId::TimeSleep => {
                let sleep_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                self.ensure_assignable(&sleep_ty, &Type::Float, span)?;
            }
            StdlibMethodId::TimeLocaltime | StdlibMethodId::TimeGmtime => {
                if args.len() == 1 {
                    let seconds_ty = self.check_expr(&mut args[0], Some(&Type::Float))?;
                    self.ensure_assignable(&seconds_ty, &Type::Float, span)?;
                }
            }
            StdlibMethodId::TimeStrftime => {
                let format_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&format_ty, &Type::Str, span)?;
                let tuple_ty = self.check_expr(&mut args[1], None)?;
                match tuple_ty {
                    Type::Tuple(items) => {
                        if items.len() != 9 {
                            return Err(
                                self.error(span, "time.strftime() expects a 9-item time tuple")
                            );
                        }
                        for item in items {
                            if !matches!(item, Type::Unknown) {
                                self.ensure_assignable(&item, &Type::Int, span)?;
                            }
                        }
                    }
                    Type::Unknown => {}
                    _ => {
                        return Err(self.error(span, "time.strftime() expects a 9-item time tuple"));
                    }
                }
            }
            StdlibMethodId::TimeStrptime => {
                let text_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&text_ty, &Type::Str, span)?;
                let format_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&format_ty, &Type::Str, span)?;
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
            StdlibMethodId::ReSearch | StdlibMethodId::ReMatch => {
                Type::Custom("__py2rust_re_match".to_string())
            }
            StdlibMethodId::ReSub => Type::Str,
            StdlibMethodId::JsonDumps => Type::Str,
            StdlibMethodId::JsonLoads | StdlibMethodId::JsonLoad => {
                Type::Custom("__py2rust_json_value".to_string())
            }
            StdlibMethodId::JsonDump => Type::None,
            StdlibMethodId::MathSqrt
            | StdlibMethodId::MathSin
            | StdlibMethodId::MathCos
            | StdlibMethodId::MathTan
            | StdlibMethodId::MathLog
            | StdlibMethodId::MathLog2
            | StdlibMethodId::MathLog10
            | StdlibMethodId::MathExp
            | StdlibMethodId::MathAsin
            | StdlibMethodId::MathAcos
            | StdlibMethodId::MathAtan
            | StdlibMethodId::MathSinh
            | StdlibMethodId::MathCosh
            | StdlibMethodId::MathTanh
            | StdlibMethodId::MathFabs
            | StdlibMethodId::MathDegrees
            | StdlibMethodId::MathRadians
            | StdlibMethodId::MathAtan2
            | StdlibMethodId::MathFmod
            | StdlibMethodId::MathCopySign
            | StdlibMethodId::MathHypot
            | StdlibMethodId::MathPow => Type::Float,
            StdlibMethodId::MathCeil
            | StdlibMethodId::MathFloor
            | StdlibMethodId::MathTrunc
            | StdlibMethodId::MathFactorial
            | StdlibMethodId::MathGcd
            | StdlibMethodId::MathLcm
            | StdlibMethodId::MathComb
            | StdlibMethodId::MathPerm => Type::Int,
            StdlibMethodId::MathIsNan
            | StdlibMethodId::MathIsInf
            | StdlibMethodId::MathIsFinite => Type::Bool,
            StdlibMethodId::TimeTime
            | StdlibMethodId::TimeMonotonic
            | StdlibMethodId::TimePerfCounter
            | StdlibMethodId::TimeProcessTime => Type::Float,
            StdlibMethodId::TimeTimeNs
            | StdlibMethodId::TimeMonotonicNs
            | StdlibMethodId::TimePerfCounterNs
            | StdlibMethodId::TimeProcessTimeNs => Type::Int,
            StdlibMethodId::TimeSleep => Type::None,
            StdlibMethodId::TimeLocaltime
            | StdlibMethodId::TimeGmtime
            | StdlibMethodId::TimeStrptime => Type::Tuple(vec![
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
            ]),
            StdlibMethodId::TimeStrftime => Type::Str,
        }
    }
}
