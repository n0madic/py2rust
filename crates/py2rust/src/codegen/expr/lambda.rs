// Lambda expression lowering and helper utilities.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Lower a lambda expression, optionally using inferred parameter types.
    pub(super) fn gen_lambda_expr(
        &mut self,
        expr: &Expr,
        params: &[String],
        _param_kinds: &[ParamKind],
        _has_defaults: &[bool],
        body: &Expr,
    ) -> Result<String, CompileError> {
        let (param_types, ret_type) =
            if let Some(Type::Lambda { params, ret, .. }) = expr.ty.as_ref() {
                (Some(params.as_slice()), Some(ret.as_ref()))
            } else {
                (None, None)
            };
        self.gen_lambda_with_param_types(params, body, param_types, ret_type)
    }

    /// Render a lambda while applying expected parameter types when available.
    pub(super) fn gen_lambda_with_param_types(
        &mut self,
        params: &[String],
        body: &Expr,
        param_types: Option<&[Type]>,
        ret_type: Option<&Type>,
    ) -> Result<String, CompileError> {
        let mut param_parts = Vec::new();
        let mut lambda_param_types: Vec<Type> = Vec::new();
        if let Some(param_tys) = param_types {
            for (name, ty) in params.iter().zip(param_tys.iter()) {
                if matches!(ty, Type::Unknown) {
                    lambda_param_types.push(ty.clone());
                    param_parts.push(name.clone());
                } else {
                    lambda_param_types.push(ty.clone());
                    param_parts.push(format!(
                        "{}: {}",
                        name,
                        self.rust_type_for_closure_param(ty)
                    ));
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
        self.lambda_return_types.push(ret_type.cloned());
        let saved_nonlocals = self.nonlocal_decls.clone();
        let saved_cells = self.cell_locals.clone();
        if let ExprKind::Block { stmts } = &body.kind {
            let nonlocal_info = self.collect_nonlocal_info_for_stmts(stmts, params);
            self.nonlocal_decls = Some(nonlocal_info.nonlocal_decls);
            self.cell_locals = Some(nonlocal_info.cell_locals);
        } else {
            self.nonlocal_decls = Some(Default::default());
            self.cell_locals = Some(Default::default());
        }
        let body_expr = self.gen_expr(body);
        self.lambda_depth -= 1;
        self.lambda_return_types.pop();
        let body_expr = body_expr?;
        self.nonlocal_decls = saved_nonlocals;
        self.cell_locals = saved_cells;
        self.local_vars = saved_locals;
        Ok(format!(
            "move |{}| {{ {} }}",
            param_parts.join(", "),
            body_expr
        ))
    }
}
