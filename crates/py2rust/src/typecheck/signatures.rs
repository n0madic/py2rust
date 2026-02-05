use super::*;
use std::collections::{HashMap, HashSet};

/// Signature collection pass.
///
/// Before type checking function bodies, we need to know the signatures
/// of all functions and classes. This allows:
/// 1. Forward references (function A calls function B defined later)
/// 2. Recursive functions (function calls itself)
/// 3. Method resolution (knowing what methods a class has)
///
/// We collect:
/// - Function signatures (params, return type)
/// - Class field types and method signatures
/// - Iterator protocol information (__iter__, next)
/// - Union variant validation
///
/// This is a multi-phase process because we need class info before
/// we can properly type-check method bodies.
impl<'a> TypeChecker<'a> {
    /// Collect all function and class signatures from the program.
    ///
    /// This is the first pass - we gather type information without
    /// checking function bodies.
    pub(super) fn collect_signatures(&mut self, program: &Program) -> Result<(), CompileError> {
        // Phase 1: Collect function signatures
        for item in &program.items {
            if let Item::Function(func) = item {
                let params = self.resolve_params(&func.params)?;
                let ret = self.resolve_type_ref(&func.ret, func.span)?;
                let defaults = func.params.iter().filter(|p| p.default.is_some()).count();
                self.ctx.functions.insert(
                    func.name.clone(),
                    FunctionSig {
                        param_names: func.params.iter().map(|p| p.name.clone()).collect(),
                        params,
                        ret,
                        span: func.span,
                        can_throw: false,
                        thrown_exceptions: Vec::new(),
                        defaults,
                    },
                );
            }
        }

        // Phase 2: Collect class field types and method signatures
        let mut class_defs: HashMap<String, &ClassDef> = HashMap::new();
        for item in &program.items {
            if let Item::Class(class_def) = item {
                class_defs.insert(class_def.name.clone(), class_def);
            }
        }
        for item in &program.items {
            if let Item::Class(class_def) = item {
                // Resolve field types
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
                // Resolve class attribute types (annotated or Unknown for now).
                let mut class_attrs = IndexMap::new();
                for attr in &class_def.class_attrs {
                    let ty = if let Some(ann) = &attr.ann {
                        let ty = self.resolve_type_ref(ann, attr.span)?;
                        if matches!(ty, Type::Iterator(_)) {
                            return Err(self
                                .error(attr.span, "Iterator[T] is only allowed as a return type"));
                        }
                        ty
                    } else {
                        Type::Unknown
                    };
                    let global_name = format!("__class_attr_{}_{}", class_def.name, attr.name);
                    class_attrs.insert(
                        attr.name.clone(),
                        ClassAttrInfo {
                            ty: ty.clone(),
                            global_name: global_name.clone(),
                        },
                    );
                    self.ctx.globals.insert(global_name, ty);
                }
                let mut methods = HashMap::new();
                let method_kinds = class_def.method_kinds.clone();
                let mut properties: HashMap<String, PropertyInfo> = HashMap::new();
                for prop in &class_def.properties {
                    let entry = properties.entry(prop.name.clone()).or_insert(PropertyInfo {
                        getter: String::new(),
                        setter: None,
                        ty: Type::Unknown,
                    });
                    if !prop.getter.is_empty() {
                        entry.getter = prop.getter.clone();
                    }
                    if prop.setter.is_some() {
                        entry.setter = prop.setter.clone();
                    }
                }
                let mut init = None;
                let mut iter_return = None;
                let mut iter_item = None;
                let mut next_item = None;

                // Collect method signatures and detect iterator protocol
                for method in &class_def.methods {
                    let params = self.resolve_params(&method.params)?;
                    let ret = self.resolve_type_ref(&method.ret, method.span)?;
                    let defaults = method.params.iter().filter(|p| p.default.is_some()).count();
                    let sig = FunctionSig {
                        param_names: method.params.iter().map(|p| p.name.clone()).collect(),
                        params,
                        ret: ret.clone(),
                        span: method.span,
                        can_throw: false,
                        thrown_exceptions: Vec::new(),
                        defaults,
                    };
                    // Track __init__ method (constructor)
                    if method.name == "__init__" {
                        init = Some(sig.clone());
                    }
                    // Track __iter__ method (for iteration protocol)
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

                // Infer property types from getter/setter signatures.
                for info in properties.values_mut() {
                    if !info.getter.is_empty() {
                        if let Some(sig) = methods.get(&info.getter) {
                            info.ty = sig.ret.clone();
                            continue;
                        }
                    }
                    if let Some(setter) = info.setter.as_ref() {
                        if let Some(sig) = methods.get(setter) {
                            if sig.params.len() >= 2 {
                                info.ty = sig.params[1].clone();
                            }
                        }
                    }
                }

                if let Some(class_info) = self.ctx.classes.get_mut(&class_def.name) {
                    class_info.base = class_def.base.clone();
                    class_info.fields = fields;
                    class_info.class_attrs = class_attrs;
                    class_info.methods = methods;
                    class_info.method_kinds = method_kinds;
                    class_info.properties = properties;
                    class_info.init = init;
                    class_info.iter_return = iter_return;
                    class_info.iter_item = iter_item;
                    class_info.next_item = next_item;
                }
            }
        }

        // Phase 2b: Merge inheritance (fields, methods, properties, class attrs).
        let mut merged: HashSet<String> = HashSet::new();
        let class_names: Vec<String> = self.ctx.classes.keys().cloned().collect();
        for name in class_names {
            self.merge_class_inheritance(&name, &class_defs, &mut merged)?;
        }

        // Phase 3: Validate union variant references
        // All union variants must be defined classes
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

    fn merge_class_inheritance(
        &mut self,
        name: &str,
        class_defs: &HashMap<String, &ClassDef>,
        merged: &mut HashSet<String>,
    ) -> Result<(), CompileError> {
        if merged.contains(name) {
            return Ok(());
        }
        let class_def = class_defs
            .get(name)
            .ok_or_else(|| self.error(Span::new(0, 0), format!("Unknown class: {name}")))?;
        if let Some(base) = &class_def.base {
            if !self.ctx.classes.contains_key(base) {
                return Err(self.error(class_def.span, format!("Unknown base class: {base}")));
            }
            self.merge_class_inheritance(base, class_defs, merged)?;
            let base_info = self
                .ctx
                .classes
                .get(base)
                .cloned()
                .ok_or_else(|| self.error(class_def.span, "Unknown base class"))?;
            let info = if let Some(info) = self.ctx.classes.get_mut(name) {
                info
            } else {
                return Err(self.error(class_def.span, "Unknown class"));
            };

            // Merge fields (base first, derived overrides).
            let mut fields = base_info.fields.clone();
            for (k, v) in info.fields.clone() {
                fields.insert(k, v);
            }
            info.fields = fields;

            // Merge class attributes (base first, derived overrides).
            let mut class_attrs = base_info.class_attrs.clone();
            for (k, v) in info.class_attrs.clone() {
                class_attrs.insert(k, v);
            }
            info.class_attrs = class_attrs;

            // Merge methods (base first, derived overrides).
            let mut methods = base_info.methods.clone();
            for (k, v) in info.methods.clone() {
                methods.insert(k, v);
            }
            info.methods = methods;

            // Merge method kinds.
            let mut method_kinds = base_info.method_kinds.clone();
            for (k, v) in info.method_kinds.clone() {
                method_kinds.insert(k, v);
            }
            info.method_kinds = method_kinds;

            // Merge properties.
            let mut properties = base_info.properties.clone();
            for (k, v) in info.properties.clone() {
                properties.insert(k, v);
            }
            info.properties = properties;

            // Inherit __init__ if missing.
            if info.init.is_none() {
                info.init = base_info.init.clone();
            }
        }
        merged.insert(name.to_string());
        Ok(())
    }
}
