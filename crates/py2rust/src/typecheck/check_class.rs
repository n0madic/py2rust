use super::*;

/// Class definition type checking.
///
/// We validate:
/// 1. All methods are well-typed
/// 2. __init__ returns None (not allowed to return values)
/// 3. Iterator protocol is correctly implemented (__iter__ and next)
///
/// Iterator protocol requirements:
/// - If __iter__ returns Iterator[T], that's a generator-style iterator
/// - If __iter__ returns self (or another class), that class must have next() -> Optional[T]
/// - next() must return Optional[T] where T is the item type
impl<'a> TypeChecker<'a> {
    /// Type check a class definition.
    ///
    /// Checks methods and validates iterator protocol if present.
    pub(super) fn check_class(&mut self, class: &mut ClassDef) -> Result<(), CompileError> {
        for method in &mut class.methods {
            let kind = class
                .method_kinds
                .get(&method.name)
                .copied()
                .unwrap_or(MethodKind::Instance);
            let require_self = matches!(kind, MethodKind::Instance);
            self.check_function(method, Some(class.name.as_str()), require_self)?;
        }

        // Type check class attribute initializers.
        if !class.class_attrs.is_empty() {
            for attr in &mut class.class_attrs {
                let expected = if let Some(ann) = &attr.ann {
                    Some(self.resolve_type_ref(ann, attr.span)?)
                } else {
                    None
                };
                let ty = self.check_expr(&mut attr.value, expected.as_ref())?;
                if let Some(expected) = expected {
                    if !matches!(ty, Type::Unknown) && !matches!(expected, Type::Unknown) {
                        self.ensure_assignable(&ty, &expected, attr.span)?;
                    }
                }
                if let Some(info) = self.ctx.classes.get_mut(&class.name) {
                    if let Some(existing) = info.class_attrs.get_mut(&attr.name) {
                        if matches!(existing.ty, Type::Unknown) && !matches!(ty, Type::Unknown) {
                            existing.ty = ty.clone();
                            if let Some(global_ty) = self.ctx.globals.get_mut(&existing.global_name)
                            {
                                *global_ty = ty.clone();
                            }
                        }
                    }
                }
            }
        }

        // Validate class-specific constraints
        if let Some(info) = self.ctx.classes.get(&class.name) {
            // __init__ must return None (Python doesn't allow return values)
            if let Some(init_sig) = &info.init {
                if !matches!(init_sig.ret, Type::None) {
                    return Err(self.error(class.span, "__init__ must return None"));
                }
            }
            // If next() exists, it must return Optional[T]
            if info.methods.contains_key("next") && info.next_item.is_none() {
                return Err(self.error(class.span, "next() must return Optional[T]"));
            }
            // If __iter__ returns a class name, that class must implement next()
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
