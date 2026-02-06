// Callable return-type inference helpers.

use super::super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn infer_callable_return(
        &mut self,
        func: &mut Expr,
        arg_ty: &Type,
        span: Span,
    ) -> Result<Type, CompileError> {
        match &mut func.kind {
            ExprKind::Name(name) => {
                match name.as_str() {
                    "str" => return Ok(Type::Str),
                    "int" => return Ok(Type::Int),
                    "float" => return Ok(Type::Float),
                    _ => {}
                }
                if let Some(sig) = self.ctx.functions.get(name).cloned() {
                    if sig.params.len() != 1 {
                        return Err(self.error(span, "Callable must take exactly one argument"));
                    }
                    if !matches!(sig.params[0], Type::Unknown) {
                        self.ensure_assignable(arg_ty, &sig.params[0], span)?;
                    }
                    return Ok(sig.ret);
                }
                if let Some(Type::Lambda { params, ret }) = self.lookup_var(name) {
                    let needs_refine = matches!(ret.as_ref(), Type::Unknown)
                        || params.iter().any(|p| matches!(p, Type::Unknown));
                    if needs_refine {
                        if let Some(lambda_expr) = self.lambda_defs.get(name).cloned() {
                            let expected = Type::Lambda {
                                params: vec![arg_ty.clone()],
                                ret: Box::new(Type::Unknown),
                            };
                            let mut expr_clone = lambda_expr.clone();
                            let inferred = self.check_expr(&mut expr_clone, Some(&expected))?;
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
                    if !params.is_empty() && params.len() != 1 {
                        return Err(self.error(span, "Callable must take exactly one argument"));
                    }
                    if let Some(param) = params.first() {
                        if !matches!(param, Type::Unknown) {
                            self.ensure_assignable(arg_ty, param, span)?;
                        }
                    }
                    return Ok(*ret);
                }
                let ty = self.check_expr(func, None)?;
                if let Type::Lambda { ret, .. } = ty {
                    return Ok(*ret);
                }
                Ok(Type::Unknown)
            }
            ExprKind::Lambda { params, body } => {
                if params.len() != 1 {
                    return Err(self.error(span, "Callable must take exactly one argument"));
                }
                self.scopes.push(HashMap::new());
                self.insert_var(&params[0], arg_ty.clone(), span)?;
                let ret_ty = self.check_expr(body, None)?;
                self.scopes.pop();
                func.ty = Some(Type::Lambda {
                    params: vec![arg_ty.clone()],
                    ret: Box::new(ret_ty.clone()),
                });
                Ok(ret_ty)
            }
            _ => {
                let ty = self.check_expr(func, None)?;
                if let Type::Lambda { ret, .. } = ty {
                    Ok(*ret)
                } else {
                    Ok(Type::Unknown)
                }
            }
        }
    }
}
