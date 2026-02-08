// Operator expressions (binary, unary, comparisons, boolean ops).

use super::super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn gen_list_concat_expr_with_storage(
        &mut self,
        left: &Expr,
        right: &Expr,
        storage: ListStorage,
    ) -> Result<String, CompileError> {
        let left_expr = self.gen_expr(left)?;
        let right_expr = self.gen_expr(right)?;
        let left_local = matches!(self.list_storage_for_expr(left), ListStorage::Local);
        let right_local = matches!(self.list_storage_for_expr(right), ListStorage::Local);
        let out_tmp = self.new_tmp();
        let mut steps = Vec::new();

        if left_local {
            steps.push(format!(
                "let mut {out_tmp} = {left_expr}.clone()",
                out_tmp = out_tmp,
                left_expr = left_expr
            ));
        } else {
            let left_tmp = self.new_tmp();
            let left_guard = self.new_tmp();
            let left_init = if matches!(left.kind, ExprKind::Name(_)) {
                format!("{}.clone()", left_expr)
            } else {
                left_expr
            };
            steps.push(format!(
                "let {left_tmp} = {left_init}",
                left_tmp = left_tmp,
                left_init = left_init
            ));
            steps.push(format!(
                "let {left_guard} = {left_tmp}.lock().expect(\"list mutex poisoned\")",
                left_guard = left_guard,
                left_tmp = left_tmp
            ));
            steps.push(format!(
                "let mut {out_tmp} = {left_guard}.iter().cloned().collect::<Vec<_>>()",
                out_tmp = out_tmp,
                left_guard = left_guard
            ));
        }

        if right_local {
            steps.push(format!(
                "{out_tmp}.extend({right_expr}.iter().cloned())",
                out_tmp = out_tmp,
                right_expr = right_expr
            ));
        } else {
            let right_tmp = self.new_tmp();
            let right_guard = self.new_tmp();
            let right_init = if matches!(right.kind, ExprKind::Name(_)) {
                format!("{}.clone()", right_expr)
            } else {
                right_expr
            };
            steps.push(format!(
                "let {right_tmp} = {right_init}",
                right_tmp = right_tmp,
                right_init = right_init
            ));
            steps.push(format!(
                "let {right_guard} = {right_tmp}.lock().expect(\"list mutex poisoned\")",
                right_guard = right_guard,
                right_tmp = right_tmp
            ));
            steps.push(format!(
                "{out_tmp}.extend({right_guard}.iter().cloned())",
                out_tmp = out_tmp,
                right_guard = right_guard
            ));
        }

        let base = format!("{{ {}; {} }}", steps.join("; "), out_tmp);
        Ok(self.wrap_list_storage_expr(&base, storage))
    }

    /// Lower a binary operation expression.
    pub(super) fn gen_binary_expr(
        &mut self,
        expr: &Expr,
        op: &BinOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<String, CompileError> {
        if matches!(op, BinOp::Add) {
            let left_is_str = matches!(left.ty.as_ref(), Some(Type::Str));
            let right_is_str = matches!(right.ty.as_ref(), Some(Type::Str));
            if left_is_str || right_is_str {
                let left_is_list = matches!(left.ty.as_ref(), Some(Type::List(_)));
                let right_is_list = matches!(right.ty.as_ref(), Some(Type::List(_)));
                let left_spec = if left_is_list {
                    "{}"
                } else if self.print_needs_debug(left) {
                    "{:?}"
                } else {
                    "{}"
                };
                let right_spec = if right_is_list {
                    "{}"
                } else if self.print_needs_debug(right) {
                    "{:?}"
                } else {
                    "{}"
                };
                let left_expr = if left_is_list {
                    self.list_str_expr(left)?
                } else if left_spec == "{:?}" {
                    self.debug_arg_expr(left)?
                } else {
                    self.gen_expr(left)?
                };
                let right_expr = if right_is_list {
                    self.list_str_expr(right)?
                } else if right_spec == "{:?}" {
                    self.debug_arg_expr(right)?
                } else {
                    self.gen_expr(right)?
                };
                let left_lit = match &left.kind {
                    ExprKind::Literal(Literal::Str(s)) => Some(s.as_str()),
                    _ => None,
                };
                let right_lit = match &right.kind {
                    ExprKind::Literal(Literal::Str(s)) => Some(s.as_str()),
                    _ => None,
                };
                if let (Some(left_lit), Some(right_lit)) = (left_lit, right_lit) {
                    let combined = format!("{left}{right}", left = left_lit, right = right_lit);
                    return Ok(format!("{:?}.to_string()", combined));
                }
                if let Some(left_lit) = left_lit {
                    let fmt = format!("{}{}", self.escape_format_literal(left_lit), right_spec);
                    let fmt_lit = format!("{:?}", fmt);
                    return Ok(format!("format!({}, {})", fmt_lit, right_expr));
                }
                if let Some(right_lit) = right_lit {
                    let fmt = format!("{}{}", left_spec, self.escape_format_literal(right_lit));
                    let fmt_lit = format!("{:?}", fmt);
                    return Ok(format!("format!({}, {})", fmt_lit, left_expr));
                }
                return Ok(format!(
                    "format!(\"{}{}\", {}, {})",
                    left_spec, right_spec, left_expr, right_expr
                ));
            }
        }
        if matches!(op, BinOp::Add)
            && matches!(left.ty.as_ref(), Some(Type::List(_)))
            && matches!(right.ty.as_ref(), Some(Type::List(_)))
        {
            return self.gen_list_concat_expr_with_storage(left, right, ListStorage::Shared);
        }
        if matches!(op, BinOp::Mul) {
            let left_is_str = matches!(left.ty.as_ref(), Some(Type::Str));
            let right_is_str = matches!(right.ty.as_ref(), Some(Type::Str));
            if left_is_str && matches!(right.ty.as_ref(), Some(Type::Int)) {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let str_tmp = self.new_tmp();
                let count_tmp = self.new_tmp();
                // Evaluate both operands once and guard against negative repeat counts.
                let str_init = if matches!(left.kind, ExprKind::Name(_)) {
                    format!("{}.clone()", left_expr)
                } else {
                    left_expr
                };
                return Ok(format!(
                    "{{ let {str_tmp} = {str_init}; let {count_tmp} = {count_expr}; if {count_tmp} <= 0 {{ \"\".to_string() }} else {{ {str_tmp}.repeat({count_tmp} as usize) }} }}",
                    str_tmp = str_tmp,
                    str_init = str_init,
                    count_tmp = count_tmp,
                    count_expr = right_expr
                ));
            }
            if right_is_str && matches!(left.ty.as_ref(), Some(Type::Int)) {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let str_tmp = self.new_tmp();
                let count_tmp = self.new_tmp();
                // Evaluate both operands once and guard against negative repeat counts.
                let str_init = if matches!(right.kind, ExprKind::Name(_)) {
                    format!("{}.clone()", right_expr)
                } else {
                    right_expr
                };
                return Ok(format!(
                    "{{ let {str_tmp} = {str_init}; let {count_tmp} = {count_expr}; if {count_tmp} <= 0 {{ \"\".to_string() }} else {{ {str_tmp}.repeat({count_tmp} as usize) }} }}",
                    str_tmp = str_tmp,
                    str_init = str_init,
                    count_tmp = count_tmp,
                    count_expr = left_expr
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
            let left_is_set = matches!(left.ty.as_ref(), Some(Type::Set(_)));
            let right_is_set = matches!(right.ty.as_ref(), Some(Type::Set(_)));
            if left_is_set && right_is_set {
                // Set ops borrow both sides to avoid moves.
                return Ok(format!(
                    "(&{} {} &{})",
                    self.gen_expr(left)?,
                    op_str,
                    self.gen_expr(right)?
                ));
            }
            // Integer bitwise ops use the plain operator.
            return Ok(format!(
                "({} {} {})",
                self.gen_expr(left)?,
                op_str,
                self.gen_expr(right)?
            ));
        }
        if matches!(op, BinOp::ShiftLeft | BinOp::ShiftRight) {
            let op_str = match op {
                BinOp::ShiftLeft => "<<",
                BinOp::ShiftRight => ">>",
                _ => unreachable!(),
            };
            // Rust shifts expect RHS as u32/u64; cast to keep i64 RHS working.
            return Ok(format!(
                "({} {} ({} as u32))",
                self.gen_expr(left)?,
                op_str,
                self.gen_expr(right)?
            ));
        }
        if matches!(op, BinOp::Sub) {
            if let (Some(Type::Set(_)), Some(Type::Set(_))) = (left.ty.as_ref(), right.ty.as_ref())
            {
                return Ok(format!(
                    "(&{} - &{})",
                    self.gen_expr(left)?,
                    self.gen_expr(right)?
                ));
            }
        }
        if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul)
            && matches!(expr.ty.as_ref(), Some(Type::Int))
            && self.current_function.is_some()
        {
            let left_expr = self.gen_numeric_operand(left, false)?;
            let right_expr = self.gen_numeric_operand(right, false)?;
            let checked_method = match op {
                BinOp::Add => "checked_add",
                BinOp::Sub => "checked_sub",
                BinOp::Mul => "checked_mul",
                _ => unreachable!(),
            };
            self.uses.py_error = true;
            self.uses.py_int = true;
            let left_checked = format!("py_int({})", left_expr);
            let right_checked = format!("py_int({})", right_expr);
            let guarded = format!(
                "{}.{}({}).ok_or_else(|| PyError::OverflowError(\"integer overflow\".into()))",
                left_checked, checked_method, right_checked
            );
            return Ok(self.wrap_result(guarded));
        }
        if matches!(op, BinOp::FloorDiv) {
            let is_float = matches!(expr.ty.as_ref(), Some(Type::Float));
            let left_expr = self.gen_numeric_operand(left, is_float)?;
            let right_expr = self.gen_numeric_operand(right, is_float)?;
            let right_is_zero_literal = matches!(&right.kind, ExprKind::Literal(Literal::Int(0)))
                || matches!(&right.kind, ExprKind::Literal(Literal::Float(v)) if *v == 0.0);
            let needs_zero_guard = right_is_zero_literal || self.current_function.is_some();
            if needs_zero_guard {
                self.uses.py_error = true;
                let rhs_tmp = self.new_tmp();
                let guarded = if is_float {
                    format!(
                        "{{ let {rhs_tmp} = {right_expr}; if {rhs_tmp} == 0.0f64 {{ Err(PyError::ZeroDivisionError(\"division by zero\".into())) }} else {{ Ok(({left_expr} / {rhs_tmp}).floor()) }} }}"
                    )
                } else {
                    format!(
                        "{{ let {rhs_tmp} = {right_expr}; if {rhs_tmp} == 0i64 {{ Err(PyError::ZeroDivisionError(\"division by zero\".into())) }} else {{ Ok({left_expr}.div_euclid({rhs_tmp})) }} }}"
                    )
                };
                return Ok(self.wrap_result(guarded));
            }
            if is_float {
                return Ok(format!("(({} / {}).floor())", left_expr, right_expr));
            }
            return Ok(format!("({}.div_euclid({}))", left_expr, right_expr));
        }
        if matches!(op, BinOp::Mod) {
            let is_float = matches!(expr.ty.as_ref(), Some(Type::Float));
            let left_expr = self.gen_numeric_operand(left, is_float)?;
            let right_expr = self.gen_numeric_operand(right, is_float)?;
            let right_is_zero_literal = matches!(&right.kind, ExprKind::Literal(Literal::Int(0)))
                || matches!(&right.kind, ExprKind::Literal(Literal::Float(v)) if *v == 0.0);
            let needs_zero_guard = right_is_zero_literal || self.current_function.is_some();
            let lhs_tmp = self.new_tmp();
            let rhs_tmp = self.new_tmp();
            let rem_tmp = self.new_tmp();
            if needs_zero_guard {
                self.uses.py_error = true;
                let guarded = if is_float {
                    format!(
                        "{{ let {lhs_tmp} = {left_expr}; let {rhs_tmp} = {right_expr}; if {rhs_tmp} == 0.0f64 {{ Err(PyError::ZeroDivisionError(\"division by zero\".into())) }} else {{ let {rem_tmp} = {lhs_tmp} % {rhs_tmp}; Ok((({rem_tmp} + {rhs_tmp}) % {rhs_tmp})) }} }}"
                    )
                } else {
                    format!(
                        "{{ let {lhs_tmp} = {left_expr}; let {rhs_tmp} = {right_expr}; if {rhs_tmp} == 0i64 {{ Err(PyError::ZeroDivisionError(\"division by zero\".into())) }} else {{ let {rem_tmp} = {lhs_tmp} % {rhs_tmp}; Ok((({rem_tmp} + {rhs_tmp}) % {rhs_tmp})) }} }}"
                    )
                };
                return Ok(self.wrap_result(guarded));
            }
            return Ok(format!(
                "{{ let {lhs_tmp} = {left_expr}; let {rhs_tmp} = {right_expr}; let {rem_tmp} = {lhs_tmp} % {rhs_tmp}; (({rem_tmp} + {rhs_tmp}) % {rhs_tmp}) }}"
            ));
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
                // Optimize tuple concatenation by inlining literal tuple elements.
                let mut elems = Vec::new();
                let mut setup = Vec::new();
                let mut needs_block = false;

                // Generate left elements.
                if let ExprKind::Tuple(items) = &left.kind {
                    // Left is a literal tuple - inline its elements directly.
                    for item in items {
                        elems.push(self.gen_expr(item)?);
                    }
                } else {
                    // Left is a variable or complex expression - use a temporary.
                    let left_tmp = self.new_tmp();
                    let left_expr = self.gen_expr(left)?;
                    setup.push(format!("let {} = &{}", left_tmp, left_expr));
                    needs_block = true;
                    for idx in 0..left_items.len() {
                        elems.push(format!("{}.{}.clone()", left_tmp, idx));
                    }
                }

                // Generate right elements.
                if let ExprKind::Tuple(items) = &right.kind {
                    // Right is a literal tuple - inline its elements directly.
                    for item in items {
                        elems.push(self.gen_expr(item)?);
                    }
                } else {
                    // Right is a variable or complex expression - use a temporary.
                    let right_tmp = self.new_tmp();
                    let right_expr = self.gen_expr(right)?;
                    setup.push(format!("let {} = &{}", right_tmp, right_expr));
                    needs_block = true;
                    for idx in 0..right_items.len() {
                        elems.push(format!("{}.{}.clone()", right_tmp, idx));
                    }
                }

                if needs_block {
                    return Ok(format!(
                        "{{ {}; ({}) }}",
                        setup.join("; "),
                        elems.join(", ")
                    ));
                } else {
                    return Ok(format!("({})", elems.join(", ")));
                }
            }
        }
        let op_str = match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow
            | BinOp::FloorDiv
            | BinOp::BitOr
            | BinOp::BitAnd
            | BinOp::BitXor
            | BinOp::ShiftLeft
            | BinOp::ShiftRight => {
                unreachable!()
            }
        };
        let is_float = matches!(expr.ty.as_ref(), Some(Type::Float));
        let left_expr = self.gen_numeric_operand(left, is_float)?;
        let right_expr = self.gen_numeric_operand(right, is_float)?;
        Ok(format!("({} {} {})", left_expr, op_str, right_expr))
    }

    /// Lower a unary operation expression.
    pub(super) fn gen_unary_expr(
        &mut self,
        op: &UnaryOp,
        inner: &Expr,
    ) -> Result<String, CompileError> {
        match op {
            UnaryOp::Neg => Ok(format!("(-{})", self.gen_expr(inner)?)),
            UnaryOp::BitNot => Ok(format!("(!{})", self.gen_expr(inner)?)),
            UnaryOp::Not => {
                // Python `not` operates on truthiness, not just bool values.
                let inner_expr = self.gen_expr(inner)?;
                let rendered = match inner.ty.as_ref() {
                    Some(Type::Bool) => format!("(!{})", inner_expr),
                    Some(Type::None) => "true".to_string(),
                    Some(Type::Option(_)) => format!("{}.is_none()", inner_expr),
                    Some(Type::Int) => format!("({} == 0)", inner_expr),
                    Some(Type::Float) => format!("({} == 0.0)", inner_expr),
                    Some(Type::Str) => format!("{}.is_empty()", inner_expr),
                    Some(Type::List(_)) => {
                        if matches!(self.list_storage_for_expr(inner), ListStorage::Local) {
                            format!("{}.is_empty()", inner_expr)
                        } else {
                            format!(
                                "{}.lock().expect(\"list mutex poisoned\").is_empty()",
                                inner_expr
                            )
                        }
                    }
                    Some(Type::Dict(_, _)) => {
                        if matches!(self.dict_storage_for_expr(inner), DictStorage::Local) {
                            format!("{}.is_empty()", inner_expr)
                        } else {
                            format!(
                                "{}.lock().expect(\"dict mutex poisoned\").is_empty()",
                                inner_expr
                            )
                        }
                    }
                    Some(Type::Set(_)) => format!("{}.is_empty()", inner_expr),
                    Some(Type::Tuple(items)) => {
                        if items.is_empty() {
                            "true".to_string()
                        } else {
                            "false".to_string()
                        }
                    }
                    _ => format!("(!{})", inner_expr),
                };
                Ok(rendered)
            }
        }
    }

    /// Escape braces so literal strings can be embedded into format! strings.
    fn escape_format_literal(&self, literal: &str) -> String {
        // format! treats `{` and `}` as placeholders, so we must escape them.
        literal.replace('{', "{{").replace('}', "}}")
    }

    /// Lower a comparison expression, including membership and None checks.
    pub(super) fn gen_compare_expr(
        &mut self,
        expr: &Expr,
        op: &CmpOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<String, CompileError> {
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
        if matches!(op, CmpOp::Eq | CmpOp::NotEq | CmpOp::Is | CmpOp::IsNot) {
            let is_type_call = |candidate: &Expr| {
                matches!(
                    &candidate.kind,
                    ExprKind::Call {
                        func,
                        args,
                        keywords
                    } if matches!(&func.kind, ExprKind::Name(name) if name == "type")
                        && args.len() == 1
                        && keywords.is_empty()
                )
            };
            let type_object_literal = |candidate: &Expr| -> Option<String> {
                let ExprKind::Name(name) = &candidate.kind else {
                    return None;
                };
                let builtin = match name.as_str() {
                    "int" => Some("<class 'int'>"),
                    "float" => Some("<class 'float'>"),
                    "bool" => Some("<class 'bool'>"),
                    "str" => Some("<class 'str'>"),
                    "bytes" => Some("<class 'bytes'>"),
                    "list" => Some("<class 'list'>"),
                    "tuple" => Some("<class 'tuple'>"),
                    "dict" => Some("<class 'dict'>"),
                    "set" => Some("<class 'set'>"),
                    _ => None,
                };
                if let Some(class_lit) = builtin {
                    return Some(class_lit.to_string());
                }
                if self.ctx.classes.contains_key(name) {
                    return Some(format!("<class '{}'>", name));
                }
                None
            };
            let op_str = if matches!(op, CmpOp::Eq | CmpOp::Is) {
                "=="
            } else {
                "!="
            };
            if is_type_call(left) {
                if let Some(class_lit) = type_object_literal(right) {
                    return Ok(format!(
                        "({} {} {:?}.to_string())",
                        self.gen_expr(left)?,
                        op_str,
                        class_lit
                    ));
                }
            }
            if is_type_call(right) {
                if let Some(class_lit) = type_object_literal(left) {
                    return Ok(format!(
                        "({:?}.to_string() {} {})",
                        class_lit,
                        op_str,
                        self.gen_expr(right)?
                    ));
                }
            }
        }
        if matches!(op, CmpOp::In | CmpOp::NotIn) {
            let types_compatible = |left_ty: &Type, elem_ty: &Type| -> bool {
                matches!(left_ty, Type::Unknown)
                    || matches!(elem_ty, Type::Unknown)
                    || left_ty == elem_ty
                    || (left_ty.is_numeric() && elem_ty.is_numeric())
            };
            let membership_mismatch = match (left.ty.as_ref(), right.ty.as_ref()) {
                (Some(left_ty), Some(Type::List(inner)))
                | (Some(left_ty), Some(Type::Set(inner)))
                | (Some(left_ty), Some(Type::Slice(inner))) => {
                    !types_compatible(left_ty, inner.as_ref())
                }
                (Some(left_ty), Some(Type::Dict(key, _))) => {
                    !types_compatible(left_ty, key.as_ref())
                }
                (Some(left_ty), Some(Type::Str)) => !matches!(left_ty, Type::Str | Type::Unknown),
                (Some(left_ty), Some(Type::Tuple(items))) => {
                    if let Some(first) = items.first() {
                        !types_compatible(left_ty, first)
                    } else {
                        false
                    }
                }
                (Some(left_ty), Some(Type::Ref(inner))) => match inner.as_ref() {
                    Type::List(elem) | Type::Set(elem) | Type::Slice(elem) => {
                        !types_compatible(left_ty, elem.as_ref())
                    }
                    Type::Dict(key, _) => !types_compatible(left_ty, key.as_ref()),
                    Type::Str => !matches!(left_ty, Type::Str | Type::Unknown),
                    Type::Tuple(items) => {
                        if let Some(first) = items.first() {
                            !types_compatible(left_ty, first)
                        } else {
                            false
                        }
                    }
                    _ => false,
                },
                _ => false,
            };
            if membership_mismatch {
                return Ok(if matches!(op, CmpOp::NotIn) {
                    "true".to_string()
                } else {
                    "false".to_string()
                });
            }
            let left_expr = self.gen_expr(left)?;
            let right_expr = self.gen_expr(right)?;
            let mut expr = match right.ty.as_ref() {
                Some(Type::List(_)) => {
                    if matches!(self.list_storage_for_expr(right), ListStorage::Local) {
                        format!("{}.contains(&{})", right_expr, left_expr)
                    } else {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\").contains(&{})",
                            right_expr, left_expr
                        )
                    }
                }
                Some(Type::Set(inner)) => {
                    if self.set_uses_pyrepr_storage(right, inner.as_ref()) {
                        // CPython-compat divergence:
                        // Unknown-typed sets are represented as `HashSet<PyRepr>` until a
                        // stable element type is known, so membership checks must use the
                        // same PyRepr coercion path as insertions.
                        self.uses.py_repr = true;
                        format!(
                            "{}.contains(&PyRepr(format!(\"{{:?}}\", {})))",
                            right_expr, left_expr
                        )
                    } else {
                        format!("{}.contains(&{})", right_expr, left_expr)
                    }
                }
                Some(Type::Slice(_)) => {
                    format!("{}.contains(&{})", right_expr, left_expr)
                }
                Some(Type::Dict(_, _)) => {
                    if matches!(self.dict_storage_for_expr(right), DictStorage::Local) {
                        format!("{}.contains_key(&{})", right_expr, left_expr)
                    } else {
                        format!(
                            "{}.lock().expect(\"dict mutex poisoned\").contains_key(&{})",
                            right_expr, left_expr
                        )
                    }
                }
                Some(Type::Str) => format!("{}.contains(&{})", right_expr, left_expr),
                Some(Type::Ref(inner)) => match inner.as_ref() {
                    Type::Dict(_, _) => {
                        if matches!(self.dict_storage_for_expr(right), DictStorage::Local) {
                            format!("{}.contains_key(&{})", right_expr, left_expr)
                        } else {
                            format!(
                                "{}.lock().expect(\"dict mutex poisoned\").contains_key(&{})",
                                right_expr, left_expr
                            )
                        }
                    }
                    Type::Set(inner) => {
                        if self.set_uses_pyrepr_storage(right, inner.as_ref()) {
                            self.uses.py_repr = true;
                            format!(
                                "{}.contains(&PyRepr(format!(\"{{:?}}\", {})))",
                                right_expr, left_expr
                            )
                        } else {
                            format!("{}.contains(&{})", right_expr, left_expr)
                        }
                    }
                    Type::List(_) | Type::Slice(_) => {
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
        if matches!(op, CmpOp::Is | CmpOp::IsNot) {
            // Treat typed None values the same as None literals for identity checks.
            let left_is_none = matches!(&left.kind, ExprKind::Literal(Literal::None))
                || matches!(left.ty.as_ref(), Some(Type::None));
            let right_is_none = matches!(&right.kind, ExprKind::Literal(Literal::None))
                || matches!(right.ty.as_ref(), Some(Type::None));
            if left_is_none && right_is_none {
                return Ok(if matches!(op, CmpOp::Is) {
                    "true".to_string()
                } else {
                    "false".to_string()
                });
            }
            if right_is_none {
                let left_expr = self.gen_expr(left)?;
                if matches!(left.ty.as_ref(), Some(Type::Option(_))) {
                    if matches!(op, CmpOp::Is) {
                        return Ok(format!("{}.is_none()", left_expr));
                    }
                    return Ok(format!("!{}.is_none()", left_expr));
                }
                return Ok(if matches!(op, CmpOp::Is) {
                    "false".to_string()
                } else {
                    "true".to_string()
                });
            }
            if left_is_none {
                let right_expr = self.gen_expr(right)?;
                if matches!(right.ty.as_ref(), Some(Type::Option(_))) {
                    if matches!(op, CmpOp::Is) {
                        return Ok(format!("{}.is_none()", right_expr));
                    }
                    return Ok(format!("!{}.is_none()", right_expr));
                }
                return Ok(if matches!(op, CmpOp::Is) {
                    "false".to_string()
                } else {
                    "true".to_string()
                });
            }
        }
        if matches!(op, CmpOp::Is | CmpOp::IsNot) {
            if matches!(left.ty.as_ref(), Some(Type::List(_)))
                && matches!(right.ty.as_ref(), Some(Type::List(_)))
            {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let expr = format!("Arc::ptr_eq(&{}, &{})", left_expr, right_expr);
                if matches!(op, CmpOp::Is) {
                    return Ok(expr);
                }
                return Ok(format!("!({})", expr));
            }
            if matches!(left.ty.as_ref(), Some(Type::Dict(_, _)))
                && matches!(right.ty.as_ref(), Some(Type::Dict(_, _)))
            {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let left_local = matches!(self.dict_storage_for_expr(left), DictStorage::Local);
                let right_local = matches!(self.dict_storage_for_expr(right), DictStorage::Local);
                let expr = if left_local && right_local {
                    format!("std::ptr::eq(&{}, &{})", left_expr, right_expr)
                } else if left_local || right_local {
                    // Different storage representations cannot be identical.
                    "false".to_string()
                } else {
                    format!("Arc::ptr_eq(&{}, &{})", left_expr, right_expr)
                };
                if matches!(op, CmpOp::Is) {
                    return Ok(expr);
                }
                return Ok(format!("!({})", expr));
            }
            if matches!(left.ty.as_ref(), Some(Type::Set(_)))
                && matches!(right.ty.as_ref(), Some(Type::Set(_)))
            {
                // CPython-compat divergence:
                // Sets are value-backed in this build, so identity checks use
                // address comparison of the current Rust bindings.
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let expr = format!("std::ptr::eq(&{}, &{})", left_expr, right_expr);
                if matches!(op, CmpOp::Is) {
                    return Ok(expr);
                }
                return Ok(format!("!({})", expr));
            }
        }
        if matches!(op, CmpOp::Eq | CmpOp::NotEq) {
            let op_str = if matches!(op, CmpOp::Eq) { "==" } else { "!=" };
            if let Some(Type::Custom(class_name)) = left.ty.as_ref() {
                if let Some(info) = self.ctx.classes.get(class_name) {
                    let left_expr = self.gen_expr(left)?;
                    let mut right_expr = self.gen_expr(right)?;
                    if info.methods.contains_key("__eq__") {
                        if let Some(sig) = info.methods.get("__eq__") {
                            if let Some(param_ty) = sig.params.get(1) {
                                let expected = self.to_borrowed_param_type(param_ty);
                                if matches!(expected, Type::Ref(_)) {
                                    right_expr = format!("&{}", right_expr);
                                }
                            }
                        }
                        let call = format!("{}.{}({})", left_expr, "__eq__", right_expr);
                        if matches!(op, CmpOp::Eq) {
                            return Ok(call);
                        }
                        return Ok(format!("!({})", call));
                    }
                    let eq_expr = format!("std::ptr::eq(&{}, &{})", left_expr, right_expr);
                    if matches!(op, CmpOp::Eq) {
                        return Ok(eq_expr);
                    }
                    return Ok(format!("!({})", eq_expr));
                }
            }
            if matches!(left.ty.as_ref(), Some(Type::List(_)))
                && matches!(right.ty.as_ref(), Some(Type::List(_)))
            {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let left_local = matches!(self.list_storage_for_expr(left), ListStorage::Local);
                let right_local = matches!(self.list_storage_for_expr(right), ListStorage::Local);
                let eq_expr = if left_local && right_local {
                    format!("{}.iter().eq({}.iter())", left_expr, right_expr)
                } else if left_local {
                    let right_tmp = self.new_tmp();
                    let right_guard = self.new_tmp();
                    format!(
                        "{{ let {right_tmp} = {right_expr}.clone(); let {right_guard} = {right_tmp}.lock().expect(\"list mutex poisoned\"); {left}.iter().eq({right_guard}.iter()) }}",
                        right_tmp = right_tmp,
                        right_guard = right_guard,
                        right_expr = right_expr,
                        left = left_expr
                    )
                } else if right_local {
                    let left_tmp = self.new_tmp();
                    let left_guard = self.new_tmp();
                    format!(
                        "{{ let {left_tmp} = {left_expr}.clone(); let {left_guard} = {left_tmp}.lock().expect(\"list mutex poisoned\"); {left_guard}.iter().eq({right}.iter()) }}",
                        left_tmp = left_tmp,
                        left_guard = left_guard,
                        left_expr = left_expr,
                        right = right_expr
                    )
                } else {
                    let left_tmp = self.new_tmp();
                    let right_tmp = self.new_tmp();
                    let left_guard = self.new_tmp();
                    let right_guard = self.new_tmp();
                    format!(
                        // Clone the Arcs to avoid moving list values out of scope.
                        "{{ let {left_tmp} = {left_expr}.clone(); let {right_tmp} = {right_expr}.clone(); if Arc::ptr_eq(&{left_tmp}, &{right_tmp}) {{ true }} else {{ let {left_guard} = {left_tmp}.lock().expect(\"list mutex poisoned\"); let {right_guard} = {right_tmp}.lock().expect(\"list mutex poisoned\"); {left_guard}.iter().eq({right_guard}.iter()) }} }}",
                        left_tmp = left_tmp,
                        right_tmp = right_tmp,
                        left_guard = left_guard,
                        right_guard = right_guard,
                        left_expr = left_expr,
                        right_expr = right_expr
                    )
                };
                if matches!(op, CmpOp::Eq) {
                    return Ok(eq_expr);
                }
                return Ok(format!("!({})", eq_expr));
            }
            if let (Some(Type::List(_)), Some(Type::Tuple(items))) =
                (left.ty.as_ref(), right.ty.as_ref())
            {
                let eq_expr = self.gen_list_tuple_eq_expr(left, right, items.len(), true)?;
                if matches!(op, CmpOp::Eq) {
                    return Ok(eq_expr);
                }
                return Ok(format!("!({})", eq_expr));
            }
            if let (Some(Type::Tuple(items)), Some(Type::List(_))) =
                (left.ty.as_ref(), right.ty.as_ref())
            {
                let eq_expr = self.gen_list_tuple_eq_expr(right, left, items.len(), false)?;
                if matches!(op, CmpOp::Eq) {
                    return Ok(eq_expr);
                }
                return Ok(format!("!({})", eq_expr));
            }
            if matches!(left.ty.as_ref(), Some(Type::Dict(_, _)))
                && matches!(right.ty.as_ref(), Some(Type::Dict(_, _)))
            {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let left_local = matches!(self.dict_storage_for_expr(left), DictStorage::Local);
                let right_local = matches!(self.dict_storage_for_expr(right), DictStorage::Local);
                let eq_expr = if left_local && right_local {
                    format!("{} == {}", left_expr, right_expr)
                } else if left_local {
                    let right_tmp = self.new_tmp();
                    let right_guard = self.new_tmp();
                    format!(
                        "{{ let {right_tmp} = {right_expr}.clone(); let {right_guard} = {right_tmp}.lock().expect(\"dict mutex poisoned\"); {left_expr} == *{right_guard} }}",
                        right_tmp = right_tmp,
                        right_guard = right_guard,
                        left_expr = left_expr,
                        right_expr = right_expr
                    )
                } else if right_local {
                    let left_tmp = self.new_tmp();
                    let left_guard = self.new_tmp();
                    format!(
                        "{{ let {left_tmp} = {left_expr}.clone(); let {left_guard} = {left_tmp}.lock().expect(\"dict mutex poisoned\"); *{left_guard} == {right_expr} }}",
                        left_tmp = left_tmp,
                        left_guard = left_guard,
                        left_expr = left_expr,
                        right_expr = right_expr
                    )
                } else {
                    let left_tmp = self.new_tmp();
                    let right_tmp = self.new_tmp();
                    let left_guard = self.new_tmp();
                    let right_guard = self.new_tmp();
                    format!(
                        "{{ let {left_tmp} = {left_expr}.clone(); let {right_tmp} = {right_expr}.clone(); if Arc::ptr_eq(&{left_tmp}, &{right_tmp}) {{ true }} else {{ let {left_guard} = {left_tmp}.lock().expect(\"dict mutex poisoned\"); let {right_guard} = {right_tmp}.lock().expect(\"dict mutex poisoned\"); *{left_guard} == *{right_guard} }} }}",
                        left_tmp = left_tmp,
                        right_tmp = right_tmp,
                        left_guard = left_guard,
                        right_guard = right_guard,
                        left_expr = left_expr,
                        right_expr = right_expr
                    )
                };
                if matches!(op, CmpOp::Eq) {
                    return Ok(eq_expr);
                }
                return Ok(format!("!({})", eq_expr));
            }
            if let (Some(Type::Option(inner)), Some(right_ty)) =
                (left.ty.as_ref(), right.ty.as_ref())
            {
                if right_ty == inner.as_ref() {
                    let left_expr = self.gen_expr(left)?;
                    let right_expr = self.gen_expr(right)?;
                    if self.is_copy_type(inner) {
                        return Ok(format!("({} {} Some({}))", left_expr, op_str, right_expr));
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
                        return Ok(format!("(Some({}) {} {})", left_expr, op_str, right_expr));
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
            if let (Some(left_ty), Some(right_ty)) = (left.ty.as_ref(), right.ty.as_ref()) {
                // Python equality across unrelated primitive types is always defined.
                let primitive = |ty: &Type| {
                    matches!(
                        ty,
                        Type::Int | Type::Float | Type::Bool | Type::Str | Type::None
                    )
                };
                let numeric_pair = matches!(
                    (left_ty, right_ty),
                    (
                        Type::Int | Type::Float | Type::Bool,
                        Type::Int | Type::Float | Type::Bool
                    )
                );
                if primitive(left_ty) && primitive(right_ty) && left_ty != right_ty && !numeric_pair
                {
                    return Ok(if matches!(op, CmpOp::Eq) {
                        "false".to_string()
                    } else {
                        "true".to_string()
                    });
                }
            }
            if let (Some(Type::Tuple(left_items)), Some(Type::Tuple(right_items))) =
                (left.ty.as_ref(), right.ty.as_ref())
            {
                if left_items.len() == right_items.len() {
                    let left_expr = self.gen_expr(left)?;
                    let right_expr = self.gen_expr(right)?;
                    let left_tmp = self.new_tmp();
                    let right_tmp = self.new_tmp();
                    let mut parts = Vec::new();
                    for idx in 0..left_items.len() {
                        let left_elem_expr = format!("{left_tmp}.{idx}");
                        let right_elem_expr = format!("{right_tmp}.{idx}");
                        let part = match (&left_items[idx], &right_items[idx]) {
                            (Type::List(_), Type::List(_)) => self.gen_list_eq_from_rendered(
                                &left_elem_expr,
                                true,
                                &right_elem_expr,
                                true,
                            ),
                            (Type::List(_), Type::Tuple(items)) => {
                                // CPython-compat divergence:
                                // We allow list-vs-tuple element equality inside tuple
                                // comparison to keep varargs interop practical.
                                self.gen_list_tuple_eq_from_rendered(
                                    &left_elem_expr,
                                    true,
                                    &right_elem_expr,
                                    items.len(),
                                    true,
                                )
                            }
                            (Type::Tuple(items), Type::List(_)) => self
                                .gen_list_tuple_eq_from_rendered(
                                    &right_elem_expr,
                                    true,
                                    &left_elem_expr,
                                    items.len(),
                                    false,
                                ),
                            (Type::Dict(_, _), Type::Dict(_, _)) => self.gen_dict_eq_from_rendered(
                                &left_elem_expr,
                                true,
                                &right_elem_expr,
                                true,
                            ),
                            _ => format!("({left_elem_expr} == {right_elem_expr})"),
                        };
                        parts.push(format!("({})", part));
                    }
                    if parts.is_empty() {
                        // CPython semantics: empty tuples are equal to each other.
                        return Ok(if matches!(op, CmpOp::Eq) {
                            "true".to_string()
                        } else {
                            "false".to_string()
                        });
                    }
                    let eq_expr = format!(
                        "{{ let {left_tmp} = &{left_expr}; let {right_tmp} = &{right_expr}; {} }}",
                        parts.join(" && ")
                    );
                    if matches!(op, CmpOp::Eq) {
                        return Ok(eq_expr);
                    }
                    return Ok(format!("!({})", eq_expr));
                }
            }
        }
        if let (Some(Type::Tuple(left_items)), Some(Type::Tuple(right_items))) =
            (left.ty.as_ref(), right.ty.as_ref())
        {
            if left_items.len() != right_items.len() {
                if matches!(op, CmpOp::Eq) {
                    return Ok("false".to_string());
                }
                if matches!(op, CmpOp::NotEq) {
                    return Ok("true".to_string());
                }
                if matches!(op, CmpOp::Lt | CmpOp::LtEq | CmpOp::Gt | CmpOp::GtEq) {
                    return self.gen_tuple_compare_mismatch(
                        op,
                        left,
                        right,
                        left_items.len(),
                        right_items.len(),
                    );
                }
            }
        }
        if let (Some(Type::Set(_)), Some(Type::Set(_))) = (left.ty.as_ref(), right.ty.as_ref()) {
            if matches!(op, CmpOp::Lt | CmpOp::LtEq | CmpOp::Gt | CmpOp::GtEq) {
                let left_expr = self.gen_expr(left)?;
                let right_expr = self.gen_expr(right)?;
                let left_tmp = self.new_tmp();
                let right_tmp = self.new_tmp();
                let cond = match op {
                    CmpOp::LtEq => format!("{left_tmp}.is_subset({right_tmp})"),
                    CmpOp::Lt => {
                        format!("{left_tmp}.is_subset({right_tmp}) && {left_tmp} != {right_tmp}")
                    }
                    CmpOp::GtEq => format!("{left_tmp}.is_superset({right_tmp})"),
                    CmpOp::Gt => {
                        format!("{left_tmp}.is_superset({right_tmp}) && {left_tmp} != {right_tmp}")
                    }
                    _ => unreachable!("set ordering handled only for subset/superset operators"),
                };
                return Ok(format!(
                    "{{ let {left_tmp} = &({left_expr}); let {right_tmp} = &({right_expr}); {cond} }}"
                ));
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
        if matches!(left.ty.as_ref(), Some(Type::Int | Type::Float | Type::Bool))
            && matches!(
                right.ty.as_ref(),
                Some(Type::Int | Type::Float | Type::Bool)
            )
        {
            let is_float = matches!(left.ty.as_ref(), Some(Type::Float))
                || matches!(right.ty.as_ref(), Some(Type::Float));
            let left_expr = self.gen_numeric_operand(left, is_float)?;
            let right_expr = self.gen_numeric_operand(right, is_float)?;
            return Ok(format!("({} {} {})", left_expr, op_str, right_expr));
        }
        Ok(format!(
            "({} {} {})",
            self.gen_expr(left)?,
            op_str,
            self.gen_expr(right)?
        ))
    }

    fn gen_list_tuple_eq_expr(
        &mut self,
        list_expr: &Expr,
        tuple_expr: &Expr,
        tuple_len: usize,
        list_on_left: bool,
    ) -> Result<String, CompileError> {
        let list_rendered = self.gen_expr(list_expr)?;
        let tuple_rendered = self.gen_expr(tuple_expr)?;
        let list_is_shared = !matches!(self.list_storage_for_expr(list_expr), ListStorage::Local);
        Ok(self.gen_list_tuple_eq_from_rendered(
            &list_rendered,
            list_is_shared,
            &tuple_rendered,
            tuple_len,
            list_on_left,
        ))
    }

    fn gen_list_tuple_eq_from_rendered(
        &mut self,
        list_expr: &str,
        list_is_shared: bool,
        tuple_expr: &str,
        tuple_len: usize,
        list_on_left: bool,
    ) -> String {
        let list_tmp = self.new_tmp();
        let tuple_tmp = self.new_tmp();
        let mut comps = Vec::new();
        for idx in 0..tuple_len {
            let list_item = if list_is_shared {
                format!("&{list_tmp}_guard[{idx}]")
            } else {
                format!("&{list_tmp}[{idx}]")
            };
            let tuple_item = format!("&{tuple_tmp}.{idx}");
            if list_on_left {
                comps.push(format!("({list_item} == {tuple_item})"));
            } else {
                comps.push(format!("({tuple_item} == {list_item})"));
            }
        }
        let checks = if comps.is_empty() {
            "true".to_string()
        } else {
            comps.join(" && ")
        };
        if list_is_shared {
            format!(
                "{{ let {list_tmp} = ({list_expr}).clone(); let {tuple_tmp} = &({tuple_expr}); let {list_tmp}_guard = {list_tmp}.lock().expect(\"list mutex poisoned\"); ({list_tmp}_guard.len() == {tuple_len}) && ({checks}) }}"
            )
        } else {
            format!(
                "{{ let {list_tmp} = &({list_expr}); let {tuple_tmp} = &({tuple_expr}); ({list_tmp}.len() == {tuple_len}) && ({checks}) }}"
            )
        }
    }

    fn gen_list_eq_from_rendered(
        &mut self,
        left_expr: &str,
        left_is_shared: bool,
        right_expr: &str,
        right_is_shared: bool,
    ) -> String {
        match (left_is_shared, right_is_shared) {
            (false, false) => {
                format!(
                    "{{ let _left = &({left_expr}); let _right = &({right_expr}); _left.iter().eq(_right.iter()) }}"
                )
            }
            (false, true) => {
                let right_tmp = self.new_tmp();
                let right_guard = self.new_tmp();
                format!(
                    "{{ let _left = &({left_expr}); let {right_tmp} = ({right_expr}).clone(); let {right_guard} = {right_tmp}.lock().expect(\"list mutex poisoned\"); _left.iter().eq({right_guard}.iter()) }}"
                )
            }
            (true, false) => {
                let left_tmp = self.new_tmp();
                let left_guard = self.new_tmp();
                format!(
                    "{{ let {left_tmp} = ({left_expr}).clone(); let {left_guard} = {left_tmp}.lock().expect(\"list mutex poisoned\"); let _right = &({right_expr}); {left_guard}.iter().eq(_right.iter()) }}"
                )
            }
            (true, true) => {
                let left_tmp = self.new_tmp();
                let right_tmp = self.new_tmp();
                let left_guard = self.new_tmp();
                let right_guard = self.new_tmp();
                format!(
                    "{{ let {left_tmp} = ({left_expr}).clone(); let {right_tmp} = ({right_expr}).clone(); if Arc::ptr_eq(&{left_tmp}, &{right_tmp}) {{ true }} else {{ let {left_guard} = {left_tmp}.lock().expect(\"list mutex poisoned\"); let {right_guard} = {right_tmp}.lock().expect(\"list mutex poisoned\"); {left_guard}.iter().eq({right_guard}.iter()) }} }}"
                )
            }
        }
    }

    fn gen_dict_eq_from_rendered(
        &mut self,
        left_expr: &str,
        left_is_shared: bool,
        right_expr: &str,
        right_is_shared: bool,
    ) -> String {
        match (left_is_shared, right_is_shared) {
            (false, false) => {
                format!("({left_expr} == {right_expr})")
            }
            (false, true) => {
                let right_tmp = self.new_tmp();
                let right_guard = self.new_tmp();
                format!(
                    "{{ let _left = &({left_expr}); let {right_tmp} = ({right_expr}).clone(); let {right_guard} = {right_tmp}.lock().expect(\"dict mutex poisoned\"); *_left == *{right_guard} }}"
                )
            }
            (true, false) => {
                let left_tmp = self.new_tmp();
                let left_guard = self.new_tmp();
                format!(
                    "{{ let {left_tmp} = ({left_expr}).clone(); let {left_guard} = {left_tmp}.lock().expect(\"dict mutex poisoned\"); let _right = &({right_expr}); *{left_guard} == *_right }}"
                )
            }
            (true, true) => {
                let left_tmp = self.new_tmp();
                let right_tmp = self.new_tmp();
                let left_guard = self.new_tmp();
                let right_guard = self.new_tmp();
                format!(
                    "{{ let {left_tmp} = ({left_expr}).clone(); let {right_tmp} = ({right_expr}).clone(); if Arc::ptr_eq(&{left_tmp}, &{right_tmp}) {{ true }} else {{ let {left_guard} = {left_tmp}.lock().expect(\"dict mutex poisoned\"); let {right_guard} = {right_tmp}.lock().expect(\"dict mutex poisoned\"); *{left_guard} == *{right_guard} }} }}"
                )
            }
        }
    }

    /// Lower a chained comparison expression (e.g., a < b < c).
    ///
    /// We preserve Python's left-to-right evaluation order and short-circuiting:
    /// - Evaluate left once
    /// - Evaluate each comparator only if previous comparison is true
    pub(super) fn gen_compare_chain_expr(
        &mut self,
        expr: &Expr,
        left: &Expr,
        ops: &[CmpOp],
        comparators: &[Expr],
    ) -> Result<String, CompileError> {
        if ops.is_empty() || ops.len() != comparators.len() {
            return Err(self.error(expr.span, "Invalid comparison chain"));
        }

        let mut out = String::new();
        out.push_str("{ ");

        let left_tmp = self.new_tmp();
        out.push_str(&self.gen_compare_chain_init(left, &left_tmp)?);
        out.push(' ');

        let mut prev_tmp = left_tmp;
        let mut prev_ty = left.ty.clone();
        for (idx, op) in ops.iter().enumerate() {
            let right_expr = &comparators[idx];
            let right_tmp = self.new_tmp();
            out.push_str(&self.gen_compare_chain_init(right_expr, &right_tmp)?);
            out.push(' ');

            let left_tmp_expr = Expr {
                kind: ExprKind::Name(prev_tmp.clone()),
                span: expr.span,
                ty: prev_ty.clone(),
            };
            let right_tmp_expr = Expr {
                kind: ExprKind::Name(right_tmp.clone()),
                span: expr.span,
                ty: right_expr.ty.clone(),
            };
            let cmp_expr = self.gen_compare_expr(expr, op, &left_tmp_expr, &right_tmp_expr)?;
            out.push_str(&format!("if !({}) {{ false }} else {{ ", cmp_expr));

            prev_tmp = right_tmp;
            prev_ty = right_expr.ty.clone();
        }

        out.push_str("true");
        for _ in 0..ops.len() {
            out.push_str(" }");
        }
        out.push_str(" }");
        Ok(out)
    }

    /// Emit a temporary binding for a chained comparison operand.
    fn gen_compare_chain_init(&mut self, value: &Expr, tmp: &str) -> Result<String, CompileError> {
        let mut rendered = self.gen_expr(value)?;
        if let ExprKind::Name(name) = &value.kind {
            if !self.is_global(name) {
                if let Some(ty) = value.ty.as_ref() {
                    if !self.is_copy_type(ty)
                        && !matches!(ty, Type::Ref(_) | Type::MutRef(_) | Type::Slice(_))
                    {
                        rendered = format!("{}.clone()", rendered);
                    }
                }
            }
        }
        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
            let storage = self.list_storage_for_expr(value);
            self.set_list_storage_for_temp(tmp, storage);
        }
        Ok(format!("let {} = {};", tmp, rendered))
    }

    /// Emit lexicographic comparison for tuples with different lengths.
    fn gen_tuple_compare_mismatch(
        &mut self,
        op: &CmpOp,
        left: &Expr,
        right: &Expr,
        left_len: usize,
        right_len: usize,
    ) -> Result<String, CompileError> {
        let left_expr = self.gen_expr(left)?;
        let right_expr = self.gen_expr(right)?;
        let left_tmp = self.new_tmp();
        let right_tmp = self.new_tmp();
        let min_len = left_len.min(right_len);
        let elem_op = match op {
            CmpOp::Lt | CmpOp::LtEq => "<",
            CmpOp::Gt | CmpOp::GtEq => ">",
            _ => unreachable!("tuple mismatch handled only for ordering ops"),
        };
        let len_cmp = match op {
            CmpOp::Lt => format!("{left_len} < {right_len}"),
            CmpOp::LtEq => format!("{left_len} <= {right_len}"),
            CmpOp::Gt => format!("{left_len} > {right_len}"),
            CmpOp::GtEq => format!("{left_len} >= {right_len}"),
            _ => unreachable!("tuple mismatch handled only for ordering ops"),
        };
        let mut chain = String::new();
        if min_len == 0 {
            chain.push_str(&len_cmp);
        } else {
            for idx in 0..min_len {
                let diff = format!("{left_tmp}.{idx} != {right_tmp}.{idx}");
                let cmp = format!("{left_tmp}.{idx} {elem_op} {right_tmp}.{idx}");
                if idx == 0 {
                    chain.push_str(&format!("if {diff} {{ {cmp} }}"));
                } else {
                    chain.push_str(&format!(" else if {diff} {{ {cmp} }}"));
                }
            }
            chain.push_str(&format!(" else {{ {len_cmp} }}"));
        }
        Ok(format!(
            "{{ let {left_tmp} = &{left_expr}; let {right_tmp} = &{right_expr}; {chain} }}"
        ))
    }

    /// Lower a boolean operator chain expression.
    pub(super) fn gen_boolop_expr(
        &mut self,
        op: &BoolOp,
        values: &[Expr],
    ) -> Result<String, CompileError> {
        let all_bool = values
            .iter()
            .all(|v| matches!(v.ty.as_ref(), Some(Type::Bool)));
        if all_bool {
            let op_str = match op {
                BoolOp::And => "&&",
                BoolOp::Or => "||",
            };
            let parts: Result<Vec<String>, CompileError> =
                values.iter().map(|v| self.gen_expr(v)).collect();
            return Ok(format!("({})", parts?.join(&format!(" {} ", op_str))));
        }

        if values.is_empty() {
            return Ok("false".to_string());
        }

        let mut out = String::new();
        out.push_str("{ ");

        let first_tmp = self.new_tmp();
        out.push_str(&self.gen_compare_chain_init(&values[0], &first_tmp)?);
        out.push(' ');

        let mut prev_tmp = first_tmp;
        let mut prev_ty = values[0].ty.clone();
        for idx in 0..values.len() - 1 {
            let cond = match prev_ty.as_ref() {
                Some(ty) if !matches!(ty, Type::Unknown) => {
                    self.truthy_expr_for_type(&prev_tmp, ty)
                }
                _ => format!("({})", prev_tmp),
            };
            let branch_cond = match op {
                BoolOp::And => format!("!{}", cond),
                BoolOp::Or => cond,
            };
            out.push_str(&format!("if {} {{ {} }} else {{ ", branch_cond, prev_tmp));

            let next_tmp = self.new_tmp();
            out.push_str(&self.gen_compare_chain_init(&values[idx + 1], &next_tmp)?);
            out.push(' ');

            prev_tmp = next_tmp;
            prev_ty = values[idx + 1].ty.clone();
        }

        out.push_str(&prev_tmp);
        for _ in 0..values.len() - 1 {
            out.push_str(" }");
        }
        out.push_str(" }");
        Ok(out)
    }
}
