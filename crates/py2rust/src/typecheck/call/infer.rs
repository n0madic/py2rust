// Callable return-type inference helpers.

use super::super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn infer_callable_return_for_args(
        &mut self,
        func: &mut Expr,
        arg_tys: &[Type],
        span: Span,
    ) -> Result<Type, CompileError> {
        // Build the arity error message once so we can use it without
        // capturing `self` in a closure (avoids borrow conflicts).
        let arity_error_msg = {
            let suffix = if arg_tys.len() == 1 { "" } else { "s" };
            format!(
                "Callable must take exactly {} argument{suffix}",
                arg_tys.len()
            )
        };

        match &mut func.kind {
            ExprKind::Name(name) => {
                match name.as_str() {
                    "str" => {
                        if arg_tys.len() != 1 {
                            return Err(self.error(span, arity_error_msg.clone()));
                        }
                        return Ok(Type::Str);
                    }
                    "int" => {
                        if arg_tys.len() != 1 {
                            return Err(self.error(span, arity_error_msg.clone()));
                        }
                        return Ok(Type::Int);
                    }
                    "float" => {
                        if arg_tys.len() != 1 {
                            return Err(self.error(span, arity_error_msg.clone()));
                        }
                        return Ok(Type::Float);
                    }
                    "max" | "min" => {
                        if arg_tys.len() != 2 {
                            return Err(self.error(span, arity_error_msg.clone()));
                        }
                        let left = &arg_tys[0];
                        let right = &arg_tys[1];
                        if left == right {
                            return Ok(left.clone());
                        }
                        let numeric = |ty: &Type| {
                            matches!(ty, Type::Int | Type::Float | Type::Bool | Type::Unknown)
                        };
                        if numeric(left) && numeric(right) {
                            if matches!(left, Type::Unknown) || matches!(right, Type::Unknown) {
                                return Ok(Type::Unknown);
                            }
                            if matches!(left, Type::Float) || matches!(right, Type::Float) {
                                return Ok(Type::Float);
                            }
                            return Ok(Type::Int);
                        }
                        return Ok(Type::Unknown);
                    }
                    _ => {}
                }
                if let Some(sig) = self.ctx.functions.get(name).cloned() {
                    if sig.params.len() != arg_tys.len() {
                        return Err(self.error(span, arity_error_msg.clone()));
                    }
                    for (arg_ty, param_ty) in arg_tys.iter().zip(sig.params.iter()) {
                        if !matches!(param_ty, Type::Unknown) {
                            self.ensure_assignable(arg_ty, param_ty, span)?;
                        }
                    }
                    return Ok(sig.ret);
                }
                if let Some(Type::Lambda { params, ret }) = self.lookup_var(name) {
                    let needs_refine = matches!(ret.as_ref(), Type::Unknown)
                        || params.iter().any(|p| matches!(p, Type::Unknown));
                    if needs_refine {
                        if let Some(lambda_expr) = self.lambda_defs.get(name).cloned() {
                            let expected = Type::Lambda {
                                params: arg_tys.to_vec(),
                                ret: Box::new(Type::Unknown),
                            };
                            let mut expr_clone = lambda_expr.clone();
                            let inferred = self.with_lambda_inference_guard(name, span, |tc| {
                                tc.check_expr(&mut expr_clone, Some(&expected))
                            })?;
                            if let Type::Lambda { params, ret } = inferred {
                                let updated = Type::Lambda {
                                    params: params.clone(),
                                    ret: ret.clone(),
                                };
                                self.set_var_type(name, updated);
                                return Ok(*ret);
                            }
                        }
                    }
                    if !params.is_empty() && params.len() != arg_tys.len() {
                        return Err(self.error(span, arity_error_msg.clone()));
                    }
                    for (arg_ty, param_ty) in arg_tys.iter().zip(params.iter()) {
                        if !matches!(param_ty, Type::Unknown) {
                            self.ensure_assignable(arg_ty, param_ty, span)?;
                        }
                    }
                    return Ok(*ret);
                }
                let ty = self.check_expr(func, None)?;
                if let Type::Lambda { params, ret } = ty {
                    if !params.is_empty() && params.len() != arg_tys.len() {
                        return Err(self.error(span, arity_error_msg.clone()));
                    }
                    for (arg_ty, param_ty) in arg_tys.iter().zip(params.iter()) {
                        if !matches!(param_ty, Type::Unknown) {
                            self.ensure_assignable(arg_ty, param_ty, span)?;
                        }
                    }
                    return Ok(*ret);
                }
                Ok(Type::Unknown)
            }
            ExprKind::Lambda { params, body } => {
                if params.len() != arg_tys.len() {
                    return Err(self.error(span, arity_error_msg.clone()));
                }
                self.scopes.push(HashMap::new());
                for (param, arg_ty) in params.iter().zip(arg_tys.iter()) {
                    self.insert_var(param, arg_ty.clone(), span)?;
                }
                let ret_ty = self.check_expr(body, None)?;
                self.scopes.pop();
                func.ty = Some(Type::Lambda {
                    params: arg_tys.to_vec(),
                    ret: Box::new(ret_ty.clone()),
                });
                Ok(ret_ty)
            }
            _ => {
                let ty = self.check_expr(func, None)?;
                if let Type::Lambda { params, ret } = ty {
                    if !params.is_empty() && params.len() != arg_tys.len() {
                        return Err(self.error(span, arity_error_msg));
                    }
                    for (arg_ty, param_ty) in arg_tys.iter().zip(params.iter()) {
                        if !matches!(param_ty, Type::Unknown) {
                            self.ensure_assignable(arg_ty, param_ty, span)?;
                        }
                    }
                    Ok(*ret)
                } else {
                    Ok(Type::Unknown)
                }
            }
        }
    }

    pub(super) fn infer_callable_return(
        &mut self,
        func: &mut Expr,
        arg_ty: &Type,
        span: Span,
    ) -> Result<Type, CompileError> {
        self.infer_callable_return_for_args(func, std::slice::from_ref(arg_ty), span)
    }
}
