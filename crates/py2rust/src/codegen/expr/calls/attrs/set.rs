// Set attribute call lowering.

use super::super::super::*;
use super::AttrValueTarget;
use crate::container::registry::{resolve_container_method, ContainerId};

impl<'a> Codegen<'a> {
    /// Lower set method calls.
    pub(super) fn gen_set_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        _keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        let Some(Type::Set(inner)) = value.ty.as_ref() else {
            return Err(self.error(
                value.span,
                "Internal error: set handler used for non-set receiver",
            ));
        };
        let spec = resolve_container_method(ContainerId::Set, attr).ok_or_else(|| {
            self.error(
                value.span,
                format!("Internal error: unsupported set method `{attr}`"),
            )
        })?;
        if !spec.accepts_arity(args.len()) {
            return Err(self.error(value.span, spec.arity_error()));
        }
        self.uses.hash_set = true;

        match attr {
            "add" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                Ok(format!("{}.insert({})", target, self.gen_args(args)?))
            }
            "remove" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                Ok(format!("{}.remove(&{})", target, self.gen_args(args)?))
            }
            "discard" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                Ok(format!(
                    "{{ {}.remove(&{}); }}",
                    target,
                    self.gen_args(args)?
                ))
            }
            "clear" => match self.resolve_attr_value_target(value)? {
                AttrValueTarget::GlobalName(name) => {
                    let guard = self.new_tmp();
                    Ok(format!(
                        "{{ let mut {guard} = {lock}; {guard}.clear(); }}",
                        guard = guard,
                        lock = self.global_lock_expr(&name)
                    ))
                }
                AttrValueTarget::Name(name) => Ok(format!("{name}.clear()")),
                AttrValueTarget::Expr(target) => {
                    let tmp = self.new_tmp();
                    Ok(format!(
                        "{{ let mut {tmp} = {target}; {tmp}.clear(); }}",
                        tmp = tmp,
                        target = target
                    ))
                }
            },
            "copy" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                Ok(format!("{target}.clone()"))
            }
            "extend" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                let iter_src = self.gen_iter_source(&args[0])?;
                let items_tmp = self.new_tmp();
                let body = format!(
                    "{{ let {items} = ({iter}).collect::<Vec<_>>(); {target}.extend({items}); }}",
                    items = items_tmp,
                    iter = iter_src.expr,
                    target = target
                );
                Ok(iter_src.wrap(body))
            }
            "pop" => {
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
                match self.resolve_attr_value_target(value)? {
                    AttrValueTarget::GlobalName(name) => {
                        let guard = self.new_tmp();
                        let pop_expr = pop_result_expr(&guard);
                        Ok(self.wrap_result(format!(
                            "{{ let mut {guard} = {lock}; {pop_expr} }}",
                            guard = guard,
                            lock = self.global_lock_expr(&name),
                            pop_expr = pop_expr
                        )))
                    }
                    AttrValueTarget::Name(name) => {
                        let pop_expr = pop_result_expr(&name);
                        Ok(self.wrap_result(pop_expr))
                    }
                    AttrValueTarget::Expr(target) => {
                        let tmp = self.new_tmp();
                        let pop_expr = pop_result_expr(&tmp);
                        Ok(self.wrap_result(format!(
                            "{{ let mut {tmp} = {target}; {pop_expr} }}",
                            tmp = tmp,
                            target = target,
                            pop_expr = pop_expr
                        )))
                    }
                }
            }
            _ => Err(self.error(
                value.span,
                format!("Internal error: unsupported set method `{attr}`"),
            )),
        }
    }
}
