use super::*;

impl<'a> TypeChecker<'a> {
    /// Type check binary operators and infer the resulting type.
    pub(super) fn check_binary_expr(
        &mut self,
        op: &BinOp,
        left: &mut Expr,
        right: &mut Expr,
        span: Span,
    ) -> Result<Type, CompileError> {
        let mut left_ty = self.check_expr(left, None)?;
        let mut right_ty = self.check_expr(right, None)?;

        let refine_from_other = |op: &BinOp, other: &Type| -> Option<Type> {
            match op {
                BinOp::Add => {
                    if matches!(other, Type::Str) {
                        Some(Type::Str)
                    } else if other.is_numeric() {
                        Some(other.clone())
                    } else {
                        None
                    }
                }
                BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::Pow
                | BinOp::FloorDiv => {
                    if other.is_numeric() {
                        Some(other.clone())
                    } else {
                        None
                    }
                }
                BinOp::BitOr | BinOp::BitAnd | BinOp::BitXor => match other {
                    Type::Int => Some(Type::Int),
                    Type::Set(inner) => Some(Type::Set(inner.clone())),
                    _ => None,
                },
                BinOp::ShiftLeft | BinOp::ShiftRight => {
                    if matches!(other, Type::Int) {
                        Some(Type::Int)
                    } else {
                        None
                    }
                }
            }
        };

        if matches!(left_ty, Type::Unknown) && !matches!(right_ty, Type::Unknown) {
            if let Some(desired) = refine_from_other(op, &right_ty) {
                if self.refine_call_return(left, &desired) {
                    left_ty = desired.clone();
                    left.ty = Some(desired);
                }
            }
        }
        if matches!(right_ty, Type::Unknown) && !matches!(left_ty, Type::Unknown) {
            if let Some(desired) = refine_from_other(op, &left_ty) {
                if self.refine_call_return(right, &desired) {
                    right_ty = desired.clone();
                    right.ty = Some(desired);
                }
            }
        }

        // Bidirectional type inference: if one side is Unknown, use the other.
        if matches!(left_ty, Type::Unknown) && !matches!(right_ty, Type::Unknown) {
            self.maybe_update_from_expr(left, &right_ty);
        }
        if matches!(right_ty, Type::Unknown) && !matches!(left_ty, Type::Unknown) {
            self.maybe_update_from_expr(right, &left_ty);
        }

        // Keep unknown arithmetic permissive, preserving string concat special-case.
        if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
            if matches!(op, BinOp::Add)
                && (matches!(left_ty, Type::Str) || matches!(right_ty, Type::Str))
            {
                return Ok(Type::Str);
            }
            return Ok(Type::Unknown);
        }

        match op {
            // Bitwise operators are either int ops or set operations.
            BinOp::BitOr | BinOp::BitAnd | BinOp::BitXor => {
                if let (Type::Set(left_inner), Type::Set(right_inner)) = (&left_ty, &right_ty) {
                    let inner = if **left_inner == **right_inner {
                        *left_inner.clone()
                    } else if matches!(**left_inner, Type::Unknown) {
                        *right_inner.clone()
                    } else if matches!(**right_inner, Type::Unknown) {
                        *left_inner.clone()
                    } else {
                        return Err(self.error(span, "Set operands must have matching types"));
                    };
                    return Ok(Type::Set(Box::new(inner)));
                }
                if matches!(left_ty, Type::Int) && matches!(right_ty, Type::Int) {
                    return Ok(Type::Int);
                }
                Err(self.error(
                    span,
                    "Bitwise operation requires int operands (or set operands)",
                ))
            }
            BinOp::ShiftLeft | BinOp::ShiftRight => {
                if matches!(left_ty, Type::Int) && matches!(right_ty, Type::Int) {
                    return Ok(Type::Int);
                }
                Err(self.error(span, "Shift operation requires int operands"))
            }
            // Subtraction supports both numeric arithmetic and set difference.
            BinOp::Sub => {
                if let (Type::Set(left_inner), Type::Set(right_inner)) = (&left_ty, &right_ty) {
                    let inner = if **left_inner == **right_inner {
                        *left_inner.clone()
                    } else if matches!(**left_inner, Type::Unknown) {
                        *right_inner.clone()
                    } else if matches!(**right_inner, Type::Unknown) {
                        *left_inner.clone()
                    } else {
                        return Err(self.error(span, "Set operands must have matching types"));
                    };
                    return Ok(Type::Set(Box::new(inner)));
                }
                if !left_ty.is_numeric() || !right_ty.is_numeric() {
                    return Err(self.error(span, "Binary arithmetic requires numeric types"));
                }
                if matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float) {
                    Ok(Type::Float)
                } else {
                    Ok(Type::Int)
                }
            }
            // Arithmetic operations with Python-specific string/tuple behavior.
            BinOp::Add | BinOp::Mul | BinOp::Div | BinOp::Pow | BinOp::Mod | BinOp::FloorDiv => {
                if matches!(op, BinOp::Add)
                    && (matches!(left_ty, Type::Str) || matches!(right_ty, Type::Str))
                {
                    return Ok(Type::Str);
                }

                if matches!(op, BinOp::Mul) {
                    let left_is_str = matches!(left_ty, Type::Str);
                    let right_is_str = matches!(right_ty, Type::Str);
                    let left_is_int = matches!(left_ty, Type::Int);
                    let right_is_int = matches!(right_ty, Type::Int);
                    if (left_is_str && right_is_int) || (right_is_str && left_is_int) {
                        return Ok(Type::Str);
                    }
                }

                if !left_ty.is_numeric() || !right_ty.is_numeric() {
                    if matches!(op, BinOp::Add) {
                        if let (Type::List(left_inner), Type::List(right_inner)) =
                            (&left_ty, &right_ty)
                        {
                            let merged =
                                Self::merge_types(*left_inner.clone(), *right_inner.clone());
                            return Ok(Type::List(Box::new(merged)));
                        }
                        if let (Type::Tuple(left_items), Type::Tuple(right_items)) =
                            (&left_ty, &right_ty)
                        {
                            let mut combined = left_items.clone();
                            combined.extend(right_items.clone());
                            return Ok(Type::Tuple(combined));
                        }
                    }
                    return Err(self.error(span, "Binary arithmetic requires numeric types"));
                }

                // Detect obvious constant division-by-zero cases.
                if matches!(op, BinOp::Div | BinOp::FloorDiv | BinOp::Mod) {
                    if let ExprKind::Literal(lit) = &right.kind {
                        let is_zero = matches!(lit, Literal::Int(0))
                            || matches!(lit, Literal::Float(f) if *f == 0.0);
                        if is_zero {
                            self.warn(right.span, "division by zero");
                        }
                    }
                }

                if matches!(op, BinOp::FloorDiv) {
                    if matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float) {
                        Ok(Type::Float)
                    } else {
                        Ok(Type::Int)
                    }
                } else if matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float) {
                    Ok(Type::Float)
                } else {
                    Ok(Type::Int)
                }
            }
        }
    }

    /// Type check unary operators.
    pub(super) fn check_unary_expr(
        &mut self,
        op: &UnaryOp,
        inner: &mut Expr,
        span: Span,
    ) -> Result<Type, CompileError> {
        let inner_ty = self.check_expr(inner, None)?;
        if matches!(inner_ty, Type::Unknown) {
            return Ok(Type::Unknown);
        }

        match op {
            UnaryOp::Neg => {
                if !inner_ty.is_numeric() {
                    return Err(self.error(span, "Unary - requires numeric type"));
                }
                Ok(inner_ty)
            }
            // Python `not` is truthiness-based and always produces bool.
            UnaryOp::Not => Ok(Type::Bool),
            UnaryOp::BitNot => {
                if !matches!(inner_ty, Type::Int) {
                    return Err(self.error(span, "Unary ~ requires int type"));
                }
                Ok(Type::Int)
            }
        }
    }

    /// Type check chained comparisons (`a < b < c`).
    pub(super) fn check_compare_chain_expr(
        &mut self,
        span: Span,
        left: &mut Expr,
        ops: &[CmpOp],
        comparators: &mut [Expr],
    ) -> Result<Type, CompileError> {
        if ops.len() != comparators.len() || ops.is_empty() {
            return Err(self.error(span, "Invalid comparison chain"));
        }

        self.check_compare_expr(span, &ops[0], left, &mut comparators[0])?;
        for idx in 1..ops.len() {
            let (head, tail) = comparators.split_at_mut(idx);
            let left_expr = &mut head[idx - 1];
            let right_expr = &mut tail[0];
            self.check_compare_expr(span, &ops[idx], left_expr, right_expr)?;
        }
        Ok(Type::Bool)
    }

    /// Type check boolean `and/or` expressions.
    pub(super) fn check_bool_op_expr(
        &mut self,
        values: &mut [Expr],
        span: Span,
    ) -> Result<Type, CompileError> {
        let mut value_tys = Vec::new();
        for value in values.iter_mut() {
            value_tys.push(self.check_expr(value, None)?);
        }

        let all_bool = value_tys
            .iter()
            .all(|ty| matches!(ty, Type::Bool | Type::Unknown));
        if all_bool {
            for (value, ty) in values.iter_mut().zip(value_tys.iter()) {
                if matches!(ty, Type::Unknown) {
                    self.maybe_update_from_expr(value, &Type::Bool);
                }
            }
            return Ok(Type::Bool);
        }

        let mut candidate: Option<Type> = None;
        for ty in &value_tys {
            if matches!(ty, Type::Unknown) {
                continue;
            }
            if let Some(existing) = &candidate {
                if ty != existing {
                    return Err(
                        self.error(span, "Boolean operator requires matching operand types")
                    );
                }
            } else {
                candidate = Some(ty.clone());
            }
        }

        if let Some(candidate) = candidate {
            for (value, ty) in values.iter_mut().zip(value_tys.iter()) {
                if matches!(ty, Type::Unknown) {
                    self.maybe_update_from_expr(value, &candidate);
                }
            }
            Ok(candidate)
        } else {
            Ok(Type::Unknown)
        }
    }

    /// Type check a single comparison expression and return its type.
    pub(super) fn check_compare_expr(
        &mut self,
        span: Span,
        op: &CmpOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<Type, CompileError> {
        let left_ty = self.check_expr(left, None)?;
        let right_ty = self.check_expr(right, None)?;

        // Propagate type information to empty list literals.
        if let (Type::List(left_inner), Type::List(right_inner)) = (&left_ty, &right_ty) {
            if matches!(left_inner.as_ref(), Type::Unknown)
                && !matches!(right_inner.as_ref(), Type::Unknown)
            {
                left.ty = Some(Type::List(right_inner.clone()));
            }
            if matches!(right_inner.as_ref(), Type::Unknown)
                && !matches!(left_inner.as_ref(), Type::Unknown)
            {
                right.ty = Some(Type::List(left_inner.clone()));
            }
        }

        match op {
            // Membership testing: x in collection, x not in collection.
            CmpOp::In | CmpOp::NotIn => {
                if matches!(right_ty, Type::Unknown) {
                    return Ok(Type::Bool);
                }

                let elem_ty = match &right_ty {
                    Type::List(inner) | Type::Set(inner) => (*inner.as_ref()).clone(),
                    Type::Dict(key, _) => (*key.as_ref()).clone(),
                    Type::Str => Type::Str,
                    Type::Tuple(items) => {
                        let mut candidate: Option<&Type> = None;
                        for item in items {
                            if matches!(item, Type::Unknown) {
                                continue;
                            }
                            if let Some(existing) = candidate {
                                if item != existing {
                                    return Err(self.error(
                                        span,
                                        "Tuple membership requires homogeneous element types",
                                    ));
                                }
                            } else {
                                candidate = Some(item);
                            }
                        }
                        candidate.cloned().unwrap_or(Type::Unknown)
                    }
                    _ => {
                        return Err(
                            self.error(span, "Membership requires list, tuple, set, dict, or str")
                        );
                    }
                };

                if matches!(left_ty, Type::Unknown) {
                    if !matches!(elem_ty, Type::Unknown) {
                        self.maybe_update_from_expr(left, &elem_ty);
                    }
                    return Ok(Type::Bool);
                }
                if matches!(elem_ty, Type::Unknown) {
                    return Ok(Type::Bool);
                }
                if left_ty != elem_ty {
                    return Err(self.error(span, "Membership requires matching element type"));
                }
                Ok(Type::Bool)
            }
            // Identity comparisons are always valid in Python.
            CmpOp::Is | CmpOp::IsNot => Ok(Type::Bool),
            // General comparisons with Python-compatible flexibility.
            _ => {
                if matches!(left_ty, Type::Unknown) && !matches!(right_ty, Type::Unknown) {
                    self.maybe_update_from_expr(left, &right_ty);
                }
                if matches!(right_ty, Type::Unknown) && !matches!(left_ty, Type::Unknown) {
                    self.maybe_update_from_expr(right, &left_ty);
                }
                if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
                    return Ok(Type::Bool);
                }
                if left_ty != right_ty {
                    if matches!(op, CmpOp::Eq | CmpOp::NotEq) {
                        return Ok(Type::Bool);
                    }
                    let opt_matches = match (&left_ty, &right_ty) {
                        (Type::Option(_), Type::None) | (Type::None, Type::Option(_)) => true,
                        (Type::Option(inner), other) => inner.as_ref() == other,
                        (other, Type::Option(inner)) => inner.as_ref() == other,
                        _ => false,
                    };
                    let container_unknown_matches = match (&left_ty, &right_ty) {
                        (Type::List(l), Type::List(r)) => {
                            matches!(l.as_ref(), Type::Unknown)
                                || matches!(r.as_ref(), Type::Unknown)
                        }
                        (Type::Set(l), Type::Set(r)) => {
                            matches!(l.as_ref(), Type::Unknown)
                                || matches!(r.as_ref(), Type::Unknown)
                        }
                        (Type::Dict(lk, lv), Type::Dict(rk, rv)) => {
                            matches!(lk.as_ref(), Type::Unknown)
                                || matches!(lv.as_ref(), Type::Unknown)
                                || matches!(rk.as_ref(), Type::Unknown)
                                || matches!(rv.as_ref(), Type::Unknown)
                        }
                        (Type::Tuple(l), Type::Tuple(r)) => {
                            if l.is_empty() || r.is_empty() {
                                true
                            } else {
                                let hom_l =
                                    l.first().is_some_and(|first| l.iter().all(|t| t == first));
                                let hom_r =
                                    r.first().is_some_and(|first| r.iter().all(|t| t == first));
                                if hom_l && hom_r {
                                    l.first() == r.first()
                                } else {
                                    false
                                }
                            }
                        }
                        _ => false,
                    };
                    let numeric_matches = left_ty.is_numeric() && right_ty.is_numeric();
                    if !opt_matches && !container_unknown_matches && !numeric_matches {
                        return Err(self.error(span, "Comparison requires matching types"));
                    }
                }
                Ok(Type::Bool)
            }
        }
    }

    /// If expression is a call to a lambda with unknown return type, refine it.
    fn refine_call_return(&mut self, expr: &Expr, desired: &Type) -> bool {
        if matches!(desired, Type::Unknown) {
            return false;
        }
        if let ExprKind::Call { func, .. } = &expr.kind {
            if let ExprKind::Name(name) = &func.kind {
                if let Some(Type::Lambda {
                    param_names,
                    params,
                    param_kinds,
                    has_defaults,
                    ret,
                }) = self.lookup_var(name)
                {
                    if matches!(ret.as_ref(), Type::Unknown) {
                        let updated = Type::Lambda {
                            param_names,
                            params,
                            param_kinds,
                            has_defaults,
                            ret: Box::new(desired.clone()),
                        };
                        self.set_var_type(name, updated);
                        return true;
                    }
                }
            }
        }
        false
    }
}
