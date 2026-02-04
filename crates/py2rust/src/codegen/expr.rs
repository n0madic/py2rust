use super::util::collect_assign_counts;
use super::*;

impl<'a> Codegen<'a> {
    /// Generate Rust code for an expression.
    ///
    /// This is one of the most complex parts of codegen because expressions:
    /// 1. Need type-specific handling (numeric suffixes, collection constructors)
    /// 2. May require helper function injection (print, len, range, etc.)
    /// 3. Must handle mixed int/float arithmetic with casts
    /// 4. Need to bridge Python's dynamic semantics to Rust's static types
    ///
    /// Key design decisions:
    /// - Literals: Always suffix numeric literals (42i64, 3.14f64) to avoid ambiguity
    /// - Strings: Use String::from() instead of .to_string() for consistency
    /// - None: Maps to () or None depending on whether it's in Optional context
    /// - __name__: Special variable backed by const __NAME__, calls .to_string() on access
    /// - Globals: Access via OnceLock mutex wrapper for thread-safe mutation
    /// - Builtins: Many Python builtins (print, len, range) are emitted as helper calls
    pub(crate) fn gen_expr(&mut self, expr: &Expr) -> Result<String, CompileError> {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                // Always suffix numeric literals to avoid Rust type inference ambiguity.
                // Without suffixes, `42` could be i8, i16, i32, i64, etc.
                Literal::Int(v) => Ok(format!("{}i64", v)),
                Literal::Float(v) => Ok(format!("{}f64", v)),
                Literal::Bool(v) => Ok(format!("{}", v)),
                // Use String::from for string literals (more consistent than .to_string())
                Literal::Str(s) => Ok(format!("String::from({s:?})")),
                // None maps to different Rust types depending on context:
                // - Option<T>: emit `None`
                // - Unit type: emit `()`
                Literal::None => {
                    if let Some(Type::Option(_)) = expr.ty.as_ref() {
                        Ok("None".to_string())
                    } else {
                        Ok("()".to_string())
                    }
                }
            },
            ExprKind::Name(name) => {
                if name == "__name__" {
                    return Ok("__NAME__.to_string()".to_string());
                }
                if self.is_global(name) {
                    return Ok(format!(
                        "{}.get().unwrap().lock().unwrap().clone()",
                        self.global_name(name)
                    ));
                }
                Ok(name.clone())
            }
            ExprKind::Attr { value, attr } => {
                if attr == "__name__" {
                    if let ExprKind::Call { func, args } = &value.kind {
                        if let ExprKind::Name(name) = &func.kind {
                            if name == "type" && args.len() == 1 {
                                if let Some(ty) = args[0].ty.as_ref() {
                                    if let Some(name) = self.python_type_name(ty) {
                                        return Ok(format!("String::from({:?})", name));
                                    }
                                }
                                self.uses.type_name = true;
                                return Ok(format!("py_type_name(&{})", self.gen_expr(&args[0])?));
                            }
                        }
                    }
                }
                Ok(format!("{}.{}", self.gen_expr(value)?, attr))
            }
            ExprKind::Call { func, args } => {
                if let ExprKind::Name(name) = &func.kind {
                    if name == "print" {
                        self.uses.print = true;
                        if args.is_empty() {
                            return Ok("py_print(\"\")".to_string());
                        }
                        if args.len() == 1 {
                            if matches!(args[0].ty.as_ref(), Some(Type::None)) {
                                return Ok("py_print(String::from(\"None\"))".to_string());
                            }
                            let arg_expr = self.gen_expr(&args[0])?;
                            if self.print_needs_debug(&args[0]) {
                                return Ok(format!("py_print(format!(\"{{:?}}\", {}))", arg_expr));
                            }
                            return Ok(format!("py_print({})", arg_expr));
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
                        return Ok(format!(
                            "py_print(format!(\"{}\", {}))",
                            fmt,
                            vals.join(", ")
                        ));
                    }
                    if name == "len" {
                        self.uses.len = true;
                        let arg_expr = self.gen_expr(&args[0])?;
                        // Don't add & if already a reference type or if it's a borrowed parameter
                        let is_borrowed = self.is_reference_type(args[0].ty.as_ref())
                            || matches!(&args[0].kind, ExprKind::Name(n) if self.is_borrowed_param(n));
                        if is_borrowed {
                            return Ok(format!("py_len({})", arg_expr));
                        }
                        return Ok(format!("py_len(&{})", arg_expr));
                    }
                    if name == "range" {
                        if args.len() == 1 {
                            self.uses.range = true;
                            return Ok(format!("py_range({})", self.gen_expr(&args[0])?));
                        }
                        if args.len() == 2 {
                            self.uses.range2 = true;
                            return Ok(format!(
                                "py_range2({}, {})",
                                self.gen_expr(&args[0])?,
                                self.gen_expr(&args[1])?
                            ));
                        }
                        if args.len() == 3 {
                            self.uses.range3 = true;
                            let start_expr = self.gen_expr(&args[0])?;
                            let end_expr = self.gen_expr(&args[1])?;
                            let step_expr = self.gen_expr(&args[2])?;
                            return Ok(self.wrap_result(format!(
                                "py_range3({}, {}, {})",
                                start_expr, end_expr, step_expr
                            )));
                        }
                    }
                    if name == "round" {
                        if args.len() == 1 {
                            let arg_expr = self.gen_expr(&args[0])?;
                            if matches!(args[0].ty.as_ref(), Some(Type::Float)) {
                                self.uses.round = true;
                                return Ok(format!("(py_round({}, 0) as i64)", arg_expr));
                            }
                            return Ok(arg_expr);
                        }
                        if args.len() == 2 {
                            let arg_expr = self.gen_expr(&args[0])?;
                            let digits_expr = self.gen_expr(&args[1])?;
                            if matches!(args[0].ty.as_ref(), Some(Type::Float)) {
                                self.uses.round = true;
                                return Ok(format!("py_round({}, {})", arg_expr, digits_expr));
                            }
                            return Ok(arg_expr);
                        }
                    }
                    if name == "list" {
                        if args.len() > 1 {
                            return Err(
                                self.error(expr.span, "list() expects zero or one argument")
                            );
                        }
                        if args.is_empty() {
                            return Ok("Vec::new()".to_string());
                        }
                        if let Some(Type::Tuple(items)) = args[0].ty.as_ref() {
                            let tmp = self.new_tmp();
                            let base = self.gen_expr(&args[0])?;
                            let mut elems = Vec::new();
                            for idx in 0..items.len() {
                                elems.push(format!("{}.{}", tmp, idx));
                            }
                            return Ok(format!(
                                "{{ let {} = {}; vec![{}] }}",
                                tmp,
                                base,
                                elems.join(", ")
                            ));
                        }
                        let iter_src = self.gen_iter_source(&args[0])?;
                        return Ok(format!("({}).collect::<Vec<_>>()", iter_src));
                    }
                    if name == "tuple" {
                        if args.len() > 1 {
                            return Err(
                                self.error(expr.span, "tuple() expects zero or one argument")
                            );
                        }
                        if args.is_empty() {
                            return Ok("Vec::new()".to_string());
                        }
                        let iter_src = self.gen_iter_source(&args[0])?;
                        return Ok(format!("({}).collect::<Vec<_>>()", iter_src));
                    }
                    if name == "dict" {
                        if args.len() > 1 {
                            return Err(
                                self.error(expr.span, "dict() expects at most one argument")
                            );
                        }
                        self.uses.hash_map = true;
                        if args.is_empty() {
                            return Ok("HashMap::new()".to_string());
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        if matches!(args[0].ty.as_ref(), Some(Type::Dict(_, _))) {
                            if let ExprKind::Name(name) = &args[0].kind {
                                if self.is_borrowed_param(name) {
                                    return Ok(format!("(*{}).clone()", arg_expr));
                                }
                            }
                            return Ok(format!("{}.clone()", arg_expr));
                        }
                        let iter_src = self.gen_iter_source(&args[0])?;
                        return Ok(format!("({}).collect::<HashMap<_, _>>()", iter_src));
                    }
                    if name == "enumerate" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "enumerate() expects one argument"));
                        }
                        let iter_src = self.gen_iter_source(&args[0])?;
                        return Ok(format!(
                            "{}.enumerate().map(|(i, v)| (i as i64, v))",
                            iter_src
                        ));
                    }
                    if name == "zip" {
                        if args.len() != 2 {
                            return Err(self.error(expr.span, "zip() expects two arguments"));
                        }
                        let left_iter = self.gen_iter_source(&args[0])?;
                        let right_iter = self.gen_iter_source(&args[1])?;
                        return Ok(format!("{}.zip({})", left_iter, right_iter));
                    }
                    if name == "map" {
                        if args.len() != 2 {
                            return Err(self.error(expr.span, "map() expects two arguments"));
                        }
                        let iter_expr = self.gen_iter_source(&args[1])?;
                        let (func_expr, inline_closure) = match &args[0].kind {
                            ExprKind::Name(n) if n == "str" => {
                                ("|x| x.to_string()".to_string(), true)
                            }
                            ExprKind::Lambda { .. } => (self.gen_expr(&args[0])?, true),
                            _ => (self.gen_expr(&args[0])?, false),
                        };
                        if inline_closure {
                            return Ok(format!("{}.map({})", iter_expr, func_expr));
                        }
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {} = {}; {}.map(move |x| ({})(x)) }}",
                            tmp, func_expr, iter_expr, tmp
                        ));
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
                            return Ok(format!(
                                "{}.filter(|x| {{ let x = x.clone(); {} }})",
                                iter_expr, truthy
                            ));
                        }
                        let pred_expr = self.gen_expr(&args[0])?;
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {} = {}; {}.filter(move |x| {{ let x = x.clone(); ({}) (x) }}) }}",
                            tmp, pred_expr, iter_expr, tmp
                        ));
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
                        return Ok(format!(
                            "{}.all(|v| {{ let v = v.clone(); {} }})",
                            iter_expr, truthy
                        ));
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
                        return Ok(format!(
                            "{}.any(|v| {{ let v = v.clone(); {} }})",
                            iter_expr, truthy
                        ));
                    }
                    if name == "reversed" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "reversed() expects one argument"));
                        }
                        let iter_expr = self.gen_iter_source(&args[0])?;
                        return Ok(format!("{}.rev()", iter_expr));
                    }
                    if name == "max" {
                        if args.is_empty() {
                            return Err(
                                self.error(expr.span, "max() expects at least one argument")
                            );
                        }
                        if args.len() == 1 {
                            self.uses.py_max = true;
                            let iter_expr = self.gen_iter_source(&args[0])?;
                            return Ok(self.wrap_result(format!("py_max({})", iter_expr)));
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
                        return Ok(expr_acc);
                    }
                    if name == "min" {
                        if args.is_empty() {
                            return Err(
                                self.error(expr.span, "min() expects at least one argument")
                            );
                        }
                        if args.len() == 1 {
                            self.uses.py_min = true;
                            let iter_expr = self.gen_iter_source(&args[0])?;
                            return Ok(self.wrap_result(format!("py_min({})", iter_expr)));
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
                        return Ok(expr_acc);
                    }
                    if name == "abs" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "abs() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Int) | Some(Type::Float) => format!("{}.abs()", arg_expr),
                            Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                            _ => format!("{}.abs()", arg_expr),
                        });
                    }
                    if name == "pow" {
                        if args.len() != 2 {
                            return Err(self.error(expr.span, "pow() expects two arguments"));
                        }
                        let left = self.gen_numeric_operand(&args[0], true)?;
                        let right = self.gen_numeric_operand(&args[1], true)?;
                        return Ok(format!("({}.powf({}))", left, right));
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
                        return Ok(format!(
                            "{}.fold({}, |acc, v| acc + {})",
                            iter_expr, start_expr, value_expr
                        ));
                    }
                    if name == "int" {
                        if args.len() > 1 {
                            return Err(self.error(expr.span, "int() expects zero or one argument"));
                        }
                        if args.is_empty() {
                            return Ok("0i64".to_string());
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Str) => {
                                self.uses.py_parse_int = true;
                                self.wrap_parse_result(format!("py_parse_int(&{})", arg_expr))
                            }
                            Some(Type::Float) => format!("{} as i64", arg_expr),
                            Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                            Some(Type::Int) => arg_expr,
                            _ => {
                                self.uses.py_parse_int = true;
                                self.wrap_parse_result(format!(
                                    "py_parse_int(&{}.to_string())",
                                    arg_expr
                                ))
                            }
                        });
                    }
                    if name == "float" {
                        if args.len() > 1 {
                            return Err(
                                self.error(expr.span, "float() expects zero or one argument")
                            );
                        }
                        if args.is_empty() {
                            return Ok("0.0f64".to_string());
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Str) => {
                                self.uses.py_parse_float = true;
                                self.wrap_parse_result(format!("py_parse_float(&{})", arg_expr))
                            }
                            Some(Type::Int) => format!("{} as f64", arg_expr),
                            Some(Type::Bool) => format!("if {} {{ 1.0 }} else {{ 0.0 }}", arg_expr),
                            Some(Type::Float) => arg_expr,
                            _ => {
                                self.uses.py_parse_float = true;
                                self.wrap_parse_result(format!(
                                    "py_parse_float(&{}.to_string())",
                                    arg_expr
                                ))
                            }
                        });
                    }
                    if name == "bool" {
                        if args.len() > 1 {
                            return Err(
                                self.error(expr.span, "bool() expects zero or one argument")
                            );
                        }
                        if args.is_empty() {
                            return Ok("false".to_string());
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        let ty = args[0].ty.as_ref().unwrap_or(&Type::Unknown);
                        return Ok(self.truthy_expr_for_type(&arg_expr, ty));
                    }
                    if name == "chr" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "chr() expects one argument"));
                        }
                        self.uses.py_chr = true;
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(self.wrap_result(format!("py_chr({})", arg_expr)));
                    }
                    if name == "ord" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "ord() expects one argument"));
                        }
                        self.uses.py_ord = true;
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(self.wrap_result(format!("py_ord(&{})", arg_expr)));
                    }
                    if name == "hash" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "hash() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Int) => arg_expr,
                            Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                            Some(Type::Str) => format!(
                                "{{ let mut _h: i64 = 0; for _b in {}.bytes() {{ _h = _h.wrapping_mul(31).wrapping_add(_b as i64); }} _h }}",
                                arg_expr
                            ),
                            Some(Type::None) => "1i64".to_string(),
                            _ => arg_expr,
                        });
                    }
                    if name == "id" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "id() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        if let ExprKind::Name(name) = &args[0].kind {
                            if self.is_global(name) {
                                return Ok(match args[0].ty.as_ref() {
                                    Some(Type::Int) => arg_expr,
                                    Some(Type::Bool) => {
                                        format!("if {} {{ 1 }} else {{ 0 }}", arg_expr)
                                    }
                                    Some(Type::None) => "0i64".to_string(),
                                    _ => format!(
                                        "{{ let _guard = {}; (&*_guard as *const _ as usize) as i64 }}",
                                        self.global_lock_expr(name)
                                    ),
                                });
                            }
                        }
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Int) => arg_expr,
                            Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                            Some(Type::None) => "0i64".to_string(),
                            _ => format!("(&{} as *const _ as usize) as i64", arg_expr),
                        });
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
                            return Ok(format!(
                                "(({} / {}).floor(), ({} % {}))",
                                left, right, left, right
                            ));
                        }
                        let left = self.gen_expr(&args[0])?;
                        let right = self.gen_expr(&args[1])?;
                        return Ok(format!("({} / {}, {} % {})", left, right, left, right));
                    }
                    if name == "next" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "next() expects one argument"));
                        }
                        self.uses.py_next = true;
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(self.wrap_result(format!("py_next({}.next())", arg_expr)));
                    }
                    if name == "bin" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "bin() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(format!(
                            "{{ let n = {}; if n < 0 {{ format!(\"-0b{{:b}}\", -n) }} else {{ format!(\"0b{{:b}}\", n) }} }}",
                            arg_expr
                        ));
                    }
                    if name == "hex" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "hex() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(format!(
                            "{{ let n = {}; if n < 0 {{ format!(\"-0x{{:x}}\", -n) }} else {{ format!(\"0x{{:x}}\", n) }} }}",
                            arg_expr
                        ));
                    }
                    if name == "oct" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "oct() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(format!(
                            "{{ let n = {}; if n < 0 {{ format!(\"-0o{{:o}}\", -n) }} else {{ format!(\"0o{{:o}}\", n) }} }}",
                            arg_expr
                        ));
                    }
                    if name == "repr" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "repr() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Int | Type::Float) => format!("format!(\"{{}}\", {})", arg_expr),
                            Some(Type::Bool) => format!(
                                "if {} {{ String::from(\"True\") }} else {{ String::from(\"False\") }}",
                                arg_expr
                            ),
                            Some(Type::None) => "String::from(\"None\")".to_string(),
                            Some(Type::Str) => format!("format!(\"'{{}}'\", {})", arg_expr),
                            _ => format!("format!(\"{{:?}}\", {})", arg_expr),
                        });
                    }
                    if name == "str" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "str() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Str) => arg_expr,
                            Some(Type::Bool) => format!(
                                "if {} {{ String::from(\"True\") }} else {{ String::from(\"False\") }}",
                                arg_expr
                            ),
                            Some(Type::None) => "String::from(\"None\")".to_string(),
                            Some(Type::Int | Type::Float) => format!("{}.to_string()", arg_expr),
                            _ => format!("format!(\"{{:?}}\", {})", arg_expr),
                        });
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
                            return Ok(matches.to_string());
                        }
                        return Ok("false".to_string());
                    }
                    if name == "type" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "type() expects one argument"));
                        }
                        if let Some(ty) = args[0].ty.as_ref() {
                            if let Some(class_str) = self.python_type_class(ty) {
                                return Ok(format!("String::from({:?})", class_str));
                            }
                        }
                        self.uses.type_name = true;
                        return Ok(format!(
                            "format!(\"<class '{{}}'>\", py_type_name(&{}))",
                            self.gen_expr(&args[0])?
                        ));
                    }
                    if name == "exit" {
                        if args.len() > 1 {
                            return Err(
                                self.error(expr.span, "exit() expects zero or one argument")
                            );
                        }
                        if args.is_empty() {
                            return Ok("std::process::exit(0)".to_string());
                        }
                        return Ok(format!(
                            "std::process::exit({} as i32)",
                            self.gen_expr(&args[0])?
                        ));
                    }
                    if self.ctx.classes.contains_key(name) {
                        let call = format!("{}::new({})", name, self.gen_args(args)?);
                        if let Some(Type::Union(union_name)) = expr.ty.as_ref() {
                            return Ok(format!("{}::{}({})", union_name, name, call));
                        }
                        return Ok(call);
                    }
                }
                if let ExprKind::Attr { value, attr } = &func.kind {
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
                    return Ok(format!(
                        "{}.{}({})",
                        self.gen_expr(value)?,
                        attr,
                        self.gen_args(args)?
                    ));
                }
                // Check if this is a user-defined function
                if let ExprKind::Name(name) = &func.kind {
                    if let Some(sig) = self.ctx.functions.get(name) {
                        let call = format!("{}({})", name, self.gen_call_args(name, args)?);
                        // Add ? operator if function can throw
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
            ExprKind::Binary { op, left, right } => {
                if matches!(op, BinOp::Add) {
                    let left_is_str = matches!(left.ty.as_ref(), Some(Type::Str));
                    let right_is_str = matches!(right.ty.as_ref(), Some(Type::Str));
                    if left_is_str || right_is_str {
                        let left_expr = self.gen_expr(left)?;
                        let right_expr = self.gen_expr(right)?;
                        let left_spec = if self.print_needs_debug(left) {
                            "{:?}"
                        } else {
                            "{}"
                        };
                        let right_spec = if self.print_needs_debug(right) {
                            "{:?}"
                        } else {
                            "{}"
                        };
                        return Ok(format!(
                            "format!(\"{}{}\", {}, {})",
                            left_spec, right_spec, left_expr, right_expr
                        ));
                    }
                }
                if matches!(op, BinOp::Mul) {
                    let left_is_str = matches!(left.ty.as_ref(), Some(Type::Str));
                    let right_is_str = matches!(right.ty.as_ref(), Some(Type::Str));
                    if left_is_str && matches!(right.ty.as_ref(), Some(Type::Int)) {
                        let left_expr = self.gen_expr(left)?;
                        let right_expr = self.gen_expr(right)?;
                        return Ok(format!("{}.repeat({} as usize)", left_expr, right_expr));
                    }
                    if right_is_str && matches!(left.ty.as_ref(), Some(Type::Int)) {
                        let left_expr = self.gen_expr(left)?;
                        let right_expr = self.gen_expr(right)?;
                        return Ok(format!("{}.repeat({} as usize)", right_expr, left_expr));
                    }
                }
                if matches!(op, BinOp::BitOr | BinOp::BitAnd | BinOp::BitXor) {
                    let op_str = match op {
                        BinOp::BitOr => "|",
                        BinOp::BitAnd => "&",
                        BinOp::BitXor => "^",
                        _ => unreachable!(),
                    };
                    return Ok(format!(
                        "(&{} {} &{})",
                        self.gen_expr(left)?,
                        op_str,
                        self.gen_expr(right)?
                    ));
                }
                if matches!(op, BinOp::Sub) {
                    if let (Some(Type::Set(_)), Some(Type::Set(_))) =
                        (left.ty.as_ref(), right.ty.as_ref())
                    {
                        return Ok(format!(
                            "(&{} - &{})",
                            self.gen_expr(left)?,
                            self.gen_expr(right)?
                        ));
                    }
                }
                if matches!(op, BinOp::FloorDiv) {
                    let is_float = matches!(expr.ty.as_ref(), Some(Type::Float));
                    let left_expr = self.gen_numeric_operand(left, is_float)?;
                    let right_expr = self.gen_numeric_operand(right, is_float)?;
                    if is_float {
                        return Ok(format!("(({} / {}).floor())", left_expr, right_expr));
                    }
                    return Ok(format!("({}.div_euclid({}))", left_expr, right_expr));
                }
                if matches!(op, BinOp::Pow) {
                    let is_float = matches!(expr.ty.as_ref(), Some(Type::Float));
                    let left_expr = self.gen_numeric_operand(left, is_float)?;
                    let right_expr = self.gen_numeric_operand(right, is_float)?;
                    if is_float {
                        return Ok(format!("({}.powf({}))", left_expr, right_expr));
                    }
                    return Ok(format!("({}.pow({} as u32))", left_expr, right_expr));
                }
                if matches!(op, BinOp::Add) {
                    if let (Some(Type::Tuple(left_items)), Some(Type::Tuple(right_items))) =
                        (left.ty.as_ref(), right.ty.as_ref())
                    {
                        let left_tmp = self.new_tmp();
                        let right_tmp = self.new_tmp();
                        let mut elems = Vec::new();
                        for idx in 0..left_items.len() {
                            elems.push(format!("{}.{}.clone()", left_tmp, idx));
                        }
                        for idx in 0..right_items.len() {
                            elems.push(format!("{}.{}.clone()", right_tmp, idx));
                        }
                        return Ok(format!(
                            "{{ let {} = &{}; let {} = &{}; ({}) }}",
                            left_tmp,
                            self.gen_expr(left)?,
                            right_tmp,
                            self.gen_expr(right)?,
                            elems.join(", ")
                        ));
                    }
                }
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Pow | BinOp::FloorDiv | BinOp::BitOr | BinOp::BitAnd | BinOp::BitXor => {
                        unreachable!()
                    }
                };
                let is_float = matches!(expr.ty.as_ref(), Some(Type::Float));
                let left_expr = self.gen_numeric_operand(left, is_float)?;
                let right_expr = self.gen_numeric_operand(right, is_float)?;
                Ok(format!("({} {} {})", left_expr, op_str, right_expr))
            }
            ExprKind::Unary { op, expr: inner } => {
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                };
                Ok(format!("({}{})", op_str, self.gen_expr(inner)?))
            }
            ExprKind::Compare { op, left, right } => {
                if self.name_compare_only {
                    if let (ExprKind::Name(name), ExprKind::Literal(Literal::Str(s))) =
                        (&left.kind, &right.kind)
                    {
                        if name == "__name__" && matches!(op, CmpOp::Eq | CmpOp::NotEq) {
                            let op_str = if matches!(op, CmpOp::Eq) { "==" } else { "!=" };
                            return Ok(format!("(__NAME__ {} {s:?})", op_str));
                        }
                    }
                    if let (ExprKind::Literal(Literal::Str(s)), ExprKind::Name(name)) =
                        (&left.kind, &right.kind)
                    {
                        if name == "__name__" && matches!(op, CmpOp::Eq | CmpOp::NotEq) {
                            let op_str = if matches!(op, CmpOp::Eq) { "==" } else { "!=" };
                            return Ok(format!("({s:?} {} __NAME__)", op_str));
                        }
                    }
                }
                if matches!(op, CmpOp::In | CmpOp::NotIn) {
                    let left_expr = self.gen_expr(left)?;
                    let right_expr = self.gen_expr(right)?;
                    let mut expr = match right.ty.as_ref() {
                        Some(Type::List(_)) | Some(Type::Set(_)) | Some(Type::Slice(_)) => {
                            format!("{}.contains(&{})", right_expr, left_expr)
                        }
                        Some(Type::Dict(_, _)) => {
                            format!("{}.contains_key(&{})", right_expr, left_expr)
                        }
                        Some(Type::Str) => format!("{}.contains(&{})", right_expr, left_expr),
                        Some(Type::Ref(inner)) => match inner.as_ref() {
                            Type::Dict(_, _) => {
                                format!("{}.contains_key(&{})", right_expr, left_expr)
                            }
                            Type::Set(_) | Type::List(_) | Type::Slice(_) => {
                                format!("{}.contains(&{})", right_expr, left_expr)
                            }
                            Type::Str => format!("{}.contains(&{})", right_expr, left_expr),
                            _ => {
                                return Err(self.error(
                                    expr.span,
                                    "Membership requires list, tuple, set, dict, or str",
                                ))
                            }
                        },
                        Some(Type::Tuple(items)) => {
                            if items.is_empty() {
                                "false".to_string()
                            } else {
                                let left_tmp = self.new_tmp();
                                let right_tmp = self.new_tmp();
                                let mut comps = Vec::new();
                                for idx in 0..items.len() {
                                    comps.push(format!("{} == &{}.{}", left_tmp, right_tmp, idx));
                                }
                                format!(
                                    "{{ let {} = &{}; let {} = &{}; {} }}",
                                    left_tmp,
                                    left_expr,
                                    right_tmp,
                                    right_expr,
                                    comps.join(" || ")
                                )
                            }
                        }
                        _ => {
                            return Err(self.error(
                                expr.span,
                                "Membership requires list, tuple, set, dict, or str",
                            ))
                        }
                    };
                    if matches!(op, CmpOp::NotIn) {
                        expr = format!("!({})", expr);
                    }
                    return Ok(expr);
                }
                if matches!(op, CmpOp::Is | CmpOp::IsNot)
                    && matches!(&right.kind, ExprKind::Literal(Literal::None))
                {
                    let left_expr = self.gen_expr(left)?;
                    if matches!(left.ty.as_ref(), Some(Type::Option(_))) {
                        if matches!(op, CmpOp::Is) {
                            return Ok(format!("{}.is_none()", left_expr));
                        }
                        return Ok(format!("!{}.is_none()", left_expr));
                    }
                    if matches!(op, CmpOp::Is) {
                        return Ok(format!("{} == ()", left_expr));
                    }
                    return Ok(format!("{} != ()", left_expr));
                }
                if matches!(op, CmpOp::Eq | CmpOp::NotEq) {
                    let op_str = if matches!(op, CmpOp::Eq) { "==" } else { "!=" };
                    if let (Some(Type::Option(inner)), Some(right_ty)) =
                        (left.ty.as_ref(), right.ty.as_ref())
                    {
                        if right_ty == inner.as_ref() {
                            let left_expr = self.gen_expr(left)?;
                            let right_expr = self.gen_expr(right)?;
                            if self.is_copy_type(inner) {
                                return Ok(format!(
                                    "({} {} Some({}))",
                                    left_expr, op_str, right_expr
                                ));
                            }
                            return Ok(format!(
                                "({}.as_ref() {} Some(&{}))",
                                left_expr, op_str, right_expr
                            ));
                        }
                        if matches!(&right.kind, ExprKind::Literal(Literal::None)) {
                            let left_expr = self.gen_expr(left)?;
                            if matches!(op, CmpOp::Eq) {
                                return Ok(format!("{}.is_none()", left_expr));
                            }
                            return Ok(format!("!{}.is_none()", left_expr));
                        }
                    }
                    if let (Some(left_ty), Some(Type::Option(inner))) =
                        (left.ty.as_ref(), right.ty.as_ref())
                    {
                        if left_ty == inner.as_ref() {
                            let left_expr = self.gen_expr(left)?;
                            let right_expr = self.gen_expr(right)?;
                            if self.is_copy_type(inner) {
                                return Ok(format!(
                                    "(Some({}) {} {})",
                                    left_expr, op_str, right_expr
                                ));
                            }
                            return Ok(format!(
                                "(Some(&{}) {} {}.as_ref())",
                                left_expr, op_str, right_expr
                            ));
                        }
                        if matches!(&left.kind, ExprKind::Literal(Literal::None)) {
                            let right_expr = self.gen_expr(right)?;
                            if matches!(op, CmpOp::Eq) {
                                return Ok(format!("{}.is_none()", right_expr));
                            }
                            return Ok(format!("!{}.is_none()", right_expr));
                        }
                    }
                }
                let op_str = match op {
                    CmpOp::Eq => "==",
                    CmpOp::NotEq => "!=",
                    CmpOp::Lt => "<",
                    CmpOp::LtEq => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::GtEq => ">=",
                    CmpOp::Is => "==",
                    CmpOp::IsNot => "!=",
                    CmpOp::In | CmpOp::NotIn => unreachable!(),
                };
                Ok(format!(
                    "({} {} {})",
                    self.gen_expr(left)?,
                    op_str,
                    self.gen_expr(right)?
                ))
            }
            ExprKind::BoolOp { op, values } => {
                let op_str = match op {
                    BoolOp::And => "&&",
                    BoolOp::Or => "||",
                };
                let parts: Result<Vec<String>, CompileError> =
                    values.iter().map(|v| self.gen_expr(v)).collect();
                Ok(format!("({})", parts?.join(&format!(" {} ", op_str))))
            }
            ExprKind::List(items) => {
                let expected = match expr.ty.as_ref() {
                    Some(Type::List(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                if items.is_empty() {
                    if let Some(Type::List(inner)) = expr.ty.as_ref() {
                        if !matches!(inner.as_ref(), Type::Unknown) {
                            return Ok(format!("Vec::<{}>::new()", self.rust_type(inner)));
                        }
                    }
                }
                let elems: Result<Vec<String>, CompileError> = items
                    .iter()
                    .map(|e| self.gen_expr_with_expected(e, expected))
                    .collect();
                Ok(format!("vec![{}]", elems?.join(", ")))
            }
            ExprKind::Tuple(items) => {
                let expected_items = match expr.ty.as_ref() {
                    Some(Type::Tuple(tys)) if tys.len() == items.len() => Some(tys),
                    _ => None,
                };
                let mut parts = Vec::new();
                for (idx, item) in items.iter().enumerate() {
                    let expected = expected_items.and_then(|tys| tys.get(idx));
                    parts.push(self.gen_expr_with_expected(item, expected)?);
                }
                let joined = parts.join(", ");
                if items.len() == 1 {
                    Ok(format!("({},)", joined))
                } else {
                    Ok(format!("({})", joined))
                }
            }
            ExprKind::Dict(items) => {
                self.uses.hash_map = true;
                if items.is_empty() {
                    return Ok("HashMap::new()".to_string());
                }
                let (expected_key, expected_val) = match expr.ty.as_ref() {
                    Some(Type::Dict(k, v)) => (Some(k.as_ref()), Some(v.as_ref())),
                    _ => (None, None),
                };
                let mut pairs = Vec::new();
                for (k, v) in items {
                    let key_expr = self.gen_expr_with_expected(k, expected_key)?;
                    let val_expr = self.gen_expr_with_expected(v, expected_val)?;
                    pairs.push(format!("({}, {})", key_expr, val_expr));
                }
                Ok(format!("HashMap::from([{}])", pairs.join(", ")))
            }
            ExprKind::Set(items) => {
                self.uses.hash_set = true;
                if items.is_empty() {
                    return Ok("HashSet::new()".to_string());
                }
                let expected = match expr.ty.as_ref() {
                    Some(Type::Set(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                let mut elems = Vec::new();
                for item in items {
                    elems.push(self.gen_expr_with_expected(item, expected)?);
                }
                Ok(format!("HashSet::from([{}])", elems.join(", ")))
            }
            ExprKind::Index { value, index } => {
                let base = self.gen_expr(value)?;
                if let Some(Type::Tuple(_)) = value.ty.as_ref() {
                    if let ExprKind::Literal(Literal::Int(idx)) = &index.kind {
                        return Ok(format!("({}).{}", base, idx));
                    }
                }
                if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                    let idx = self.gen_expr(index)?;
                    self.uses.py_dict_get = true;
                    self.uses.hash_map = true;
                    return Ok(self.wrap_result(format!("py_dict_get(&{}, &{})", base, idx)));
                }
                // Handle list/tuple indexing with negative index support
                if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                    let idx_expr = self.gen_expr(index)?;
                    self.uses.py_list_get = true;
                    return Ok(self.wrap_result(format!("py_list_get(&{}, {})", base, idx_expr)));
                }
                let idx = self.gen_expr(index)?;
                Ok(format!("{}[{}]", base, idx))
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                let base = self.gen_expr(value)?;
                let start_arg = match start.as_deref() {
                    Some(s) => format!("Some({})", self.gen_expr(s)?),
                    None => "None".to_string(),
                };
                let end_arg = match end.as_deref() {
                    Some(e) => format!("Some({})", self.gen_expr(e)?),
                    None => "None".to_string(),
                };
                if matches!(value.ty.as_ref(), Some(Type::Str)) {
                    // Use character-based slicing for Python string semantics
                    if let Some(step) = step.as_deref() {
                        self.uses.py_str_slice_step = true;
                        let step_arg = self.gen_expr(step)?;
                        return Ok(self.wrap_result(format!(
                            "py_str_slice_step(&{}, {}, {}, {})",
                            base, start_arg, end_arg, step_arg
                        )));
                    }
                    self.uses.py_str_slice = true;
                    return Ok(format!(
                        "py_str_slice(&{}, {}, {})",
                        base, start_arg, end_arg
                    ));
                }
                if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                    if let Some(step) = step.as_deref() {
                        self.uses.py_list_slice_step = true;
                        let step_arg = self.gen_expr(step)?;
                        return Ok(self.wrap_result(format!(
                            "py_list_slice_step(&{}, {}, {}, {})",
                            base, start_arg, end_arg, step_arg
                        )));
                    }
                    let range = self.slice_range(start.as_deref(), end.as_deref())?;
                    return Ok(format!("{}[{}].to_vec()", base, range));
                }
                Err(self.error(expr.span, "Slicing requires list or str"))
            }
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
            } => {
                let tmp = self.new_tmp();
                let mut out = String::new();
                out.push('{');
                out.push_str(&format!(" let mut {} = Vec::new();", tmp));
                out.push_str(&format!(
                    " for {} in {}.into_iter() {{",
                    target,
                    self.gen_expr(iter)?
                ));
                if ifs.is_empty() {
                    out.push_str(&format!(" {}.push({});", tmp, self.gen_expr(elt)?));
                } else {
                    let conds: Result<Vec<String>, CompileError> =
                        ifs.iter().map(|c| self.gen_expr(c)).collect();
                    out.push_str(&format!(
                        " if {} {{ {}.push({}); }}",
                        conds?.join(" && "),
                        tmp,
                        self.gen_expr(elt)?
                    ));
                }
                out.push_str(" }");
                out.push_str(&format!(" {} }}", tmp));
                Ok(out)
            }
            ExprKind::Lambda { params, body } => {
                let param_types = if let Some(Type::Lambda { params, .. }) = expr.ty.as_ref() {
                    Some(params.as_slice())
                } else {
                    None
                };
                self.gen_lambda_with_param_types(params, body, param_types)
            }
            ExprKind::IfExpr { test, body, orelse } => Ok(format!(
                "if {} {{ {} }} else {{ {} }}",
                self.gen_expr(test)?,
                self.gen_expr(body)?,
                self.gen_expr(orelse)?
            )),
            ExprKind::Block { stmts } => self.gen_block_expr(stmts),
            ExprKind::UnionCtor {
                union,
                variant,
                inner,
            } => Ok(format!("{}::{}({})", union, variant, self.gen_expr(inner)?)),
        }
    }

    fn gen_lambda_with_param_types(
        &mut self,
        params: &[String],
        body: &Expr,
        param_types: Option<&[Type]>,
    ) -> Result<String, CompileError> {
        let mut param_parts = Vec::new();
        let mut lambda_param_types: Vec<Type> = Vec::new();
        if let Some(param_tys) = param_types {
            for (name, ty) in params.iter().zip(param_tys.iter()) {
                lambda_param_types.push(ty.clone());
                if matches!(ty, Type::Unknown) {
                    param_parts.push(name.clone());
                } else {
                    param_parts.push(format!("{}: {}", name, self.rust_type(ty)));
                }
            }
        } else {
            param_parts.extend(params.iter().cloned());
            lambda_param_types.resize(params.len(), Type::Unknown);
        }
        let saved_locals = self.local_vars.clone();
        let mut scoped_locals = saved_locals.clone().unwrap_or_default();
        for (name, ty) in params.iter().zip(lambda_param_types.iter()) {
            scoped_locals.insert(name.clone(), ty.clone());
        }
        self.local_vars = Some(scoped_locals);
        let body_expr = self.gen_expr(body)?;
        self.local_vars = saved_locals;
        Ok(format!(
            "move |{}| {{ {} }}",
            param_parts.join(", "),
            body_expr
        ))
    }

    fn gen_args(&mut self, args: &[Expr]) -> Result<String, CompileError> {
        let parts: Result<Vec<String>, CompileError> =
            args.iter().map(|a| self.gen_expr(a)).collect();
        Ok(parts?.join(", "))
    }

    pub(crate) fn gen_expr_with_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Type>,
    ) -> Result<String, CompileError> {
        if let Some(Type::Lambda { params, .. }) = expected {
            if let ExprKind::Lambda {
                params: names,
                body,
            } = &expr.kind
            {
                return self.gen_lambda_with_param_types(names, body, Some(params.as_slice()));
            }
        }
        if let Some(Type::Option(_)) = expected {
            if matches!(expr.ty.as_ref(), Some(Type::Option(_))) {
                return self.gen_expr(expr);
            }
            if matches!(expr.kind, ExprKind::Literal(Literal::None)) {
                return Ok("None".to_string());
            }
            let inner = self.gen_expr(expr)?;
            return Ok(format!("Some({})", inner));
        }
        self.gen_expr(expr)
    }

    /// Generate arguments for a user-defined function call, adding & where needed
    fn gen_call_args(&mut self, func_name: &str, args: &[Expr]) -> Result<String, CompileError> {
        // Look up the function signature
        let param_types: Vec<Type> = if let Some(sig) = self.ctx.functions.get(func_name) {
            sig.params
                .iter()
                .map(|t| self.to_borrowed_param_type(t))
                .collect()
        } else {
            // Fallback: no signature found, use simple args
            return self.gen_args(args);
        };

        let mut parts = Vec::new();
        for (idx, arg) in args.iter().enumerate() {
            let rendered = if let Some(param_ty) = param_types.get(idx) {
                self.gen_expr_with_expected(arg, Some(param_ty))?
            } else {
                self.gen_expr(arg)?
            };
            // Check if this parameter expects a reference
            if let Some(param_ty) = param_types.get(idx) {
                if self.needs_borrow(arg.ty.as_ref(), param_ty) {
                    parts.push(format!("&{}", rendered));
                } else {
                    parts.push(rendered);
                }
            } else {
                parts.push(rendered);
            }
        }
        Ok(parts.join(", "))
    }

    /// Check if we need to add & when passing an argument
    fn needs_borrow(&self, arg_ty: Option<&Type>, param_ty: &Type) -> bool {
        match param_ty {
            // Parameter expects a slice, argument is a list
            Type::Slice(_) => {
                matches!(arg_ty, Some(Type::List(_)))
            }
            // Parameter expects &str, argument is String
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Str) => {
                matches!(arg_ty, Some(Type::Str))
            }
            // Parameter expects &HashMap, argument is HashMap
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Dict(_, _)) => {
                matches!(arg_ty, Some(Type::Dict(_, _)))
            }
            // Parameter expects &HashSet, argument is HashSet
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Set(_)) => {
                matches!(arg_ty, Some(Type::Set(_)))
            }
            // Parameter expects &Custom, argument is Custom
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Custom(_)) => {
                matches!(arg_ty, Some(Type::Custom(_)))
            }
            // Parameter expects &Union, argument is Union
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Union(_)) => {
                matches!(arg_ty, Some(Type::Union(_)))
            }
            _ => false,
        }
    }

    fn gen_numeric_operand(
        &mut self,
        expr: &Expr,
        target_float: bool,
    ) -> Result<String, CompileError> {
        let rendered = self.gen_expr(expr)?;
        if target_float && matches!(expr.ty.as_ref(), Some(Type::Int)) {
            return Ok(format!("({} as f64)", rendered));
        }
        Ok(rendered)
    }

    pub(crate) fn gen_iter_source(&mut self, expr: &Expr) -> Result<String, CompileError> {
        let rendered = self.gen_expr(expr)?;
        let use_owned = match &expr.kind {
            ExprKind::Name(name) => self.is_global(name),
            _ => true,
        };
        match expr.ty.as_ref() {
            // Slice references: just .iter() - items are already references
            Some(Type::Slice(_)) => Ok(format!("{}.iter().copied()", rendered)),
            // Owned lists/sets need .iter().cloned() (or .copied() for Copy types)
            Some(Type::List(inner)) | Some(Type::Set(inner)) => {
                if use_owned {
                    Ok(format!("{}.into_iter()", rendered))
                } else if self.is_copy_type(inner) {
                    Ok(format!("{}.iter().copied()", rendered))
                } else {
                    Ok(format!("{}.iter().cloned()", rendered))
                }
            }
            Some(Type::Str) => {
                if use_owned {
                    Ok(format!(
                        "{}.chars().map(|c| c.to_string()).collect::<Vec<_>>().into_iter()",
                        rendered
                    ))
                } else {
                    Ok(format!("{}.chars().map(|c| c.to_string())", rendered))
                }
            }
            Some(Type::Tuple(items)) => {
                if items.is_empty() {
                    return Ok("std::iter::empty::<()>()".to_string());
                }
                let tmp = self.new_tmp();
                let mut elems = Vec::new();
                for (idx, ty) in items.iter().enumerate() {
                    if self.is_copy_type(ty) {
                        elems.push(format!("{}.{}", tmp, idx));
                    } else {
                        elems.push(format!("{}.{}.clone()", tmp, idx));
                    }
                }
                if use_owned {
                    Ok(format!(
                        "{{ let {} = {}; vec![{}].into_iter() }}",
                        tmp,
                        rendered,
                        elems.join(", ")
                    ))
                } else {
                    Ok(format!(
                        "{{ let {} = &{}; vec![{}].into_iter() }}",
                        tmp,
                        rendered,
                        elems.join(", ")
                    ))
                }
            }
            // References to collections
            Some(Type::Ref(inner)) => match inner.as_ref() {
                Type::Set(elem) => {
                    if self.is_copy_type(elem) {
                        Ok(format!("{}.iter().copied()", rendered))
                    } else {
                        Ok(format!("{}.iter().cloned()", rendered))
                    }
                }
                _ => Ok(format!("{}.iter()", rendered)),
            },
            _ => Ok(format!("{}.into_iter()", rendered)),
        }
    }

    /// Check if a type implements Copy (primitives)
    fn is_copy_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float | Type::Bool)
    }

    pub(crate) fn iter_item_type_hint(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::List(inner) | Type::Set(inner) => Some(*inner.clone()),
            Type::Dict(key, _) => Some(*key.clone()),
            Type::Tuple(items) => {
                if items.is_empty() {
                    None
                } else if items.iter().all(|t| t == &items[0]) {
                    Some(items[0].clone())
                } else {
                    None
                }
            }
            Type::Str => Some(Type::Str),
            Type::Iterator(inner) => Some(*inner.clone()),
            Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => {
                self.iter_item_type_hint(inner)
            }
            _ => None,
        }
    }

    fn truthy_expr_for_type(&self, expr_str: &str, ty: &Type) -> String {
        let expr = match ty {
            Type::Bool => expr_str.to_string(),
            Type::Int => format!("{} != 0", expr_str),
            Type::Float => format!("{} != 0.0", expr_str),
            Type::Str => format!("!{}.is_empty()", expr_str),
            Type::List(_) | Type::Set(_) | Type::Dict(_, _) => format!("!{}.is_empty()", expr_str),
            Type::Tuple(items) => {
                if items.is_empty() {
                    "false".to_string()
                } else {
                    "true".to_string()
                }
            }
            Type::None => "false".to_string(),
            Type::Option(inner) => {
                let inner_expr = self.truthy_expr_for_type("v", inner);
                format!(
                    "match {} {{ Some(v) => {}, None => false }}",
                    expr_str, inner_expr
                )
            }
            Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => {
                self.truthy_expr_for_type(expr_str, inner)
            }
            _ => "true".to_string(),
        };
        format!("({})", expr)
    }

    fn in_throwing_context(&self) -> bool {
        if self.try_block_return_type.is_some() {
            return true;
        }
        if let Some(Type::Result(_, _)) = self.current_function_ret.as_ref() {
            return true;
        }
        self.current_function.is_none() && self.top_level_can_throw
    }

    pub(crate) fn wrap_result(&self, expr: String) -> String {
        if self.in_throwing_context() {
            format!("({}?)", expr)
        } else {
            format!("{}.unwrap()", expr)
        }
    }

    fn wrap_parse_result(&self, expr: String) -> String {
        self.wrap_result(expr)
    }

    fn python_type_name(&self, ty: &Type) -> Option<String> {
        let name = match ty {
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Str => "str",
            Type::None => "NoneType",
            Type::List(_) => "list",
            Type::Tuple(_) => "tuple",
            Type::Dict(_, _) => "dict",
            Type::Set(_) => "set",
            Type::Custom(name) | Type::Union(name) => return Some(name.clone()),
            _ => return None,
        };
        Some(name.to_string())
    }

    fn python_type_class(&self, ty: &Type) -> Option<String> {
        self.python_type_name(ty)
            .map(|name| format!("<class '{}'>", name))
    }

    /// Check if a type is already a reference type
    fn is_reference_type(&self, ty: Option<&Type>) -> bool {
        matches!(
            ty,
            Some(Type::Ref(_)) | Some(Type::MutRef(_)) | Some(Type::Slice(_))
        )
    }

    /// Check if a name refers to a borrowed parameter
    fn is_borrowed_param(&self, name: &str) -> bool {
        self.borrowed_params.contains(name)
    }

    fn gen_block_expr(&mut self, stmts: &[Stmt]) -> Result<String, CompileError> {
        let mut_counts = collect_assign_counts(stmts);
        let saved_out = mem::take(&mut self.out);
        let saved_indent = self.indent;
        let saved_tmp = self.tmp_counter;
        self.out = String::new();
        self.indent = 0;
        self.push_line("{");
        self.indent += 1;
        for stmt in stmts {
            self.emit_stmt(stmt, &mut_counts)?;
        }
        self.indent -= 1;
        self.push_line("}");
        let block = self.out.trim_end().to_string();
        self.out = saved_out;
        self.indent = saved_indent;
        self.tmp_counter = saved_tmp;
        Ok(block)
    }

    fn slice_range(
        &mut self,
        start: Option<&Expr>,
        end: Option<&Expr>,
    ) -> Result<String, CompileError> {
        // For slicing, we can't easily use py_index without knowing the length at this point
        // Slicing with negative indices requires runtime handling
        let start_str = match start {
            Some(expr) => format!("{} as usize", self.gen_expr(expr)?),
            None => String::new(),
        };
        let end_str = match end {
            Some(expr) => format!("{} as usize", self.gen_expr(expr)?),
            None => String::new(),
        };
        Ok(format!("{}..{}", start_str, end_str))
    }
    pub(crate) fn print_needs_debug(&self, expr: &Expr) -> bool {
        let ty = match expr.ty.as_ref() {
            Some(Type::Unknown) | None => {
                if let ExprKind::Name(name) = &expr.kind {
                    self.local_var_type(name)
                } else {
                    None
                }
            }
            Some(other) => Some(other),
        };
        match ty {
            Some(Type::Int | Type::Float | Type::Bool | Type::Str | Type::None) => false,
            Some(_) => true,
            None => true,
        }
    }
}
