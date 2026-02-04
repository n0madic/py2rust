use super::util::collect_assign_counts;
use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn gen_expr(&mut self, expr: &Expr) -> Result<String, CompileError> {
        match &expr.kind {
            ExprKind::Literal(lit) => match lit {
                Literal::Int(v) => Ok(format!("{}i64", v)),
                Literal::Float(v) => Ok(format!("{}f64", v)),
                Literal::Bool(v) => Ok(format!("{}", v)),
                Literal::Str(s) => Ok(format!("String::from({s:?})")),
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
                            let spec = if self.print_needs_debug(arg) {
                                "{:?}"
                            } else {
                                "{}"
                            };
                            fmt.push_str(spec);
                            vals.push(self.gen_expr(arg)?);
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
                            return Ok(format!(
                                "({}..{}).step_by({} as usize)",
                                self.gen_expr(&args[0])?,
                                self.gen_expr(&args[1])?,
                                self.gen_expr(&args[2])?
                            ));
                        }
                    }
                    if name == "round" {
                        if args.len() == 1 {
                            let arg_expr = self.gen_expr(&args[0])?;
                            if matches!(args[0].ty.as_ref(), Some(Type::Float)) {
                                return Ok(format!("({}.round() as i64)", arg_expr));
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
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "list() expects one argument"));
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
                        let func_expr = match &args[0].kind {
                            ExprKind::Name(n) if n == "str" => "|x| x.to_string()".to_string(),
                            _ => self.gen_expr(&args[0])?,
                        };
                        return Ok(format!("{}.map({})", iter_expr, func_expr));
                    }
                    if name == "filter" {
                        if args.len() != 2 {
                            return Err(self.error(expr.span, "filter() expects two arguments"));
                        }
                        let iter_expr = self.gen_iter_source(&args[1])?;
                        let pred_expr = self.gen_expr(&args[0])?;
                        return Ok(format!(
                            "{}.filter(|x| {{ let x = x.clone(); ({}) (x) }})",
                            iter_expr, pred_expr
                        ));
                    }
                    if name == "all" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "all() expects one argument"));
                        }
                        let iter_expr = self.gen_iter_source(&args[0])?;
                        return Ok(format!("{}.all(|v| v)", iter_expr));
                    }
                    if name == "any" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "any() expects one argument"));
                        }
                        let iter_expr = self.gen_iter_source(&args[0])?;
                        return Ok(format!("{}.any(|v| v)", iter_expr));
                    }
                    if name == "reversed" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "reversed() expects one argument"));
                        }
                        let iter_expr = self.gen_iter_source(&args[0])?;
                        return Ok(format!("{}.rev()", iter_expr));
                    }
                    if name == "max" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "max() expects one argument"));
                        }
                        self.uses.py_max = true;
                        let iter_expr = self.gen_iter_source(&args[0])?;
                        return Ok(format!("py_max({})", iter_expr));
                    }
                    if name == "min" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "min() expects one argument"));
                        }
                        self.uses.py_min = true;
                        let iter_expr = self.gen_iter_source(&args[0])?;
                        return Ok(format!("py_min({})", iter_expr));
                    }
                    if name == "int" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "int() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Str) => {
                                self.uses.py_parse_int = true;
                                format!("py_parse_int(&{})", arg_expr)
                            }
                            Some(Type::Float) => format!("{} as i64", arg_expr),
                            Some(Type::Bool) => format!("if {} {{ 1 }} else {{ 0 }}", arg_expr),
                            Some(Type::Int) => arg_expr,
                            _ => {
                                self.uses.py_parse_int = true;
                                format!("py_parse_int(&{}.to_string())", arg_expr)
                            }
                        });
                    }
                    if name == "float" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "float() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Str) => {
                                self.uses.py_parse_float = true;
                                format!("py_parse_float(&{})", arg_expr)
                            }
                            Some(Type::Int) => format!("{} as f64", arg_expr),
                            Some(Type::Bool) => format!("if {} {{ 1.0 }} else {{ 0.0 }}", arg_expr),
                            Some(Type::Float) => arg_expr,
                            _ => {
                                self.uses.py_parse_float = true;
                                format!("py_parse_float(&{}.to_string())", arg_expr)
                            }
                        });
                    }
                    if name == "str" {
                        if args.len() != 1 {
                            return Err(self.error(expr.span, "str() expects one argument"));
                        }
                        let arg_expr = self.gen_expr(&args[0])?;
                        return Ok(match args[0].ty.as_ref() {
                            Some(Type::Str) => arg_expr,
                            _ => format!("{}.to_string()", arg_expr),
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
                        self.uses.type_name = true;
                        return Ok(format!("py_type_name(&{})", self.gen_expr(&args[0])?));
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
                            return Ok(format!(
                                "{}.push({})",
                                self.gen_expr(value)?,
                                self.gen_args(args)?
                            ));
                        }
                    }
                    if attr == "add" {
                        if let Some(Type::Set(_)) = value.ty.as_ref() {
                            self.uses.hash_set = true;
                            return Ok(format!(
                                "{}.insert({})",
                                self.gen_expr(value)?,
                                self.gen_args(args)?
                            ));
                        }
                    }
                    if attr == "remove" {
                        if let Some(Type::Set(_)) = value.ty.as_ref() {
                            self.uses.hash_set = true;
                            return Ok(format!(
                                "{}.remove(&{})",
                                self.gen_expr(value)?,
                                self.gen_args(args)?
                            ));
                        }
                    }
                    if attr == "format" {
                        if let ExprKind::Literal(Literal::Str(fmt)) = &value.kind {
                            let fmt_lit = format!("{fmt:?}");
                            if args.is_empty() {
                                return Ok(format!("String::from({})", fmt_lit));
                            }
                            let vals = self.gen_args(args)?;
                            return Ok(format!("format!({}, {})", fmt_lit, vals));
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
                                    comps.push(format!(
                                        "{} == &{}.{}",
                                        left_tmp, right_tmp, idx
                                    ));
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
                let elems: Result<Vec<String>, CompileError> =
                    items.iter().map(|e| self.gen_expr(e)).collect();
                Ok(format!("vec![{}]", elems?.join(", ")))
            }
            ExprKind::Tuple(items) => {
                let elems: Result<Vec<String>, CompileError> =
                    items.iter().map(|e| self.gen_expr(e)).collect();
                let joined = elems?.join(", ");
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
                let mut pairs = Vec::new();
                for (k, v) in items {
                    pairs.push(format!("({}, {})", self.gen_expr(k)?, self.gen_expr(v)?));
                }
                Ok(format!("HashMap::from([{}])", pairs.join(", ")))
            }
            ExprKind::Set(items) => {
                self.uses.hash_set = true;
                if items.is_empty() {
                    return Ok("HashSet::new()".to_string());
                }
                let mut elems = Vec::new();
                for item in items {
                    elems.push(self.gen_expr(item)?);
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
                    return Ok(format!(
                        "{}.get(&{}).cloned().expect(\"KeyError\")",
                        base, idx
                    ));
                }
                // Handle list/tuple indexing with negative index support
                if matches!(
                    value.ty.as_ref(),
                    Some(Type::List(_)) | Some(Type::Tuple(_))
                ) {
                    let idx_expr = self.gen_expr(index)?;
                    if self.may_be_negative(index) {
                        self.uses.py_index = true;
                        return Ok(format!("{}[py_index({}, {}.len())]", base, idx_expr, base));
                    }
                    return Ok(format!("{}[{} as usize]", base, idx_expr));
                }
                let idx = self.gen_expr(index)?;
                Ok(format!("{}[{}]", base, idx))
            }
            ExprKind::Slice { value, start, end } => {
                let base = self.gen_expr(value)?;
                if matches!(value.ty.as_ref(), Some(Type::Str)) {
                    // Use character-based slicing for Python string semantics
                    self.uses.py_str_slice = true;
                    let start_arg = match start {
                        Some(s) => format!("Some({})", self.gen_expr(s)?),
                        None => "None".to_string(),
                    };
                    let end_arg = match end {
                        Some(e) => format!("Some({})", self.gen_expr(e)?),
                        None => "None".to_string(),
                    };
                    Ok(format!(
                        "py_str_slice(&{}, {}, {})",
                        base, start_arg, end_arg
                    ))
                } else {
                    let range = self.slice_range(start.as_deref(), end.as_deref())?;
                    Ok(format!("{}[{}].to_vec()", base, range))
                }
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
                let body_expr = self.gen_expr(body)?;
                let mut param_parts = Vec::new();
                if let Some(Type::Lambda {
                    params: param_tys, ..
                }) = expr.ty.as_ref()
                {
                    for (name, ty) in params.iter().zip(param_tys.iter()) {
                        if matches!(ty, Type::Unknown) {
                            param_parts.push(name.clone());
                        } else {
                            param_parts.push(format!("{}: {}", name, self.rust_type(ty)));
                        }
                    }
                } else {
                    param_parts.extend(params.iter().cloned());
                }
                Ok(format!(
                    "move |{}| {{ {} }}",
                    param_parts.join(", "),
                    body_expr
                ))
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

    fn gen_args(&mut self, args: &[Expr]) -> Result<String, CompileError> {
        let parts: Result<Vec<String>, CompileError> =
            args.iter().map(|a| self.gen_expr(a)).collect();
        Ok(parts?.join(", "))
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
            let rendered = self.gen_expr(arg)?;
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
        match expr.ty.as_ref() {
            // Slice references: just .iter() - items are already references
            Some(Type::Slice(_)) => Ok(format!("{}.iter().copied()", rendered)),
            // Owned lists/sets need .iter().cloned() (or .copied() for Copy types)
            Some(Type::List(inner)) | Some(Type::Set(inner)) => {
                if self.is_copy_type(inner) {
                    Ok(format!("{}.iter().copied()", rendered))
                } else {
                    Ok(format!("{}.iter().cloned()", rendered))
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
        match expr.ty.as_ref() {
            Some(Type::Int | Type::Float | Type::Bool | Type::Str | Type::None) => false,
            Some(_) => true,
            None => true,
        }
    }

    /// Check if an index expression might be negative.
    /// Returns true for variables and expressions that could be negative,
    /// false for non-negative literals.
    pub(crate) fn may_be_negative(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Literal(Literal::Int(n)) => *n < 0,
            // Variables, binary expressions, and other forms could be negative
            ExprKind::Name(_) => true,
            ExprKind::Binary { .. } => true,
            ExprKind::Unary {
                op: UnaryOp::Neg, ..
            } => true,
            ExprKind::Call { .. } => true,
            ExprKind::IfExpr { .. } => true,
            // If it's a constant non-negative literal, it's safe
            _ => false,
        }
    }
}
