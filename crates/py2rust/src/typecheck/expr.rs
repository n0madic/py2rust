use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_expr(
        &mut self,
        expr: &mut Expr,
        expected: Option<&Type>,
    ) -> Result<Type, CompileError> {
        let ty = match &mut expr.kind {
            ExprKind::Literal(lit) => match lit {
                Literal::Int(_) => Type::Int,
                Literal::Float(_) => Type::Float,
                Literal::Bool(_) => Type::Bool,
                Literal::Str(_) => Type::Str,
                Literal::None => Type::None,
            },
            ExprKind::Name(name) => {
                self.note_global_use(name, expr.span);
                if let Some(mut ty) = self.lookup_var(name) {
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
                    Type::Lambda {
                        params: sig.params.clone(),
                        ret: Box::new(sig.ret.clone()),
                    }
                } else {
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
                            expr.kind = ExprKind::Literal(Literal::None);
                            Type::None
                        }
                    }
                }
            }
            ExprKind::Attr { value, attr } => {
                if attr == "__name__" {
                    if let ExprKind::Call { func, args } = &mut value.kind {
                        if let ExprKind::Name(name) = &func.kind {
                            if name == "type" && args.len() == 1 {
                                let _ = self.check_expr(&mut args[0], None)?;
                                return Ok(Type::Str);
                            }
                        }
                    }
                }
                let value_ty = self.check_expr(value, None)?;
                match value_ty {
                    Type::Custom(class_name) => {
                        let class_info = self.ctx.classes.get(&class_name).ok_or_else(|| {
                            self.error(expr.span, format!("Unknown class: {class_name}"))
                        })?;
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
            ExprKind::Call { func, args } => self.check_call(func, args, expected, expr.span)?,
            ExprKind::Binary { op, left, right } => {
                let left_ty = self.check_expr(left, None)?;
                let right_ty = self.check_expr(right, None)?;
                if matches!(left_ty, Type::Unknown) && !matches!(right_ty, Type::Unknown) {
                    self.maybe_update_from_expr(left, &right_ty);
                }
                if matches!(right_ty, Type::Unknown) && !matches!(left_ty, Type::Unknown) {
                    self.maybe_update_from_expr(right, &left_ty);
                }
                if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
                    return Ok(Type::Unknown);
                }
                match op {
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
                        return Err(self.error(expr.span, "Set operation requires set operands"));
                    }
                    BinOp::Sub => {
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
                        // Check for division by zero with literal zero
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
                }
            }
            ExprKind::Compare { op, left, right } => {
                let left_ty = self.check_expr(left, None)?;
                let right_ty = self.check_expr(right, None)?;
                match op {
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
                                                expr.span,
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
                                return Err(self.error(
                                    expr.span,
                                    "Membership requires list, tuple, set, dict, or str",
                                ))
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
                            return Err(self.error(
                                expr.span,
                                "Membership requires matching element type",
                            ));
                        }
                        Type::Bool
                    }
                    CmpOp::Is | CmpOp::IsNot => {
                        if matches!(right_ty, Type::None)
                            && !left_ty.is_optional()
                            && !matches!(left_ty, Type::None)
                        {
                            return Err(self.error(expr.span, "is None requires Optional type"));
                        }
                        Type::Bool
                    }
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
                            return Err(self.error(expr.span, "Comparison requires matching types"));
                        }
                        Type::Bool
                    }
                }
            }
            ExprKind::BoolOp { op: _, values } => {
                for v in values {
                    let ty = self.check_expr(v, Some(&Type::Bool))?;
                    if !matches!(ty, Type::Unknown) {
                        self.ensure_assignable(&ty, &Type::Bool, expr.span)?;
                    }
                }
                Type::Bool
            }
            ExprKind::List(items) => {
                if items.is_empty() {
                    if let Some(Type::List(inner)) = expected {
                        Type::List(inner.clone())
                    } else {
                        Type::List(Box::new(Type::Unknown))
                    }
                } else {
                    let first_ty = self.check_expr(&mut items[0], None)?;
                    for item in &mut items[1..] {
                        let ty = self.check_expr(item, Some(&first_ty))?;
                        self.ensure_assignable(&ty, &first_ty, expr.span)?;
                    }
                    Type::List(Box::new(first_ty))
                }
            }
            ExprKind::Tuple(items) => {
                let mut tys = Vec::new();
                for item in items {
                    tys.push(self.check_expr(item, None)?);
                }
                Type::Tuple(tys)
            }
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
            ExprKind::Index { value, index } => {
                let value_ty = self.check_expr(value, None)?;
                let index_ty = self.check_expr(index, None)?;
                match value_ty {
                    Type::List(inner) => {
                        self.ensure_assignable(&index_ty, &Type::Int, expr.span)?;
                        *inner
                    }
                    Type::Dict(key_ty, val_ty) => {
                        self.ensure_assignable(&index_ty, &key_ty, expr.span)?;
                        *val_ty
                    }
                    Type::Tuple(items) => {
                        if let ExprKind::Literal(Literal::Int(idx)) = &index.as_ref().kind {
                            let idx = *idx as usize;
                            if idx >= items.len() {
                                return Err(self.error(expr.span, "Tuple index out of bounds"));
                            }
                            items[idx].clone()
                        } else {
                            return Err(self.error(expr.span, "Tuple indices must be literals"));
                        }
                    }
                    _ => {
                        return Err(self.error(expr.span, "Indexing requires list, dict, or tuple"))
                    }
                }
            }
            ExprKind::Slice {
                value,
                start: _,
                end: _,
            } => {
                let value_ty = self.check_expr(value, None)?;
                match value_ty {
                    Type::List(inner) => Type::List(inner),
                    Type::Str => Type::Str,
                    _ => return Err(self.error(expr.span, "Slicing requires list or str")),
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
                self.scopes.pop();
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
}
