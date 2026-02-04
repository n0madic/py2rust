// Operator expressions (binary, unary, comparisons, boolean ops).

use super::super::*;

impl<'a> Codegen<'a> {
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
            if let (Some(Type::Set(_)), Some(Type::Set(_))) = (left.ty.as_ref(), right.ty.as_ref())
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

    /// Lower a unary operation expression.
    pub(super) fn gen_unary_expr(
        &mut self,
        op: &UnaryOp,
        inner: &Expr,
    ) -> Result<String, CompileError> {
        let op_str = match op {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        };
        Ok(format!("({}{})", op_str, self.gen_expr(inner)?))
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
        if matches!(op, CmpOp::In | CmpOp::NotIn) {
            let left_expr = self.gen_expr(left)?;
            let right_expr = self.gen_expr(right)?;
            let mut expr = match right.ty.as_ref() {
                Some(Type::List(_)) | Some(Type::Set(_)) | Some(Type::Slice(_)) => {
                    format!("{}.contains(&{})", right_expr, left_expr)
                }
                Some(Type::Dict(_, _)) => format!("{}.contains_key(&{})", right_expr, left_expr),
                Some(Type::Str) => format!("{}.contains(&{})", right_expr, left_expr),
                Some(Type::Ref(inner)) => match inner.as_ref() {
                    Type::Dict(_, _) => format!("{}.contains_key(&{})", right_expr, left_expr),
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
        let op_str = match op {
            BoolOp::And => "&&",
            BoolOp::Or => "||",
        };
        let parts: Result<Vec<String>, CompileError> =
            values.iter().map(|v| self.gen_expr(v)).collect();
        Ok(format!("({})", parts?.join(&format!(" {} ", op_str))))
    }
}
