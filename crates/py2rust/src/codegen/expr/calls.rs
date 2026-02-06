// Function and method call expression lowering.

use super::super::*;
use crate::stdlib::registry::{resolve_method, resolve_module, StdlibMethodSpec};
use crate::typecheck::FunctionSig;

impl<'a> Codegen<'a> {
    /// Lower a call expression, including builtins and method calls.
    pub(super) fn gen_call_expr(
        &mut self,
        expr: &Expr,
        func: &Expr,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if let Some(Type::StdlibFunction { module, method }) = func.ty.as_ref() {
            let module_id = resolve_module(module.as_str()).ok_or_else(|| {
                self.error(
                    expr.span,
                    format!("module '{module}' is not registered in stdlib registry"),
                )
            })?;
            let spec = resolve_method(module_id, method.as_str()).ok_or_else(|| {
                self.error(
                    expr.span,
                    format!("{module} has no supported member '{method}'"),
                )
            })?;
            return self.gen_stdlib_call(expr.span, spec, args, keywords);
        }
        if let ExprKind::Name(name) = &func.kind {
            if let Some(result) = self.gen_builtin_call(expr, name, args, keywords)? {
                return Ok(result);
            }
        }
        if let ExprKind::Attr { value, attr } = &func.kind {
            return self.gen_attr_call(value, attr, args, keywords);
        }
        // Check if this is a user-defined function.
        if let ExprKind::Name(name) = &func.kind {
            if let Some(sig) = self.ctx.functions.get(name) {
                let has_unpacking = args
                    .iter()
                    .any(|arg| matches!(arg.kind, ExprKind::Starred { .. }))
                    || keywords.iter().any(|kw| kw.name.is_none());
                if has_unpacking {
                    if let Some(def) = self.function_defs.get(name).cloned() {
                        return self
                            .gen_user_call_with_unpacking(expr, name, sig, &def, args, keywords);
                    }
                    return Err(self.error(
                        expr.span,
                        "Call-site unpacking requires a known function definition",
                    ));
                }
                let param_types: Vec<Type> = sig
                    .params
                    .iter()
                    .map(|t| self.to_borrowed_param_type(t))
                    .collect();
                let full_args = if let Some(def) = self.function_defs.get(name) {
                    self.resolve_call_args(
                        args,
                        keywords,
                        &def.params,
                        &param_types,
                        (None, name),
                        false,
                    )?
                } else {
                    if !keywords.is_empty() {
                        return Err(self.error(
                            expr.span,
                            "Keyword arguments require a known function signature",
                        ));
                    }
                    args.to_vec()
                };
                let call = format!(
                    "{}({})",
                    name,
                    self.gen_call_args_for_sig(&param_types, &full_args)?
                );
                // Add ? operator if function can throw.
                if sig.can_throw {
                    return Ok(format!("({}?)", call));
                }
                return Ok(call);
            }
        }
        if !keywords.is_empty() {
            return Err(self.error(
                expr.span,
                "Keyword arguments are not supported for this call target",
            ));
        }
        if let Some(Type::Lambda { params, .. }) = func.ty.as_ref() {
            if !params.is_empty() && params.len() != args.len() {
                return Err(self.error(expr.span, "Argument count mismatch"));
            }
            let mut rendered_args = Vec::new();
            for (idx, arg) in args.iter().enumerate() {
                let expected = params.get(idx);
                let mut rendered = if let Some(param_ty) = expected {
                    self.gen_expr_with_expected(arg, Some(param_ty))?
                } else {
                    self.gen_expr(arg)?
                };
                if let Some(param_ty) = expected {
                    if matches!(
                        param_ty,
                        Type::List(_) | Type::Dict(_, _) | Type::Str | Type::Bytes
                    ) {
                        rendered = format!("{}.clone()", rendered);
                    } else if self.needs_borrow(arg.ty.as_ref(), param_ty) {
                        rendered = format!("&{}", rendered);
                    } else if matches!(param_ty, Type::Lambda { .. }) {
                        // Higher-order callable values are passed as boxed trait objects.
                        rendered = format!("Box::new({})", rendered);
                    }
                }
                rendered_args.push(rendered);
            }
            return Ok(format!(
                "{}({})",
                self.gen_expr(func)?,
                rendered_args.join(", ")
            ));
        }
        Ok(format!(
            "{}({})",
            self.gen_expr(func)?,
            self.gen_args(args)?
        ))
    }

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
    fn gen_user_call_with_unpacking(
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

    /// Try to lower a builtin call; return Some(expr) if handled.
    fn gen_builtin_call(
        &mut self,
        expr: &Expr,
        name: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<Option<String>, CompileError> {
        let is_builtin_name = matches!(
            name,
            "print"
                | "len"
                | "range"
                | "round"
                | "list"
                | "tuple"
                | "set"
                | "dict"
                | "bytes"
                | "enumerate"
                | "zip"
                | "map"
                | "filter"
                | "all"
                | "any"
                | "reversed"
                | "max"
                | "min"
                | "abs"
                | "pow"
                | "sum"
                | "int"
                | "float"
                | "bool"
                | "chr"
                | "ord"
                | "hash"
                | "id"
                | "divmod"
                | "next"
                | "bin"
                | "hex"
                | "oct"
                | "repr"
                | "str"
                | "isinstance"
                | "type"
                | "open"
                | "exit"
        );
        let builtin_accepts_keywords = matches!(name, "print");
        if is_builtin_name && !builtin_accepts_keywords && !keywords.is_empty() {
            return Err(self.error(
                expr.span,
                format!("Keyword arguments are not supported for {name}()"),
            ));
        }
        if name == "print" {
            self.uses.print = true;
            let mut sep_kw: Option<&Expr> = None;
            for kw in keywords {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        expr.span,
                        "Call-site **kwargs unpacking is not supported for print()",
                    ));
                };
                if kw_name != "sep" {
                    return Err(self.error(
                        expr.span,
                        format!("Unknown keyword argument `{kw_name}` for print()"),
                    ));
                }
                if sep_kw.is_some() {
                    return Err(self.error(expr.span, "Multiple values for keyword argument `sep`"));
                }
                sep_kw = Some(&kw.value);
            }
            if args.is_empty() {
                return Ok(Some("py_print(\"\")".to_string()));
            }
            if args.len() == 1 {
                if matches!(args[0].ty.as_ref(), Some(Type::None)) {
                    return Ok(Some("py_print(\"None\".to_string())".to_string()));
                }
                if matches!(args[0].ty.as_ref(), Some(Type::List(_))) {
                    let list_expr = self.list_str_expr(&args[0])?;
                    return Ok(Some(format!("py_print({})", list_expr)));
                }
                if self.print_needs_debug(&args[0]) {
                    let arg_expr = self.debug_arg_expr(&args[0])?;
                    return Ok(Some(format!("py_print(format!(\"{{:?}}\", {}))", arg_expr)));
                }
                let arg_expr = self.gen_expr(&args[0])?;
                return Ok(Some(format!("py_print({})", arg_expr)));
            }
            if let Some(sep_expr) = sep_kw {
                let mut parts = Vec::new();
                for arg in args {
                    if matches!(arg.ty.as_ref(), Some(Type::None)) {
                        parts.push("\"None\".to_string()".to_string());
                    } else if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                        parts.push(self.list_str_expr(arg)?);
                    } else if self.print_needs_debug(arg) {
                        parts.push(format!(
                            "format!(\"{{:?}}\", {})",
                            self.debug_arg_expr(arg)?
                        ));
                    } else {
                        parts.push(format!("format!(\"{{}}\", {})", self.gen_expr(arg)?));
                    }
                }
                let sep_code = self.gen_expr(sep_expr)?;
                return Ok(Some(format!(
                    "py_print(vec![{}].join(&{}))",
                    parts.join(", "),
                    sep_code
                )));
            }
            let mut fmt = String::new();
            let mut vals = Vec::new();
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    fmt.push(' ');
                }
                if matches!(arg.ty.as_ref(), Some(Type::None)) {
                    fmt.push_str("{}");
                    vals.push("\"None\".to_string()".to_string());
                } else if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                    fmt.push_str("{}");
                    vals.push(self.list_str_expr(arg)?);
                } else {
                    let spec = if self.print_needs_debug(arg) {
                        "{:?}"
                    } else {
                        "{}"
                    };
                    fmt.push_str(spec);
                    if spec == "{:?}" {
                        vals.push(self.debug_arg_expr(arg)?);
                    } else {
                        vals.push(self.gen_expr(arg)?);
                    }
                }
            }
            return Ok(Some(format!(
                "py_print(format!(\"{}\", {}))",
                fmt,
                vals.join(", ")
            )));
        }
        if name == "len" {
            self.uses.len = true;
            if let Some(Type::Custom(class_name)) = args[0].ty.as_ref() {
                if let Some(class_info) = self.ctx.classes.get(class_name) {
                    if class_info.methods.contains_key("__len__") {
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(Some(format!("{}.__len__()", arg_expr)));
                    }
                }
            }
            let arg_expr = self.gen_expr(&args[0])?;
            // Don't add & if already a reference type or if it's a borrowed parameter.
            let is_borrowed = self.is_reference_type(args[0].ty.as_ref())
                || matches!(&args[0].kind, ExprKind::Name(n) if self.is_borrowed_param(n));
            if is_borrowed {
                return Ok(Some(format!("py_len({})", arg_expr)));
            }
            return Ok(Some(format!("py_len(&{})", arg_expr)));
        }
        if name == "range" {
            if args.len() == 1 {
                self.uses.range = true;
                return Ok(Some(format!("py_range({})", self.gen_expr(&args[0])?)));
            }
            if args.len() == 2 {
                self.uses.range2 = true;
                return Ok(Some(format!(
                    "py_range2({}, {})",
                    self.gen_expr(&args[0])?,
                    self.gen_expr(&args[1])?
                )));
            }
            if args.len() == 3 {
                self.uses.range3 = true;
                let start_expr = self.gen_expr(&args[0])?;
                let end_expr = self.gen_expr(&args[1])?;
                let step_expr = self.gen_expr(&args[2])?;
                return Ok(Some(self.wrap_result(format!(
                    "py_range3({}, {}, {})",
                    start_expr, end_expr, step_expr
                ))));
            }
        }
        if name == "round" {
            if args.len() == 1 {
                let arg_expr = self.gen_expr(&args[0])?;
                if matches!(args[0].ty.as_ref(), Some(Type::Float)) {
                    self.uses.round = true;
                    return Ok(Some(format!("py_round({}, 0)", arg_expr)));
                }
                return Ok(Some(arg_expr));
            }
            if args.len() == 2 {
                let arg_expr = self.gen_expr(&args[0])?;
                let digits_expr = self.gen_expr(&args[1])?;
                if matches!(args[0].ty.as_ref(), Some(Type::Float)) {
                    self.uses.round = true;
                    return Ok(Some(format!("py_round({}, {})", arg_expr, digits_expr)));
                }
                return Ok(Some(arg_expr));
            }
        }
        if name == "list" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "list() expects zero or one argument"));
            }
            if args.is_empty() {
                if let Some(Type::List(inner)) = expr.ty.as_ref() {
                    if !matches!(inner.as_ref(), Type::Unknown) {
                        return Ok(Some(format!(
                            "Arc::new(Mutex::new(Vec::<{}>::new()))",
                            self.rust_type(inner)
                        )));
                    }
                }
                // Default to PyRepr so empty lists have a concrete element type.
                self.uses.py_repr = true;
                return Ok(Some(
                    "Arc::new(Mutex::new(Vec::<PyRepr>::new()))".to_string(),
                ));
            }
            if let Some(Type::Tuple(items)) = args[0].ty.as_ref() {
                let tmp = self.new_tmp();
                let base = self.gen_expr(&args[0])?;
                let mut elems = Vec::new();
                for idx in 0..items.len() {
                    elems.push(format!("{}.{}", tmp, idx));
                }
                return Ok(Some(format!(
                    "{{ let {} = {}; Arc::new(Mutex::new(vec![{}])) }}",
                    tmp,
                    base,
                    elems.join(", ")
                )));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            // Scope iterator consumption to avoid holding list locks across expressions.
            let body = format!(
                "Arc::new(Mutex::new(({}).collect::<Vec<_>>()))",
                iter_src.expr
            );
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "tuple" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "tuple() expects zero or one argument"));
            }
            if args.is_empty() {
                if let Some(Type::List(inner)) = expr.ty.as_ref() {
                    if !matches!(inner.as_ref(), Type::Unknown) {
                        return Ok(Some(format!(
                            "Arc::new(Mutex::new(Vec::<{}>::new()))",
                            self.rust_type(inner)
                        )));
                    }
                }
                // Default to PyRepr so empty tuples have a concrete element type.
                self.uses.py_repr = true;
                return Ok(Some(
                    "Arc::new(Mutex::new(Vec::<PyRepr>::new()))".to_string(),
                ));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let body = format!(
                "Arc::new(Mutex::new(({}).collect::<Vec<_>>()))",
                iter_src.expr
            );
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "set" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "set() expects zero or one argument"));
            }
            self.uses.hash_set = true;
            if args.is_empty() {
                return Ok(Some("HashSet::new()".to_string()));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            if matches!(args[0].ty.as_ref(), Some(Type::Set(_))) {
                if let ExprKind::Name(name) = &args[0].kind {
                    if self.is_borrowed_param(name) {
                        return Ok(Some(format!("(*{}).clone()", arg_expr)));
                    }
                }
                return Ok(Some(format!("{}.clone()", arg_expr)));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let body = format!("({}).collect::<HashSet<_>>()", iter_src.expr);
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "dict" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "dict() expects at most one argument"));
            }
            self.uses.hash_map = true;
            if args.is_empty() {
                return Ok(Some("Arc::new(Mutex::new(HashMap::new()))".to_string()));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            if matches!(args[0].ty.as_ref(), Some(Type::Dict(_, _))) {
                // dict(existing_dict) creates a shallow copy, not a shared alias.
                let tmp = self.new_tmp();
                let guard = self.new_tmp();
                let init = if matches!(args[0].kind, ExprKind::Name(_)) {
                    format!("{}.clone()", arg_expr)
                } else {
                    arg_expr
                };
                return Ok(Some(format!(
                    "{{ let {tmp} = {init}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                    tmp = tmp,
                    init = init,
                    guard = guard
                )));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let body = format!(
                "Arc::new(Mutex::new(({}).collect::<HashMap<_, _>>()))",
                iter_src.expr
            );
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "bytes" {
            if args.len() > 2 {
                return Err(self.error(expr.span, "bytes() expects up to two arguments"));
            }
            if args.is_empty() {
                return Ok(Some("Vec::<i64>::new()".to_string()));
            }
            if args.len() == 2 {
                self.uses.py_bytes_from_str = true;
                let s_expr = self.gen_expr(&args[0])?;
                let enc_expr = self.gen_expr(&args[1])?;
                return Ok(Some(self.wrap_result(format!(
                    "py_bytes_from_str(&{}, &{})",
                    s_expr, enc_expr
                ))));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            match args[0].ty.as_ref() {
                Some(Type::Bytes) => {
                    if let ExprKind::Name(name) = &args[0].kind {
                        if self.is_borrowed_param(name) {
                            return Ok(Some(format!("(*{}).clone()", arg_expr)));
                        }
                    }
                    return Ok(Some(format!("{}.clone()", arg_expr)));
                }
                Some(Type::Int) => {
                    self.uses.py_bytes_from_len = true;
                    return Ok(Some(
                        self.wrap_result(format!("py_bytes_from_len({})", arg_expr)),
                    ));
                }
                Some(Type::List(_))
                | Some(Type::Set(_))
                | Some(Type::Iterator(_))
                | Some(Type::Tuple(_)) => {
                    let iter_src = self.gen_iter_source(&args[0])?;
                    let body = format!(
                        "({}).map(|b| b as i64).collect::<Vec<i64>>()",
                        iter_src.expr
                    );
                    return Ok(Some(iter_src.wrap(body)));
                }
                Some(Type::Str) => {
                    return Err(self.error(expr.span, "bytes() expects encoding for str"));
                }
                _ => {
                    return Err(self.error(expr.span, "bytes() expects int or iterable of ints"));
                }
            }
        }
        if name == "enumerate" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "enumerate() expects one argument"));
            }
            // enumerate() consumes the iterator immediately, so use single-lock pattern
            let iter_expr =
                self.gen_iter_source_owned(&args[0], IterContext::ImmediateConsumption)?;
            return Ok(Some(format!(
                "({}).enumerate().map(|(i, v)| (i as i64, v))",
                iter_expr
            )));
        }
        if name == "zip" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "zip() expects two arguments"));
            }
            if let (ExprKind::Name(left), ExprKind::Name(right)) = (&args[0].kind, &args[1].kind) {
                if left == right && matches!(args[0].ty.as_ref(), Some(Type::List(_))) {
                    let iter_expr =
                        self.gen_iter_source_owned(&args[0], IterContext::DeferredCapture)?;
                    // Use a single list iterator to avoid double-locking the same list.
                    return Ok(Some(format!("({}).map(|x| (x.clone(), x))", iter_expr)));
                }
            }
            let left_iter = self.gen_iter_source_owned(&args[0], IterContext::DeferredCapture)?;
            let right_iter = self.gen_iter_source_owned(&args[1], IterContext::DeferredCapture)?;
            return Ok(Some(format!("({}).zip({})", left_iter, right_iter)));
        }
        if name == "map" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "map() expects two arguments"));
            }
            let iter_expr = self.gen_iter_source_owned(&args[1], IterContext::DeferredCapture)?;
            let (func_expr, inline_closure) = match &args[0].kind {
                ExprKind::Name(n) if n == "str" => ("|x| x.to_string()".to_string(), true),
                ExprKind::Lambda { .. } => (self.gen_expr(&args[0])?, true),
                _ => (self.gen_expr(&args[0])?, false),
            };
            if inline_closure {
                return Ok(Some(format!("({}).map({})", iter_expr, func_expr)));
            }
            // Use cleaner function call syntax: func(x) instead of (func)(x)
            let tmp = self.new_tmp();
            return Ok(Some(format!(
                "{{ let {} = {}; ({}).map(move |x| {}(x)) }}",
                tmp, func_expr, iter_expr, tmp
            )));
        }
        if name == "filter" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "filter() expects two arguments"));
            }
            let iter_expr = self.gen_iter_source_owned(&args[1], IterContext::DeferredCapture)?;
            let item_ty = args[1]
                .ty
                .as_ref()
                .and_then(|ty| self.iter_item_type_hint(ty));
            let item_is_copy = item_ty
                .as_ref()
                .map(|ty| self.is_copy_type(ty))
                .unwrap_or(false);

            // filter(None, iter) - truthiness filter
            if matches!(args[0].kind, ExprKind::Literal(Literal::None)) {
                let truthy = match item_ty.as_ref() {
                    Some(ty) => self.truthy_expr_for_type("x", ty),
                    None => "true".to_string(),
                };
                let bind = if item_is_copy {
                    "let x = *x;"
                } else {
                    "let x = x.clone();"
                };
                return Ok(Some(format!(
                    "({}).filter(|x| {{ {} {} }})",
                    iter_expr, bind, truthy
                )));
            }

            // filter(lambda, iter) - inline lambda directly
            if let ExprKind::Lambda { params, body } = &args[0].kind {
                if params.len() == 1 {
                    let param = &params[0];
                    let lambda_body = self.gen_expr(body)?;
                    let bind = if item_is_copy {
                        format!("let {} = *x;", param)
                    } else {
                        format!("let {} = x.clone();", param)
                    };
                    return Ok(Some(format!(
                        "({}).filter(|x| {{ {} {} }})",
                        iter_expr, bind, lambda_body
                    )));
                }
            }

            // filter(func, iter) - use predicate function directly
            let pred_expr = self.gen_expr(&args[0])?;
            let tmp = self.new_tmp();
            let bind = if item_is_copy {
                "let x = *x;"
            } else {
                "let x = x.clone();"
            };
            return Ok(Some(format!(
                "{{ let {} = {}; ({}).filter(move |x| {{ {} {}(x) }}) }}",
                tmp, pred_expr, iter_expr, bind, tmp
            )));
        }
        if name == "all" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "all() expects one argument"));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let item_ty = args[0]
                .ty
                .as_ref()
                .and_then(|ty| self.iter_item_type_hint(ty));
            let truthy = match item_ty.as_ref() {
                Some(ty) => self.truthy_expr_for_type("v", ty),
                None => "true".to_string(),
            };
            let item_is_copy = item_ty
                .as_ref()
                .map(|ty| self.is_copy_type(ty))
                .unwrap_or(false);
            // Only some borrowed containers yield references (e.g., &HashMap); sets yield owned.
            let yields_ref = matches!(
                args[0].ty.as_ref(),
                Some(Type::Ref(inner)) if !matches!(inner.as_ref(), Type::Set(_))
            );
            let body = if item_is_copy {
                if yields_ref {
                    format!("{}.all(|v| {{ let v = *v; {} }})", iter_src.expr, truthy)
                } else {
                    format!("{}.all(|v| {})", iter_src.expr, truthy)
                }
            } else {
                format!(
                    "{}.all(|v| {{ let v = v.clone(); {} }})",
                    iter_src.expr, truthy
                )
            };
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "any" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "any() expects one argument"));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let item_ty = args[0]
                .ty
                .as_ref()
                .and_then(|ty| self.iter_item_type_hint(ty));
            let truthy = match item_ty.as_ref() {
                Some(ty) => self.truthy_expr_for_type("v", ty),
                None => "true".to_string(),
            };
            let item_is_copy = item_ty
                .as_ref()
                .map(|ty| self.is_copy_type(ty))
                .unwrap_or(false);
            // Only some borrowed containers yield references (e.g., &HashMap); sets yield owned.
            let yields_ref = matches!(
                args[0].ty.as_ref(),
                Some(Type::Ref(inner)) if !matches!(inner.as_ref(), Type::Set(_))
            );
            let body = if item_is_copy {
                if yields_ref {
                    format!("{}.any(|v| {{ let v = *v; {} }})", iter_src.expr, truthy)
                } else {
                    format!("{}.any(|v| {})", iter_src.expr, truthy)
                }
            } else {
                format!(
                    "{}.any(|v| {{ let v = v.clone(); {} }})",
                    iter_src.expr, truthy
                )
            };
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "reversed" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "reversed() expects one argument"));
            }
            if let Some(Type::List(inner)) = args[0].ty.as_ref() {
                let arg_expr = self.gen_expr(&args[0])?;
                let body = if matches!(self.list_storage_for_expr(&args[0]), ListStorage::Local) {
                    let idx = self.new_tmp();
                    let list_ref = self.new_tmp();
                    let item_expr = if self.is_copy_type(inner) {
                        format!("{list}[{idx}]", list = list_ref, idx = idx)
                    } else {
                        format!("{list}[{idx}].clone()", list = list_ref, idx = idx)
                    };
                    // Index-based reverse iteration avoids cloning the full Vec.
                    format!(
                        "{{ let {list_ref} = &{list}; let mut {idx}: usize = {list_ref}.len(); std::iter::from_fn(move || {{ if {idx} == 0 {{ None }} else {{ {idx} -= 1; Some({item}) }} }}) }}",
                        idx = idx,
                        list = arg_expr,
                        list_ref = list_ref,
                        item = item_expr
                    )
                } else {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    // Lists need a bounded lock scope; collect in reverse, then iterate owned.
                    if self.is_copy_type(inner) {
                        format!(
                            "{{ let {tmp} = {expr}.clone(); let {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.iter().rev().copied().collect::<Vec<_>>().into_iter() }}",
                            tmp = tmp,
                            expr = arg_expr,
                            guard = guard
                        )
                    } else {
                        format!(
                            "{{ let {tmp} = {expr}.clone(); let {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.iter().rev().cloned().collect::<Vec<_>>().into_iter() }}",
                            tmp = tmp,
                            expr = arg_expr,
                            guard = guard
                        )
                    }
                };
                return Ok(Some(body));
            }
            let iter_expr = self.gen_iter_source_owned(&args[0], IterContext::DeferredCapture)?;
            return Ok(Some(format!("({}).rev()", iter_expr)));
        }
        if name == "max" {
            if args.is_empty() {
                return Err(self.error(expr.span, "max() expects at least one argument"));
            }
            if args.len() == 1 {
                self.uses.py_max = true;
                let iter_src = self.gen_iter_source(&args[0])?;
                let body = self.wrap_result(format!("py_max({})", iter_src.expr));
                return Ok(Some(iter_src.wrap(body)));
            }
            let use_float = args
                .iter()
                .any(|a| matches!(a.ty.as_ref(), Some(Type::Float)));
            let mut parts = Vec::new();
            for arg in args {
                parts.push(self.gen_numeric_operand(arg, use_float)?);
            }
            let mut expr_acc = parts[0].clone();
            for part in parts.iter().skip(1) {
                expr_acc = format!("{}.max({})", expr_acc, part);
            }
            return Ok(Some(expr_acc));
        }
        if name == "min" {
            if args.is_empty() {
                return Err(self.error(expr.span, "min() expects at least one argument"));
            }
            if args.len() == 1 {
                self.uses.py_min = true;
                let iter_src = self.gen_iter_source(&args[0])?;
                let body = self.wrap_result(format!("py_min({})", iter_src.expr));
                return Ok(Some(iter_src.wrap(body)));
            }
            let use_float = args
                .iter()
                .any(|a| matches!(a.ty.as_ref(), Some(Type::Float)));
            let mut parts = Vec::new();
            for arg in args {
                parts.push(self.gen_numeric_operand(arg, use_float)?);
            }
            let mut expr_acc = parts[0].clone();
            for part in parts.iter().skip(1) {
                expr_acc = format!("{}.min({})", expr_acc, part);
            }
            return Ok(Some(expr_acc));
        }
        if name == "abs" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "abs() expects one argument"));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Int) | Some(Type::Float) => format!("{}.abs()", arg_expr),
                Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                _ => format!("{}.abs()", arg_expr),
            }));
        }
        if name == "pow" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "pow() expects two arguments"));
            }
            let left = self.gen_numeric_operand(&args[0], true)?;
            let right = self.gen_numeric_operand(&args[1], true)?;
            return Ok(Some(format!("({}.powf({}))", left, right)));
        }
        if name == "sum" {
            if args.is_empty() || args.len() > 2 {
                return Err(self.error(expr.span, "sum() expects one or two arguments"));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let item_ty = args[0]
                .ty
                .as_ref()
                .and_then(|ty| self.iter_item_type_hint(ty));
            let item_is_float = matches!(item_ty.as_ref(), Some(Type::Float));
            let start_ty = if args.len() == 2 {
                args[1].ty.as_ref()
            } else {
                None
            };
            let use_float = item_is_float
                || matches!(start_ty, Some(Type::Float))
                || matches!(item_ty.as_ref(), Some(Type::Unknown));
            let start_expr = if args.len() == 2 {
                self.gen_numeric_operand(&args[1], use_float)?
            } else if use_float {
                "0.0f64".to_string()
            } else {
                "0i64".to_string()
            };
            let value_expr = if use_float {
                if item_is_float {
                    "v".to_string()
                } else {
                    "v as f64".to_string()
                }
            } else {
                "v".to_string()
            };
            let body = format!(
                "{}.fold({}, |acc, v| acc + {})",
                iter_src.expr, start_expr, value_expr
            );
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "int" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "int() expects zero or one argument"));
            }
            if args.is_empty() {
                return Ok(Some("0i64".to_string()));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Str) => {
                    self.uses.py_parse_int = true;
                    self.wrap_parse_result(format!("py_parse_int(&{})", arg_expr))
                }
                Some(Type::Float) => format!("{} as i64", arg_expr),
                Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                Some(Type::Int) => arg_expr,
                _ => {
                    self.uses.py_parse_int = true;
                    self.wrap_parse_result(format!("py_parse_int(&{}.to_string())", arg_expr))
                }
            }));
        }
        if name == "float" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "float() expects zero or one argument"));
            }
            if args.is_empty() {
                return Ok(Some("0.0f64".to_string()));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Str) => {
                    self.uses.py_parse_float = true;
                    self.wrap_parse_result(format!("py_parse_float(&{})", arg_expr))
                }
                Some(Type::Int) => format!("{} as f64", arg_expr),
                Some(Type::Bool) => format!("if {} {{ 1.0 }} else {{ 0.0 }}", arg_expr),
                Some(Type::Float) => arg_expr,
                _ => {
                    self.uses.py_parse_float = true;
                    self.wrap_parse_result(format!("py_parse_float(&{}.to_string())", arg_expr))
                }
            }));
        }
        if name == "bool" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "bool() expects zero or one argument"));
            }
            if args.is_empty() {
                return Ok(Some("false".to_string()));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            let ty = args[0].ty.as_ref().unwrap_or(&Type::Unknown);
            return Ok(Some(self.truthy_expr_for_type(&arg_expr, ty)));
        }
        if name == "chr" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "chr() expects one argument"));
            }
            self.uses.py_chr = true;
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(self.wrap_result(format!("py_chr({})", arg_expr))));
        }
        if name == "ord" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "ord() expects one argument"));
            }
            self.uses.py_ord = true;
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(self.wrap_result(format!("py_ord(&{})", arg_expr))));
        }
        if name == "hash" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "hash() expects one argument"));
            }
            if let Some(Type::Custom(class_name)) = args[0].ty.as_ref() {
                let arg_expr = self.gen_expr(&args[0])?;
                if let Some(info) = self.ctx.classes.get(class_name) {
                    if info.methods.contains_key("__hash__") {
                        return Ok(Some(format!("{}.__hash__()", arg_expr)));
                    }
                }
                return Ok(Some(format!("(&{} as *const _ as usize) as i64", arg_expr)));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Int) => arg_expr,
                Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                Some(Type::Str) => format!(
                    "{{ let mut _h: i64 = 0; for _b in {}.bytes() {{ _h = _h.wrapping_mul(31).wrapping_add(_b as i64); }} _h }}",
                    arg_expr
                ),
                Some(Type::None) => "1i64".to_string(),
                _ => arg_expr,
            }));
        }
        if name == "id" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "id() expects one argument"));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            if let ExprKind::Name(name) = &args[0].kind {
                if self.is_global(name) {
                    return Ok(Some(match args[0].ty.as_ref() {
                        Some(Type::Int) => arg_expr,
                        Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                        Some(Type::None) => "0i64".to_string(),
                        _ => format!(
                            "{{ let _guard = {}; (&*_guard as *const _ as usize) as i64 }}",
                            self.global_lock_expr(name)
                        ),
                    }));
                }
            }
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Int) => arg_expr,
                Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                Some(Type::None) => "0i64".to_string(),
                _ => format!("(&{} as *const _ as usize) as i64", arg_expr),
            }));
        }
        if name == "divmod" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "divmod() expects two arguments"));
            }
            let use_float = args
                .iter()
                .any(|a| matches!(a.ty.as_ref(), Some(Type::Float)));
            if use_float {
                let left = self.gen_numeric_operand(&args[0], true)?;
                let right = self.gen_numeric_operand(&args[1], true)?;
                return Ok(Some(format!(
                    "(({} / {}).floor(), ({} % {}))",
                    left, right, left, right
                )));
            }
            let left = self.gen_expr(&args[0])?;
            let right = self.gen_expr(&args[1])?;
            return Ok(Some(format!(
                "({} / {}, {} % {})",
                left, right, left, right
            )));
        }
        if name == "next" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "next() expects one argument"));
            }
            if let ExprKind::Call {
                func,
                args: range_args,
                keywords,
            } = &args[0].kind
            {
                if let ExprKind::Name(range_name) = &func.kind {
                    if range_name == "range"
                        && keywords.is_empty()
                        && (1..=3).contains(&range_args.len())
                    {
                        self.uses.py_error = true;
                        return Ok(Some(self.wrap_result(
                            "Err::<_, PyError>(PyError::TypeError(\"'range' object is not an iterator\".to_string()))"
                                .to_string(),
                        )));
                    }
                }
            }
            self.uses.py_next = true;
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(
                self.wrap_result(format!("py_next({}.next())", arg_expr)),
            ));
        }
        if name == "bin" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "bin() expects one argument"));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(format!(
                "{{ let n = {}; if n < 0 {{ format!(\"-0b{{:b}}\", -n) }} else {{ format!(\"0b{{:b}}\", n) }} }}",
                arg_expr
            )));
        }
        if name == "hex" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "hex() expects one argument"));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(format!(
                "{{ let n = {}; if n < 0 {{ format!(\"-0x{{:x}}\", -n) }} else {{ format!(\"0x{{:x}}\", n) }} }}",
                arg_expr
            )));
        }
        if name == "oct" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "oct() expects one argument"));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(format!(
                "{{ let n = {}; if n < 0 {{ format!(\"-0o{{:o}}\", -n) }} else {{ format!(\"0o{{:o}}\", n) }} }}",
                arg_expr
            )));
        }
        if name == "repr" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "repr() expects one argument"));
            }
            if let Some(Type::Custom(class_name)) = args[0].ty.as_ref() {
                let arg_expr = self.gen_expr(&args[0])?;
                if let Some(info) = self.ctx.classes.get(class_name) {
                    if info.methods.contains_key("__repr__") {
                        return Ok(Some(format!("{}.__repr__()", arg_expr)));
                    }
                }
                return Ok(Some(format!(
                    "format!(\"<{} instance at {{:p}}>\", &{})",
                    class_name, arg_expr
                )));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Int) => format!("format!(\"{{}}\", {})", arg_expr),
                Some(Type::Float) => {
                    self.uses.py_float_str = true;
                    format!("py_float_str({})", arg_expr)
                }
                Some(Type::Bool) => format!(
                    "if {} {{ \"True\".to_string() }} else {{ \"False\".to_string() }}",
                    arg_expr
                ),
                Some(Type::None) => "\"None\".to_string()".to_string(),
                Some(Type::Str) => {
                    self.uses.py_str_repr = true;
                    format!("py_str_repr(&{})", arg_expr)
                }
                Some(Type::List(_)) => self.list_str_expr(&args[0])?,
                Some(Type::Tuple(_)) => {
                    self.uses.py_list_str = true;
                    format!("{}.py_repr()", arg_expr)
                }
                _ => format!("format!(\"{{:?}}\", {})", arg_expr),
            }));
        }
        if name == "str" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "str() expects one argument"));
            }
            if let Some(Type::Custom(class_name)) = args[0].ty.as_ref() {
                let arg_expr = self.gen_expr(&args[0])?;
                if let Some(info) = self.ctx.classes.get(class_name) {
                    if info.methods.contains_key("__str__") {
                        return Ok(Some(format!("{}.__str__()", arg_expr)));
                    }
                }
                return Ok(Some(format!(
                    "format!(\"<{} instance at {{:p}}>\", &{})",
                    class_name, arg_expr
                )));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Option(inner)) => {
                    let opt_tmp = self.new_tmp();
                    let val_tmp = self.new_tmp();
                    let some_expr = match inner.as_ref() {
                        Type::Str => format!("{val}.clone()", val = val_tmp),
                        Type::Bool => format!(
                            "if {val} {{ \"True\".to_string() }} else {{ \"False\".to_string() }}",
                            val = val_tmp
                        ),
                        Type::Int => format!("{val}.to_string()", val = val_tmp),
                        Type::Float => {
                            self.uses.py_float_str = true;
                            format!("py_float_str({val})", val = val_tmp)
                        }
                        _ => format!("format!(\"{{:?}}\", {val})", val = val_tmp),
                    };
                    format!(
                        "{{ let {opt} = {arg}; match {opt} {{ Some({val}) => {some}, None => \"None\".to_string() }} }}",
                        opt = opt_tmp,
                        arg = arg_expr,
                        val = val_tmp,
                        some = some_expr
                    )
                }
                Some(Type::Str) => arg_expr,
                Some(Type::Bool) => format!(
                    "if {} {{ \"True\".to_string() }} else {{ \"False\".to_string() }}",
                    arg_expr
                ),
                Some(Type::None) => "\"None\".to_string()".to_string(),
                Some(Type::Int) => format!("{}.to_string()", arg_expr),
                Some(Type::Float) => {
                    self.uses.py_float_str = true;
                    format!("py_float_str({})", arg_expr)
                }
                Some(Type::List(_)) => self.list_str_expr(&args[0])?,
                Some(Type::Tuple(_)) => {
                    self.uses.py_list_str = true;
                    format!("{}.py_repr()", arg_expr)
                }
                _ => format!("format!(\"{{:?}}\", {})", arg_expr),
            }));
        }
        if name == "isinstance" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "isinstance() expects two arguments"));
            }
            if let ExprKind::Name(type_name) = &args[1].kind {
                let matches = match type_name.as_str() {
                    "int" => matches!(args[0].ty.as_ref(), Some(Type::Int) | Some(Type::Bool)),
                    "float" => matches!(args[0].ty.as_ref(), Some(Type::Float)),
                    "bool" => matches!(args[0].ty.as_ref(), Some(Type::Bool)),
                    "str" => matches!(args[0].ty.as_ref(), Some(Type::Str)),
                    "bytes" => matches!(args[0].ty.as_ref(), Some(Type::Bytes)),
                    _ => {
                        if self.ctx.classes.contains_key(type_name) {
                            if let Some(Type::Custom(class_name)) = args[0].ty.as_ref() {
                                self.is_subclass_of(class_name, type_name)
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                };
                return Ok(Some(matches.to_string()));
            }
            return Ok(Some("false".to_string()));
        }
        if name == "type" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "type() expects one argument"));
            }
            if let Some(ty) = args[0].ty.as_ref() {
                if let Some(class_str) = self.python_type_class(ty) {
                    return Ok(Some(format!("{:?}.to_string()", class_str)));
                }
            }
            self.uses.type_name = true;
            return Ok(Some(format!(
                "format!(\"<class '{{}}'>\", py_type_name(&{}))",
                self.gen_expr(&args[0])?
            )));
        }
        if name == "open" {
            if args.is_empty() || args.len() > 2 {
                return Err(self.error(expr.span, "open() expects one or two arguments"));
            }
            self.uses.py_file = true;
            let path_expr = self.gen_expr(&args[0])?;
            let mode_expr = if args.len() == 2 {
                self.gen_expr(&args[1])?
            } else {
                "\"r\".to_string()".to_string()
            };
            return Ok(Some(
                self.wrap_result(format!("py_open(&{}, &{})", path_expr, mode_expr)),
            ));
        }
        if name == "exit" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "exit() expects zero or one argument"));
            }
            if args.is_empty() {
                return Ok(Some("std::process::exit(0)".to_string()));
            }
            return Ok(Some(format!(
                "std::process::exit({} as i32)",
                self.gen_expr(&args[0])?
            )));
        }
        if self.ctx.classes.contains_key(name) {
            let call = if let Some(class_def) = self.class_defs.get(name) {
                if let Some(init_def) = class_def.methods.iter().find(|m| m.name == "__init__") {
                    let init_sig = self
                        .ctx
                        .classes
                        .get(name)
                        .and_then(|info| info.init.clone());
                    let param_types: Vec<Type> = init_sig
                        .map(|sig| sig.params.into_iter().skip(1).collect())
                        .unwrap_or_default();
                    let full_args = self.resolve_call_args(
                        args,
                        keywords,
                        &init_def.params[1..],
                        &param_types,
                        (Some(name), "__init__"),
                        false,
                    )?;
                    format!(
                        "{}::new({})",
                        name,
                        self.gen_call_args_for_sig(&param_types, &full_args)?
                    )
                } else {
                    if !args.is_empty() || !keywords.is_empty() {
                        return Err(
                            self.error(expr.span, format!("Class {name} takes no arguments"))
                        );
                    }
                    format!("{}::new()", name)
                }
            } else {
                if !keywords.is_empty() {
                    return Err(self.error(
                        expr.span,
                        "Keyword arguments require a known class signature",
                    ));
                }
                format!("{}::new({})", name, self.gen_args(args)?)
            };
            if let Some(Type::Union(union_name)) = expr.ty.as_ref() {
                return Ok(Some(format!("{}::{}({})", union_name, name, call)));
            }
            return Ok(Some(call));
        }
        Ok(None)
    }

    /// Lower attribute-based method calls with special cases for collections and format().
    fn gen_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if let Some(Type::Module(module_name)) = value.ty.as_ref() {
            let module_id = resolve_module(module_name.as_str()).ok_or_else(|| {
                self.error(
                    value.span,
                    format!("module '{module_name}' is not registered in stdlib registry"),
                )
            })?;
            let spec = resolve_method(module_id, attr).ok_or_else(|| {
                self.error(
                    value.span,
                    format!("{module_name} has no supported member '{attr}'"),
                )
            })?;
            return self.gen_stdlib_call(value.span, spec, args, keywords);
        }
        if attr == "upper" {
            if let Some(Type::Str) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(value.span, "str.upper() expects no arguments"));
                }
                // Rust's to_uppercase() matches Python's str.upper() semantics.
                return Ok(format!("{}.to_uppercase()", self.gen_expr(value)?));
            }
        }
        if attr == "lower" {
            if let Some(Type::Str) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(value.span, "str.lower() expects no arguments"));
                }
                // Rust's to_lowercase() matches Python's str.lower() semantics.
                return Ok(format!("{}.to_lowercase()", self.gen_expr(value)?));
            }
        }
        if attr == "startswith" {
            if let Some(Type::Str) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "str.startswith() expects one argument"));
                }
                return Ok(format!(
                    "{}.starts_with(&{})",
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?
                ));
            }
        }
        if attr == "find" {
            if let Some(Type::Str) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "str.find() expects one argument"));
                }
                return Ok(format!(
                    "{}.find(&{}).map(|i| i as i64).unwrap_or(-1)",
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?
                ));
            }
        }
        if let Some(Type::Custom(class_name)) = value.ty.as_ref() {
            if class_name == "__py2rust_file" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                self.uses.py_file = true;
                let file_expr = self.gen_expr(value)?;
                if attr == "read" {
                    if args.len() > 1 {
                        return Err(
                            self.error(value.span, "file.read() expects zero or one argument")
                        );
                    }
                    let read_size = if args.len() == 1 {
                        format!("Some({})", self.gen_expr(&args[0])?)
                    } else {
                        "None".to_string()
                    };
                    return Ok(self
                        .wrap_result(format!("py_file_read(&mut {}, {})", file_expr, read_size)));
                }
                if attr == "readline" {
                    if !args.is_empty() {
                        return Err(self.error(value.span, "file.readline() expects no arguments"));
                    }
                    return Ok(self.wrap_result(format!("py_file_readline(&mut {})", file_expr)));
                }
                if attr == "readlines" {
                    if !args.is_empty() {
                        return Err(self.error(value.span, "file.readlines() expects no arguments"));
                    }
                    let lines_expr =
                        self.wrap_result(format!("py_file_readlines(&mut {})", file_expr));
                    return Ok(format!("Arc::new(Mutex::new({}))", lines_expr));
                }
                if attr == "write" {
                    if args.len() != 1 {
                        return Err(self.error(value.span, "file.write() expects one argument"));
                    }
                    let data_expr = self.gen_expr(&args[0])?;
                    return Ok(self.wrap_result(format!(
                        "py_file_write(&mut {}, &{})",
                        file_expr, data_expr
                    )));
                }
                if attr == "close" {
                    if !args.is_empty() {
                        return Err(self.error(value.span, "file.close() expects no arguments"));
                    }
                    return Ok(self.wrap_result(format!("py_file_close(&mut {})", file_expr)));
                }
            }
        }
        if attr == "append" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.global_lock_expr(name)
                        )
                    } else if self.is_local_list_name(name) {
                        return Ok(format!("{}.push({})", name, self.gen_args(args)?));
                    } else {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.gen_expr(value)?
                        )
                    }
                } else {
                    format!(
                        "{}.lock().expect(\"list mutex poisoned\")",
                        self.gen_expr(value)?
                    )
                };
                return Ok(format!("{}.push({})", target, self.gen_args(args)?));
            }
        }
        if attr == "extend" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                let mut target = None;
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        target = Some(format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.global_lock_expr(name)
                        ));
                    } else if self.is_local_list_name(name) {
                        target = Some(name.clone());
                    }
                }
                let target = match target {
                    Some(expr) => expr,
                    None => format!(
                        "{}.lock().expect(\"list mutex poisoned\")",
                        self.gen_expr(value)?
                    ),
                };
                if args.is_empty() {
                    return Ok(format!("{{ {}.extend(std::iter::empty()); }}", target));
                }
                let arg = &args[0];
                // Avoid moving the source list/tuple by iterating and cloning elements.
                if matches!(arg.ty.as_ref(), Some(Type::Tuple(_))) {
                    let tuple_tmp = self.new_tmp();
                    let arg_expr = self.gen_expr(arg)?;
                    let mut elems = Vec::new();
                    if let Some(Type::Tuple(items)) = arg.ty.as_ref() {
                        for idx in 0..items.len() {
                            elems.push(format!("{}.{}.clone()", tuple_tmp, idx));
                        }
                    }
                    return Ok(format!(
                        "{{ let {} = {}; {}.extend(vec![{}]); }}",
                        tuple_tmp,
                        arg_expr,
                        target,
                        elems.join(", ")
                    ));
                }
                let arg_expr = self.gen_expr(arg)?;
                if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                    if matches!(self.list_storage_for_expr(arg), ListStorage::Local) {
                        return Ok(format!("{}.extend({}.iter().cloned())", target, arg_expr));
                    }
                    return Ok(format!(
                        "{}.extend({}.lock().expect(\"list mutex poisoned\").iter().cloned())",
                        target, arg_expr
                    ));
                }
                return Ok(format!("{}.extend({}.into_iter())", target, arg_expr));
            }
        }
        if attr == "pop" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() > 1 {
                    return Err(self.error(value.span, "list.pop() expects zero or one argument"));
                }
                let idx_arg = args.first();
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        if let Some(arg) = idx_arg {
                            let idx_raw = self.gen_expr(arg)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            return Ok(format!(
                                "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = {idx_expr}; {target}.remove({idx_tmp}) }}",
                                len_tmp = len_tmp,
                                idx_tmp = idx_tmp,
                                idx_expr = self.wrap_result(format!(
                                    "py_index({}, {})",
                                    idx_raw, len_tmp
                                )),
                                target = name
                            ));
                        }
                        let pop_expr = format!(
                            "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                            name
                        );
                        return Ok(self.wrap_result(pop_expr));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(arg) = idx_arg {
                            let idx_raw = self.gen_expr(arg)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                len_tmp = len_tmp,
                                idx_tmp = idx_tmp,
                                idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                            ));
                        }
                        let pop_expr = format!(
                            "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                            guard
                        );
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {pop} }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            pop = self.wrap_result(pop_expr),
                        ));
                    }
                }

                let target_expr = self.gen_expr(value)?;
                // For non-name targets, evaluate once into a mutable temporary.
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    if let Some(arg) = idx_arg {
                        let idx_raw = self.gen_expr(arg)?;
                        self.uses.py_index = true;
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                            tmp = tmp,
                            guard = guard,
                            target = target_expr,
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                        ));
                    }
                    let pop_expr = format!(
                        "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                        guard
                    );
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {pop} }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        pop = self.wrap_result(pop_expr),
                    ));
                }

                // Simple local name: emit direct mutation.
                if let Some(arg) = idx_arg {
                    let idx_raw = self.gen_expr(arg)?;
                    self.uses.py_index = true;
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {guard} = {target}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                        len_tmp = len_tmp,
                        target = target_expr,
                        idx_tmp = idx_tmp,
                        idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                        guard = guard,
                    ));
                }
                let pop_expr = format!(
                    "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                    "guard"
                );
                return Ok(self.wrap_result(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); {} }}",
                    target_expr, pop_expr
                )));
            }
        }
        if attr == "insert" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if args.len() != 2 {
                    return Err(self.error(value.span, "list.insert() expects two arguments"));
                }
                let idx_raw = self.gen_expr(&args[0])?;
                let val_expr = self.gen_expr_with_expected(&args[1], Some(inner.as_ref()))?;
                self.uses.py_insert_index = true;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {target}.insert({idx_tmp}, {val}); }}",
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_raw = idx_raw,
                            target = name,
                            val = val_expr
                        ));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_raw = idx_raw,
                            val = val_expr
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        len_tmp = len_tmp,
                        idx_tmp = idx_tmp,
                        idx_raw = idx_raw,
                        val = val_expr
                    ));
                }
                let len_tmp = self.new_tmp();
                let idx_tmp = self.new_tmp();
                return Ok(format!(
                    "{{ let mut guard = {target}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = guard.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); guard.insert({idx_tmp}, {val}); }}",
                    len_tmp = len_tmp,
                    idx_tmp = idx_tmp,
                    idx_raw = idx_raw,
                    target = target_expr,
                    val = val_expr
                ));
            }
        }
        if attr == "clear" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.clear() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.clear(); }}", name));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {guard}.clear(); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.clear(); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr
                    ));
                }
                return Ok(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); guard.clear(); }}",
                    target_expr
                ));
            }
        }
        if attr == "copy" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.copy() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return Ok(format!(
                            "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                            self.global_lock_expr(name)
                        ));
                    }
                    if self.is_local_list_name(name) {
                        return Ok(format!("Arc::new(Mutex::new({}.clone()))", name));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!(
                    "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                    target_expr
                ));
            }
        }
        if attr == "reverse" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.reverse() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.reverse(); }}", name));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {guard}.reverse(); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.reverse(); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr
                    ));
                }
                return Ok(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); guard.reverse(); }}",
                    target_expr
                ));
            }
        }
        if attr == "index" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "list.index() expects one argument"));
                }
                self.uses.py_list_index = true;
                let needle_expr = self.gen_expr(&args[0])?;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        let call = format!(
                            "py_list_index(&{target}, &{needle})",
                            target = name,
                            needle = needle_expr
                        );
                        return Ok(self.wrap_result(call));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        let call = format!(
                            "py_list_index(&{guard}, &{needle})",
                            guard = guard,
                            needle = needle_expr
                        );
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {result} }}",
                            outer = outer,
                            lock = self.global_lock_expr(name),
                            guard = guard,
                            result = self.wrap_result(call)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let call = format!(
                    "py_list_index(&{}.lock().expect(\"list mutex poisoned\"), &{})",
                    target_expr, needle_expr
                );
                return Ok(self.wrap_result(call));
            }
        }
        if attr == "sort" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.sort() expects no arguments"));
                }
                let sort_call = if matches!(inner.as_ref(), Type::Float) {
                    // `total_cmp` avoids panics for NaN while still providing deterministic order.
                    "sort_by(|a, b| a.total_cmp(b))"
                } else {
                    "sort()"
                };
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.{}; }}", name, sort_call));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {guard}.{sort_call}; }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            sort_call = sort_call
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.{sort_call}; }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        sort_call = sort_call
                    ));
                }
                return Ok(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); guard.{}; }}",
                    target_expr, sort_call
                ));
            }
        }
        if attr == "count" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "list.count() expects one argument"));
                }
                self.uses.py_list_count = true;
                let needle_expr = self.gen_expr(&args[0])?;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("py_list_count(&{}, &{})", name, needle_expr));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"list mutex poisoned\"); py_list_count(&{guard}, &{needle}) }}",
                            outer = outer,
                            lock = self.global_lock_expr(name),
                            guard = guard,
                            needle = needle_expr
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!(
                    "py_list_count(&{}.lock().expect(\"list mutex poisoned\"), &{})",
                    target_expr, needle_expr
                ));
            }
        }
        if attr == "get" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(value.span, "dict.get() expects one or two arguments"));
                }
                self.uses.hash_map = true;
                let key_expr = self.gen_expr(&args[0])?;
                let default_expr = if args.len() == 2 {
                    Some(self.gen_expr(&args[1])?)
                } else {
                    None
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    if let Some(default_expr) = default_expr {
                        return Ok(format!(
                            "{target}.get(&{key}).cloned().unwrap_or({default})",
                            target = target_expr,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    self.uses.py_dict_get = true;
                    return Ok(
                        self.wrap_result(format!("py_dict_get(&{}, &{})", target_expr, key_expr))
                    );
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Global dicts store an Arc<Mutex<...>> inside a global lock.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(default_expr) = default_expr {
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                key = key_expr,
                                default = default_expr
                            ));
                        }
                        self.uses.py_dict_get = true;
                        return Ok(self.wrap_result(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            key = key_expr
                        )));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if let Some(default_expr) = default_expr {
                    let guard = self.new_tmp();
                    if !matches!(value.kind, ExprKind::Name(_)) {
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                            tmp = tmp,
                            target = target_expr,
                            guard = guard,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    return Ok(format!(
                        "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                        guard = guard,
                        target = target_expr,
                        key = key_expr,
                        default = default_expr
                    ));
                }
                self.uses.py_dict_get = true;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard,
                        key = key_expr
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                    guard = guard,
                    target = target_expr,
                    key = key_expr
                )));
            }
        }
        if attr == "pop" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(value.span, "dict.pop() expects one or two arguments"));
                }
                self.uses.hash_map = true;
                let key_expr = self.gen_expr(&args[0])?;
                let default_expr = if args.len() == 2 {
                    Some(self.gen_expr(&args[1])?)
                } else {
                    None
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    if let Some(default_expr) = default_expr {
                        return Ok(format!(
                            "{target}.remove(&{key}).unwrap_or({default})",
                            target = target_expr,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    let pop_expr = format!(
                        "{target}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".to_string()))",
                        target = target_expr,
                        key = key_expr
                    );
                    return Ok(self.wrap_result(pop_expr));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Lock the inner dict before mutating.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(default_expr) = default_expr {
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.remove(&{key}).unwrap_or({default}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                key = key_expr,
                                default = default_expr
                            ));
                        }
                        let pop_expr = format!(
                            "{guard}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".to_string()))",
                            guard = guard,
                            key = key_expr
                        );
                        return Ok(self.wrap_result(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {pop} }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            pop = pop_expr
                        )));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if let Some(default_expr) = default_expr {
                    let guard = self.new_tmp();
                    if !matches!(value.kind, ExprKind::Name(_)) {
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.remove(&{key}).unwrap_or({default}) }}",
                            tmp = tmp,
                            target = target_expr,
                            guard = guard,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    return Ok(format!(
                        "{{ let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.remove(&{key}).unwrap_or({default}) }}",
                        guard = guard,
                        target = target_expr,
                        key = key_expr,
                        default = default_expr
                    ));
                }
                let pop_expr = format!(
                    "{guard}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".to_string()))",
                    guard = "{guard}",
                    key = key_expr
                );
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {pop} }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard,
                        pop = pop_expr.replace("{guard}", &guard)
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {pop} }}",
                    guard = guard,
                    target = target_expr,
                    pop = pop_expr.replace("{guard}", &guard)
                )));
            }
        }
        if attr == "clear" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "dict.clear() expects no arguments"));
                }
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    return Ok(format!("{}.clear()", target_expr));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Clear through the inner dict lock for globals.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.clear(); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.clear(); }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard
                    ));
                }
                return Ok(format!(
                    "{{ let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.clear(); }}",
                    guard = guard,
                    target = target_expr
                ));
            }
        }
        if attr == "copy" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "dict.copy() expects no arguments"));
                }
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    // HashMap::clone creates a new dict object.
                    return Ok(format!("{}.clone()", target_expr));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Copy the underlying HashMap so the result is a new dict object.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard
                    ));
                }
                return Ok(format!(
                    "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                    guard = guard,
                    target = target_expr
                ));
            }
        }
        if attr == "update" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "dict.update() expects one argument"));
                }
                self.uses.hash_map = true;
                let arg_expr = self.gen_expr(&args[0])?;
                // Snapshot key/value pairs to avoid holding two dict borrows/locks at once.
                let pairs_tmp = self.new_tmp();
                let pairs_expr = if matches!(
                    self.dict_storage_for_expr(&args[0]),
                    DictStorage::Local
                ) {
                    format!(
                        "{arg}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()",
                        arg = arg_expr
                    )
                } else {
                    let arg_tmp = self.new_tmp();
                    let arg_guard = self.new_tmp();
                    let arg_init = if matches!(args[0].kind, ExprKind::Name(_)) {
                        format!("{}.clone()", arg_expr)
                    } else {
                        arg_expr
                    };
                    format!(
                        "{{ let {arg_tmp} = {arg_init}; let {arg_guard} = {arg_tmp}.lock().expect(\"dict mutex poisoned\"); {arg_guard}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>() }}",
                        arg_tmp = arg_tmp,
                        arg_init = arg_init,
                        arg_guard = arg_guard
                    )
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    return Ok(format!(
                        "{{ let {pairs} = {pairs_expr}; {target}.extend({pairs}); }}",
                        pairs = pairs_tmp,
                        pairs_expr = pairs_expr,
                        target = target_expr
                    ));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {pairs} = {pairs_expr}; let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.extend({pairs}); }}",
                            pairs = pairs_tmp,
                            pairs_expr = pairs_expr,
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {pairs} = {pairs_expr}; let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.extend({pairs}); }}",
                        pairs = pairs_tmp,
                        pairs_expr = pairs_expr,
                        tmp = tmp,
                        target = target_expr,
                        guard = guard
                    ));
                }
                return Ok(format!(
                    "{{ let {pairs} = {pairs_expr}; let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.extend({pairs}); }}",
                    pairs = pairs_tmp,
                    pairs_expr = pairs_expr,
                    guard = guard,
                    target = target_expr
                ));
            }
        }
        if attr == "add" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.insert({})", target, self.gen_args(args)?));
            }
        }
        if attr == "remove" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.remove(&{})", target, self.gen_args(args)?));
            }
        }
        if attr == "discard" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!(
                    "{{ {}.remove(&{}); }}",
                    target,
                    self.gen_args(args)?
                ));
            }
        }
        if attr == "clear" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.clear() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; {guard}.clear(); }}",
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; {tmp}.clear(); }}",
                        tmp = tmp,
                        target = target_expr
                    ));
                }
                return Ok(format!("{}.clear()", target_expr));
            }
        }
        if attr == "copy" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.copy() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return Ok(format!("{}.clone()", self.global_lock_expr(name)));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!("{}.clone()", target_expr));
            }
        }
        if attr == "format" {
            if let ExprKind::Literal(Literal::Str(fmt)) = &value.kind {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                let fmt_lit = format!("{fmt:?}");
                if args.is_empty() {
                    return Ok(format!("{}.to_string()", fmt_lit));
                }
                // Track which replacement fields explicitly specify a format spec.
                // Float arguments without a spec should use Python's str(float) style.
                let mut placeholder_has_spec = Vec::new();
                let chars: Vec<char> = fmt.chars().collect();
                let mut i = 0usize;
                while i < chars.len() {
                    if chars[i] == '{' {
                        if i + 1 < chars.len() && chars[i + 1] == '{' {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        let mut has_spec = false;
                        while i < chars.len() && chars[i] != '}' {
                            if chars[i] == ':' {
                                has_spec = true;
                            }
                            i += 1;
                        }
                        placeholder_has_spec.push(has_spec);
                        if i < chars.len() {
                            i += 1;
                        }
                        continue;
                    }
                    if chars[i] == '}' && i + 1 < chars.len() && chars[i + 1] == '}' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                let mut vals = Vec::new();
                for (idx, arg) in args.iter().enumerate() {
                    if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                        vals.push(self.list_str_expr(arg)?);
                    } else if self.print_needs_debug(arg) {
                        let arg_expr = self.debug_arg_expr(arg)?;
                        vals.push(format!("format!(\"{{:?}}\", {})", arg_expr));
                    } else if matches!(arg.ty.as_ref(), Some(Type::Float))
                        && !placeholder_has_spec.get(idx).copied().unwrap_or(false)
                    {
                        self.uses.py_float_str = true;
                        vals.push(format!("py_float_str({})", self.gen_expr(arg)?));
                    } else {
                        vals.push(self.gen_expr(arg)?);
                    }
                }
                return Ok(format!("format!({}, {})", fmt_lit, vals.join(", ")));
            }
        }
        if let Some((class_name, is_class_value)) = match &value.kind {
            ExprKind::Name(name) if self.ctx.classes.contains_key(name) => {
                Some((name.as_str(), true))
            }
            _ => value.ty.as_ref().and_then(|ty| match ty {
                Type::Custom(name) => Some((name.as_str(), false)),
                _ => None,
            }),
        } {
            if let Some(class_info) = self.ctx.classes.get(class_name) {
                if let Some(sig) = class_info.methods.get(attr) {
                    let kind = class_info
                        .method_kinds
                        .get(attr)
                        .copied()
                        .unwrap_or(MethodKind::Instance);
                    let method_def =
                        self.method_def(class_name, attr).cloned().ok_or_else(|| {
                            self.error(value.span, format!("Unknown method {class_name}.{attr}"))
                        })?;
                    let mut call = match kind {
                        MethodKind::Instance => {
                            if is_class_value {
                                return Err(
                                    self.error(value.span, "Instance methods require an instance")
                                );
                            }
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .skip(1)
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params[1..],
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            if self.method_is_mutating(&method_def) {
                                if let ExprKind::Name(name) = &value.kind {
                                    if self.is_global(name) {
                                        let guard = self.new_tmp();
                                        return Ok(format!(
                                            "{{ let mut {guard} = {lock}; {guard}.{attr}({args}) }}",
                                            guard = guard,
                                            lock = self.global_lock_expr(name),
                                            attr = attr,
                                            args = call_args
                                        ));
                                    }
                                }
                            }
                            format!("{}.{}({})", self.gen_expr(value)?, attr, call_args)
                        }
                        MethodKind::Static => {
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params,
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            format!("{}::{}({})", class_name, attr, call_args)
                        }
                        MethodKind::Class => {
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params,
                                &param_types,
                                (Some(class_name), attr),
                                true,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            format!("{}::{}({})", class_name, attr, call_args)
                        }
                    };
                    if sig.can_throw {
                        call = format!("({}?)", call);
                    }
                    return Ok(call);
                }
            }
        }
        // Handle method calls on Union types by generating match expression.
        if let Some(Type::Union(union_name)) = value.ty.as_ref() {
            if let Some(union_info) = self.ctx.unions.get(union_name) {
                if !keywords.is_empty() {
                    return Err(self.error(
                        value.span,
                        "Keyword arguments are not supported for union method calls",
                    ));
                }
                // Get method signature from first variant to check if it can throw.
                let can_throw = union_info.variants.first().and_then(|v| {
                    self.ctx
                        .classes
                        .get(v)
                        .and_then(|c| c.methods.get(attr))
                        .map(|sig| sig.can_throw)
                });
                let value_expr = self.gen_expr(value)?;
                let args_str = self.gen_args(args)?;
                let mut arms = Vec::new();
                for variant in &union_info.variants {
                    arms.push(format!(
                        "{}::{}(ref _x) => _x.{}({})",
                        union_name, variant, attr, args_str
                    ));
                }
                let mut call = format!("match {} {{ {} }}", value_expr, arms.join(", "));
                if can_throw == Some(true) {
                    call = format!("({}?)", call);
                }
                return Ok(call);
            }
        }
        if !keywords.is_empty() {
            return Err(self.error(
                value.span,
                "Keyword arguments are not supported for this method call",
            ));
        }
        Ok(format!(
            "{}.{}({})",
            self.gen_expr(value)?,
            attr,
            self.gen_args(args)?
        ))
    }

    /// Emit a stdlib call resolved by registry metadata.
    fn gen_stdlib_call(
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
