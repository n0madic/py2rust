// Argument resolution and unpacking call paths.

use super::super::*;
use crate::typecheck::FunctionSig;

impl<'a> Codegen<'a> {
    /// Resolve positional/keyword call arguments and fill defaults.
    pub(crate) fn resolve_call_args(
        &self,
        args: &[Expr],
        keywords: &[KeywordArg],
        params: &[Param],
        param_types: &[Type],
        call_target: (Option<&str>, &str),
        implicit_first: bool,
    ) -> Result<Vec<Expr>, CompileError> {
        let (class_name, func_name) = call_target;
        if params.len() != param_types.len() {
            return Err(self.error(
                Span::new(0, 0),
                format!("Internal error: arity mismatch in {func_name}"),
            ));
        }
        let mut positional_params = Vec::new();
        let mut vararg_idx = None;
        let mut varkw_idx = None;
        for (idx, param) in params.iter().enumerate() {
            match param.kind {
                ParamKind::PositionalOrKeyword => positional_params.push(idx),
                ParamKind::VarArgs => vararg_idx = Some(idx),
                ParamKind::KeywordOnly => {}
                ParamKind::VarKeywords => varkw_idx = Some(idx),
            }
        }

        let mut out: Vec<Option<Expr>> = vec![None; params.len()];
        let mut positional_cursor = 0usize;
        if implicit_first && !positional_params.is_empty() && positional_params[0] == 0 {
            out[0] = Some(Expr {
                kind: ExprKind::Literal(Literal::None),
                span: params[0].span,
                ty: Some(param_types[0].clone()),
            });
            positional_cursor = 1;
        }

        let mut vararg_values = Vec::new();
        for arg in args {
            if matches!(arg.kind, ExprKind::Starred { .. }) {
                return Err(self.error(
                    arg.span,
                    "Call-site *args unpacking is not supported in this context yet",
                ));
            }
            if positional_cursor < positional_params.len() {
                let param_idx = positional_params[positional_cursor];
                out[param_idx] = Some(arg.clone());
                positional_cursor += 1;
            } else if vararg_idx.is_some() {
                vararg_values.push(arg.clone());
            } else {
                return Err(self.error(
                    params.last().map(|p| p.span).unwrap_or(Span::new(0, 0)),
                    "Argument count mismatch",
                ));
            }
        }

        let mut seen_keywords = std::collections::HashSet::new();
        let mut varkw_values: Vec<(String, Expr)> = Vec::new();
        for kw in keywords {
            let Some(kw_name) = kw.name.as_deref() else {
                return Err(self.error(
                    kw.value.span,
                    "Call-site **kwargs unpacking is not supported in this context yet",
                ));
            };
            if !seen_keywords.insert(kw_name.to_string()) {
                return Err(self.error(
                    kw.value.span,
                    format!("Multiple values for argument `{kw_name}`"),
                ));
            }
            let direct_param_idx = params
                .iter()
                .enumerate()
                .find(|(_, p)| {
                    p.name == kw_name
                        && matches!(
                            p.kind,
                            ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly
                        )
                })
                .map(|(idx, _)| idx);
            if let Some(param_idx) = direct_param_idx {
                if implicit_first && param_idx == 0 && positional_params.first() == Some(&0) {
                    return Err(self.error(
                        kw.value.span,
                        format!("Unexpected keyword argument `{kw_name}`"),
                    ));
                }
                if out[param_idx].is_some() {
                    return Err(self.error(
                        kw.value.span,
                        format!("Multiple values for argument `{kw_name}`"),
                    ));
                }
                out[param_idx] = Some(kw.value.clone());
            } else if varkw_idx.is_some() {
                varkw_values.push((kw_name.to_string(), kw.value.clone()));
            } else {
                return Err(self.error(
                    kw.value.span,
                    format!("Unknown keyword argument `{kw_name}`"),
                ));
            }
        }

        for (idx, param) in params.iter().enumerate() {
            if matches!(param.kind, ParamKind::VarArgs | ParamKind::VarKeywords) {
                continue;
            }
            if implicit_first && idx == 0 && positional_params.first() == Some(&0) {
                continue;
            }
            if out[idx].is_some() {
                continue;
            }
            if param.default.is_none() {
                return Err(self.error(
                    param.span,
                    format!("Missing required argument for {func_name}"),
                ));
            }
            let global_name = self.default_global_name(class_name, func_name, param.name.as_str());
            let mut ty = param_types.get(idx).cloned();
            if let Some(Type::Ref(inner)) = ty {
                ty = Some(*inner);
            }
            out[idx] = Some(Expr {
                kind: ExprKind::Name(global_name),
                span: param.span,
                ty,
            });
        }

        if let Some(vararg_idx) = vararg_idx {
            out[vararg_idx] = Some(Expr {
                kind: ExprKind::List(vararg_values),
                span: params[vararg_idx].span,
                ty: Some(param_types[vararg_idx].clone()),
            });
        } else if !vararg_values.is_empty() {
            return Err(self.error(
                params.last().map(|p| p.span).unwrap_or(Span::new(0, 0)),
                "Argument count mismatch",
            ));
        }

        if let Some(varkw_idx) = varkw_idx {
            let mut items = Vec::new();
            for (name, value) in varkw_values {
                items.push((
                    Expr {
                        kind: ExprKind::Literal(Literal::Str(name)),
                        span: params[varkw_idx].span,
                        ty: Some(Type::Str),
                    },
                    value,
                ));
            }
            out[varkw_idx] = Some(Expr {
                kind: ExprKind::Dict(items),
                span: params[varkw_idx].span,
                ty: Some(param_types[varkw_idx].clone()),
            });
        } else if !varkw_values.is_empty() {
            return Err(self.error(
                params.last().map(|p| p.span).unwrap_or(Span::new(0, 0)),
                "Unknown keyword argument",
            ));
        }

        if out.iter().any(Option::is_none) {
            return Err(self.error(
                params.last().map(|p| p.span).unwrap_or(Span::new(0, 0)),
                "Internal error: unresolved call argument",
            ));
        }

        Ok(out.into_iter().flatten().collect())
    }

    /// Generate a runtime binder for calls that use `*args` or `**kwargs` unpacking.
    ///
    /// This path materializes positional and keyword collections, then binds them to
    /// the declared function parameters using Python-like precedence:
    /// positional -> keyword -> default.
    pub(super) fn gen_user_call_with_unpacking(
        &mut self,
        expr: &Expr,
        func_name: &str,
        sig: &FunctionSig,
        def: &Function,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if def.params.len() != sig.params.len() {
            return Err(self.error(
                expr.span,
                format!("Internal error: signature mismatch for `{func_name}`"),
            ));
        }

        let mut scalar_ty: Option<Type> = None;
        for (idx, param) in def.params.iter().enumerate() {
            let Some(sig_ty) = sig.params.get(idx) else {
                continue;
            };
            let candidate = match param.kind {
                ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly => sig_ty.clone(),
                ParamKind::VarArgs => match sig_ty {
                    Type::List(inner) => *inner.clone(),
                    _ => Type::Unknown,
                },
                ParamKind::VarKeywords => match sig_ty {
                    Type::Dict(_, value) => *value.clone(),
                    _ => Type::Unknown,
                },
            };
            if matches!(candidate, Type::Unknown) {
                continue;
            }
            if let Some(existing) = &scalar_ty {
                if existing != &candidate {
                    return Err(self.error(
                        expr.span,
                        "Call-site unpacking currently requires homogeneous argument types",
                    ));
                }
            } else {
                scalar_ty = Some(candidate);
            }
        }
        let scalar_ty = scalar_ty.ok_or_else(|| {
            self.error(
                expr.span,
                "Unable to infer unpacked argument type for this call",
            )
        })?;
        let scalar_rust_ty = self.rust_type(&scalar_ty);

        let pos_vec = self.new_tmp();
        let kw_map = self.new_tmp();
        let pos_idx = self.new_tmp();
        let mut lines = Vec::new();
        lines.push(format!(
            "let mut {pos_vec}: Vec<{scalar_rust_ty}> = Vec::new();",
            pos_vec = pos_vec,
            scalar_rust_ty = scalar_rust_ty
        ));
        for arg in args {
            match &arg.kind {
                ExprKind::Starred { value } => {
                    if matches!(&value.kind, ExprKind::List(items) if items.is_empty())
                        || matches!(&value.kind, ExprKind::Tuple(items) if items.is_empty())
                    {
                        // `*[]` and `*()` contribute nothing to positional argument packing.
                        continue;
                    }
                    let iter_expr =
                        self.gen_iter_source_owned(value, IterContext::ImmediateConsumption)?;
                    lines.push(format!(
                        "{pos_vec}.extend({iter_expr});",
                        pos_vec = pos_vec,
                        iter_expr = iter_expr
                    ));
                }
                _ => {
                    let arg_expr = self.gen_expr_with_expected(arg, Some(&scalar_ty))?;
                    lines.push(format!(
                        "{pos_vec}.push({arg_expr});",
                        pos_vec = pos_vec,
                        arg_expr = arg_expr
                    ));
                }
            }
        }

        self.uses.hash_map = true;
        lines.push(format!(
            "let mut {kw_map}: HashMap<String, {scalar_rust_ty}> = HashMap::new();",
            kw_map = kw_map,
            scalar_rust_ty = scalar_rust_ty
        ));
        for kw in keywords {
            if let Some(name) = kw.name.as_deref() {
                let value_expr = self.gen_expr_with_expected(&kw.value, Some(&scalar_ty))?;
                lines.push(format!(
                    "{kw_map}.insert(\"{name}\".to_string(), {value_expr});",
                    kw_map = kw_map,
                    name = name,
                    value_expr = value_expr
                ));
            } else {
                let pairs_expr = self.gen_kwarg_pairs_expr(&kw.value)?;
                let key_tmp = self.new_tmp();
                let value_tmp = self.new_tmp();
                lines.push(format!(
                    "for ({key_tmp}, {value_tmp}) in {pairs_expr} {{ {kw_map}.insert({key_tmp}, {value_tmp}); }}",
                    key_tmp = key_tmp,
                    value_tmp = value_tmp,
                    pairs_expr = pairs_expr,
                    kw_map = kw_map
                ));
            }
        }

        lines.push(format!("let mut {pos_idx}: usize = 0;", pos_idx = pos_idx));
        // Call-site unpacking can fail at runtime (missing args, unknown kwargs, etc.),
        // so this path always materializes a `Result<_, PyError>` binder.
        self.uses.py_error = true;
        let mut call_args = Vec::new();
        let mut has_vararg = false;
        let mut has_varkw = false;
        for (idx, param) in def.params.iter().enumerate() {
            let arg_var = self.new_tmp();
            let fallback = if param.default.is_some() {
                let global_name = self.default_global_name(None, func_name, param.name.as_str());
                let default_expr = Expr {
                    kind: ExprKind::Name(global_name),
                    span: param.span,
                    ty: sig.params.get(idx).cloned(),
                };
                self.gen_expr_with_expected(&default_expr, sig.params.get(idx))?
            } else {
                format!(
                    "return Err(PyError::TypeError(\"Missing required argument `{}`\".to_string()))",
                    param.name
                )
            };

            match param.kind {
                ParamKind::PositionalOrKeyword => {
                    lines.push(format!(
                        "let {arg_var} = if {pos_idx} < {pos_vec}.len() {{ let v = {pos_vec}[{pos_idx}].clone(); {pos_idx} += 1; v }} else if let Some(v) = {kw_map}.remove(\"{param_name}\") {{ v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        pos_idx = pos_idx,
                        pos_vec = pos_vec,
                        kw_map = kw_map,
                        param_name = param.name,
                        fallback = fallback
                    ));
                }
                ParamKind::KeywordOnly => {
                    lines.push(format!(
                        "let {arg_var} = if let Some(v) = {kw_map}.remove(\"{param_name}\") {{ v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        kw_map = kw_map,
                        param_name = param.name,
                        fallback = fallback
                    ));
                }
                ParamKind::VarArgs => {
                    has_vararg = true;
                    lines.push(format!(
                        "let {arg_var} = Arc::new(Mutex::new({pos_vec}[{pos_idx}..].to_vec()));",
                        arg_var = arg_var,
                        pos_vec = pos_vec,
                        pos_idx = pos_idx
                    ));
                    lines.push(format!(
                        "{pos_idx} = {pos_vec}.len();",
                        pos_idx = pos_idx,
                        pos_vec = pos_vec
                    ));
                }
                ParamKind::VarKeywords => {
                    has_varkw = true;
                    lines.push(format!(
                        "let {arg_var} = Arc::new(Mutex::new({kw_map}));",
                        arg_var = arg_var,
                        kw_map = kw_map
                    ));
                }
            }
            call_args.push(arg_var);
        }

        if !has_vararg {
            lines.push(format!(
                "if {pos_idx} < {pos_vec}.len() {{ return Err(PyError::TypeError(\"Argument count mismatch\".to_string())); }}",
                pos_idx = pos_idx,
                pos_vec = pos_vec
            ));
        }
        if !has_varkw {
            lines.push(format!(
                "if !{kw_map}.is_empty() {{ return Err(PyError::TypeError(\"Unknown keyword argument\".to_string())); }}",
                kw_map = kw_map
            ));
        }

        let call = format!("{}({})", func_name, call_args.join(", "));
        let call_value = if sig.can_throw {
            format!("({call}?)", call = call)
        } else {
            call
        };
        lines.push(format!("Ok({call_value})", call_value = call_value));
        let unpack_result = self.new_tmp();
        Ok(format!(
            "{{ let {unpack_result} = (|| -> Result<_, PyError> {{ {body} }})(); {wrapped} }}",
            unpack_result = unpack_result,
            body = lines.join(" "),
            wrapped = self.wrap_result(unpack_result.clone())
        ))
    }

    /// Convert a kwargs expression into owned key/value pairs.
    fn gen_kwarg_pairs_expr(&mut self, kwargs: &Expr) -> Result<String, CompileError> {
        if !matches!(kwargs.ty.as_ref(), Some(Type::Dict(_, _))) {
            return Err(self.error(
                kwargs.span,
                "Call-site **kwargs unpacking expects a dict expression",
            ));
        }
        let dict_expr = self.gen_expr(kwargs)?;
        if matches!(self.dict_storage_for_expr(kwargs), DictStorage::Local) {
            return Ok(format!(
                "{dict_expr}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()",
                dict_expr = dict_expr
            ));
        }
        let dict_tmp = self.new_tmp();
        let dict_guard = self.new_tmp();
        let dict_init = if matches!(kwargs.kind, ExprKind::Name(_)) {
            format!("{}.clone()", dict_expr)
        } else {
            dict_expr
        };
        Ok(format!(
            "{{ let {dict_tmp} = {dict_init}; let {dict_guard} = {dict_tmp}.lock().expect(\"dict mutex poisoned\"); {dict_guard}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>() }}",
            dict_tmp = dict_tmp,
            dict_init = dict_init,
            dict_guard = dict_guard
        ))
    }
}
