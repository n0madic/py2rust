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
        if attr == "extend" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if args.len() != 1 {
                    return Err(self.error(value.span, "set.extend() expects one argument"));
                }
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                let iter_src = self.gen_iter_source(&args[0])?;
                let items_tmp = self.new_tmp();
                let body = format!(
                    "{{ let {items} = ({iter}).collect::<Vec<_>>(); {target}.extend({items}); }}",
                    items = items_tmp,
                    iter = iter_src.expr,
                    target = target
                );
                return Ok(iter_src.wrap(body));
            }
        }
        if attr == "pop" {
            if let Some(Type::Set(inner)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.pop() expects no arguments"));
                }
                let item_is_copy = self.is_copy_type(inner);
                let pop_result_expr = |target: &str| {
                    let first_expr = if item_is_copy {
                        format!("{target}.iter().next().copied()", target = target)
                    } else {
                        format!("{target}.iter().next().cloned()", target = target)
                    };
                    format!(
                        "match {first} {{ Some(item) => Ok({target}.take(&item).expect(\"set member missing during pop\")), None => Err(PyError::KeyError(\"KeyError\".into())) }}",
                        first = first_expr,
                        target = target
                    )
                };
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        let pop_expr = pop_result_expr(&guard);
                        return Ok(self.wrap_result(format!(
                            "{{ let mut {guard} = {lock}; {pop_expr} }}",
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            pop_expr = pop_expr
                        )));
                    }
                    let pop_expr = pop_result_expr(name);
                    return Ok(self.wrap_result(pop_expr));
                }
                let target_expr = self.gen_expr(value)?;
                let tmp = self.new_tmp();
                let pop_expr = pop_result_expr(&tmp);
                return Ok(self.wrap_result(format!(
                    "{{ let mut {tmp} = {target}; {pop_expr} }}",
                    tmp = tmp,
                    target = target_expr,
                    pop_expr = pop_expr
                )));
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported set method `{attr}`"),
        ))
    }
}
