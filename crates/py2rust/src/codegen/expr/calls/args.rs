// Argument resolution and unpacking call paths.

use super::super::*;
use crate::call_bind::{plan_non_unpacking_bind, BoundArg};
use crate::typecheck::FunctionSig;

/// Metadata bundle for lambda call-site unpacking.
pub(super) struct LambdaUnpackCallMeta<'a> {
    pub(super) param_names: &'a [String],
    pub(super) params: &'a [Type],
    pub(super) param_kinds: &'a [ParamKind],
    pub(super) has_defaults: &'a [bool],
}

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
        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        let param_kinds: Vec<ParamKind> = params.iter().map(|p| p.kind).collect();
        let has_defaults: Vec<bool> = params.iter().map(|p| p.default.is_some()).collect();
        let keyword_names: Vec<Option<&str>> =
            keywords.iter().map(|kw| kw.name.as_deref()).collect();
        let plan = plan_non_unpacking_bind(
            &param_names,
            &param_kinds,
            &has_defaults,
            args.len(),
            &keyword_names,
            implicit_first,
        )
        .map_err(|err| {
            self.error(
                params.last().map(|p| p.span).unwrap_or(Span::new(0, 0)),
                err.message(),
            )
        })?;

        let mut out: Vec<Option<Expr>> = vec![None; params.len()];
        if implicit_first && !plan.positional_params.is_empty() && plan.positional_params[0] == 0 {
            out[0] = Some(Expr {
                kind: ExprKind::Literal(Literal::None),
                span: params[0].span,
                ty: Some(param_types[0].clone()),
            });
        }

        for (idx, maybe_source) in plan.bound.iter().copied().enumerate() {
            let Some(source) = maybe_source else {
                continue;
            };
            out[idx] = Some(match source {
                BoundArg::Positional(pos_idx) => args[pos_idx].clone(),
                BoundArg::Keyword(kw_idx) => keywords[kw_idx].value.clone(),
            });
        }

        for (idx, param) in params.iter().enumerate() {
            if matches!(param.kind, ParamKind::VarArgs | ParamKind::VarKeywords) {
                continue;
            }
            if implicit_first && idx == 0 && plan.positional_params.first() == Some(&0) {
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

        if let Some(vararg_idx) = plan.vararg_idx {
            let mut vararg_values = Vec::new();
            for pos_idx in plan.vararg_positional.iter().copied() {
                vararg_values.push(args[pos_idx].clone());
            }
            // Always type the varargs pack as List, even if the param is Slice —
            // the call-site codegen will handle the List→Slice conversion.
            let vararg_ty = match &param_types[vararg_idx] {
                Type::Slice(inner) => Type::List(inner.clone()),
                other => other.clone(),
            };
            out[vararg_idx] = Some(Expr {
                kind: ExprKind::List(vararg_values),
                span: params[vararg_idx].span,
                ty: Some(vararg_ty),
            });
        } else if !plan.vararg_positional.is_empty() {
            return Err(self.error(
                params.last().map(|p| p.span).unwrap_or(Span::new(0, 0)),
                "Argument count mismatch",
            ));
        }

        if let Some(varkw_idx) = plan.varkw_idx {
            let mut varkw_values: Vec<(String, Expr)> = Vec::new();
            for kw_idx in plan.varkw_keywords.iter().copied() {
                let kw_name = keywords[kw_idx]
                    .name
                    .as_deref()
                    .expect("non-unpacking planner guarantees keyword names");
                varkw_values.push((kw_name.to_string(), keywords[kw_idx].value.clone()));
            }
            let mut items = Vec::new();
            for (name, value) in varkw_values {
                items.push(DictEntry::Item {
                    key: Expr {
                        kind: ExprKind::Literal(Literal::Str(name)),
                        span: params[varkw_idx].span,
                        ty: Some(Type::Str),
                    },
                    value: Box::new(value),
                });
            }
            out[varkw_idx] = Some(Expr {
                kind: ExprKind::Dict(items),
                span: params[varkw_idx].span,
                ty: Some(param_types[varkw_idx].clone()),
            });
        } else if !plan.varkw_keywords.is_empty() {
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
                ParamKind::PositionalOnly
                | ParamKind::PositionalOrKeyword
                | ParamKind::KeywordOnly => sig_ty.clone(),
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
        let Some(scalar_ty) = scalar_ty else {
            // CPython-compat divergence:
            // Dynamic callable unpacking with fully unknown scalar type still falls
            // back to runtime `unimplemented!` in this build.
            return Ok(
                "unimplemented!(\"dynamic callable unpacking is not supported yet\")".to_string(),
            );
        };
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

        self.uses.index_map = true;
        lines.push(format!(
            "let mut {kw_map}: IndexMap<String, {scalar_rust_ty}> = IndexMap::new();",
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
                let pairs_expr = self.gen_kwarg_pairs_expr(&kw.value, Some(&scalar_ty))?;
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
        // Preserve CPython behavior: positional-only names cannot arrive via kwargs,
        // even when `**kwargs` is present in the callee signature.
        for param in &def.params {
            if matches!(param.kind, ParamKind::PositionalOnly) {
                lines.push(format!(
                    "if {kw_map}.contains_key(\"{param_name}\") {{ return Err(PyError::TypeError(\"Positional-only argument passed as keyword: `{param_name}`\".into())); }}",
                    kw_map = kw_map,
                    param_name = param.name
                ));
            }
        }
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
                    "return Err(PyError::TypeError(\"Missing required argument `{}`\".into()))",
                    param.name
                )
            };

            match param.kind {
                ParamKind::PositionalOnly => {
                    lines.push(format!(
                        "let {arg_var} = if {pos_idx} < {pos_vec}.len() {{ let v = {pos_vec}[{pos_idx}].clone(); {pos_idx} += 1; v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        pos_idx = pos_idx,
                        pos_vec = pos_vec,
                        fallback = fallback
                    ));
                }
                ParamKind::PositionalOrKeyword => {
                    // Keep CPython keyword ordering behavior by using `shift_remove`
                    // instead of deprecated `remove` on IndexMap.
                    lines.push(format!(
                        "let {arg_var} = if {pos_idx} < {pos_vec}.len() {{ let v = {pos_vec}[{pos_idx}].clone(); {pos_idx} += 1; v }} else if let Some(v) = {kw_map}.shift_remove(\"{param_name}\") {{ v }} else {{ {fallback} }};",
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
                        "let {arg_var} = if let Some(v) = {kw_map}.shift_remove(\"{param_name}\") {{ v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        kw_map = kw_map,
                        param_name = param.name,
                        fallback = fallback
                    ));
                }
                ParamKind::VarArgs => {
                    has_vararg = true;
                    // Check if this varargs param is read-only → pass as slice reference.
                    let is_readonly = self
                        .readonly_list_params
                        .get(func_name)
                        .is_some_and(|ro| ro.contains(param.name.as_str()));
                    if is_readonly {
                        lines.push(format!(
                            "let {arg_var} = &{pos_vec}[{pos_idx}..];",
                            arg_var = arg_var,
                            pos_vec = pos_vec,
                            pos_idx = pos_idx
                        ));
                    } else {
                        let wrapped = self.wrap_list_storage_expr(
                            &format!("{pos_vec}[{pos_idx}..].to_vec()"),
                            ListStorage::SharedCell,
                        );
                        lines.push(format!(
                            "let {arg_var} = {wrapped};",
                            arg_var = arg_var,
                            wrapped = wrapped
                        ));
                    }
                    lines.push(format!(
                        "{pos_idx} = {pos_vec}.len();",
                        pos_idx = pos_idx,
                        pos_vec = pos_vec
                    ));
                }
                ParamKind::VarKeywords => {
                    has_varkw = true;
                    let wrapped = self.wrap_dict_storage_expr(&kw_map, DictStorage::SharedCell);
                    lines.push(format!(
                        "let {arg_var} = {wrapped};",
                        arg_var = arg_var,
                        wrapped = wrapped
                    ));
                }
            }
            call_args.push(arg_var);
        }

        if !has_vararg {
            lines.push(format!(
                "if {pos_idx} < {pos_vec}.len() {{ return Err(PyError::TypeError(\"Argument count mismatch\".into())); }}",
                pos_idx = pos_idx,
                pos_vec = pos_vec
            ));
        }
        if !has_varkw {
            lines.push(format!(
                "if !{kw_map}.is_empty() {{ return Err(PyError::TypeError(\"Unknown keyword argument\".into())); }}",
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

    /// Generate runtime argument binding for lambda call targets that use `*args`/`**kwargs`.
    pub(super) fn gen_lambda_call_with_unpacking(
        &mut self,
        expr: &Expr,
        func: &Expr,
        meta: LambdaUnpackCallMeta<'_>,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        let param_names = meta.param_names;
        let params = meta.params;
        let param_kinds = meta.param_kinds;
        let has_defaults = meta.has_defaults;
        if param_names.len() != params.len()
            || param_kinds.len() != params.len()
            || has_defaults.len() != params.len()
        {
            return Err(self.error(
                expr.span,
                "Internal error: lambda signature metadata length mismatch",
            ));
        }

        let mut scalar_ty: Option<Type> = None;
        for (idx, param_ty) in params.iter().enumerate() {
            let kind = param_kinds[idx];
            let candidate = match kind {
                ParamKind::PositionalOnly
                | ParamKind::PositionalOrKeyword
                | ParamKind::KeywordOnly => param_ty.clone(),
                ParamKind::VarArgs => match param_ty {
                    Type::List(inner) => *inner.clone(),
                    _ => Type::Unknown,
                },
                ParamKind::VarKeywords => match param_ty {
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
        let Some(scalar_ty) = scalar_ty else {
            // CPython-compat divergence:
            // Dynamic callable unpacking with fully unknown scalar type still falls
            // back to runtime `unimplemented!` in this build.
            return Ok(
                "unimplemented!(\"dynamic callable unpacking is not supported yet\")".to_string(),
            );
        };
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

        self.uses.index_map = true;
        lines.push(format!(
            "let mut {kw_map}: IndexMap<String, {scalar_rust_ty}> = IndexMap::new();",
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
                let pairs_expr = self.gen_kwarg_pairs_expr(&kw.value, Some(&scalar_ty))?;
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
        for (idx, param_name) in param_names.iter().enumerate() {
            if matches!(param_kinds[idx], ParamKind::PositionalOnly) {
                lines.push(format!(
                    "if {kw_map}.contains_key(\"{param_name}\") {{ return Err(PyError::TypeError(\"Positional-only argument passed as keyword: `{param_name}`\".into())); }}",
                    kw_map = kw_map,
                    param_name = param_name
                ));
            }
        }

        self.uses.py_error = true;
        let mut call_args = Vec::new();
        let mut has_vararg = false;
        let mut has_varkw = false;
        for idx in 0..params.len() {
            let arg_var = self.new_tmp();
            let fallback = if has_defaults[idx] {
                return Err(self.error(
                    expr.span,
                    "Default arguments for nested callables are not supported yet",
                ));
            } else {
                format!(
                    "return Err(PyError::TypeError(\"Missing required argument `{}`\".into()))",
                    param_names[idx]
                )
            };

            match param_kinds[idx] {
                ParamKind::PositionalOnly => {
                    lines.push(format!(
                        "let {arg_var} = if {pos_idx} < {pos_vec}.len() {{ let v = {pos_vec}[{pos_idx}].clone(); {pos_idx} += 1; v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        pos_idx = pos_idx,
                        pos_vec = pos_vec,
                        fallback = fallback
                    ));
                }
                ParamKind::PositionalOrKeyword => {
                    lines.push(format!(
                        "let {arg_var} = if {pos_idx} < {pos_vec}.len() {{ let v = {pos_vec}[{pos_idx}].clone(); {pos_idx} += 1; v }} else if let Some(v) = {kw_map}.shift_remove(\"{param_name}\") {{ v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        pos_idx = pos_idx,
                        pos_vec = pos_vec,
                        kw_map = kw_map,
                        param_name = param_names[idx],
                        fallback = fallback
                    ));
                }
                ParamKind::KeywordOnly => {
                    lines.push(format!(
                        "let {arg_var} = if let Some(v) = {kw_map}.shift_remove(\"{param_name}\") {{ v }} else {{ {fallback} }};",
                        arg_var = arg_var,
                        kw_map = kw_map,
                        param_name = param_names[idx],
                        fallback = fallback
                    ));
                }
                ParamKind::VarArgs => {
                    has_vararg = true;
                    let wrapped = self.wrap_list_storage_expr(
                        &format!("{pos_vec}[{pos_idx}..].to_vec()"),
                        ListStorage::SharedCell,
                    );
                    lines.push(format!(
                        "let {arg_var} = {wrapped};",
                        arg_var = arg_var,
                        wrapped = wrapped
                    ));
                    lines.push(format!(
                        "{pos_idx} = {pos_vec}.len();",
                        pos_idx = pos_idx,
                        pos_vec = pos_vec
                    ));
                }
                ParamKind::VarKeywords => {
                    has_varkw = true;
                    let wrapped = self.wrap_dict_storage_expr(&kw_map, DictStorage::SharedCell);
                    lines.push(format!(
                        "let {arg_var} = {wrapped};",
                        arg_var = arg_var,
                        wrapped = wrapped
                    ));
                }
            }
            call_args.push(arg_var);
        }

        if !has_vararg {
            lines.push(format!(
                "if {pos_idx} < {pos_vec}.len() {{ return Err(PyError::TypeError(\"Argument count mismatch\".into())); }}",
                pos_idx = pos_idx,
                pos_vec = pos_vec
            ));
        }
        if !has_varkw {
            lines.push(format!(
                "if !{kw_map}.is_empty() {{ return Err(PyError::TypeError(\"Unknown keyword argument\".into())); }}",
                kw_map = kw_map
            ));
        }

        let call = format!("{}({})", self.gen_expr(func)?, call_args.join(", "));
        lines.push(format!("Ok({call})", call = call));
        let unpack_result = self.new_tmp();
        Ok(format!(
            "{{ let {unpack_result} = (|| -> Result<_, PyError> {{ {body} }})(); {wrapped} }}",
            unpack_result = unpack_result,
            body = lines.join(" "),
            wrapped = self.wrap_result(unpack_result.clone())
        ))
    }

    /// Convert a kwargs expression into owned key/value pairs.
    fn gen_kwarg_pairs_expr(
        &mut self,
        kwargs: &Expr,
        value_ty: Option<&Type>,
    ) -> Result<String, CompileError> {
        if !matches!(kwargs.ty.as_ref(), Some(Type::Dict(_, _))) {
            return Err(self.error(
                kwargs.span,
                "Call-site **kwargs unpacking expects a dict expression",
            ));
        }
        let expected_dict =
            value_ty.map(|ty| Type::Dict(Box::new(Type::Str), Box::new(ty.clone())));
        let dict_expr = if let Some(expected) = expected_dict.as_ref() {
            self.gen_expr_with_expected(kwargs, Some(expected))?
        } else {
            self.gen_expr(kwargs)?
        };
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
        let shared_dict_ty = value_ty.map(|ty| {
            self.uses.index_map = true;
            let dict_ty = Type::Dict(Box::new(Type::Str), Box::new(ty.clone()));
            self.rust_type_for_dict_storage(&dict_ty, self.dict_storage_for_expr(kwargs))
        });
        Ok(format!(
            "{{ let {dict_tmp}{dict_ty_suffix} = {dict_init}; let {dict_guard} = {dict_tmp}.py_dict_guard(); {dict_guard}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>() }}",
            dict_tmp = dict_tmp,
            dict_ty_suffix = shared_dict_ty
                .as_ref()
                .map(|ty| format!(": {}", ty))
                .unwrap_or_default(),
            dict_init = dict_init,
            dict_guard = dict_guard
        ))
    }
}
