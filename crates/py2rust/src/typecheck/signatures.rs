use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn collect_signatures(&mut self, program: &Program) -> Result<(), CompileError> {
        for item in &program.items {
            if let Item::Function(func) = item {
                let params = self.resolve_params(&func.params)?;
                let ret = self.resolve_type_ref(&func.ret, func.span)?;
                self.ctx.functions.insert(
                    func.name.clone(),
                    FunctionSig {
                        params,
                        ret,
                        span: func.span,
                    },
                );
            }
        }

        for item in &program.items {
            if let Item::Class(class_def) = item {
                let mut fields = IndexMap::new();
                for field in &class_def.fields {
                    let ty = self.resolve_type_ref(&field.ty, field.span)?;
                    if matches!(ty, Type::Iterator(_)) {
                        return Err(
                            self.error(field.span, "Iterator[T] is only allowed as a return type")
                        );
                    }
                    fields.insert(field.name.clone(), ty);
                }
                let mut methods = HashMap::new();
                let mut init = None;
                let mut iter_return = None;
                let mut iter_item = None;
                let mut next_item = None;

                for method in &class_def.methods {
                    let params = self.resolve_params(&method.params)?;
                    let ret = self.resolve_type_ref(&method.ret, method.span)?;
                    let sig = FunctionSig {
                        params,
                        ret: ret.clone(),
                        span: method.span,
                    };
                    if method.name == "__init__" {
                        init = Some(sig.clone());
                    }
                    if method.name == "__iter__" {
                        if let Type::Iterator(item_ty) = ret.clone() {
                            iter_item = Some(*item_ty);
                        }
                        if let Type::Custom(name) = ret.clone() {
                            iter_return = Some(name);
                        }
                    }
                    if method.name == "next" {
                        if let Type::Option(item_ty) = ret.clone() {
                            next_item = Some(*item_ty);
                        }
                    }
                    methods.insert(method.name.clone(), sig);
                }

                if let Some(class_info) = self.ctx.classes.get_mut(&class_def.name) {
                    class_info.fields = fields;
                    class_info.methods = methods;
                    class_info.init = init;
                    class_info.iter_return = iter_return;
                    class_info.iter_item = iter_item;
                    class_info.next_item = next_item;
                }
            }
        }

        for (name, union) in &self.ctx.unions {
            for variant in &union.variants {
                if !self.ctx.classes.contains_key(variant) {
                    return Err(self.error(
                        Span::new(0, 0),
                        format!("Union {name} refers to unknown class {variant}"),
                    ));
                }
            }
        }

        Ok(())
    }
}
