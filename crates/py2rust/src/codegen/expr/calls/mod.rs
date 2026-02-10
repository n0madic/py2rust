// Function and method call expression lowering.

mod args;
mod attrs;
mod builtins;
mod format;
mod stdlib;

use super::super::*;
use crate::call_bind::{plan_non_unpacking_bind, BoundArg};
use crate::stdlib::registry::{find_stdlib_method, resolve_module};

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
            let spec = find_stdlib_method(module_id, method.as_str()).ok_or_else(|| {
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
            return self.gen_attr_call(expr, value, attr, args, keywords);
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
                let def = self.function_defs.get(name).cloned();
                let full_args = if let Some(def) = def.as_ref() {
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
                let call = if let Some(def) = def.as_ref() {
                    self.gen_user_call_with_mutable_default_sync(
                        name,
                        &param_types,
                        &full_args,
                        def,
                    )?
                } else {
                    format!(
                        "{}({})",
                        name,
                        self.gen_call_args_for_sig(&param_types, &full_args)?
                    )
                };
                // Add ? operator if function can throw.
                if sig.can_throw {
                    return Ok(format!("({}?)", call));
                }
                return Ok(call);
            }
        }
        let callable_ty = match func.ty.clone() {
            Some(Type::Unknown) => {
                // CPython-compat note:
                // typecheck can later refine callable shape in scope without rewriting
                // every cached expression node. Prefer scope lookup over stale Unknown.
                if let ExprKind::Name(name) = &func.kind {
                    self.local_var_type(name)
                        .cloned()
                        .or_else(|| self.ctx.globals.get(name).cloned())
                } else {
                    Some(Type::Unknown)
                }
            }
            Some(ty) => Some(ty),
            None => {
                if let ExprKind::Name(name) = &func.kind {
                    self.local_var_type(name)
                        .cloned()
                        .or_else(|| self.ctx.globals.get(name).cloned())
                } else {
                    None
                }
            }
        };
        if let Some(Type::Lambda {
            param_names,
            params,
            param_kinds,
            has_defaults,
            ..
        }) = callable_ty.as_ref()
        {
            let arity = params.len();
            let normalized_names = if param_names.len() == arity {
                param_names.clone()
            } else {
                (0..arity)
                    .map(|idx| format!("arg{idx}"))
                    .collect::<Vec<_>>()
            };
            let normalized_kinds = if param_kinds.len() == arity {
                param_kinds.clone()
            } else {
                vec![ParamKind::PositionalOrKeyword; arity]
            };
            let normalized_defaults = if has_defaults.len() == arity {
                has_defaults.clone()
            } else {
                vec![false; arity]
            };
            let has_keyword_shape = param_names.len() == arity;
            let has_unpacking = args
                .iter()
                .any(|arg| matches!(arg.kind, ExprKind::Starred { .. }))
                || keywords.iter().any(|kw| kw.name.is_none());
            if has_unpacking {
                return self.gen_lambda_call_with_unpacking(
                    expr,
                    func,
                    args::LambdaUnpackCallMeta {
                        param_names: &normalized_names,
                        params,
                        param_kinds: &normalized_kinds,
                        has_defaults: &normalized_defaults,
                    },
                    args,
                    keywords,
                );
            }
            if !has_unpacking {
                if !has_keyword_shape && !keywords.is_empty() {
                    return Err(self.error(
                        expr.span,
                        "Keyword arguments are not supported for this call target",
                    ));
                }
                let keyword_names: Vec<Option<&str>> =
                    keywords.iter().map(|kw| kw.name.as_deref()).collect();
                let plan = plan_non_unpacking_bind(
                    &normalized_names,
                    &normalized_kinds,
                    &normalized_defaults,
                    args.len(),
                    &keyword_names,
                    false,
                )
                .map_err(|err| self.error(expr.span, err.message()))?;

                let mut rendered_args: Vec<Option<String>> = vec![None; params.len()];
                for (idx, maybe_source) in plan.bound.iter().copied().enumerate() {
                    let Some(source) = maybe_source else {
                        continue;
                    };
                    let param_ty = &params[idx];
                    let (arg_expr, arg_ty_ref): (&Expr, Option<&Type>) = match source {
                        BoundArg::Positional(pos_idx) => {
                            (&args[pos_idx], args[pos_idx].ty.as_ref())
                        }
                        BoundArg::Keyword(kw_idx) => {
                            (&keywords[kw_idx].value, keywords[kw_idx].value.ty.as_ref())
                        }
                    };
                    let mut rendered = self.gen_expr_with_expected(arg_expr, Some(param_ty))?;
                    if self.call_arg_needs_owned_clone(arg_expr, param_ty) {
                        rendered = format!("{}.clone()", rendered);
                    } else if self.needs_borrow(arg_ty_ref, param_ty) {
                        rendered = format!("&{}", rendered);
                    } else if matches!(param_ty, Type::Lambda { .. }) {
                        rendered = format!("Box::new({})", rendered);
                    }
                    rendered_args[idx] = Some(rendered);
                }

                if let Some(vararg_idx) = plan.vararg_idx {
                    let inner_ty = match params.get(vararg_idx) {
                        Some(Type::List(inner)) => inner.as_ref().clone(),
                        _ => Type::Unknown,
                    };
                    let mut elems = Vec::new();
                    for pos_idx in plan.vararg_positional {
                        if matches!(inner_ty, Type::Unknown) {
                            self.uses.py_repr = true;
                            let raw = self.gen_expr(&args[pos_idx])?;
                            elems.push(format!("PyRepr(format!(\"{{:?}}\", {}))", raw));
                        } else {
                            elems.push(
                                self.gen_expr_with_expected(&args[pos_idx], Some(&inner_ty))?,
                            );
                        }
                    }
                    let vec_expr = if elems.is_empty() {
                        if matches!(inner_ty, Type::Unknown) {
                            "Vec::<PyRepr>::new()".to_string()
                        } else {
                            format!("Vec::<{}>::new()", self.rust_type(&inner_ty))
                        }
                    } else {
                        format!("vec![{}]", elems.join(", "))
                    };
                    if matches!(inner_ty, Type::Unknown) {
                        self.uses.py_repr = true;
                    }
                    rendered_args[vararg_idx] =
                        Some(self.wrap_list_storage_expr(&vec_expr, ListStorage::SharedCell));
                } else if !plan.vararg_positional.is_empty() {
                    return Err(self.error(expr.span, "Argument count mismatch"));
                }

                if let Some(varkw_idx) = plan.varkw_idx {
                    let value_ty = match params.get(varkw_idx) {
                        Some(Type::Dict(_, value_ty)) => value_ty.as_ref().clone(),
                        _ => Type::Unknown,
                    };
                    self.uses.index_map = true;
                    let mut entries = Vec::new();
                    for kw_idx in plan.varkw_keywords {
                        let kw_name = keywords[kw_idx].name.as_deref().ok_or_else(|| {
                            self.error(
                                expr.span,
                                "Call-site **kwargs unpacking is not supported for this callable",
                            )
                        })?;
                        let value_expr =
                            self.gen_expr_with_expected(&keywords[kw_idx].value, Some(&value_ty))?;
                        entries.push(format!(
                            "\"{kw_name}\".to_string(), {value_expr}",
                            kw_name = kw_name,
                            value_expr = value_expr
                        ));
                    }
                    let dict_expr = if entries.is_empty() {
                        "IndexMap::new()".to_string()
                    } else {
                        format!("IndexMap::from([({})])", entries.join("), ("))
                    };
                    rendered_args[varkw_idx] =
                        Some(self.wrap_dict_storage_expr(&dict_expr, DictStorage::SharedCell));
                } else if !plan.varkw_keywords.is_empty() {
                    return Err(self.error(expr.span, "Unknown keyword argument"));
                }

                for (idx, slot) in rendered_args.iter_mut().enumerate() {
                    if slot.is_some() {
                        continue;
                    }
                    if normalized_defaults.get(idx).copied().unwrap_or(false) {
                        return Err(self.error(
                            expr.span,
                            "Default arguments for nested callables are not supported yet",
                        ));
                    }
                    return Err(self.error(expr.span, "Argument count mismatch"));
                }

                let final_args = rendered_args
                    .into_iter()
                    .map(|item| item.expect("filled above"))
                    .collect::<Vec<_>>();
                return Ok(format!(
                    "({})({})",
                    self.gen_expr(func)?,
                    final_args.join(", ")
                ));
            }
            if !keywords.is_empty() {
                return Err(self.error(
                    expr.span,
                    "Keyword arguments are not supported for this call target",
                ));
            }
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
                    if self.call_arg_needs_owned_clone(arg, param_ty) {
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
                "({})({})",
                self.gen_expr(func)?,
                rendered_args.join(", ")
            ));
        }
        if matches!(callable_ty, Some(Type::Unknown)) {
            let has_unpacking = args
                .iter()
                .any(|arg| matches!(arg.kind, ExprKind::Starred { .. }))
                || keywords.iter().any(|kw| kw.name.is_none());
            if has_unpacking || !keywords.is_empty() {
                // CPython-compat divergence:
                // CPython supports forwarding dynamic call targets with *args/**kwargs.
                // Our current Rust backend cannot represent this call protocol for an
                // unknown callable shape, so we emit a runtime placeholder instead of
                // failing compilation. This keeps transpilation progressing for code
                // paths that are not executed.
                return Ok(
                    "unimplemented!(\"dynamic callable unpacking is not supported yet\")"
                        .to_string(),
                );
            }
        }
        if !keywords.is_empty() {
            return Err(self.error(
                expr.span,
                "Keyword arguments are not supported for this call target",
            ));
        }
        Ok(format!(
            "({})({})",
            self.gen_expr(func)?,
            self.gen_args(args)?
        ))
    }

    /// Emit a user-function call while preserving mutable default list/dict semantics.
    ///
    /// For omitted mutable defaults, defaults live in global sync storage
    /// (`Arc<Mutex<...>>`) but function parameters are list/dict locals
    /// (`Rc<RefCell<...>>`). We bridge by cloning into a temporary local arg,
    /// executing the call, then writing back the mutated contents.
    fn gen_user_call_with_mutable_default_sync(
        &mut self,
        func_name: &str,
        param_types: &[Type],
        full_args: &[Expr],
        def: &Function,
    ) -> Result<String, CompileError> {
        let mut pre_lines = Vec::new();
        let mut post_lines = Vec::new();
        let mut rendered_args = Vec::new();

        for (idx, arg) in full_args.iter().enumerate() {
            let param_ty = param_types.get(idx);
            let default_global = def.params.get(idx).and_then(|param| {
                param.default.as_ref()?;
                let ExprKind::Name(name) = &arg.kind else {
                    return None;
                };
                let expected = self.default_global_name(None, func_name, param.name.as_str());
                if *name == expected {
                    Some(name.clone())
                } else {
                    None
                }
            });

            if let Some(default_name) = default_global {
                match param_ty {
                    Some(Type::List(_)) => {
                        let arg_tmp = self.new_tmp();
                        let cache_name = self.default_cache_name(&default_name);
                        let src_tmp = self.new_tmp();
                        let src_guard = self.new_tmp();
                        let dst_guard = self.new_tmp();
                        let global_lock = self.global_lock_expr(&default_name);
                        pre_lines.push(format!(
                            "let {arg_tmp} = {cache_name}.with(|slot| {{ let mut slot = slot.borrow_mut(); if slot.is_none() {{ let {src_tmp} = {global_lock}.clone(); let {src_guard} = {src_tmp}.py_list_guard(); *slot = Some(Rc::new(RefCell::new({src_guard}.clone()))); }} slot.as_ref().expect(\"default list cache initialized\").clone() }});",
                            arg_tmp = arg_tmp,
                            cache_name = cache_name,
                            src_tmp = src_tmp,
                            global_lock = global_lock,
                            src_guard = src_guard
                        ));
                        post_lines.push(format!(
                            "*{global_lock} = {{ let {dst_guard} = {arg_tmp}.py_list_guard(); Arc::new(Mutex::new({dst_guard}.clone())) }};",
                            global_lock = global_lock,
                            dst_guard = dst_guard,
                            arg_tmp = arg_tmp
                        ));
                        rendered_args.push(format!("{arg_tmp}.clone()", arg_tmp = arg_tmp));
                        continue;
                    }
                    Some(Type::Dict(_, _)) => {
                        let arg_tmp = self.new_tmp();
                        let cache_name = self.default_cache_name(&default_name);
                        let src_tmp = self.new_tmp();
                        let src_guard = self.new_tmp();
                        let dst_guard = self.new_tmp();
                        let global_lock = self.global_lock_expr(&default_name);
                        pre_lines.push(format!(
                            "let {arg_tmp} = {cache_name}.with(|slot| {{ let mut slot = slot.borrow_mut(); if slot.is_none() {{ let {src_tmp} = {global_lock}.clone(); let {src_guard} = {src_tmp}.py_dict_guard(); *slot = Some(Rc::new(RefCell::new({src_guard}.clone()))); }} slot.as_ref().expect(\"default dict cache initialized\").clone() }});",
                            arg_tmp = arg_tmp,
                            cache_name = cache_name,
                            src_tmp = src_tmp,
                            global_lock = global_lock,
                            src_guard = src_guard
                        ));
                        post_lines.push(format!(
                            "*{global_lock} = {{ let {dst_guard} = {arg_tmp}.py_dict_guard(); Arc::new(Mutex::new({dst_guard}.clone())) }};",
                            global_lock = global_lock,
                            dst_guard = dst_guard,
                            arg_tmp = arg_tmp
                        ));
                        rendered_args.push(format!("{arg_tmp}.clone()", arg_tmp = arg_tmp));
                        continue;
                    }
                    _ => {}
                }
            }

            let mut rendered = if let Some(param_ty) = param_ty {
                self.gen_expr_with_expected(arg, Some(param_ty))?
            } else {
                self.gen_expr(arg)?
            };
            if let Some(param_ty) = param_ty {
                if self.call_arg_needs_owned_clone(arg, param_ty) {
                    rendered = format!("{}.clone()", rendered);
                } else if self.needs_borrow(arg.ty.as_ref(), param_ty) {
                    rendered = format!("&{}", rendered);
                }
            }
            rendered_args.push(rendered);
        }

        let base_call = format!("{func_name}({})", rendered_args.join(", "));
        if pre_lines.is_empty() {
            return Ok(base_call);
        }
        let result_tmp = self.new_tmp();
        let mut wrapped = String::from("{ ");
        for line in pre_lines {
            wrapped.push_str(&line);
            wrapped.push(' ');
        }
        wrapped.push_str(&format!("let {result_tmp} = {base_call}; "));
        for line in post_lines {
            wrapped.push_str(&line);
            wrapped.push(' ');
        }
        wrapped.push_str(&format!("{result_tmp} }}"));
        Ok(wrapped)
    }
}
