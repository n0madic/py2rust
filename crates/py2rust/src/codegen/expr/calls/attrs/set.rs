// Set attribute call lowering.

use super::super::super::*;

impl<'a> Codegen<'a> {
    /// Lower set method calls.
    pub(super) fn gen_set_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        _keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if attr == "add" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.insert({})", target, self.gen_args(args)?));
            }
        }
        if attr == "remove" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.remove(&{})", target, self.gen_args(args)?));
            }
        }
        if attr == "discard" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!(
                    "{{ {}.remove(&{}); }}",
                    target,
                    self.gen_args(args)?
                ));
            }
        }
        if attr == "clear" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.clear() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; {guard}.clear(); }}",
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; {tmp}.clear(); }}",
                        tmp = tmp,
                        target = target_expr
                    ));
                }
                return Ok(format!("{}.clear()", target_expr));
            }
        }
        if attr == "copy" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.copy() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return Ok(format!("{}.clone()", self.global_lock_expr(name)));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!("{}.clone()", target_expr));
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported set method `{attr}`"),
        ))
    }
}
