// Set attribute call lowering.

use super::super::super::*;
use crate::container::registry::{find_container_method, ContainerId};

impl<'a> Codegen<'a> {
    /// Return true when the concrete Rust storage for this set uses `PyRepr`.
    ///
    /// Prioritizes local/global variable types over the receiver expression type,
    /// because the Let statement may have been back-propagated to a more refined
    /// type (e.g., `Set(Str)`) while the receiver expression still says `Set(Unknown)`.
    pub(crate) fn set_uses_pyrepr_storage(&self, value: &Expr, inner: &Type) -> bool {
        // Check local/global variable types first — they may be refined beyond expr type.
        if let ExprKind::Name(name) = &value.kind {
            if let Some(Type::Set(local_inner)) = self.local_var_type(name) {
                return matches!(local_inner.as_ref(), Type::Unknown);
            }
            if let Some(Type::Set(global_inner)) = self.ctx.globals.get(name) {
                return matches!(global_inner.as_ref(), Type::Unknown);
            }
        }
        // Fall back to the receiver expression's element type.
        matches!(inner, Type::Unknown)
    }

    /// Render one set item for method calls, preserving the receiver element type.
    ///
    /// CPython-compat divergence:
    /// When a set still has `Unknown` element type at codegen time (for example,
    /// `s = set(); s.add("x")` before full local refinement is materialized),
    /// we store items as `PyRepr` to keep Rust types concrete and consistent.
    fn gen_set_item_expr(
        &mut self,
        receiver: &Expr,
        item: &Expr,
        inner: &Type,
    ) -> Result<String, CompileError> {
        if self.set_uses_pyrepr_storage(receiver, inner) {
            self.uses.py_repr = true;
            let raw = self.gen_expr(item)?;
            return Ok(format!("PyRepr(format!(\"{{:?}}\", {}))", raw));
        }
        let expr = self.gen_expr_with_expected(item, Some(inner))?;
        // HashSet::insert takes ownership; clone non-Copy items so the caller
        // can still use the variable after `.add(v)` (matches Python semantics).
        if !self.is_copy_type(inner) {
            Ok(format!("{}.clone()", expr))
        } else {
            Ok(expr)
        }
    }

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
        let _ = find_container_method(ContainerId::Set, attr).ok_or_else(|| {
            self.error(
                value.span,
                format!("Internal error: unsupported set method `{attr}`"),
            )
        })?;
        self.uses.hash_set = true;

        match attr {
            "add" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                let item_expr = self.gen_set_item_expr(value, &args[0], inner.as_ref())?;
                Ok(format!("{}.insert({})", target, item_expr))
            }
            "remove" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                let item_expr = self.gen_set_item_expr(value, &args[0], inner.as_ref())?;
                Ok(format!("{}.remove(&{})", target, item_expr))
            }
            "discard" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                let item_expr = self.gen_set_item_expr(value, &args[0], inner.as_ref())?;
                Ok(format!("{{ {}.remove(&{}); }}", target, item_expr))
            }
            "clear" => self.with_attr_target_binding(value, true, |_tc, target| {
                format!("{target}.clear()", target = target)
            }),
            "copy" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                Ok(format!("{target}.clone()"))
            }
            "extend" => {
                let target = self.resolve_mut_attr_target_expr(value)?;
                let iter_src = self.gen_iter_source(&args[0])?;
                let items_tmp = self.new_tmp();
                let body = if self.set_uses_pyrepr_storage(value, inner.as_ref()) {
                    self.uses.py_repr = true;
                    format!(
                        "{{ let {items} = ({iter}).map(|item| PyRepr(format!(\"{{:?}}\", item))).collect::<Vec<_>>(); {target}.extend({items}); }}",
                        items = items_tmp,
                        iter = iter_src.expr,
                        target = target
                    )
                } else {
                    format!(
                        "{{ let {items} = ({iter}).collect::<Vec<_>>(); {target}.extend({items}); }}",
                        items = items_tmp,
                        iter = iter_src.expr,
                        target = target
                    )
                };
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
                self.with_attr_target_binding(value, true, |tc, target| {
                    tc.wrap_result(pop_result_expr(target))
                })
            }
            _ => Err(self.error(
                value.span,
                format!("Internal error: unsupported set method `{attr}`"),
            )),
        }
    }
}
