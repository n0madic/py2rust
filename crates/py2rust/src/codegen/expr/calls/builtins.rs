// Builtin function call lowering.

use super::super::*;
use crate::builtin::registry::find_builtin;
use crate::callspec::validate_call_shape;

impl<'a> Codegen<'a> {
    /// Try to lower a builtin call; return Some(expr) if handled.
    pub(super) fn gen_builtin_call(
        &mut self,
        expr: &Expr,
        name: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<Option<String>, CompileError> {
        let builtin_spec = find_builtin(name);
        if let Some(spec) = builtin_spec {
            let callable = format!("{name}()");
            let keyword_names: Vec<Option<&str>> =
                keywords.iter().map(|kw| kw.name.as_deref()).collect();
            if let Err(shape_err) =
                validate_call_shape(&callable, spec.shape, args.len(), &keyword_names)
            {
                return Err(self.error(expr.span, shape_err.message()));
            }
        }
        if name == "print" {
            self.uses.print = true;
            let mut sep_kw: Option<&Expr> = None;
            let mut end_kw: Option<&Expr> = None;
            for kw in keywords {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        expr.span,
                        "Call-site **kwargs unpacking is not supported for print()",
                    ));
                };
                if kw_name == "sep" {
                    if sep_kw.is_some() {
                        return Err(
                            self.error(expr.span, "Multiple values for keyword argument `sep`")
                        );
                    }
                    sep_kw = Some(&kw.value);
                    continue;
                }
                if kw_name == "end" {
                    if end_kw.is_some() {
                        return Err(
                            self.error(expr.span, "Multiple values for keyword argument `end`")
                        );
                    }
                    end_kw = Some(&kw.value);
                    continue;
                }
                return Err(self.error(
                    expr.span,
                    format!("Unknown keyword argument `{kw_name}` for print()"),
                ));
            }

            // Render each argument to String for join-based print paths.
            let mut render_arg = |arg: &Expr| -> Result<String, CompileError> {
                if matches!(arg.ty.as_ref(), Some(Type::None)) {
                    return Ok("\"None\".to_string()".to_string());
                }
                if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                    return self.list_str_expr(arg);
                }
                let is_dict_ctor_call = matches!(
                    &arg.kind,
                    ExprKind::Call { func, .. }
                        if matches!(func.kind, ExprKind::Name(ref n) if n == "dict")
                );
                if matches!(arg.ty.as_ref(), Some(Type::Dict(_, _)))
                    || matches!(arg.kind, ExprKind::Dict(_))
                    || is_dict_ctor_call
                {
                    self.uses.index_map = true;
                    let dict_expr = self.gen_expr(arg)?;
                    let is_local = matches!(self.dict_storage_for_expr(arg), DictStorage::Local);
                    let needs_concrete_dict = match arg.ty.as_ref() {
                        Some(Type::Dict(key_ty, val_ty)) => {
                            matches!(key_ty.as_ref(), Type::Unknown)
                                || matches!(val_ty.as_ref(), Type::Unknown)
                        }
                        _ => true,
                    };
                    if needs_concrete_dict {
                        // Empty/unknown dict expressions need concrete K/V to satisfy Rust inference.
                        self.uses.py_repr = true;
                        let tmp = self.new_tmp();
                        if is_local {
                            return Ok(format!(
                                "{{ let {tmp}: IndexMap<PyRepr, PyRepr> = {dict_expr}; format!(\"{{:?}}\", {tmp}) }}",
                                tmp = tmp,
                                dict_expr = dict_expr
                            ));
                        }
                        return Ok(format!(
                            "{{ let {tmp}: Arc<Mutex<IndexMap<PyRepr, PyRepr>>> = {dict_expr}; format!(\"{{:?}}\", {tmp}.lock().expect(\"dict mutex poisoned\")) }}",
                            tmp = tmp,
                            dict_expr = dict_expr
                        ));
                    }
                    if is_local {
                        return Ok(format!("format!(\"{{:?}}\", {})", dict_expr));
                    }
                    return Ok(format!(
                        "format!(\"{{:?}}\", {}.lock().expect(\"dict mutex poisoned\"))",
                        dict_expr
                    ));
                }
                if self.print_needs_debug(arg) {
                    return Ok(format!(
                        "format!(\"{{:?}}\", {})",
                        self.debug_arg_expr(arg)?
                    ));
                }
                Ok(format!("format!(\"{{}}\", {})", self.gen_expr(arg)?))
            };

            let rendered = if args.is_empty() {
                "\"\"".to_string()
            } else if args.len() == 1 && sep_kw.is_none() && end_kw.is_none() {
                // Fast-path: single-argument print without sep/end doesn't need
                // intermediate Vec<String> + join allocation or format! wrapping.
                let arg = &args[0];
                let simple_display = matches!(
                    arg.ty.as_ref(),
                    Some(Type::Int | Type::Float | Type::Bool | Type::Str)
                );
                if matches!(arg.ty.as_ref(), Some(Type::None))
                    || matches!(arg.ty.as_ref(), Some(Type::List(_)))
                    || matches!(arg.ty.as_ref(), Some(Type::Dict(_, _)))
                    || matches!(arg.kind, ExprKind::Dict(_))
                {
                    render_arg(arg)?
                } else if simple_display {
                    if let ExprKind::Literal(Literal::Str(s)) = &arg.kind {
                        // Print plain string literals without allocating a temporary String.
                        format!("{s:?}")
                    } else {
                        self.gen_expr(arg)?
                    }
                } else if let ExprKind::Literal(Literal::Str(s)) = &arg.kind {
                    // Print plain string literals without allocating a temporary String.
                    format!("{s:?}")
                } else {
                    render_arg(arg)?
                }
            } else if let Some(sep_expr) = sep_kw {
                let parts: Result<Vec<String>, CompileError> =
                    args.iter().map(&mut render_arg).collect();
                let sep_code = self.gen_expr(sep_expr)?;
                format!("vec![{}].join(&{})", parts?.join(", "), sep_code)
            } else {
                let parts: Result<Vec<String>, CompileError> =
                    args.iter().map(&mut render_arg).collect();
                format!("vec![{}].join(\" \")", parts?.join(", "))
            };

            if let Some(end_expr) = end_kw {
                let end_code = self.gen_expr(end_expr)?;
                return Ok(Some(format!(
                    "print!(\"{{}}{{}}\", {}, {})",
                    rendered, end_code
                )));
            }
            return Ok(Some(format!("py_print(&({}))", rendered)));
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
            self.uses.index_map = true;
            if args.is_empty() {
                return Ok(Some("Arc::new(Mutex::new(IndexMap::new()))".to_string()));
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
                "Arc::new(Mutex::new(({}).collect::<IndexMap<_, _>>()))",
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
            if args.is_empty() || args.len() > 2 {
                return Err(self.error(expr.span, "enumerate() expects one or two arguments"));
            }
            // enumerate() consumes the iterator immediately, so use single-lock pattern
            let iter_expr =
                self.gen_iter_source_owned(&args[0], IterContext::ImmediateConsumption)?;
            let start_expr = if args.len() == 2 {
                self.gen_expr(&args[1])?
            } else {
                "0i64".to_string()
            };
            return Ok(Some(format!(
                "({}).enumerate().map(|(i, v)| (i as i64 + {}, v))",
                iter_expr, start_expr
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
            // Only some borrowed containers yield references (e.g., &IndexMap); sets yield owned.
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
            // Only some borrowed containers yield references (e.g., &IndexMap); sets yield owned.
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
            if let ExprKind::Call {
                func,
                args: range_args,
                keywords: range_keywords,
            } = &args[0].kind
            {
                if let ExprKind::Name(range_name) = &func.kind {
                    if range_name == "range" && range_keywords.is_empty() {
                        if range_args.len() == 1 {
                            let end = self.gen_expr(&range_args[0])?;
                            return Ok(Some(format!("(py_range({})).rev()", end)));
                        }
                        if range_args.len() == 2 {
                            let start = self.gen_expr(&range_args[0])?;
                            let end = self.gen_expr(&range_args[1])?;
                            return Ok(Some(format!("(py_range2({}, {})).rev()", start, end)));
                        }
                        if range_args.len() == 3 {
                            let start = self.gen_expr(&range_args[0])?;
                            let end = self.gen_expr(&range_args[1])?;
                            let step = self.gen_expr(&range_args[2])?;
                            let range_expr = self
                                .wrap_result(format!("py_range3({}, {}, {})", start, end, step));
                            return Ok(Some(format!(
                                "({}).collect::<Vec<_>>().into_iter().rev()",
                                range_expr
                            )));
                        }
                    }
                }
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
        if name == "sorted" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "sorted() expects one positional argument"));
            }
            let mut key_kw: Option<&Expr> = None;
            let mut reverse_kw: Option<&Expr> = None;
            for kw in keywords {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        expr.span,
                        "Call-site **kwargs unpacking is not supported for sorted()",
                    ));
                };
                match kw_name {
                    "key" => {
                        if key_kw.is_some() {
                            return Err(
                                self.error(expr.span, "Multiple values for keyword argument `key`")
                            );
                        }
                        key_kw = Some(&kw.value);
                    }
                    "reverse" => {
                        if reverse_kw.is_some() {
                            return Err(self.error(
                                expr.span,
                                "Multiple values for keyword argument `reverse`",
                            ));
                        }
                        reverse_kw = Some(&kw.value);
                    }
                    _ => {
                        return Err(self.error(
                            expr.span,
                            format!("Unknown keyword argument `{kw_name}` for sorted()"),
                        ));
                    }
                }
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            let reverse_expr = if let Some(reverse) = reverse_kw {
                self.gen_expr(reverse)?
            } else {
                "false".to_string()
            };
            let buf = self.new_tmp();
            if let Some(key_expr) = key_kw {
                let key_fn = self.gen_expr(key_expr)?;
                let body = format!(
                    "{{ let mut {buf} = ({iter}).map(|item| (({key})(item.clone()), item)).collect::<Vec<_>>(); {buf}.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if {reverse} {{ {buf}.reverse(); }} Arc::new(Mutex::new({buf}.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) }}",
                    buf = buf,
                    iter = iter_src.expr,
                    key = key_fn,
                    reverse = reverse_expr
                );
                return Ok(Some(iter_src.wrap(body)));
            }
            let body = format!(
                "{{ let mut {buf} = ({iter}).collect::<Vec<_>>(); {buf}.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if {reverse} {{ {buf}.reverse(); }} Arc::new(Mutex::new({buf})) }}",
                buf = buf,
                iter = iter_src.expr,
                reverse = reverse_expr
            );
            return Ok(Some(iter_src.wrap(body)));
        }
        if name == "max" {
            if args.is_empty() {
                return Err(self.error(expr.span, "max() expects at least one argument"));
            }
            let mut key_kw: Option<&Expr> = None;
            for kw in keywords {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        expr.span,
                        "Call-site **kwargs unpacking is not supported for max()",
                    ));
                };
                if kw_name != "key" {
                    return Err(self.error(
                        expr.span,
                        format!("Unknown keyword argument `{kw_name}` for max()"),
                    ));
                }
                if key_kw.is_some() {
                    return Err(self.error(expr.span, "Multiple values for keyword argument `key`"));
                }
                key_kw = Some(&kw.value);
            }
            if args.len() == 1 {
                let is_empty_tuple_iter = matches!(&args[0].kind, ExprKind::Tuple(items) if items.is_empty())
                    || matches!(args[0].ty.as_ref(), Some(Type::Tuple(items)) if items.is_empty());
                if is_empty_tuple_iter {
                    let ok_ty = expr
                        .ty
                        .as_ref()
                        .map(|ty| self.rust_type(ty))
                        .unwrap_or_else(|| "()".to_string());
                    let body = self.wrap_result(
                        format!(
                            "Err::<{}, PyError>(PyError::ValueError(\"max() arg is an empty sequence\".into()))",
                            ok_ty
                        ),
                    );
                    return Ok(Some(body));
                }
                let iter_src = self.gen_iter_source(&args[0])?;
                if let Some(key_expr) = key_kw {
                    let key_fn = self.gen_expr(key_expr)?;
                    let key_returns_result = matches!(
                        key_expr.ty.as_ref(),
                        Some(Type::Lambda { ret, .. }) if matches!(ret.as_ref(), Type::Result(_, _))
                    );
                    let item_is_copy = args[0]
                        .ty
                        .as_ref()
                        .and_then(|ty| self.iter_item_type_hint(ty))
                        .map(|ty| self.is_copy_type(&ty))
                        .unwrap_or(false);
                    let iter_name = self.new_tmp();
                    let best_name = self.new_tmp();
                    let best_key_name = self.new_tmp();
                    let item_name = self.new_tmp();
                    let item_key_name = self.new_tmp();
                    let best_key_arg = if item_is_copy {
                        best_name.clone()
                    } else {
                        format!("{best_name}.clone()")
                    };
                    let item_key_arg = if item_is_copy {
                        item_name.clone()
                    } else {
                        format!("{item_name}.clone()")
                    };
                    let best_key_expr = if key_returns_result {
                        self.wrap_result(format!("({key_fn})({best_key_arg})"))
                    } else {
                        format!("({key_fn})({best_key_arg})")
                    };
                    let item_key_expr = if key_returns_result {
                        self.wrap_result(format!("({key_fn})({item_key_arg})"))
                    } else {
                        format!("({key_fn})({item_key_arg})")
                    };
                    // Iterator expressions built from iterable literals may borrow temporaries
                    // (for example, vec![...].iter().cloned()). Buffer those defensively.
                    let iter_expr = if matches!(
                        args[0].kind,
                        ExprKind::List(_) | ExprKind::Tuple(_) | ExprKind::Set(_)
                    ) {
                        format!("({}).collect::<Vec<_>>().into_iter()", iter_src.expr)
                    } else {
                        iter_src.expr.clone()
                    };
                    let body = self.wrap_result(format!(
                        "{{ let mut {iter_name} = {iter}; match {iter_name}.next() {{ Some(first_item) => {{ let mut {best_name} = first_item; let mut {best_key_name} = {best_key_expr}; for {item_name} in {iter_name} {{ let {item_key_name} = {item_key_expr}; if {item_key_name} > {best_key_name} {{ {best_name} = {item_name}; {best_key_name} = {item_key_name}; }} }} Ok({best_name}) }}, None => Err(PyError::ValueError(\"max() arg is an empty sequence\".into())) }} }}",
                        iter_name = iter_name,
                        iter = iter_expr,
                        best_name = best_name,
                        best_key_name = best_key_name,
                        item_name = item_name,
                        item_key_name = item_key_name,
                        best_key_expr = best_key_expr,
                        item_key_expr = item_key_expr,
                    ));
                    return Ok(Some(iter_src.wrap(body)));
                }
                self.uses.py_max = true;
                let body = self.wrap_result(format!("py_max({})", iter_src.expr));
                return Ok(Some(iter_src.wrap(body)));
            }
            if key_kw.is_some() {
                return Err(self.error(
                    expr.span,
                    "max() with key= currently supports only iterable form",
                ));
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
            let mut key_kw: Option<&Expr> = None;
            for kw in keywords {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        expr.span,
                        "Call-site **kwargs unpacking is not supported for min()",
                    ));
                };
                if kw_name != "key" {
                    return Err(self.error(
                        expr.span,
                        format!("Unknown keyword argument `{kw_name}` for min()"),
                    ));
                }
                if key_kw.is_some() {
                    return Err(self.error(expr.span, "Multiple values for keyword argument `key`"));
                }
                key_kw = Some(&kw.value);
            }
            if args.len() == 1 {
                let is_empty_tuple_iter = matches!(&args[0].kind, ExprKind::Tuple(items) if items.is_empty())
                    || matches!(args[0].ty.as_ref(), Some(Type::Tuple(items)) if items.is_empty());
                if is_empty_tuple_iter {
                    let ok_ty = expr
                        .ty
                        .as_ref()
                        .map(|ty| self.rust_type(ty))
                        .unwrap_or_else(|| "()".to_string());
                    let body = self.wrap_result(
                        format!(
                            "Err::<{}, PyError>(PyError::ValueError(\"min() arg is an empty sequence\".into()))",
                            ok_ty
                        ),
                    );
                    return Ok(Some(body));
                }
                let iter_src = self.gen_iter_source(&args[0])?;
                if let Some(key_expr) = key_kw {
                    let key_fn = self.gen_expr(key_expr)?;
                    let key_returns_result = matches!(
                        key_expr.ty.as_ref(),
                        Some(Type::Lambda { ret, .. }) if matches!(ret.as_ref(), Type::Result(_, _))
                    );
                    let item_is_copy = args[0]
                        .ty
                        .as_ref()
                        .and_then(|ty| self.iter_item_type_hint(ty))
                        .map(|ty| self.is_copy_type(&ty))
                        .unwrap_or(false);
                    let iter_name = self.new_tmp();
                    let best_name = self.new_tmp();
                    let best_key_name = self.new_tmp();
                    let item_name = self.new_tmp();
                    let item_key_name = self.new_tmp();
                    let best_key_arg = if item_is_copy {
                        best_name.clone()
                    } else {
                        format!("{best_name}.clone()")
                    };
                    let item_key_arg = if item_is_copy {
                        item_name.clone()
                    } else {
                        format!("{item_name}.clone()")
                    };
                    let best_key_expr = if key_returns_result {
                        self.wrap_result(format!("({key_fn})({best_key_arg})"))
                    } else {
                        format!("({key_fn})({best_key_arg})")
                    };
                    let item_key_expr = if key_returns_result {
                        self.wrap_result(format!("({key_fn})({item_key_arg})"))
                    } else {
                        format!("({key_fn})({item_key_arg})")
                    };
                    // Iterator expressions built from iterable literals may borrow temporaries
                    // (for example, vec![...].iter().cloned()). Buffer those defensively.
                    let iter_expr = if matches!(
                        args[0].kind,
                        ExprKind::List(_) | ExprKind::Tuple(_) | ExprKind::Set(_)
                    ) {
                        format!("({}).collect::<Vec<_>>().into_iter()", iter_src.expr)
                    } else {
                        iter_src.expr.clone()
                    };
                    let body = self.wrap_result(format!(
                        "{{ let mut {iter_name} = {iter}; match {iter_name}.next() {{ Some(first_item) => {{ let mut {best_name} = first_item; let mut {best_key_name} = {best_key_expr}; for {item_name} in {iter_name} {{ let {item_key_name} = {item_key_expr}; if {item_key_name} < {best_key_name} {{ {best_name} = {item_name}; {best_key_name} = {item_key_name}; }} }} Ok({best_name}) }}, None => Err(PyError::ValueError(\"min() arg is an empty sequence\".into())) }} }}",
                        iter_name = iter_name,
                        iter = iter_expr,
                        best_name = best_name,
                        best_key_name = best_key_name,
                        item_name = item_name,
                        item_key_name = item_key_name,
                        best_key_expr = best_key_expr,
                        item_key_expr = item_key_expr,
                    ));
                    return Ok(Some(iter_src.wrap(body)));
                }
                self.uses.py_min = true;
                let body = self.wrap_result(format!("py_min({})", iter_src.expr));
                return Ok(Some(iter_src.wrap(body)));
            }
            if key_kw.is_some() {
                return Err(self.error(
                    expr.span,
                    "min() with key= currently supports only iterable form",
                ));
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
                            "Err::<_, PyError>(PyError::TypeError(\"'range' object is not an iterator\".into()))"
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
        if name == "iter" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "iter() expects one argument"));
            }
            let iter_expr = self.gen_iter_source_owned(&args[0], IterContext::DeferredCapture)?;
            return Ok(Some(iter_expr));
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
        if name == "ascii" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "ascii() expects one argument"));
            }
            let repr_expr = self
                .gen_builtin_call(expr, "repr", args, &[])?
                .ok_or_else(|| self.error(expr.span, "failed to lower ascii() via repr()"))?;
            self.uses.py_ascii = true;
            return Ok(Some(format!("py_ascii_escape(&{})", repr_expr)));
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
                    "list" => matches!(args[0].ty.as_ref(), Some(Type::List(_))),
                    "tuple" => matches!(args[0].ty.as_ref(), Some(Type::Tuple(_))),
                    "dict" => matches!(args[0].ty.as_ref(), Some(Type::Dict(_, _))),
                    "set" => matches!(args[0].ty.as_ref(), Some(Type::Set(_))),
                    "NoneType" => matches!(args[0].ty.as_ref(), Some(Type::None)),
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
}
