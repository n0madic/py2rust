// Function and method call expression lowering.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Lower a call expression, including builtins and method calls.
    pub(super) fn gen_call_expr(
        &mut self,
        expr: &Expr,
        func: &Expr,
        args: &[Expr],
    ) -> Result<String, CompileError> {
        if let ExprKind::Name(name) = &func.kind {
            if let Some(result) = self.gen_builtin_call(expr, name, args)? {
                return Ok(result);
            }
        }
        if let ExprKind::Attr { value, attr } = &func.kind {
            return self.gen_attr_call(value, attr, args);
        }
        // Check if this is a user-defined function.
        if let ExprKind::Name(name) = &func.kind {
            if let Some(sig) = self.ctx.functions.get(name) {
                let call = format!("{}({})", name, self.gen_call_args(name, args)?);
                // Add ? operator if function can throw.
                if sig.can_throw {
                    return Ok(format!("({}?)", call));
                }
                return Ok(call);
            }
        }
        Ok(format!(
            "{}({})",
            self.gen_expr(func)?,
            self.gen_args(args)?
        ))
    }

    /// Try to lower a builtin call; return Some(expr) if handled.
    fn gen_builtin_call(
        &mut self,
        expr: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Result<Option<String>, CompileError> {
        if name == "print" {
            self.uses.print = true;
            if args.is_empty() {
                return Ok(Some("py_print(\"\")".to_string()));
            }
            if args.len() == 1 {
                if matches!(args[0].ty.as_ref(), Some(Type::None)) {
                    return Ok(Some("py_print(String::from(\"None\"))".to_string()));
                }
                let arg_expr = self.gen_expr(&args[0])?;
                if self.print_needs_debug(&args[0]) {
                    return Ok(Some(format!("py_print(format!(\"{{:?}}\", {}))", arg_expr)));
                }
                return Ok(Some(format!("py_print({})", arg_expr)));
            }
            let mut fmt = String::new();
            let mut vals = Vec::new();
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    fmt.push(' ');
                }
                if matches!(arg.ty.as_ref(), Some(Type::None)) {
                    fmt.push_str("{}");
                    vals.push("String::from(\"None\")".to_string());
                } else {
                    let spec = if self.print_needs_debug(arg) {
                        "{:?}"
                    } else {
                        "{}"
                    };
                    fmt.push_str(spec);
                    vals.push(self.gen_expr(arg)?);
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
                    return Ok(Some(format!("(py_round({}, 0) as i64)", arg_expr)));
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
                return Ok(Some("Vec::new()".to_string()));
            }
            if let Some(Type::Tuple(items)) = args[0].ty.as_ref() {
                let tmp = self.new_tmp();
                let base = self.gen_expr(&args[0])?;
                let mut elems = Vec::new();
                for idx in 0..items.len() {
                    elems.push(format!("{}.{}", tmp, idx));
                }
                return Ok(Some(format!(
                    "{{ let {} = {}; vec![{}] }}",
                    tmp,
                    base,
                    elems.join(", ")
                )));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            return Ok(Some(format!("({}).collect::<Vec<_>>()", iter_src)));
        }
        if name == "tuple" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "tuple() expects zero or one argument"));
            }
            if args.is_empty() {
                return Ok(Some("Vec::new()".to_string()));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            return Ok(Some(format!("({}).collect::<Vec<_>>()", iter_src)));
        }
        if name == "dict" {
            if args.len() > 1 {
                return Err(self.error(expr.span, "dict() expects at most one argument"));
            }
            self.uses.hash_map = true;
            if args.is_empty() {
                return Ok(Some("HashMap::new()".to_string()));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            if matches!(args[0].ty.as_ref(), Some(Type::Dict(_, _))) {
                if let ExprKind::Name(name) = &args[0].kind {
                    if self.is_borrowed_param(name) {
                        return Ok(Some(format!("(*{}).clone()", arg_expr)));
                    }
                }
                return Ok(Some(format!("{}.clone()", arg_expr)));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            return Ok(Some(format!("({}).collect::<HashMap<_, _>>()", iter_src)));
        }
        if name == "enumerate" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "enumerate() expects one argument"));
            }
            let iter_src = self.gen_iter_source(&args[0])?;
            return Ok(Some(format!(
                "{}.enumerate().map(|(i, v)| (i as i64, v))",
                iter_src
            )));
        }
        if name == "zip" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "zip() expects two arguments"));
            }
            let left_iter = self.gen_iter_source(&args[0])?;
            let right_iter = self.gen_iter_source(&args[1])?;
            return Ok(Some(format!("{}.zip({})", left_iter, right_iter)));
        }
        if name == "map" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "map() expects two arguments"));
            }
            let iter_expr = self.gen_iter_source(&args[1])?;
            let (func_expr, inline_closure) = match &args[0].kind {
                ExprKind::Name(n) if n == "str" => ("|x| x.to_string()".to_string(), true),
                ExprKind::Lambda { .. } => (self.gen_expr(&args[0])?, true),
                _ => (self.gen_expr(&args[0])?, false),
            };
            if inline_closure {
                return Ok(Some(format!("{}.map({})", iter_expr, func_expr)));
            }
            let tmp = self.new_tmp();
            return Ok(Some(format!(
                "{{ let {} = {}; {}.map(move |x| ({})(x)) }}",
                tmp, func_expr, iter_expr, tmp
            )));
        }
        if name == "filter" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "filter() expects two arguments"));
            }
            let iter_expr = self.gen_iter_source(&args[1])?;
            if matches!(args[0].kind, ExprKind::Literal(Literal::None)) {
                let item_ty = args[1]
                    .ty
                    .as_ref()
                    .and_then(|ty| self.iter_item_type_hint(ty));
                let truthy = match item_ty.as_ref() {
                    Some(ty) => self.truthy_expr_for_type("x", ty),
                    None => "true".to_string(),
                };
                return Ok(Some(format!(
                    "{}.filter(|x| {{ let x = x.clone(); {} }})",
                    iter_expr, truthy
                )));
            }
            let pred_expr = self.gen_expr(&args[0])?;
            let tmp = self.new_tmp();
            return Ok(Some(format!(
                "{{ let {} = {}; {}.filter(move |x| {{ let x = x.clone(); ({}) (x) }}) }}",
                tmp, pred_expr, iter_expr, tmp
            )));
        }
        if name == "all" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "all() expects one argument"));
            }
            let iter_expr = self.gen_iter_source(&args[0])?;
            let item_ty = args[0]
                .ty
                .as_ref()
                .and_then(|ty| self.iter_item_type_hint(ty));
            let truthy = match item_ty.as_ref() {
                Some(ty) => self.truthy_expr_for_type("v", ty),
                None => "true".to_string(),
            };
            return Ok(Some(format!(
                "{}.all(|v| {{ let v = v.clone(); {} }})",
                iter_expr, truthy
            )));
        }
        if name == "any" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "any() expects one argument"));
            }
            let iter_expr = self.gen_iter_source(&args[0])?;
            let item_ty = args[0]
                .ty
                .as_ref()
                .and_then(|ty| self.iter_item_type_hint(ty));
            let truthy = match item_ty.as_ref() {
                Some(ty) => self.truthy_expr_for_type("v", ty),
                None => "true".to_string(),
            };
            return Ok(Some(format!(
                "{}.any(|v| {{ let v = v.clone(); {} }})",
                iter_expr, truthy
            )));
        }
        if name == "reversed" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "reversed() expects one argument"));
            }
            let iter_expr = self.gen_iter_source(&args[0])?;
            return Ok(Some(format!("{}.rev()", iter_expr)));
        }
        if name == "max" {
            if args.is_empty() {
                return Err(self.error(expr.span, "max() expects at least one argument"));
            }
            if args.len() == 1 {
                self.uses.py_max = true;
                let iter_expr = self.gen_iter_source(&args[0])?;
                return Ok(Some(self.wrap_result(format!("py_max({})", iter_expr))));
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
                let iter_expr = self.gen_iter_source(&args[0])?;
                return Ok(Some(self.wrap_result(format!("py_min({})", iter_expr))));
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
            let iter_expr = self.gen_iter_source(&args[0])?;
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
            return Ok(Some(format!(
                "{}.fold({}, |acc, v| acc + {})",
                iter_expr, start_expr, value_expr
            )));
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
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Int | Type::Float) => format!("format!(\"{{}}\", {})", arg_expr),
                Some(Type::Bool) => format!(
                    "if {} {{ String::from(\"True\") }} else {{ String::from(\"False\") }}",
                    arg_expr
                ),
                Some(Type::None) => "String::from(\"None\")".to_string(),
                Some(Type::Str) => format!("format!(\"'{{}}'\", {})", arg_expr),
                _ => format!("format!(\"{{:?}}\", {})", arg_expr),
            }));
        }
        if name == "str" {
            if args.len() != 1 {
                return Err(self.error(expr.span, "str() expects one argument"));
            }
            let arg_expr = self.gen_expr(&args[0])?;
            return Ok(Some(match args[0].ty.as_ref() {
                Some(Type::Str) => arg_expr,
                Some(Type::Bool) => format!(
                    "if {} {{ String::from(\"True\") }} else {{ String::from(\"False\") }}",
                    arg_expr
                ),
                Some(Type::None) => "String::from(\"None\")".to_string(),
                Some(Type::Int | Type::Float) => format!("{}.to_string()", arg_expr),
                _ => format!("format!(\"{{:?}}\", {})", arg_expr),
            }));
        }
        if name == "isinstance" {
            if args.len() != 2 {
                return Err(self.error(expr.span, "isinstance() expects two arguments"));
            }
            if let ExprKind::Name(type_name) = &args[1].kind {
                let matches = match type_name.as_str() {
                    "int" => matches!(args[0].ty.as_ref(), Some(Type::Int)),
                    "float" => matches!(args[0].ty.as_ref(), Some(Type::Float)),
                    "bool" => matches!(args[0].ty.as_ref(), Some(Type::Bool)),
                    "str" => matches!(args[0].ty.as_ref(), Some(Type::Str)),
                    _ => false,
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
                    return Ok(Some(format!("String::from({:?})", class_str)));
                }
            }
            self.uses.type_name = true;
            return Ok(Some(format!(
                "format!(\"<class '{{}}'>\", py_type_name(&{}))",
                self.gen_expr(&args[0])?
            )));
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
            let call = format!("{}::new({})", name, self.gen_args(args)?);
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
    ) -> Result<String, CompileError> {
        if attr == "append" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.push({})", target, self.gen_args(args)?));
            }
        }
        if attr == "extend" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
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
                return Ok(format!("{}.extend({}.iter().cloned())", target, arg_expr));
            }
        }
        if attr == "pop" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() > 1 {
                    return Err(self.error(value.span, "list.pop() expects zero or one argument"));
                }
                let idx_arg = args.get(0);
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        if let Some(arg) = idx_arg {
                            let idx_raw = self.gen_expr(arg)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            return Ok(format!(
                                "{{ let mut {guard} = {lock}; let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                len_tmp = len_tmp,
                                idx_tmp = idx_tmp,
                                idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                            ));
                        }
                        let pop_expr = format!(
                            "{}.pop().ok_or_else(|| PyError::IndexError(String::from(\"IndexError\")))",
                            guard
                        );
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; {pop} }}",
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
                    if let Some(arg) = idx_arg {
                        let idx_raw = self.gen_expr(arg)?;
                        self.uses.py_index = true;
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {tmp} = {target}; let {len_tmp} = {tmp}.len(); let {idx_tmp} = {idx_expr}; {tmp}.remove({idx_tmp}) }}",
                            tmp = tmp,
                            target = target_expr,
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                        ));
                    }
                    let pop_expr = format!(
                        "{}.pop().ok_or_else(|| PyError::IndexError(String::from(\"IndexError\")))",
                        tmp
                    );
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; {pop} }}",
                        tmp = tmp,
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
                    return Ok(format!(
                        "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = {idx_expr}; {target}.remove({idx_tmp}) }}",
                        len_tmp = len_tmp,
                        target = target_expr,
                        idx_tmp = idx_tmp,
                        idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                    ));
                }
                let pop_expr = format!(
                    "{}.pop().ok_or_else(|| PyError::IndexError(String::from(\"IndexError\")))",
                    target_expr
                );
                return Ok(self.wrap_result(pop_expr));
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
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
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
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; let {len_tmp} = {tmp}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {tmp}.insert({idx_tmp}, {val}); }}",
                        tmp = tmp,
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
                    "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {target}.insert({idx_tmp}, {val}); }}",
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
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.copy() expects no arguments"));
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
        if attr == "reverse" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.reverse() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; {guard}.reverse(); }}",
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; {tmp}.reverse(); }}",
                        tmp = tmp,
                        target = target_expr
                    ));
                }
                return Ok(format!("{}.reverse()", target_expr));
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
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        let call = format!(
                            "py_list_index(&{guard}, &{needle})",
                            guard = guard,
                            needle = needle_expr
                        );
                        return Ok(format!(
                            "{{ let {guard} = {lock}; {result} }}",
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            result = self.wrap_result(call)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let call = format!("py_list_index(&{}, &{})", target_expr, needle_expr);
                return Ok(self.wrap_result(call));
            }
        }
        if attr == "sort" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.sort() expects no arguments"));
                }
                let sort_call = if matches!(inner.as_ref(), Type::Float) {
                    "sort_by(|a, b| a.partial_cmp(b).unwrap())"
                } else {
                    "sort()"
                };
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; {guard}.{sort_call}; }}",
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            sort_call = sort_call
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; {tmp}.{sort_call}; }}",
                        tmp = tmp,
                        target = target_expr,
                        sort_call = sort_call
                    ));
                }
                return Ok(format!("{}.{}", target_expr, sort_call));
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
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {guard} = {lock}; py_list_count(&{guard}, &{needle}) }}",
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            needle = needle_expr
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!("py_list_count(&{}, &{})", target_expr, needle_expr));
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
        if attr == "format" {
            if let ExprKind::Literal(Literal::Str(fmt)) = &value.kind {
                let fmt_lit = format!("{fmt:?}");
                if args.is_empty() {
                    return Ok(format!("String::from({})", fmt_lit));
                }
                let mut vals = Vec::new();
                for arg in args {
                    let arg_expr = self.gen_expr(arg)?;
                    if self.print_needs_debug(arg) {
                        vals.push(format!("format!(\"{{:?}}\", {})", arg_expr));
                    } else {
                        vals.push(arg_expr);
                    }
                }
                return Ok(format!("format!({}, {})", fmt_lit, vals.join(", ")));
            }
        }
        Ok(format!(
            "{}.{}({})",
            self.gen_expr(value)?,
            attr,
            self.gen_args(args)?
        ))
    }
}
