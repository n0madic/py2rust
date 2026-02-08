use super::*;
use crate::stdlib::registry::{
    find_stdlib_attribute, find_stdlib_runtime_attribute, is_stdlib_runtime_type, resolve_module,
};

impl<'a> TypeChecker<'a> {
    /// Map literal syntax to its direct static type.
    pub(super) fn literal_type(lit: &Literal) -> Type {
        match lit {
            Literal::Int(_) => Type::Int,
            Literal::Float(_) => Type::Float,
            Literal::Bool(_) => Type::Bool,
            Literal::Str(_) => Type::Str,
            Literal::Bytes(_) => Type::Bytes,
            Literal::None => Type::None,
        }
    }

    /// Type check a variable reference expression.
    pub(super) fn check_name_expr(
        &mut self,
        name: &mut String,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        // Track global/nonlocal usage for declaration-order validation.
        self.note_global_use(name, span);
        self.note_nonlocal_use(name, span);

        if let Some(mut ty) = self.lookup_var(name) {
            // If variable is Unknown, use the expected type as an inference hint.
            if matches!(ty, Type::Unknown) {
                if let Some(expected) = expected {
                    if !matches!(expected, Type::Unknown) {
                        self.set_var_type(name, expected.clone());
                        ty = expected.clone();
                    }
                }
            }
            return Ok(ty);
        }

        if let Some(sig) = self.ctx.functions.get(name) {
            // Function reference used as a value.
            return Ok(Type::Lambda {
                params: sig.params.clone(),
                ret: Box::new(sig.ret.clone()),
            });
        }

        // Built-in type constructors are handled as lambda values.
        let builtin_ctor = match name.as_str() {
            "str" => Some(Type::Lambda {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Str),
            }),
            "int" => Some(Type::Lambda {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Int),
            }),
            "float" => Some(Type::Lambda {
                params: vec![Type::Unknown],
                ret: Box::new(Type::Float),
            }),
            _ => None,
        };
        if let Some(ty) = builtin_ctor {
            return Ok(ty);
        }

        if self.ctx.classes.contains_key(name) {
            return Ok(Type::Custom(name.clone()));
        }

        Err(self.error(span, format!("NameError: name '{name}' is not defined")))
    }

    /// Type check `obj.attr` access.
    pub(super) fn check_attr_expr(
        &mut self,
        value: &mut Expr,
        attr: &str,
        span: Span,
    ) -> Result<Type, CompileError> {
        // Special case: `type(x).__name__` is always a string.
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
        if let Type::Module(module_name) = &value_ty {
            let module_id = resolve_module(module_name.as_str()).ok_or_else(|| {
                self.error(
                    span,
                    format!("module '{module_name}' is not registered in stdlib registry"),
                )
            })?;
            let attr_spec = find_stdlib_attribute(module_id, attr).ok_or_else(|| {
                self.error(
                    span,
                    format!("{module_name} has no supported member '{attr}'"),
                )
            })?;
            return Ok((attr_spec.type_resolver)());
        }
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
                if let Some(attr_spec) = find_stdlib_runtime_attribute(class_name.as_str(), attr) {
                    return Ok((attr_spec.type_resolver)());
                }
                if is_stdlib_runtime_type(class_name.as_str()) {
                    return Err(self.error(
                        span,
                        format!("{class_name} has no supported member '{attr}'"),
                    ));
                }
                let class_info = self
                    .ctx
                    .classes
                    .get(&class_name)
                    .ok_or_else(|| self.error(span, format!("Unknown class: {class_name}")))?;
                if let Some(prop) = class_info.properties.get(attr) {
                    return Ok(prop.ty.clone());
                }
                class_info
                    .fields
                    .get(attr)
                    .cloned()
                    .ok_or_else(|| self.error(span, format!("Unknown field {class_name}.{attr}")))
            }
            _ => Err(self.error(span, "Attribute access only allowed on class instances")),
        }
    }

    /// Validate starred call arguments.
    pub(super) fn check_starred_expr(
        &mut self,
        value: &mut Expr,
        span: Span,
    ) -> Result<Type, CompileError> {
        let value_ty = self.check_expr(value, None)?;
        let _ = self.iter_item_type(&value_ty, span)?;
        Err(self.error(
            span,
            "Starred argument is only valid directly inside a call expression",
        ))
    }
}
