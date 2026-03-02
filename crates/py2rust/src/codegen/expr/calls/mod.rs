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
                let call = if sig.can_throw {
                    format!("({}?)", call)
                } else {
                    call
                };
                // Fresh-return functions return Vec<T>; wrap with
                // Arc<Mutex<>> unless we're in a local-list context.
                if self.fresh_return_functions.contains(name) && !self.force_local_list_storage {
                    let inner_ret = sig
                        .ret
                        .unwrap_result()
                        .map(|(ok, _)| ok)
                        .unwrap_or(&sig.ret);
                    if matches!(inner_ret, Type::List(_)) {
                        return Ok(format!("Arc::new(Mutex::new({}))", call));
                    }
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

                // Look up lambda defaults keyed by the callable name.
                let lambda_name = if let ExprKind::Name(name) = &func.kind {
                    Some(name.clone())
                } else {
                    None
                };
                let stored_defaults = lambda_name
                    .as_deref()
                    .and_then(|name| self.lambda_defaults.get(name))
                    .cloned();

                for (idx, slot) in rendered_args.iter_mut().enumerate() {
                    if slot.is_some() {
                        continue;
                    }
                    if normalized_defaults.get(idx).copied().unwrap_or(false) {
                        // Try to fill in the default from stored lambda defaults.
                        let filled = stored_defaults
                            .as_ref()
                            .and_then(|defs| defs.get(idx))
                            .and_then(|d| d.as_ref());
                        if let Some(default_expr) = filled {
                            *slot = Some(self.gen_expr(default_expr)?);
                            continue;
                        }
                        return Err(self.error(
                            expr.span,
                            "Default arguments for nested callables are not supported yet",
                        ));
                    }
                    return Err(self.error(expr.span, "Argument count mismatch"));
                }

                let mut final_args: Vec<String> = rendered_args
                    .into_iter()
                    .map(|item| item.expect("filled above"))
                    .collect::<Vec<_>>();
                // Append extra `&mut` args for recursive nested function captures.
                if let ExprKind::Name(fn_name) = &func.kind {
                    if let Some(captures) = self.recursive_fn_captures.get(fn_name).cloned() {
                        for cap in &captures {
                            // Inside the inner function, captures are already &mut params —
                            // pass them directly to avoid double-referencing.
                            if self.already_mut_ref_captures.contains(cap.as_str()) {
                                final_args.push(cap.clone());
                            } else {
                                final_args.push(format!("&mut {}", cap));
                            }
                        }
                    }
                }
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
            // Append extra `&mut` args for recursive nested function captures.
            if let ExprKind::Name(fn_name) = &func.kind {
                if let Some(captures) = self.recursive_fn_captures.get(fn_name).cloned() {
                    for cap in &captures {
                        if self.already_mut_ref_captures.contains(cap.as_str()) {
                            rendered_args.push(cap.clone());
                        } else {
                            rendered_args.push(format!("&mut {}", cap));
                        }
                    }
                }
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
        let pre_lines: Vec<String> = Vec::new();
        let post_lines: Vec<String> = Vec::new();
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
                        // With uniform Arc<Mutex<>> storage, just clone the global's Arc directly.
                        let global_lock = self.global_lock_expr(&default_name);
                        rendered_args.push(format!("{global_lock}.clone()"));
                        continue;
                    }
                    Some(Type::Dict(_, _)) => {
                        // With uniform Arc<Mutex<>> storage, just clone the global's Arc directly.
                        let global_lock = self.global_lock_expr(&default_name);
                        rendered_args.push(format!("{global_lock}.clone()"));
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
