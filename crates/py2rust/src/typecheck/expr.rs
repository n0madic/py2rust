use super::*;

/// Expression type checking.
///
/// This module implements the core type checking logic for expressions.
/// Key responsibilities:
/// 1. Infer types for expressions without annotations
/// 2. Check that operations are valid for their operand types
/// 3. Propagate type information (e.g., x is int implies x+1 is int)
/// 4. Handle Python-specific semantics (division, string ops, etc.)
///
/// Design decisions:
/// - Type::Unknown is allowed locally but we try to resolve it
/// - Mixed int/float arithmetic promotes to float
/// - String operations (+ for concat, * for repeat) are special-cased
/// - Set operations (|, &, -, ^) are distinct from bitwise ops
/// - Division always returns float (Python 3 semantics)
/// - We track expected types and use them to guide inference
impl<'a> TypeChecker<'a> {
    /// Check an expression and determine its type.
    ///
    /// The expected parameter guides type inference when we have a hint
    /// about what type we want (e.g., from an annotation or context).
    ///
    /// Returns the inferred type and updates expr.ty.
    pub(super) fn check_expr(
        &mut self,
        expr: &mut Expr,
        expected: Option<&Type>,
    ) -> Result<Type, CompileError> {
        let ty = match &mut expr.kind {
            // Literals have straightforward types
            ExprKind::Literal(lit) => match lit {
                Literal::Int(_) => Type::Int,
                Literal::Float(_) => Type::Float,
                Literal::Bool(_) => Type::Bool,
                Literal::Str(_) => Type::Str,
                Literal::Bytes(_) => Type::Bytes,
                Literal::None => Type::None,
            },
            // Variable reference: look up in scopes
            ExprKind::Name(name) => {
                // Track global usage for validation
                self.note_global_use(name, expr.span);
                // Track nonlocal usage for validation
                self.note_nonlocal_use(name, expr.span);
                if let Some(mut ty) = self.lookup_var(name) {
                    // If variable is Unknown, try to use expected type
                    if matches!(ty, Type::Unknown) {
                        if let Some(expected) = expected {
                            if !matches!(expected, Type::Unknown) {
                                self.set_var_type(name, expected.clone());
                                ty = expected.clone();
                            }
                        }
                    }
                    ty
                } else if let Some(sig) = self.ctx.functions.get(name) {
                    // Function reference (can be called later)
                    Type::Lambda {
                        params: sig.params.clone(),
                        ret: Box::new(sig.ret.clone()),
                    }
                } else {
                    // Built-in type constructors: str(), int(), float()
                    // These are special - they're not in ctx.functions
                    match name.as_str() {
                        "str" => Type::Lambda {
                            params: vec![Type::Unknown],
                            ret: Box::new(Type::Str),
                        },
                        "int" => Type::Lambda {
                            params: vec![Type::Unknown],
                            ret: Box::new(Type::Int),
                        },
                        "float" => Type::Lambda {
                            params: vec![Type::Unknown],
                            ret: Box::new(Type::Float),
                        },
                        _ => {
                            if self.ctx.classes.contains_key(name) {
                                return Ok(Type::Custom(name.clone()));
                            }
                            // Unknown name - set to None literal to avoid cascading errors
                            expr.kind = ExprKind::Literal(Literal::None);
                            Type::None
                        }
                    }
                }
            }
            // Attribute access: obj.field
            ExprKind::Attr { value, attr } => {
                // Special case: type(x).__name__ is always a str
                if attr == "__name__" {
                    if let ExprKind::Call {
                        func,
                        args,
                        keywords,
                    } = &mut value.kind
                    {
                        if let ExprKind::Name(name) = &func.kind {
                            if name == "type" && args.len() == 1 && keywords.is_empty() {
                                let _ = self.check_expr(&mut args[0], None)?;
                                return Ok(Type::Str);
                            }
                        }
                    }
                }
                let value_ty = self.check_expr(value, None)?;
                if matches!(value_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        if let Some(class_name) = self.current_class.as_ref() {
                            let prop_ty = self
                                .ctx
                                .classes
                                .get(class_name)
                                .and_then(|info| info.properties.get(attr))
                                .map(|prop| prop.ty.clone());
                            let field_ty = self
                                .ctx
                                .classes
                                .get(class_name)
                                .and_then(|info| info.fields.get(attr))
                                .cloned();
                            if let Some(ty) = prop_ty.or(field_ty) {
                                // Infer unknown parameter types inside dunder methods.
                                self.set_var_type(name, Type::Custom(class_name.clone()));
                                return Ok(ty);
                            }
                        }
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if let Some(class_info) = self.ctx.classes.get(name) {
                        if let Some(attr_info) = class_info.class_attrs.get(attr) {
                            return Ok(attr_info.ty.clone());
                        }
                    }
                }
                match value_ty {
                    Type::Custom(class_name) => {
                        let class_info = self.ctx.classes.get(&class_name).ok_or_else(|| {
                            self.error(expr.span, format!("Unknown class: {class_name}"))
                        })?;
                        if let Some(prop) = class_info.properties.get(attr) {
                            return Ok(prop.ty.clone());
                        }
                        class_info.fields.get(attr).cloned().ok_or_else(|| {
                            self.error(expr.span, format!("Unknown field {class_name}.{attr}"))
                        })?
                    }
                    _ => {
                        return Err(self.error(
                            expr.span,
                            "Attribute access only allowed on class instances",
                        ))
                    }
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => self.check_call(func, args, keywords, expected, expr.span)?,
            ExprKind::Starred { value } => {
                let value_ty = self.check_expr(value, None)?;
                let _ = self.iter_item_type(&value_ty, expr.span)?;
                return Err(self.error(
                    expr.span,
                    "Starred argument is only valid directly inside a call expression",
                ));
            }
            // Binary operations: +, -, *, /, etc.
            ExprKind::Binary { op, left, right } => {
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
                // Bidirectional type inference: if one side is Unknown, use the other
                if matches!(left_ty, Type::Unknown) && !matches!(right_ty, Type::Unknown) {
                    self.maybe_update_from_expr(left, &right_ty);
                }
                if matches!(right_ty, Type::Unknown) && !matches!(left_ty, Type::Unknown) {
                    self.maybe_update_from_expr(right, &left_ty);
                }
                // If both Unknown, we can't check much (but string concat is special)
                if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
                    if matches!(op, BinOp::Add)
                        && (matches!(left_ty, Type::Str) || matches!(right_ty, Type::Str))
                    {
                        return Ok(Type::Str);
                    }
                    return Ok(Type::Unknown);
                }
                match op {
                    // Set operations: | (union), & (intersection), ^ (symmetric difference)
                    // These overlap with bitwise operators but we detect set usage by type
                    // Bitwise ops are either set operations or integer bitwise.
                    BinOp::BitOr | BinOp::BitAnd | BinOp::BitXor => {
                        if let (Type::Set(left_inner), Type::Set(right_inner)) =
                            (&left_ty, &right_ty)
                        {
                            let inner = if **left_inner == **right_inner {
                                *left_inner.clone()
                            } else if matches!(**left_inner, Type::Unknown) {
                                *right_inner.clone()
                            } else if matches!(**right_inner, Type::Unknown) {
                                *left_inner.clone()
                            } else {
                                return Err(
                                    self.error(expr.span, "Set operands must have matching types")
                                );
                            };
                            return Ok(Type::Set(Box::new(inner)));
                        }
                        if matches!(left_ty, Type::Int) && matches!(right_ty, Type::Int) {
                            return Ok(Type::Int);
                        }
                        return Err(self.error(
                            expr.span,
                            "Bitwise operation requires int operands (or set operands)",
                        ));
                    }
                    BinOp::ShiftLeft | BinOp::ShiftRight => {
                        if matches!(left_ty, Type::Int) && matches!(right_ty, Type::Int) {
                            return Ok(Type::Int);
                        }
                        return Err(self.error(expr.span, "Shift operation requires int operands"));
                    }
                    // Subtraction: numeric or set difference
                    BinOp::Sub => {
                        // Set difference: {1, 2, 3} - {2} = {1, 3}
                        if let (Type::Set(left_inner), Type::Set(right_inner)) =
                            (&left_ty, &right_ty)
                        {
                            let inner = if **left_inner == **right_inner {
                                *left_inner.clone()
                            } else if matches!(**left_inner, Type::Unknown) {
                                *right_inner.clone()
                            } else if matches!(**right_inner, Type::Unknown) {
                                *left_inner.clone()
                            } else {
                                return Err(
                                    self.error(expr.span, "Set operands must have matching types")
                                );
                            };
                            return Ok(Type::Set(Box::new(inner)));
                        }
                        if !left_ty.is_numeric() || !right_ty.is_numeric() {
                            return Err(
                                self.error(expr.span, "Binary arithmetic requires numeric types")
                            );
                        }
                        if matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float) {
                            Type::Float
                        } else {
                            Type::Int
                        }
                    }
                    // Arithmetic operations: +, *, /, **, %, //
                    // Special cases:
                    // - str + str (concatenation)
                    // - str * int (repetition)
                    // - tuple + tuple (concatenation)
                    BinOp::Add
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Pow
                    | BinOp::Mod
                    | BinOp::FloorDiv => {
                        if matches!(op, BinOp::Add)
                            && (matches!(left_ty, Type::Str) || matches!(right_ty, Type::Str))
                        {
                            let ty = Type::Str;
                            expr.ty = Some(ty.clone());
                            return Ok(ty);
                        }
                        if matches!(op, BinOp::Mul) {
                            let left_is_str = matches!(left_ty, Type::Str);
                            let right_is_str = matches!(right_ty, Type::Str);
                            let left_is_int = matches!(left_ty, Type::Int);
                            let right_is_int = matches!(right_ty, Type::Int);
                            if (left_is_str && right_is_int) || (right_is_str && left_is_int) {
                                let ty = Type::Str;
                                expr.ty = Some(ty.clone());
                                return Ok(ty);
                            }
                        }
                        if !left_ty.is_numeric() || !right_ty.is_numeric() {
                            if matches!(op, BinOp::Add) {
                                if let (Type::Tuple(left_items), Type::Tuple(right_items)) =
                                    (&left_ty, &right_ty)
                                {
                                    let mut combined = left_items.clone();
                                    combined.extend(right_items.clone());
                                    return Ok(Type::Tuple(combined));
                                }
                            }
                            return Err(
                                self.error(expr.span, "Binary arithmetic requires numeric types")
                            );
                        }
                        // Detect division by zero with constant literals
                        // This is a warning, not an error (might be intentional for infinity)
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
                                Type::Float
                            } else {
                                Type::Int
                            }
                        } else if matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float)
                        {
                            Type::Float
                        } else {
                            Type::Int
                        }
                    }
                }
            }
            ExprKind::Unary { op, expr: inner } => {
                let inner_ty = self.check_expr(inner, None)?;
                if matches!(inner_ty, Type::Unknown) {
                    return Ok(Type::Unknown);
                }
                match op {
                    UnaryOp::Neg => {
                        if !inner_ty.is_numeric() {
                            return Err(self.error(expr.span, "Unary - requires numeric type"));
                        }
                        inner_ty
                    }
                    UnaryOp::Not => {
                        self.ensure_assignable(&inner_ty, &Type::Bool, expr.span)?;
                        Type::Bool
                    }
                    UnaryOp::BitNot => {
                        if !matches!(inner_ty, Type::Int) {
                            return Err(self.error(expr.span, "Unary ~ requires int type"));
                        }
                        Type::Int
                    }
                }
            }
            // Comparison operators: ==, !=, <, <=, >, >=, is, is not, in, not in
            ExprKind::Compare { op, left, right } => {
                self.check_compare_expr(expr.span, op, left, right)?
            }
            // Chained comparisons: a < b < c
            ExprKind::CompareChain {
                left,
                ops,
                comparators,
            } => {
                if ops.len() != comparators.len() {
                    return Err(self.error(expr.span, "Invalid comparison chain"));
                }
                if ops.is_empty() {
                    return Err(self.error(expr.span, "Invalid comparison chain"));
                }
                self.check_compare_expr(expr.span, &ops[0], left, &mut comparators[0])?;
                for idx in 1..ops.len() {
                    let (head, tail) = comparators.split_at_mut(idx);
                    let left_expr = &mut head[idx - 1];
                    let right_expr = &mut tail[0];
                    self.check_compare_expr(expr.span, &ops[idx], left_expr, right_expr)?;
                }
                Type::Bool
            }
            // Boolean operations: and, or
            // These return one of the operands (Python semantics), not just bool.
            ExprKind::BoolOp { op: _, values } => {
                let mut value_tys = Vec::new();
                for v in values.iter_mut() {
                    value_tys.push(self.check_expr(v, None)?);
                }

                let all_bool = value_tys
                    .iter()
                    .all(|ty| matches!(ty, Type::Bool | Type::Unknown));
                if all_bool {
                    for (v, ty) in values.iter_mut().zip(value_tys.iter()) {
                        if matches!(ty, Type::Unknown) {
                            self.maybe_update_from_expr(v, &Type::Bool);
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
                            return Err(self.error(
                                expr.span,
                                "Boolean operator requires matching operand types",
                            ));
                        }
                    } else {
                        candidate = Some(ty.clone());
                    }
                }

                if let Some(candidate) = candidate {
                    for (v, ty) in values.iter_mut().zip(value_tys.iter()) {
                        if matches!(ty, Type::Unknown) {
                            self.maybe_update_from_expr(v, &candidate);
                        }
                    }
                    candidate
                } else {
                    Type::Unknown
                }
            }
            // List literal: [1, 2, 3]
            // All elements must have the same type (homogeneous)
            ExprKind::List(items) => {
                // Empty list: infer from expected type or default to Unknown
                if items.is_empty() {
                    if let Some(Type::List(inner)) = expected {
                        Type::List(inner.clone())
                    } else {
                        Type::List(Box::new(Type::Unknown))
                    }
                } else {
                    let expected_inner = match expected {
                        Some(Type::List(inner)) => Some((*inner.as_ref()).clone()),
                        _ => None,
                    };
                    let mut elem_ty = expected_inner.clone();
                    if elem_ty.is_none() {
                        for item in items.iter_mut() {
                            let ty = self.check_expr(item, None)?;
                            if !matches!(ty, Type::Unknown) {
                                elem_ty = Some(ty);
                                break;
                            }
                        }
                    }
                    if let Some(elem_ty) = elem_ty {
                        let mut all_ok = true;
                        for item in items.iter_mut() {
                            let ty = self.check_expr(item, Some(&elem_ty))?;
                            if !matches!(ty, Type::Unknown)
                                && !matches!(elem_ty, Type::Unknown)
                                && self.ensure_assignable(&ty, &elem_ty, expr.span).is_err()
                            {
                                all_ok = false;
                                if expected_inner.is_some() {
                                    return Err(self.error(expr.span, "List element type mismatch"));
                                }
                                break;
                            }
                        }
                        if all_ok {
                            Type::List(Box::new(elem_ty))
                        } else {
                            Type::List(Box::new(Type::Unknown))
                        }
                    } else {
                        Type::List(Box::new(Type::Unknown))
                    }
                }
            }
            // Tuple literal: (1, "hello", True)
            // Elements can have different types (heterogeneous)
            ExprKind::Tuple(items) => {
                let mut tys = Vec::new();
                for item in items {
                    tys.push(self.check_expr(item, None)?);
                }
                Type::Tuple(tys)
            }
            // Set literal: {1, 2, 3}
            // All elements must have the same type
            ExprKind::Set(items) => {
                if items.is_empty() {
                    if let Some(Type::Set(inner)) = expected {
                        Type::Set(inner.clone())
                    } else {
                        Type::Unknown
                    }
                } else {
                    let first_ty = self.check_expr(&mut items[0], None)?;
                    for item in &mut items[1..] {
                        let ty = self.check_expr(item, Some(&first_ty))?;
                        if !matches!(ty, Type::Unknown) && !matches!(first_ty, Type::Unknown) {
                            self.ensure_assignable(&ty, &first_ty, expr.span)?;
                        }
                    }
                    Type::Set(Box::new(first_ty))
                }
            }
            // Dict literal: {"a": 1, "b": 2}
            // All keys must have same type, all values must have same type
            ExprKind::Dict(items) => {
                if items.is_empty() {
                    if let Some(Type::Dict(key_ty, val_ty)) = expected {
                        Type::Dict(key_ty.clone(), val_ty.clone())
                    } else {
                        Type::Unknown
                    }
                } else {
                    let (k0, v0) = &mut items[0];
                    let key_ty = self.check_expr(k0, None)?;
                    let val_ty = self.check_expr(v0, None)?;
                    for (k, v) in &mut items[1..] {
                        let kt = self.check_expr(k, Some(&key_ty))?;
                        let vt = self.check_expr(v, Some(&val_ty))?;
                        self.ensure_assignable(&kt, &key_ty, expr.span)?;
                        self.ensure_assignable(&vt, &val_ty, expr.span)?;
                    }
                    Type::Dict(Box::new(key_ty), Box::new(val_ty))
                }
            }
            // Indexing: list[0], dict["key"], tuple[1]
            // Returns element type for list/dict, specific tuple element for tuple
            ExprKind::Index { value, index } => {
                let value_ty = self.check_expr(value, None)?;
                let index_ty = self.check_expr(index, None)?;
                match value_ty {
                    Type::List(inner) => {
                        // List indexing requires int
                        self.ensure_assignable(&index_ty, &Type::Int, expr.span)?;
                        *inner
                    }
                    Type::Bytes => {
                        // Bytes indexing returns int
                        self.ensure_assignable(&index_ty, &Type::Int, expr.span)?;
                        Type::Int
                    }
                    Type::Dict(key_ty, val_ty) => {
                        // Dict indexing requires matching key type
                        self.ensure_assignable(&index_ty, &key_ty, expr.span)?;
                        *val_ty
                    }
                    Type::Tuple(items) => {
                        // Tuple indexing: if literal index, check bounds and return specific type
                        let idx_opt = match &index.as_ref().kind {
                            ExprKind::Literal(Literal::Int(idx)) => Some(*idx),
                            ExprKind::Unary {
                                op: UnaryOp::Neg,
                                expr,
                            } => {
                                if let ExprKind::Literal(Literal::Int(idx)) = &expr.as_ref().kind {
                                    Some(-idx)
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if let Some(idx) = idx_opt {
                            let len_i = items.len() as i64;
                            let mut adj = idx;
                            if adj < 0 {
                                adj += len_i;
                            }
                            if adj < 0 || adj >= len_i {
                                return Err(self.error(expr.span, "Tuple index out of bounds"));
                            }
                            items[adj as usize].clone()
                        } else {
                            return Err(self.error(expr.span, "Tuple indices must be literals"));
                        }
                    }
                    _ => {
                        return Err(
                            self.error(expr.span, "Indexing requires list, dict, tuple, or bytes")
                        )
                    }
                }
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                let value_ty = self.check_expr(value, None)?;
                if let Some(s) = start.as_deref_mut() {
                    let s_ty = self.check_expr(s, Some(&Type::Int))?;
                    self.ensure_assignable(&s_ty, &Type::Int, expr.span)?;
                }
                if let Some(e) = end.as_deref_mut() {
                    let e_ty = self.check_expr(e, Some(&Type::Int))?;
                    self.ensure_assignable(&e_ty, &Type::Int, expr.span)?;
                }
                if let Some(step) = step.as_deref_mut() {
                    let step_ty = self.check_expr(step, Some(&Type::Int))?;
                    self.ensure_assignable(&step_ty, &Type::Int, expr.span)?;
                    if let ExprKind::Literal(Literal::Int(0)) = &step.kind {
                        return Err(self.error(expr.span, "Slice step cannot be zero"));
                    }
                }
                match value_ty {
                    Type::List(inner) => Type::List(inner),
                    Type::Str => Type::Str,
                    Type::Bytes => Type::Bytes,
                    Type::Tuple(items) => {
                        // Tuple slicing requires literal bounds so we can compute the resulting type.
                        let lit_int = |expr: &Expr| -> Option<i64> {
                            match &expr.kind {
                                ExprKind::Literal(Literal::Int(idx)) => Some(*idx),
                                ExprKind::Unary {
                                    op: UnaryOp::Neg,
                                    expr,
                                } => {
                                    if let ExprKind::Literal(Literal::Int(idx)) =
                                        &expr.as_ref().kind
                                    {
                                        Some(-idx)
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            }
                        };

                        let start_lit = start.as_deref().and_then(lit_int);
                        let end_lit = end.as_deref().and_then(lit_int);
                        let step_lit = step.as_deref().and_then(lit_int).unwrap_or(1);
                        if step.is_some() && step_lit == 0 {
                            return Err(self.error(expr.span, "Slice step cannot be zero"));
                        }
                        if (start.is_some() && start_lit.is_none())
                            || (end.is_some() && end_lit.is_none())
                            || (step.is_some() && step_lit == 0)
                            || (step.is_some()
                                && step_lit != 0
                                && step.as_deref().and_then(lit_int).is_none())
                        {
                            return Err(
                                self.error(expr.span, "Tuple slicing requires literal bounds")
                            );
                        }

                        let len = items.len() as i64;
                        let mut indices = Vec::new();
                        if step_lit > 0 {
                            let mut i = match start_lit {
                                Some(s) => {
                                    let s = if s < 0 { len + s } else { s };
                                    s.max(0).min(len)
                                }
                                None => 0,
                            };
                            let end_i = match end_lit {
                                Some(e) => {
                                    let e = if e < 0 { len + e } else { e };
                                    e.max(0).min(len)
                                }
                                None => len,
                            };
                            while i < end_i {
                                if i >= 0 && i < len {
                                    indices.push(i as usize);
                                }
                                i += step_lit;
                            }
                        } else {
                            let mut i = match start_lit {
                                Some(s) => {
                                    let s = if s < 0 { len + s } else { s };
                                    if s < 0 {
                                        -1
                                    } else if s >= len {
                                        len - 1
                                    } else {
                                        s
                                    }
                                }
                                None => len - 1,
                            };
                            let end_i = match end_lit {
                                Some(e) => {
                                    let e = if e < 0 { len + e } else { e };
                                    if e < 0 {
                                        -1
                                    } else if e >= len {
                                        len - 1
                                    } else {
                                        e
                                    }
                                }
                                None => -1,
                            };
                            while i > end_i {
                                if i >= 0 && i < len {
                                    indices.push(i as usize);
                                }
                                i += step_lit;
                            }
                        }

                        let mut out = Vec::new();
                        for idx in indices {
                            if let Some(item) = items.get(idx) {
                                out.push(item.clone());
                            }
                        }
                        Type::Tuple(out)
                    }
                    _ => {
                        return Err(
                            self.error(expr.span, "Slicing requires list, tuple, str, or bytes")
                        )
                    }
                }
            }
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
            } => {
                let iter_ty = self.check_expr(iter, None)?;
                let item_ty = self.iter_item_type(&iter_ty, expr.span)?;
                self.scopes.push(HashMap::new());
                self.insert_var(target, item_ty.clone(), expr.span)?;
                for cond in ifs {
                    let cond_ty = self.check_expr(cond, Some(&Type::Bool))?;
                    self.ensure_assignable(&cond_ty, &Type::Bool, expr.span)?;
                }
                let elt_ty = self.check_expr(elt, None)?;
                self.scopes.pop();
                Type::List(Box::new(elt_ty))
            }
            ExprKind::SetComp {
                elt,
                target,
                iter,
                ifs,
            } => {
                let iter_ty = self.check_expr(iter, None)?;
                let item_ty = self.iter_item_type(&iter_ty, expr.span)?;
                self.scopes.push(HashMap::new());
                self.insert_var(target, item_ty.clone(), expr.span)?;
                for cond in ifs {
                    let cond_ty = self.check_expr(cond, Some(&Type::Bool))?;
                    self.ensure_assignable(&cond_ty, &Type::Bool, expr.span)?;
                }
                let elt_ty = self.check_expr(elt, None)?;
                self.scopes.pop();
                Type::Set(Box::new(elt_ty))
            }
            ExprKind::UnionCtor {
                union,
                variant,
                inner,
            } => {
                let inner_ty = self.check_expr(inner, None)?;
                let expected_union = Type::Union(union.clone());
                if let Type::Custom(class_name) = inner_ty {
                    if class_name != *variant {
                        return Err(self.error(expr.span, "Union constructor mismatch"));
                    }
                    expected_union
                } else {
                    expected_union
                }
            }
            ExprKind::Lambda { params, body } => {
                self.scopes.push(HashMap::new());
                self.global_scopes.push(GlobalScope::default());
                self.nonlocal_scopes.push(NonlocalScope::default());
                self.function_scopes.push(self.scopes.len() - 1);
                let expected_params = match expected {
                    Some(Type::Lambda { params, .. }) => Some(params),
                    _ => None,
                };
                if let Some(expected_params) = expected_params {
                    if !expected_params.is_empty() && expected_params.len() != params.len() {
                        return Err(self.error(expr.span, "Lambda parameter count mismatch"));
                    }
                    for (param, ty) in params.iter().zip(expected_params.iter()) {
                        self.insert_var(param, ty.clone(), expr.span)?;
                    }
                } else {
                    for param in params.iter() {
                        self.insert_var(param, Type::Unknown, expr.span)?;
                    }
                }
                let ret_ty = if let ExprKind::Block { stmts } = &mut body.kind {
                    let expected_ret = match expected {
                        Some(Type::Lambda { ret, .. }) => Self::type_to_ref(ret.as_ref()),
                        _ => TypeRef::Unknown,
                    };
                    for stmt in stmts.iter_mut() {
                        self.check_stmt(stmt, Some(&expected_ret))?;
                    }
                    if matches!(expected_ret, TypeRef::Unknown) {
                        let mut inferred: Option<Type> = None;
                        fn visit(stmt: &Stmt, inferred: &mut Option<Type>) {
                            match &stmt.kind {
                                StmtKind::Return { value } => {
                                    let ty = match value {
                                        Some(expr) => expr.ty.clone().unwrap_or(Type::Unknown),
                                        None => Type::None,
                                    };
                                    if let Some(existing) = inferred {
                                        if existing != &ty {
                                            *inferred = Some(Type::Unknown);
                                        }
                                    } else {
                                        *inferred = Some(ty);
                                    }
                                }
                                StmtKind::If { body, orelse, .. } => {
                                    for stmt in body {
                                        visit(stmt, inferred);
                                    }
                                    for stmt in orelse {
                                        visit(stmt, inferred);
                                    }
                                }
                                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                                    for stmt in body {
                                        visit(stmt, inferred);
                                    }
                                }
                                StmtKind::Match { cases, .. } => {
                                    for case in cases {
                                        for stmt in &case.body {
                                            visit(stmt, inferred);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        for stmt in stmts.iter() {
                            visit(stmt, &mut inferred);
                        }
                        inferred.unwrap_or(Type::None)
                    } else {
                        self.resolve_type_ref(&expected_ret, expr.span)?
                    }
                } else {
                    self.check_expr(body, None)?
                };
                let mut param_tys = Vec::new();
                if let Some(scope) = self.scopes.last() {
                    for param in params.iter() {
                        param_tys.push(scope.get(param).cloned().unwrap_or(Type::Unknown));
                    }
                }
                self.global_scopes.pop();
                self.nonlocal_scopes.pop();
                self.scopes.pop();
                self.function_scopes.pop();
                Type::Lambda {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                }
            }
            ExprKind::IfExpr { test, body, orelse } => {
                let cond_ty = self.check_expr(test, Some(&Type::Bool))?;
                if !matches!(cond_ty, Type::Unknown) {
                    self.ensure_assignable(&cond_ty, &Type::Bool, expr.span)?;
                }
                let body_ty = self.check_expr(body, None)?;
                let else_ty = self.check_expr(orelse, None)?;
                if body_ty == else_ty {
                    body_ty
                } else if body_ty.is_numeric() && else_ty.is_numeric() {
                    if matches!(body_ty, Type::Float) || matches!(else_ty, Type::Float) {
                        Type::Float
                    } else {
                        Type::Int
                    }
                } else if matches!(body_ty, Type::Unknown) {
                    else_ty
                } else if matches!(else_ty, Type::Unknown) {
                    body_ty
                } else {
                    Type::Unknown
                }
            }
            ExprKind::Block { stmts } => {
                self.scopes.push(HashMap::new());
                let expected = TypeRef::Unknown;
                for stmt in stmts {
                    self.check_stmt(stmt, Some(&expected))?;
                }
                self.scopes.pop();
                Type::None
            }
        };

        expr.ty = Some(ty.clone());
        Ok(ty)
    }

    /// Type check a single comparison expression and return its type.
    fn check_compare_expr(
        &mut self,
        span: Span,
        op: &CmpOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<Type, CompileError> {
        let left_ty = self.check_expr(left, None)?;
        let right_ty = self.check_expr(right, None)?;
        // Propagate type information to empty list literals
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
            // Membership testing: x in collection, x not in collection
            CmpOp::In | CmpOp::NotIn => {
                if matches!(right_ty, Type::Unknown) {
                    return Ok(Type::Bool);
                }
                // Extract element type from container
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
                        )
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
            // Identity comparison: x is None, x is not None
            // We require left side to be Optional if comparing to None
            CmpOp::Is | CmpOp::IsNot => {
                if matches!(right_ty, Type::None)
                    && !left_ty.is_optional()
                    && !matches!(left_ty, Type::None)
                {
                    return Err(self.error(span, "is None requires Optional type"));
                }
                Ok(Type::Bool)
            }
            // General comparisons: <, <=, >, >=, ==, !=
            // Require matching types with some flexibility for Optional and Unknown
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
                if let Some(Type::Lambda { params, ret }) = self.lookup_var(name) {
                    if matches!(ret.as_ref(), Type::Unknown) {
                        let updated = Type::Lambda {
                            params,
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
