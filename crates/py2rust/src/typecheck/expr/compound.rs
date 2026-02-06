use super::*;

impl<'a> TypeChecker<'a> {
    /// Type check list literal expressions.
    pub(super) fn check_list_expr(
        &mut self,
        items: &mut [Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        if items.is_empty() {
            if let Some(Type::List(inner)) = expected {
                return Ok(Type::List(inner.clone()));
            }
            return Ok(Type::List(Box::new(Type::Unknown)));
        }

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
                    && self.ensure_assignable(&ty, &elem_ty, span).is_err()
                {
                    all_ok = false;
                    if expected_inner.is_some() {
                        return Err(self.error(span, "List element type mismatch"));
                    }
                    break;
                }
            }
            if all_ok {
                Ok(Type::List(Box::new(elem_ty)))
            } else {
                Ok(Type::List(Box::new(Type::Unknown)))
            }
        } else {
            Ok(Type::List(Box::new(Type::Unknown)))
        }
    }

    /// Type check tuple literal expressions.
    pub(super) fn check_tuple_expr(&mut self, items: &mut [Expr]) -> Result<Type, CompileError> {
        let mut tys = Vec::new();
        for item in items {
            tys.push(self.check_expr(item, None)?);
        }
        Ok(Type::Tuple(tys))
    }

    /// Type check set literal expressions.
    pub(super) fn check_set_expr(
        &mut self,
        items: &mut [Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        if items.is_empty() {
            if let Some(Type::Set(inner)) = expected {
                return Ok(Type::Set(inner.clone()));
            }
            return Ok(Type::Unknown);
        }

        let first_ty = self.check_expr(&mut items[0], None)?;
        for item in &mut items[1..] {
            let ty = self.check_expr(item, Some(&first_ty))?;
            if !matches!(ty, Type::Unknown) && !matches!(first_ty, Type::Unknown) {
                self.ensure_assignable(&ty, &first_ty, span)?;
            }
        }
        Ok(Type::Set(Box::new(first_ty)))
    }

    /// Type check dict literal expressions.
    pub(super) fn check_dict_expr(
        &mut self,
        items: &mut [(Expr, Expr)],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        if items.is_empty() {
            if let Some(Type::Dict(key_ty, val_ty)) = expected {
                return Ok(Type::Dict(key_ty.clone(), val_ty.clone()));
            }
            return Ok(Type::Unknown);
        }

        if let Some(Type::Dict(expected_key, expected_val)) = expected {
            // Honor dict annotation hints so wide unions can flow as Unknown.
            let key_ty = (*expected_key.clone()).clone();
            let val_ty = (*expected_val.clone()).clone();
            for (k, v) in items.iter_mut() {
                let kt = self.check_expr(k, Some(&key_ty))?;
                let vt = self.check_expr(v, Some(&val_ty))?;
                self.ensure_assignable(&kt, &key_ty, span)?;
                self.ensure_assignable(&vt, &val_ty, span)?;
            }
            return Ok(Type::Dict(Box::new(key_ty), Box::new(val_ty)));
        }

        let (k0, v0) = &mut items[0];
        let key_ty = self.check_expr(k0, None)?;
        let val_ty = self.check_expr(v0, None)?;
        for (k, v) in &mut items[1..] {
            let kt = self.check_expr(k, Some(&key_ty))?;
            let vt = self.check_expr(v, Some(&val_ty))?;
            self.ensure_assignable(&kt, &key_ty, span)?;
            self.ensure_assignable(&vt, &val_ty, span)?;
        }
        Ok(Type::Dict(Box::new(key_ty), Box::new(val_ty)))
    }

    /// Type check index operations (`x[i]`).
    pub(super) fn check_index_expr(
        &mut self,
        value: &mut Expr,
        index: &mut Expr,
        span: Span,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, None)?;
        let index_ty = self.check_expr(index, None)?;
        match value_ty {
            Type::List(inner) => {
                self.ensure_assignable(&index_ty, &Type::Int, span)?;
                Ok(*inner)
            }
            Type::Str => {
                self.ensure_assignable(&index_ty, &Type::Int, span)?;
                Ok(Type::Str)
            }
            Type::Bytes => {
                self.ensure_assignable(&index_ty, &Type::Int, span)?;
                Ok(Type::Int)
            }
            Type::Dict(key_ty, val_ty) => {
                self.ensure_assignable(&index_ty, &key_ty, span)?;
                Ok(*val_ty)
            }
            Type::Tuple(items) => {
                // Tuple indexing requires a literal index to preserve precise output type.
                let idx_opt = match &index.kind {
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
                        return Err(self.error(span, "Tuple index out of bounds"));
                    }
                    Ok(items[adj as usize].clone())
                } else {
                    Err(self.error(span, "Tuple indices must be literals"))
                }
            }
            Type::Option(inner) => {
                // Optional container indexing mirrors runtime behavior.
                match inner.as_ref() {
                    Type::List(elem_ty) => {
                        self.ensure_assignable(&index_ty, &Type::Int, span)?;
                        Ok(elem_ty.as_ref().clone())
                    }
                    Type::Str => {
                        self.ensure_assignable(&index_ty, &Type::Int, span)?;
                        Ok(Type::Str)
                    }
                    Type::Bytes => {
                        self.ensure_assignable(&index_ty, &Type::Int, span)?;
                        Ok(Type::Int)
                    }
                    Type::Dict(key_ty, val_ty) => {
                        self.ensure_assignable(&index_ty, key_ty.as_ref(), span)?;
                        Ok(val_ty.as_ref().clone())
                    }
                    Type::Tuple(items) => {
                        let idx_opt = match &index.kind {
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
                                return Err(self.error(span, "Tuple index out of bounds"));
                            }
                            Ok(items[adj as usize].clone())
                        } else {
                            Err(self.error(span, "Tuple indices must be literals"))
                        }
                    }
                    _ => {
                        Err(self.error(span, "Indexing requires list, dict, tuple, str, or bytes"))
                    }
                }
            }
            _ => Err(self.error(span, "Indexing requires list, dict, tuple, str, or bytes")),
        }
    }

    /// Type check slicing expressions (`x[a:b:c]`).
    pub(super) fn check_slice_expr(
        &mut self,
        value: &mut Expr,
        start: &mut Option<Box<Expr>>,
        end: &mut Option<Box<Expr>>,
        step: &mut Option<Box<Expr>>,
        span: Span,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, None)?;

        if let Some(s) = start.as_deref_mut() {
            let s_ty = self.check_expr(s, Some(&Type::Int))?;
            self.ensure_assignable(&s_ty, &Type::Int, span)?;
        }
        if let Some(e) = end.as_deref_mut() {
            let e_ty = self.check_expr(e, Some(&Type::Int))?;
            self.ensure_assignable(&e_ty, &Type::Int, span)?;
        }
        if let Some(step_expr) = step.as_deref_mut() {
            let step_ty = self.check_expr(step_expr, Some(&Type::Int))?;
            self.ensure_assignable(&step_ty, &Type::Int, span)?;
            if let ExprKind::Literal(Literal::Int(0)) = &step_expr.kind {
                return Err(self.error(span, "Slice step cannot be zero"));
            }
        }

        match value_ty {
            Type::List(inner) => Ok(Type::List(inner)),
            Type::Str => Ok(Type::Str),
            Type::Bytes => Ok(Type::Bytes),
            Type::Tuple(items) => {
                // Tuple slicing requires literal bounds to compute output tuple type.
                let lit_int = |expr: &Expr| -> Option<i64> {
                    match &expr.kind {
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
                    }
                };

                let start_lit = start.as_deref().and_then(lit_int);
                let end_lit = end.as_deref().and_then(lit_int);
                let step_lit = step.as_deref().and_then(lit_int).unwrap_or(1);
                if step.is_some() && step_lit == 0 {
                    return Err(self.error(span, "Slice step cannot be zero"));
                }
                if (start.is_some() && start_lit.is_none())
                    || (end.is_some() && end_lit.is_none())
                    || (step.is_some() && step_lit == 0)
                    || (step.is_some()
                        && step_lit != 0
                        && step.as_deref().and_then(lit_int).is_none())
                {
                    return Err(self.error(span, "Tuple slicing requires literal bounds"));
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
                Ok(Type::Tuple(out))
            }
            _ => Err(self.error(span, "Slicing requires list, tuple, str, or bytes")),
        }
    }

    /// Type check list comprehension expressions.
    pub(super) fn check_list_comp_expr(
        &mut self,
        elt: &mut Expr,
        target: &str,
        iter: &mut Expr,
        ifs: &mut [Expr],
        span: Span,
    ) -> Result<Type, CompileError> {
        let iter_ty = self.check_expr(iter, None)?;
        let item_ty = self.iter_item_type(&iter_ty, span)?;
        self.scopes.push(HashMap::new());
        self.insert_var(target, item_ty.clone(), span)?;
        for cond in ifs {
            let cond_ty = self.check_expr(cond, Some(&Type::Bool))?;
            self.ensure_assignable(&cond_ty, &Type::Bool, span)?;
        }
        let elt_ty = self.check_expr(elt, None)?;
        self.scopes.pop();
        Ok(Type::List(Box::new(elt_ty)))
    }

    /// Type check set comprehension expressions.
    pub(super) fn check_set_comp_expr(
        &mut self,
        elt: &mut Expr,
        target: &str,
        iter: &mut Expr,
        ifs: &mut [Expr],
        span: Span,
    ) -> Result<Type, CompileError> {
        let iter_ty = self.check_expr(iter, None)?;
        let item_ty = self.iter_item_type(&iter_ty, span)?;
        self.scopes.push(HashMap::new());
        self.insert_var(target, item_ty.clone(), span)?;
        for cond in ifs {
            let cond_ty = self.check_expr(cond, Some(&Type::Bool))?;
            self.ensure_assignable(&cond_ty, &Type::Bool, span)?;
        }
        let elt_ty = self.check_expr(elt, None)?;
        self.scopes.pop();
        Ok(Type::Set(Box::new(elt_ty)))
    }

    /// Type check union constructor nodes.
    pub(super) fn check_union_ctor_expr(
        &mut self,
        union: &str,
        variant: &str,
        inner: &mut Expr,
        span: Span,
    ) -> Result<Type, CompileError> {
        let inner_ty = self.check_expr(inner, None)?;
        let expected_union = Type::Union(union.to_string());
        if let Type::Custom(class_name) = inner_ty {
            if class_name != variant {
                return Err(self.error(span, "Union constructor mismatch"));
            }
            Ok(expected_union)
        } else {
            Ok(expected_union)
        }
    }

    /// Type check lambda expressions.
    pub(super) fn check_lambda_expr(
        &mut self,
        params: &mut [String],
        body: &mut Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
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
                return Err(self.error(span, "Lambda parameter count mismatch"));
            }
            for (param, ty) in params.iter().zip(expected_params.iter()) {
                self.insert_var(param, ty.clone(), span)?;
            }
        } else {
            for param in params.iter() {
                self.insert_var(param, Type::Unknown, span)?;
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
                Self::infer_lambda_block_return(stmts)
            } else {
                self.resolve_type_ref(&expected_ret, span)?
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

        Ok(Type::Lambda {
            params: param_tys,
            ret: Box::new(ret_ty),
        })
    }

    /// Infer return type from explicit `return` statements inside a lambda block.
    fn infer_lambda_block_return(stmts: &[Stmt]) -> Type {
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

        let mut inferred: Option<Type> = None;
        for stmt in stmts {
            visit(stmt, &mut inferred);
        }
        inferred.unwrap_or(Type::None)
    }

    /// Type check conditional expression (`a if cond else b`).
    pub(super) fn check_if_expr_expr(
        &mut self,
        test: &mut Expr,
        body: &mut Expr,
        orelse: &mut Expr,
        span: Span,
    ) -> Result<Type, CompileError> {
        let cond_ty = self.check_expr(test, Some(&Type::Bool))?;
        if !matches!(cond_ty, Type::Unknown) {
            self.ensure_assignable(&cond_ty, &Type::Bool, span)?;
        }

        let body_ty = self.check_expr(body, None)?;
        let else_ty = self.check_expr(orelse, None)?;
        if body_ty == else_ty {
            Ok(body_ty)
        } else if body_ty.is_numeric() && else_ty.is_numeric() {
            if matches!(body_ty, Type::Float) || matches!(else_ty, Type::Float) {
                Ok(Type::Float)
            } else {
                Ok(Type::Int)
            }
        } else if matches!(body_ty, Type::Unknown) {
            Ok(else_ty)
        } else if matches!(else_ty, Type::Unknown) {
            Ok(body_ty)
        } else {
            Ok(Type::Unknown)
        }
    }

    /// Type check block expressions.
    pub(super) fn check_block_expr(&mut self, stmts: &mut [Stmt]) -> Result<Type, CompileError> {
        self.scopes.push(HashMap::new());
        let expected = TypeRef::Unknown;
        for stmt in stmts {
            self.check_stmt(stmt, Some(&expected))?;
        }
        self.scopes.pop();
        Ok(Type::None)
    }
}
