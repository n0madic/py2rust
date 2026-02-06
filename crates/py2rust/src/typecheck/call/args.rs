// Argument binding and validation for call signatures.

use super::super::*;
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
        if sig.param_names.len() != sig.params.len()
            || sig.param_kinds.len() != sig.params.len()
            || sig.has_defaults.len() != sig.params.len()
        {
            return Err(self.error(span, "Internal error: malformed function signature"));
        }
        let has_unpacking = args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Starred { .. }))
            || keywords.iter().any(|kw| kw.name.is_none());
        if has_unpacking {
            return self.check_call_args_with_unpacking(sig, args, keywords, span, allow_self);
        }

        #[derive(Copy, Clone)]
        enum BoundArg {
            Pos(usize),
            Kw(usize),
        }

        let mut positional_params = Vec::new();
        let mut vararg_idx = None;
        let mut varkw_idx = None;
        for (idx, kind) in sig.param_kinds.iter().enumerate() {
            match kind {
                ParamKind::PositionalOrKeyword => positional_params.push(idx),
                ParamKind::VarArgs => vararg_idx = Some(idx),
                ParamKind::KeywordOnly => {}
                ParamKind::VarKeywords => varkw_idx = Some(idx),
            }
        }

        let mut bound: Vec<Option<BoundArg>> = vec![None; sig.params.len()];
        let mut positional_cursor = 0usize;
        if allow_self && !positional_params.is_empty() && positional_params[0] == 0 {
            positional_cursor = 1;
        }
        let mut vararg_positional = Vec::new();
        for (pos_idx, arg) in args.iter().enumerate() {
            if matches!(arg.kind, ExprKind::Starred { .. }) {
                return Err(self.error(
                    arg.span,
                    "Call-site *args unpacking is not supported in this context yet",
                ));
            }
            if positional_cursor < positional_params.len() {
                let param_idx = positional_params[positional_cursor];
                bound[param_idx] = Some(BoundArg::Pos(pos_idx));
                positional_cursor += 1;
            } else if vararg_idx.is_some() {
                vararg_positional.push(pos_idx);
            } else {
                return Err(self.error(span, "Argument count mismatch"));
            }
        }

        let mut varkw_keywords = Vec::new();
        let mut seen_kw = HashSet::new();
        for (kw_idx, kw) in keywords.iter().enumerate() {
            let Some(kw_name) = kw.name.as_deref() else {
                return Err(self.error(
                    span,
                    "Call-site **kwargs unpacking is not supported in this context yet",
                ));
            };
            if !seen_kw.insert(kw_name.to_string()) {
                return Err(self.error(span, format!("Multiple values for argument `{kw_name}`")));
            }
            let direct_param = sig
                .param_names
                .iter()
                .enumerate()
                .find(|(idx, name)| {
                    **name == kw_name
                        && matches!(
                            sig.param_kinds[*idx],
                            ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly
                        )
                })
                .map(|(idx, _)| idx);
            if let Some(param_idx) = direct_param {
                if allow_self && param_idx == 0 && positional_params.first() == Some(&0) {
                    return Err(
                        self.error(span, format!("Unexpected keyword argument `{kw_name}`"))
                    );
                }
                if bound[param_idx].is_some() {
                    return Err(
                        self.error(span, format!("Multiple values for argument `{kw_name}`"))
                    );
                }
                bound[param_idx] = Some(BoundArg::Kw(kw_idx));
            } else if varkw_idx.is_some() {
                varkw_keywords.push(kw_idx);
            } else {
                return Err(self.error(span, format!("Unknown keyword argument `{kw_name}`")));
            }
        }

        for (idx, maybe_bound) in bound.iter().enumerate().take(sig.params.len()) {
            match sig.param_kinds[idx] {
                ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly => {
                    if allow_self && idx == 0 && positional_params.first() == Some(&0) {
                        continue;
                    }
                    if maybe_bound.is_none() && !sig.has_defaults[idx] {
                        let name = sig
                            .param_names
                            .get(idx)
                            .cloned()
                            .unwrap_or_else(|| format!("arg{idx}"));
                        return Err(self.error(span, format!("Missing required argument `{name}`")));
                    }
                }
                ParamKind::VarArgs | ParamKind::VarKeywords => {}
            }
        }

        for (idx, maybe_source) in bound.iter().copied().enumerate().take(sig.params.len()) {
            let Some(source) = maybe_source else {
                continue;
            };
            let param_ty = &sig.params[idx];
            let arg = match source {
                BoundArg::Pos(pos_idx) => &mut args[pos_idx],
                BoundArg::Kw(kw_idx) => &mut keywords[kw_idx].value,
            };
            let mut arg_ty = self.check_expr(arg, Some(param_ty))?;
            if matches!(param_ty, Type::Str)
                && !matches!(arg_ty, Type::Str)
                && matches!(arg_ty, Type::Int | Type::Float | Type::Bool)
            {
                let inner = arg.clone();
                *arg = Expr {
                    kind: ExprKind::Call {
                        func: Box::new(Expr {
                            kind: ExprKind::Name("str".to_string()),
                            span: arg.span,
                            ty: Some(Type::Str),
                        }),
                        args: vec![inner],
                        keywords: Vec::new(),
                    },
                    span: arg.span,
                    ty: Some(Type::Str),
                };
                arg_ty = Type::Str;
            }
            self.ensure_assignable(&arg_ty, param_ty, span)?;
        }

        if let Some(vararg_idx) = vararg_idx {
            let Some(Type::List(inner_ty)) = sig.params.get(vararg_idx) else {
                return Err(self.error(span, "Internal error: *args parameter must be list type"));
            };
            for pos_idx in vararg_positional {
                let arg_ty = self.check_expr(&mut args[pos_idx], Some(inner_ty.as_ref()))?;
                self.ensure_assignable(&arg_ty, inner_ty.as_ref(), span)?;
            }
        } else if !vararg_positional.is_empty() {
            return Err(self.error(span, "Argument count mismatch"));
        }

        if let Some(varkw_idx) = varkw_idx {
            let Some(Type::Dict(_, value_ty)) = sig.params.get(varkw_idx) else {
                return Err(
                    self.error(span, "Internal error: **kwargs parameter must be dict type")
                );
            };
            for kw_idx in varkw_keywords {
                let arg_ty =
                    self.check_expr(&mut keywords[kw_idx].value, Some(value_ty.as_ref()))?;
                self.ensure_assignable(&arg_ty, value_ty.as_ref(), span)?;
            }
        } else if !varkw_keywords.is_empty() {
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
