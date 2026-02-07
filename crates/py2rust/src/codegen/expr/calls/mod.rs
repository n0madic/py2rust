// Function and method call expression lowering.

mod args;
mod attrs;
mod builtins;
mod format;
mod stdlib;

use super::super::*;
use crate::stdlib::registry::{find_stdlib_method, resolve_module};

impl<'a> Codegen<'a> {
    /// Lower a call expression, including builtins and method calls.
    pub(super) fn gen_call_expr(
        &mut self,
        expr: &Expr,
        func: &Expr,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if let Some(Type::StdlibFunction { module, method }) = func.ty.as_ref() {
            let module_id = resolve_module(module.as_str()).ok_or_else(|| {
                self.error(
                    expr.span,
                    format!("module '{module}' is not registered in stdlib registry"),
                )
            })?;
            let spec = find_stdlib_method(module_id, method.as_str()).ok_or_else(|| {
                self.error(
                    expr.span,
                    format!("{module} has no supported member '{method}'"),
                )
            })?;
            return self.gen_stdlib_call(expr.span, spec, args, keywords);
        }
        if let ExprKind::Name(name) = &func.kind {
            if let Some(result) = self.gen_builtin_call(expr, name, args, keywords)? {
                return Ok(result);
            }
        }
        if let ExprKind::Attr { value, attr } = &func.kind {
            return self.gen_attr_call(value, attr, args, keywords);
        }
        // Check if this is a user-defined function.
        if let ExprKind::Name(name) = &func.kind {
            if let Some(sig) = self.ctx.functions.get(name) {
                let has_unpacking = args
                    .iter()
                    .any(|arg| matches!(arg.kind, ExprKind::Starred { .. }))
                    || keywords.iter().any(|kw| kw.name.is_none());
                if has_unpacking {
                    if let Some(def) = self.function_defs.get(name).cloned() {
                        return self
                            .gen_user_call_with_unpacking(expr, name, sig, &def, args, keywords);
                    }
                    return Err(self.error(
                        expr.span,
                        "Call-site unpacking requires a known function definition",
                    ));
                }
                let param_types: Vec<Type> = sig
                    .params
                    .iter()
                    .map(|t| self.to_borrowed_param_type(t))
                    .collect();
                let full_args = if let Some(def) = self.function_defs.get(name) {
                    self.resolve_call_args(
                        args,
                        keywords,
                        &def.params,
                        &param_types,
                        (None, name),
                        false,
                    )?
                } else {
                    if !keywords.is_empty() {
                        return Err(self.error(
                            expr.span,
                            "Keyword arguments require a known function signature",
                        ));
                    }
                    args.to_vec()
                };
                let call = format!(
                    "{}({})",
                    name,
                    self.gen_call_args_for_sig(&param_types, &full_args)?
                );
                // Add ? operator if function can throw.
                if sig.can_throw {
                    return Ok(format!("({}?)", call));
                }
                return Ok(call);
            }
        }
        if !keywords.is_empty() {
            return Err(self.error(
                expr.span,
                "Keyword arguments are not supported for this call target",
            ));
        }
        if let Some(Type::Lambda { params, .. }) = func.ty.as_ref() {
            if !params.is_empty() && params.len() != args.len() {
                return Err(self.error(expr.span, "Argument count mismatch"));
            }
            let mut rendered_args = Vec::new();
            for (idx, arg) in args.iter().enumerate() {
                let expected = params.get(idx);
                let mut rendered = if let Some(param_ty) = expected {
                    self.gen_expr_with_expected(arg, Some(param_ty))?
                } else {
                    self.gen_expr(arg)?
                };
                if let Some(param_ty) = expected {
                    if matches!(
                        param_ty,
                        Type::List(_) | Type::Dict(_, _) | Type::Str | Type::Bytes
                    ) {
                        rendered = format!("{}.clone()", rendered);
                    } else if self.needs_borrow(arg.ty.as_ref(), param_ty) {
                        rendered = format!("&{}", rendered);
                    } else if matches!(param_ty, Type::Lambda { .. }) {
                        // Higher-order callable values are passed as boxed trait objects.
                        rendered = format!("Box::new({})", rendered);
                    }
                }
                rendered_args.push(rendered);
            }
            return Ok(format!(
                "{}({})",
                self.gen_expr(func)?,
                rendered_args.join(", ")
            ));
        }
        Ok(format!(
            "{}({})",
            self.gen_expr(func)?,
            self.gen_args(args)?
        ))
    }
}
