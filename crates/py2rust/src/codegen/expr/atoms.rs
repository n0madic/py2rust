// Small, self-contained expression forms (literals, names, attrs, ifs).

use super::super::*;
use crate::stdlib::registry::{
    find_stdlib_attribute, find_stdlib_runtime_attribute, is_stdlib_runtime_type, resolve_module,
};

impl<'a> Codegen<'a> {
    /// Lower a literal to its Rust expression form.
    pub(super) fn gen_literal_expr(
        &mut self,
        expr: &Expr,
        lit: &Literal,
    ) -> Result<String, CompileError> {
        match lit {
            // Always suffix numeric literals to avoid Rust type inference ambiguity.
            Literal::Int(v) => Ok(format!("{}i64", v)),
            Literal::Float(v) => Ok(format!("{}f64", v)),
            Literal::Bool(v) => Ok(format!("{}", v)),
            // Use literal .to_string() to avoid String::from on literals.
            Literal::Str(s) => Ok(format!("{s:?}.to_string()")),
            // Bytes map to Vec<i64> for Python-style byte access (0-255 as ints).
            Literal::Bytes(bytes) => {
                if bytes.is_empty() {
                    return Ok("Vec::<i64>::new()".to_string());
                }
                let parts: Vec<String> = bytes.iter().map(|b| format!("{}i64", b)).collect();
                Ok(format!("vec![{}]", parts.join(", ")))
            }
            // None maps to different Rust types depending on context.
            Literal::None => {
                if let Some(Type::Option(_)) = expr.ty.as_ref() {
                    Ok("None".to_string())
                } else {
                    Ok("()".to_string())
                }
            }
        }
    }

    /// Lower a variable reference, including globals and __name__ handling.
    pub(super) fn gen_name_expr(&mut self, name: &str) -> Result<String, CompileError> {
        if name == "__name__" {
            return Ok("__NAME__.to_string()".to_string());
        }
        if let Some(override_expr) = self.name_override(name) {
            if self.is_cell_local(name) || self.is_nonlocal_decl(name) {
                return Ok(format!("{}.lock().unwrap().clone()", override_expr));
            }
            return Ok(override_expr.to_string());
        }
        if self.is_cell_local(name) || self.is_nonlocal_decl(name) {
            return Ok(format!("{}.lock().unwrap().clone()", name));
        }
        if self.is_global(name) {
            // Global reads go through OnceLock + Mutex with context-rich expects.
            if let Some(override_expr) = self.global_override(name) {
                return Ok(override_expr.to_string());
            }
            if self.readonly_globals.contains(name) {
                // Write-once scalar globals are Copy — dereference directly.
                return Ok(format!("*{}", self.global_lock_expr(name)));
            }
            return Ok(format!("{}.clone()", self.global_lock_expr(name)));
        }
        Ok(name.to_string())
    }

    /// Lower attribute access, including type(x).__name__ special case.
    pub(super) fn gen_attr_expr(
        &mut self,
        value: &Expr,
        attr: &str,
    ) -> Result<String, CompileError> {
        if let ExprKind::Name(name) = &value.kind {
            if let Some(global_name) = self.class_attr_global(name, attr) {
                if let Some(override_expr) = self.global_override(global_name) {
                    return Ok(override_expr.to_string());
                }
                return Ok(format!("{}.clone()", self.global_lock_expr(global_name)));
            }
            if let Some(override_name) = self.name_override(name) {
                if self.ctx.classes.contains_key(override_name) {
                    if let Some(global_name) = self.class_attr_global(override_name, attr) {
                        if let Some(override_expr) = self.global_override(global_name) {
                            return Ok(override_expr.to_string());
                        }
                        return Ok(format!("{}.clone()", self.global_lock_expr(global_name)));
                    }
                }
            }
            if self.is_global(name)
                && matches!(self.ctx.globals.get(name), Some(Type::Option(_)))
                && matches!(value.ty.as_ref(), Some(Type::Custom(_)))
            {
                return Ok(format!(
                    "{}.as_ref().expect(\"optional global '{}' is None\").{}",
                    self.global_lock_expr(name),
                    name,
                    attr
                ));
            }
            if !self.is_global(name)
                && matches!(self.local_var_type(name), Some(Type::Option(_)))
                && matches!(value.ty.as_ref(), Some(Type::Custom(_)))
            {
                if let Some(Type::Option(inner)) = self.local_var_type(name) {
                    if let Type::Custom(class_name) = inner.as_ref() {
                        let getter = self.class_property(class_name, attr).and_then(|prop| {
                            if prop.getter.is_empty() {
                                None
                            } else {
                                Some(prop.getter.clone())
                            }
                        });
                        let getter_fallible = self
                            .deletable_property_backing_int_field(class_name, attr)
                            .is_some();
                        let base = if let Some(override_expr) = self.name_override(name) {
                            // Narrowing overrides already yield an unwrapped inner value.
                            override_expr.to_string()
                        } else {
                            format!(
                                "{}.as_ref().expect(\"optional value '{}' is None\")",
                                name, name
                            )
                        };
                        if let Some(getter) = getter {
                            let call = format!("{}.{}()", base, getter);
                            if getter_fallible {
                                return Ok(self.wrap_result(call));
                            }
                            return Ok(call);
                        }
                        return Ok(format!("{}.{}", base, attr));
                    }
                }
            }
            if self.local_vars.is_none()
                && !self.is_global(name)
                && matches!(self.ctx.globals.get(name), Some(Type::Option(_)))
                && matches!(value.ty.as_ref(), Some(Type::Custom(_)))
            {
                if let Some(Type::Option(inner)) = self.ctx.globals.get(name) {
                    if let Type::Custom(class_name) = inner.as_ref() {
                        let getter = self.class_property(class_name, attr).and_then(|prop| {
                            if prop.getter.is_empty() {
                                None
                            } else {
                                Some(prop.getter.clone())
                            }
                        });
                        let getter_fallible = self
                            .deletable_property_backing_int_field(class_name, attr)
                            .is_some();
                        let base = if let Some(override_expr) = self.name_override(name) {
                            // Narrowing overrides already yield an unwrapped inner value.
                            override_expr.to_string()
                        } else {
                            format!(
                                "{}.as_ref().expect(\"optional value '{}' is None\")",
                                name, name
                            )
                        };
                        if let Some(getter) = getter {
                            let call = format!("{}.{}()", base, getter);
                            if getter_fallible {
                                return Ok(self.wrap_result(call));
                            }
                            return Ok(call);
                        }
                        return Ok(format!("{}.{}", base, attr));
                    }
                }
            }
        }
        if let Some(Type::Module(module_name)) = value.ty.as_ref() {
            let module_id = resolve_module(module_name.as_str()).ok_or_else(|| {
                self.error(
                    value.span,
                    format!("module '{module_name}' is not registered in stdlib registry"),
                )
            })?;
            let attr_spec = find_stdlib_attribute(module_id, attr).ok_or_else(|| {
                self.error(
                    value.span,
                    format!("{module_name} has no supported member '{attr}'"),
                )
            })?;
            return (attr_spec.codegen_handler)(self, value.span);
        }
        if let Some(Type::Option(inner)) = value.ty.as_ref() {
            if let Type::Custom(class_name) = inner.as_ref() {
                let getter = self.class_property(class_name, attr).and_then(|prop| {
                    if prop.getter.is_empty() {
                        None
                    } else {
                        Some(prop.getter.clone())
                    }
                });
                let getter_fallible = self
                    .deletable_property_backing_int_field(class_name, attr)
                    .is_some();
                if let ExprKind::Name(name) = &value.kind {
                    let base = if let Some(override_expr) = self.name_override(name) {
                        // Narrowing overrides already yield an unwrapped inner value.
                        override_expr.to_string()
                    } else {
                        format!(
                            "{}.as_ref().expect(\"optional value '{}' is None\")",
                            name, name
                        )
                    };
                    if let Some(getter) = getter {
                        let call = format!("{}.{}()", base, getter);
                        if getter_fallible {
                            return Ok(self.wrap_result(call));
                        }
                        return Ok(call);
                    }
                    return Ok(format!("{}.{}", base, attr));
                }
                let tmp = self.new_tmp();
                let expr = self.gen_expr(value)?;
                if let Some(getter) = getter {
                    let call = format!(
                        "{{ let {tmp} = {expr}; {tmp}.as_ref().expect(\"optional value is None\").{getter}() }}",
                        tmp = tmp,
                        expr = expr,
                        getter = getter
                    );
                    if getter_fallible {
                        return Ok(self.wrap_result(call));
                    }
                    return Ok(call);
                }
                return Ok(format!(
                    "{{ let {tmp} = {expr}; {tmp}.as_ref().expect(\"optional value is None\").{attr} }}",
                    tmp = tmp,
                    expr = expr,
                    attr = attr
                ));
            }
        }
        if let Some(Type::Custom(class_name)) = value.ty.as_ref() {
            if find_stdlib_runtime_attribute(class_name.as_str(), attr).is_some() {
                return Ok(format!("{}.{}.clone()", self.gen_expr(value)?, attr));
            }
            if is_stdlib_runtime_type(class_name.as_str()) {
                return Err(self.error(
                    value.span,
                    format!("{class_name} has no supported member '{attr}'"),
                ));
            }
            let getter = self.class_property(class_name, attr).and_then(|prop| {
                if prop.getter.is_empty() {
                    None
                } else {
                    Some(prop.getter.clone())
                }
            });
            let getter_fallible = self
                .deletable_property_backing_int_field(class_name, attr)
                .is_some();
            if let Some(getter) = getter {
                let call = format!("{}.{}()", self.gen_expr(value)?, getter);
                if getter_fallible {
                    return Ok(self.wrap_result(call));
                }
                return Ok(call);
            }
        }
        if attr == "__name__" {
            if let ExprKind::Call {
                func,
                args,
                keywords,
            } = &value.kind
            {
                if let ExprKind::Name(name) = &func.kind {
                    if name == "type" && args.len() == 1 && keywords.is_empty() {
                        if let Some(ty) = args[0].ty.as_ref() {
                            if let Some(name) = self.python_type_name(ty) {
                                return Ok(format!("{:?}.to_string()", name));
                            }
                        }
                        self.uses.type_name = true;
                        return Ok(format!("py_type_name(&{})", self.gen_expr(&args[0])?));
                    }
                }
            }
        }
        // Check for shared mutable fields (Arc<Atomic*>) before calling gen_expr
        // to avoid borrow checker issues with the self.ctx reference.
        //
        // We check two sources for the receiver's class type:
        // 1. The expression's own `ty` annotation (set by typechecking).
        // 2. The local variable scope (fallback for cases where the expr node's
        //    type is Unknown but the local variable's type was resolved, e.g.
        //    for-loop variables over a typed list).
        let receiver_class: Option<String> = match value.ty.as_ref() {
            Some(Type::Custom(cn)) => Some(cn.clone()),
            _ => {
                if let ExprKind::Name(var_name) = &value.kind {
                    match self.local_var_type(var_name) {
                        Some(Type::Custom(cn)) => Some(cn.clone()),
                        _ => None,
                    }
                } else {
                    None
                }
            }
        };
        let shared_field_ty = receiver_class
            .as_deref()
            .and_then(|cn| self.shared_mutable_field_ty(cn, attr));
        let obj_str = self.gen_expr(value)?;
        let base = format!("{}.{}", obj_str, attr);
        if let Some(field_ty) = shared_field_ty {
            // Read the scalar value out of the atomic wrapper.
            // AtomicU64 stores f64 bits; AtomicI64/AtomicBool are used directly.
            return match field_ty {
                Type::Float => Ok(format!("f64::from_bits({}.load(Ordering::Relaxed))", base)),
                Type::Int | Type::Bool => Ok(format!("{}.load(Ordering::Relaxed)", base)),
                _ => Ok(base),
            };
        }
        Ok(base)
    }

    /// Lower a conditional (ternary) expression.
    pub(super) fn gen_if_expr(
        &mut self,
        test: &Expr,
        body: &Expr,
        orelse: &Expr,
    ) -> Result<String, CompileError> {
        // Optimize compile-time constant conditions (e.g., isinstance folded to true/false).
        // Emit only the live branch to avoid type errors in dead code.
        if let ExprKind::Literal(Literal::Bool(val)) = &test.kind {
            return if *val {
                self.gen_expr(body)
            } else {
                self.gen_expr(orelse)
            };
        }
        let test_str = self.gen_expr(test)?;
        if test_str == "true" {
            return self.gen_expr(body);
        }
        if test_str == "false" {
            return self.gen_expr(orelse);
        }
        Ok(format!(
            "if {} {{ {} }} else {{ {} }}",
            test_str,
            self.gen_expr(body)?,
            self.gen_expr(orelse)?
        ))
    }

    /// Lower a union constructor expression.
    pub(super) fn gen_union_ctor_expr(
        &mut self,
        union: &str,
        variant: &str,
        inner: &Expr,
    ) -> Result<String, CompileError> {
        Ok(format!("{}::{}({})", union, variant, self.gen_expr(inner)?))
    }
}
