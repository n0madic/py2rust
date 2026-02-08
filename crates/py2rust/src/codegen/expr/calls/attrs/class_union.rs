// Class and union attribute call lowering.

use super::super::super::*;

impl<'a> Codegen<'a> {
    /// Lower class/instance method calls.
    pub(super) fn gen_class_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<Option<String>, CompileError> {
        if let Some((class_name, is_class_value)) = match &value.kind {
            ExprKind::Name(name) if self.ctx.classes.contains_key(name) => {
                Some((name.as_str(), true))
            }
            _ => value.ty.as_ref().and_then(|ty| match ty {
                Type::Custom(name) => Some((name.as_str(), false)),
                _ => None,
            }),
        } {
            if let Some(class_info) = self.ctx.classes.get(class_name) {
                if let Some(sig) = class_info.methods.get(attr) {
                    let kind = class_info
                        .method_kinds
                        .get(attr)
                        .copied()
                        .unwrap_or(MethodKind::Instance);
                    let method_def =
                        self.method_def(class_name, attr).cloned().ok_or_else(|| {
                            self.error(value.span, format!("Unknown method {class_name}.{attr}"))
                        })?;
                    let mut call = match kind {
                        MethodKind::Instance => {
                            if is_class_value {
                                return Err(
                                    self.error(value.span, "Instance methods require an instance")
                                );
                            }
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .skip(1)
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params[1..],
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            if self.method_is_mutating(&method_def) {
                                if let ExprKind::Name(name) = &value.kind {
                                    if self.is_global(name) {
                                        let guard = self.new_tmp();
                                        return Ok(Some(format!(
                                            "{{ let mut {guard} = {lock}; {guard}.{attr}({args}) }}",
                                            guard = guard,
                                            lock = self.global_lock_expr(name),
                                            attr = attr,
                                            args = call_args
                                        )));
                                    }
                                }
                            }
                            format!("{}.{}({})", self.gen_expr(value)?, attr, call_args)
                        }
                        MethodKind::Static => {
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params,
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            format!("{}::{}({})", class_name, attr, call_args)
                        }
                        MethodKind::Class => {
                            let def_params = if method_def.params.is_empty() {
                                &method_def.params[..]
                            } else {
                                &method_def.params[1..]
                            };
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .skip(1)
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                def_params,
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            format!("{}::{}({})", class_name, attr, call_args)
                        }
                    };
                    if sig.can_throw {
                        call = format!("({}?)", call);
                    }
                    return Ok(Some(call));
                }
            }
        }
        Ok(None)
    }

    /// Lower union method calls by generating a match dispatch.
    pub(super) fn gen_union_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<Option<String>, CompileError> {
        // Handle method calls on Union types by generating match expression.
        if let Some(Type::Union(union_name)) = value.ty.as_ref() {
            if let Some(union_info) = self.ctx.unions.get(union_name) {
                if !keywords.is_empty() {
                    return Err(self.error(
                        value.span,
                        "Keyword arguments are not supported for union method calls",
                    ));
                }
                // Get method signature from first variant to check if it can throw.
                let can_throw = union_info.variants.first().and_then(|v| {
                    self.ctx
                        .classes
                        .get(v)
                        .and_then(|c| c.methods.get(attr))
                        .map(|sig| sig.can_throw)
                });
                let value_expr = self.gen_expr(value)?;
                let args_str = self.gen_args(args)?;
                let mut arms = Vec::new();
                for variant in &union_info.variants {
                    arms.push(format!(
                        "{}::{}(ref _x) => _x.{}({})",
                        union_name, variant, attr, args_str
                    ));
                }
                let mut call = format!("match {} {{ {} }}", value_expr, arms.join(", "));
                if can_throw == Some(true) {
                    call = format!("({}?)", call);
                }
                return Ok(Some(call));
            }
        }
        Ok(None)
    }
}
