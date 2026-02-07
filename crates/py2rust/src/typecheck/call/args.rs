// Argument binding and validation for call signatures.

use super::super::*;
use crate::call_bind::{plan_non_unpacking_bind, BoundArg};
use std::collections::HashSet;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_call_args(
        &mut self,
        sig: &FunctionSig,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        span: Span,
        allow_self: bool,
    ) -> Result<(), CompileError> {
        let has_unpacking = args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Starred { .. }))
            || keywords.iter().any(|kw| kw.name.is_none());
        if has_unpacking {
            return self.check_call_args_with_unpacking(sig, args, keywords, span, allow_self);
        }

        let keyword_names: Vec<Option<&str>> =
            keywords.iter().map(|kw| kw.name.as_deref()).collect();
        let plan = plan_non_unpacking_bind(
            &sig.param_names,
            &sig.param_kinds,
            &sig.has_defaults,
            args.len(),
            &keyword_names,
            allow_self,
        )
        .map_err(|err| self.error(span, err.message()))?;

        for (idx, maybe_source) in plan
            .bound
            .iter()
            .copied()
            .enumerate()
            .take(sig.params.len())
        {
            let Some(source) = maybe_source else {
                continue;
            };
            let param_ty = &sig.params[idx];
            let arg = match source {
                BoundArg::Positional(pos_idx) => &mut args[pos_idx],
                BoundArg::Keyword(kw_idx) => &mut keywords[kw_idx].value,
            };
            // Keep implicit conversion strict: only explicit str(...) calls convert values.
            let arg_ty = self.check_expr(arg, Some(param_ty))?;
            self.ensure_assignable(&arg_ty, param_ty, span)?;
        }

        if let Some(vararg_idx) = plan.vararg_idx {
            let Some(Type::List(inner_ty)) = sig.params.get(vararg_idx) else {
                return Err(self.error(span, "Internal error: *args parameter must be list type"));
            };
            for pos_idx in plan.vararg_positional {
                let arg_ty = self.check_expr(&mut args[pos_idx], Some(inner_ty.as_ref()))?;
                self.ensure_assignable(&arg_ty, inner_ty.as_ref(), span)?;
            }
        } else if !plan.vararg_positional.is_empty() {
            return Err(self.error(span, "Argument count mismatch"));
        }

        if let Some(varkw_idx) = plan.varkw_idx {
            let Some(Type::Dict(_, value_ty)) = sig.params.get(varkw_idx) else {
                return Err(
                    self.error(span, "Internal error: **kwargs parameter must be dict type")
                );
            };
            for kw_idx in plan.varkw_keywords {
                let arg_ty =
                    self.check_expr(&mut keywords[kw_idx].value, Some(value_ty.as_ref()))?;
                self.ensure_assignable(&arg_ty, value_ty.as_ref(), span)?;
            }
        } else if !plan.varkw_keywords.is_empty() {
            return Err(self.error(span, "Unknown keyword argument"));
        }
        Ok(())
    }

    /// Type check calls that use `*args`/`**kwargs` unpacking.
    ///
    /// We intentionally keep this path permissive: exact argument cardinality can depend on
    /// runtime container sizes, so we validate value kinds and type compatibility where known.
    fn check_call_args_with_unpacking(
        &mut self,
        sig: &FunctionSig,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        span: Span,
        allow_self: bool,
    ) -> Result<(), CompileError> {
        let mut seen_keywords = HashSet::new();
        let mut varkw_value_ty: Option<&Type> = None;
        for (idx, kind) in sig.param_kinds.iter().enumerate() {
            if matches!(kind, ParamKind::VarKeywords) {
                if let Some(Type::Dict(_, value_ty)) = sig.params.get(idx) {
                    varkw_value_ty = Some(value_ty.as_ref());
                }
            }
        }

        for arg in args.iter_mut() {
            if let ExprKind::Starred { value } = &mut arg.kind {
                let iter_ty = self.check_expr(value, None)?;
                let _ = self.iter_item_type(&iter_ty, span)?;
            } else {
                self.check_expr(arg, None)?;
            }
        }

        for kw in keywords.iter_mut() {
            if let Some(name) = kw.name.as_deref() {
                if !seen_keywords.insert(name.to_string()) {
                    return Err(self.error(span, format!("Multiple values for argument `{name}`")));
                }
                if sig.param_names.iter().enumerate().any(|(idx, param_name)| {
                    param_name == name && matches!(sig.param_kinds[idx], ParamKind::PositionalOnly)
                }) {
                    return Err(self.error(
                        span,
                        format!("Positional-only argument passed as keyword: `{name}`"),
                    ));
                }
                let direct_param = sig
                    .param_names
                    .iter()
                    .enumerate()
                    .find(|(idx, param_name)| {
                        **param_name == name
                            && matches!(
                                sig.param_kinds[*idx],
                                ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly
                            )
                    })
                    .map(|(idx, _)| idx);
                if let Some(param_idx) = direct_param {
                    if allow_self
                        && param_idx == 0
                        && sig.param_kinds.first() == Some(&ParamKind::PositionalOrKeyword)
                    {
                        return Err(
                            self.error(span, format!("Unexpected keyword argument `{name}`"))
                        );
                    }
                    let expected = sig.params.get(param_idx);
                    let value_ty = self.check_expr(&mut kw.value, expected)?;
                    if let Some(expected) = expected {
                        if !matches!(expected, Type::Unknown) {
                            self.ensure_assignable(&value_ty, expected, span)?;
                        }
                    }
                } else if let Some(value_ty_expected) = varkw_value_ty {
                    let value_ty = self.check_expr(&mut kw.value, Some(value_ty_expected))?;
                    if !matches!(value_ty_expected, Type::Unknown) {
                        self.ensure_assignable(&value_ty, value_ty_expected, span)?;
                    }
                } else {
                    return Err(self.error(span, format!("Unknown keyword argument `{name}`")));
                }
            } else {
                let unpack_ty = self.check_expr(&mut kw.value, None)?;
                match unpack_ty {
                    Type::Dict(key_ty, _) => {
                        if !matches!(key_ty.as_ref(), Type::Str | Type::Unknown) {
                            return Err(self.error(
                                span,
                                "Call-site **kwargs unpacking requires dict[str, T]",
                            ));
                        }
                    }
                    Type::Unknown => {}
                    _ => {
                        return Err(self.error(
                            span,
                            "Call-site **kwargs unpacking expects a dict expression",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
