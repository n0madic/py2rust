// Lambda expression lowering and helper utilities.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Lower a lambda expression, optionally using inferred parameter types.
    pub(super) fn gen_lambda_expr(
        &mut self,
        expr: &Expr,
        params: &[String],
        body: &Expr,
    ) -> Result<String, CompileError> {
        let param_types = if let Some(Type::Lambda { params, .. }) = expr.ty.as_ref() {
            Some(params.as_slice())
        } else {
            None
        };
        self.gen_lambda_with_param_types(params, body, param_types)
    }

    /// Render a lambda while applying expected parameter types when available.
    pub(super) fn gen_lambda_with_param_types(
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
        self.lambda_depth += 1;
        let body_expr = self.gen_expr(body);
        self.lambda_depth -= 1;
        let body_expr = body_expr?;
        self.local_vars = saved_locals;
        Ok(format!(
            "move |{}| {{ {} }}",
            param_parts.join(", "),
            body_expr
        ))
    }
}
