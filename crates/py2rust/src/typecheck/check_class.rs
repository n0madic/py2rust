use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_class(&mut self, class: &mut ClassDef) -> Result<(), CompileError> {
        for method in &mut class.methods {
            self.check_function(method, Some(&class.name))?;
        }

        if let Some(info) = self.ctx.classes.get(&class.name) {
            if let Some(init_sig) = &info.init {
                if !matches!(init_sig.ret, Type::None) {
                    return Err(self.error(class.span, "__init__ must return None"));
                }
            }
            if info.methods.contains_key("next") && info.next_item.is_none() {
                return Err(self.error(class.span, "next() must return Optional[T]"));
            }
            if let Some(iter_return) = &info.iter_return {
                let iter_info = self.ctx.classes.get(iter_return).ok_or_else(|| {
                    self.error(class.span, format!("Unknown iterator class: {iter_return}"))
                })?;
                if iter_info.next_item.is_none() {
                    return Err(self.error(
                        class.span,
                        format!("Iterator class {iter_return} must define next() -> Optional[T]"),
                    ));
                }
            }
        }

        Ok(())
    }
}
