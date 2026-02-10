// Assignment and destructuring emission helpers.

use super::super::super::util::mut_kw_for_name;
use super::super::super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_pre_main_inits(&mut self, program: &Program) -> Result<(), CompileError> {
        let mut_counts = HashMap::new();
        for item in &program.items {
            match item {
                Item::Function(func) => {
                    for param in &func.params {
                        if let Some(default) = &param.default {
                            let name =
                                self.default_global_name(None, func.name.as_str(), &param.name);
                            let target = AssignTarget::Name(name);
                            self.emit_simple_assign(&target, default, &mut_counts, true)?;
                        }
                    }
                }
                Item::Class(class_def) => {
                    for method in &class_def.methods {
                        for param in &method.params {
                            if let Some(default) = &param.default {
                                let name = self.default_global_name(
                                    Some(class_def.name.as_str()),
                                    method.name.as_str(),
                                    &param.name,
                                );
                                let target = AssignTarget::Name(name);
                                self.emit_simple_assign(&target, default, &mut_counts, true)?;
                            }
                        }
                    }
                    for attr in &class_def.class_attrs {
                        let target = AssignTarget::Attr {
                            value: Expr {
                                kind: ExprKind::Name(class_def.name.clone()),
                                span: attr.span,
                                ty: Some(Type::Custom(class_def.name.clone())),
                            },
                            attr: attr.name.clone(),
                        };
                        self.emit_simple_assign(&target, &attr.value, &mut_counts, true)?;
                    }
                }
                Item::Stmt(_) | Item::Union(_) => {}
            }
        }
        Ok(())
    }
    /// Wrap global values that need special ownership semantics.
    pub(super) fn wrap_global_value(
        &mut self,
        expr: String,
        value: &Expr,
        expected: Option<&Type>,
    ) -> String {
        match expected {
            Some(Type::Lambda { .. }) => {
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return expr;
                    }
                }
                format!("Arc::new({})", expr)
            }
            Some(Type::Iterator(_)) => {
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return expr;
                    }
                }
                self.uses.py_iter = true;
                format!("py_iter({})", expr)
            }
            _ => expr,
        }
    }

    /// Coerce list/dict RHS into global sync storage when assigning globals.
    pub(crate) fn coerce_container_to_global_storage(
        &mut self,
        expr: String,
        value: &Expr,
        expected: Option<&Type>,
    ) -> String {
        match expected {
            Some(Type::List(_)) => match self.list_storage_for_expr(value) {
                ListStorage::SharedSync => expr,
                ListStorage::Local => self.wrap_list_storage_expr(&expr, ListStorage::SharedSync),
                ListStorage::SharedCell => {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    format!(
                        "{{ let {tmp} = {expr}; let {guard} = {tmp}.py_list_guard(); Arc::new(Mutex::new({guard}.clone())) }}",
                        tmp = tmp,
                        expr = expr,
                        guard = guard
                    )
                }
            },
            Some(Type::Dict(_, _)) => match self.dict_storage_for_expr(value) {
                DictStorage::SharedSync => expr,
                DictStorage::Local => self.wrap_dict_storage_expr(&expr, DictStorage::SharedSync),
                DictStorage::SharedCell => {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    format!(
                        "{{ let {tmp} = {expr}; let {guard} = {tmp}.py_dict_guard(); Arc::new(Mutex::new({guard}.clone())) }}",
                        tmp = tmp,
                        expr = expr,
                        guard = guard
                    )
                }
            },
            _ => expr,
        }
    }

    /// Emit a non-destructuring assignment target, optionally allowing new bindings.
    pub(crate) fn emit_simple_assign(
        &mut self,
        target: &AssignTarget,
        value: &Expr,
        mut_counts: &HashMap<String, usize>,
        allow_let: bool,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Name(name) => {
                if self.is_cell_local(name) || self.is_nonlocal_decl(name) {
                    let cell_binding = self
                        .name_override(name)
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| name.clone());
                    let expected = self.local_var_type(name).cloned();
                    if allow_let
                        && self.local_var_type(name).is_none()
                        && self.is_cell_local(name)
                        && !self.is_nonlocal_decl(name)
                    {
                        if let Some((expr, elem_ty)) = self.gen_empty_list_with_hint(name, value)? {
                            let expr = format!("Rc::new(RefCell::new({}))", expr);
                            let mut_kw = mut_kw_for_name(name, mut_counts);
                            self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                            self.set_local_var_type(name, Type::List(Box::new(elem_ty)));
                            return Ok(());
                        }
                        let expr =
                            if let Some(local_expr) = self.gen_list_assignment_expr(name, value)? {
                                local_expr
                            } else if let Some(local_expr) =
                                self.gen_dict_assignment_expr(name, value)?
                            {
                                local_expr
                            } else {
                                let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                                self.maybe_clone_list_expr(expr, value, expected.as_ref())
                            };
                        let expr = format!("Rc::new(RefCell::new({}))", expr);
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        if let Some(ty) = value.ty.clone() {
                            self.set_local_var_type(name, ty);
                        }
                        return Ok(());
                    }

                    // Assign through the RefCell, guarding against self-references.
                    let current = self.new_tmp();
                    self.push_line(&format!(
                        "let {} = {}.borrow().clone();",
                        current, cell_binding
                    ));
                    let expected_is_optional = matches!(expected.as_ref(), Some(Type::Option(_)));
                    // When substituting the pre-read value (`current`) for self-references,
                    // temporarily disable cell/nonlocal lookup for this name so the override
                    // is treated as a plain value, not as another RefCell handle.
                    let saved_cells = self.cell_locals.clone();
                    if let Some(cells) = self.cell_locals.as_mut() {
                        cells.remove(name);
                    }
                    let saved_nonlocals = self.nonlocal_decls.clone();
                    if let Some(nonlocals) = self.nonlocal_decls.as_mut() {
                        nonlocals.remove(name);
                    }
                    let expr_result = self.with_name_override(name, current, |this| {
                        if !expected_is_optional {
                            if let Some(local_expr) = this.gen_list_assignment_expr(name, value)? {
                                return Ok(local_expr);
                            }
                            if let Some(local_expr) = this.gen_dict_assignment_expr(name, value)? {
                                return Ok(local_expr);
                            }
                        }
                        let expr = this.gen_expr_with_expected(value, expected.as_ref())?;
                        Ok(this.maybe_clone_list_expr(expr, value, expected.as_ref()))
                    });
                    self.cell_locals = saved_cells;
                    self.nonlocal_decls = saved_nonlocals;
                    let expr = expr_result?;
                    self.push_line(&format!("*{}.borrow_mut() = {};", cell_binding, expr));
                    return Ok(());
                }
                // Global assignment uses OnceLock + Mutex for initialization and mutation.
                if self.is_global(name) {
                    let expected = self.ctx.globals.get(name).cloned();
                    if allow_let && !self.initialized_globals.contains(name) {
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        let expr = self.maybe_clone_list_expr(expr, value, expected.as_ref());
                        let expr =
                            self.coerce_container_to_global_storage(expr, value, expected.as_ref());
                        let expr = self.wrap_global_value(expr, value, expected.as_ref());
                        let tmp = self.new_tmp();
                        let gname = self.global_name(name);
                        self.push_line(&format!("let {} = {};", tmp, expr));
                        self.push_line(&format!(
                            "let _ = {}.get_or_init(|| Mutex::new({}));",
                            gname, tmp
                        ));
                        self.initialized_globals.insert(name.clone());
                        return Ok(());
                    }
                    // Lock the global once to avoid deadlocks when the RHS reads the same global.
                    let guard = self.new_tmp();
                    let current = self.new_tmp();
                    let expr = self.with_global_override(name, current.clone(), |this| {
                        let expr = this.gen_expr_with_expected(value, expected.as_ref())?;
                        let expr = this.maybe_clone_list_expr(expr, value, expected.as_ref());
                        let expr =
                            this.coerce_container_to_global_storage(expr, value, expected.as_ref());
                        Ok(this.wrap_global_value(expr, value, expected.as_ref()))
                    })?;
                    self.push_line("{");
                    self.indent += 1;
                    self.push_line(&format!(
                        "let mut {} = {};",
                        guard,
                        self.global_lock_expr(name)
                    ));
                    self.push_line(&format!("let {} = {}.clone();", current, guard));
                    self.push_line(&format!("*{} = {};", guard, expr));
                    self.indent -= 1;
                    self.push_line("}");
                    return Ok(());
                }

                if allow_let && self.local_var_type(name).is_none() {
                    if let Some((expr, elem_ty)) = self.gen_empty_list_with_hint(name, value)? {
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        self.set_local_var_type(name, Type::List(Box::new(elem_ty)));
                        self.set_list_storage_for_temp(name, self.list_storage_for_name(name));
                        return Ok(());
                    }
                    if let Some(expr) = self.gen_list_assignment_expr(name, value)? {
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        if let Some(ty) = value.ty.clone() {
                            self.set_local_var_type(name, ty);
                            self.set_list_storage_for_temp(name, ListStorage::Local);
                        }
                        return Ok(());
                    }
                    if let Some(expr) = self.gen_dict_assignment_expr(name, value)? {
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        if let Some(ty) = value.ty.clone() {
                            self.set_local_var_type(name, ty);
                            self.set_dict_storage_for_temp(name, DictStorage::Local);
                        }
                        return Ok(());
                    }
                    let expr = self.gen_expr(value)?;
                    let expr = self.maybe_clone_list_expr(expr, value, None);
                    let mut_kw = mut_kw_for_name(name, mut_counts);
                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                        self.sync_binding_container_storage_from_value(name, value);
                    }
                } else {
                    let expected = self.local_var_type(name).cloned();
                    if let (Some(current_ty), Some(new_ty)) = (expected.as_ref(), value.ty.as_ref())
                    {
                        if !matches!(current_ty, Type::Option(_))
                            && Self::should_shadow_on_type_change(current_ty, new_ty)
                        {
                            // CPython allows re-binding a local name to an unrelated type.
                            // Rust assignments cannot change binding types, so we model this
                            // with an explicit shadowing `let` that keeps RHS evaluation
                            // semantics while switching static type.
                            let stale_empty_list_hint = matches!(current_ty, Type::List(_))
                                && matches!(new_ty, Type::List(inner) if matches!(inner.as_ref(), Type::Unknown));
                            if stale_empty_list_hint {
                                if let ExprKind::List(items) = &value.kind {
                                    if items.is_empty() {
                                        // CPython-compat divergence:
                                        // On list rebinding (`x = []`) after a prior typed
                                        // list binding, we emit `Vec::new()` without an
                                        // explicit element type and let subsequent list
                                        // mutations infer the new Rust element type.
                                        let storage = self.list_storage_for_name(name);
                                        let expr =
                                            self.wrap_list_storage_expr("Vec::new()", storage);
                                        let mut_kw = mut_kw_for_name(name, mut_counts);
                                        self.push_line(&format!(
                                            "let {}{} = {};",
                                            mut_kw, name, expr
                                        ));
                                        self.set_local_var_type(
                                            name,
                                            Type::List(Box::new(Type::Unknown)),
                                        );
                                        self.set_list_storage_for_temp(
                                            name,
                                            self.list_storage_for_name(name),
                                        );
                                        return Ok(());
                                    }
                                }
                            }
                            if !stale_empty_list_hint {
                                if let Some((expr, elem_ty)) =
                                    self.gen_empty_list_with_hint(name, value)?
                                {
                                    let mut_kw = mut_kw_for_name(name, mut_counts);
                                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                                    self.set_local_var_type(name, Type::List(Box::new(elem_ty)));
                                    self.set_list_storage_for_temp(
                                        name,
                                        self.list_storage_for_name(name),
                                    );
                                    return Ok(());
                                }
                            }
                            let expr = self.gen_expr(value)?;
                            let expr = self.maybe_clone_list_expr(expr, value, None);
                            let mut_kw = mut_kw_for_name(name, mut_counts);
                            self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                            self.set_local_var_type(name, new_ty.clone());
                            self.sync_binding_container_storage_from_value(name, value);
                            return Ok(());
                        }
                    }
                    let expected_is_optional = matches!(expected.as_ref(), Some(Type::Option(_)));
                    if !allow_let && self.emit_inplace_list_add_assign(name, value)? {
                        return Ok(());
                    }
                    let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                    if !expected_is_optional {
                        if let Some(local_expr) = self.gen_list_assignment_expr(name, value)? {
                            self.push_line(&format!("{} = {};", name, local_expr));
                            return Ok(());
                        }
                        if let Some(local_expr) = self.gen_dict_assignment_expr(name, value)? {
                            self.push_line(&format!("{} = {};", name, local_expr));
                            return Ok(());
                        }
                    }
                    let expr = self.maybe_clone_list_expr(expr, value, expected.as_ref());
                    self.push_line(&format!("{} = {};", name, expr));
                }
            }
            AssignTarget::Attr { value: obj, attr } => {
                if let Some(Type::Custom(class_name)) = obj.ty.as_ref() {
                    if let Some(prop) = self.class_property(class_name, attr).cloned() {
                        if let Some(setter) = prop.setter {
                            let expected = Some(&prop.ty);
                            let val_expr = self.gen_expr_with_expected(value, expected)?;
                            let val_expr = self.maybe_clone_list_expr(val_expr, value, expected);
                            if let ExprKind::Name(name) = &obj.kind {
                                if self.is_global(name) {
                                    let guard = self.new_tmp();
                                    self.push_line("{");
                                    self.indent += 1;
                                    self.push_line(&format!(
                                        "let mut {} = {};",
                                        guard,
                                        self.global_lock_expr(name)
                                    ));
                                    self.push_line(&format!("{}.{}({});", guard, setter, val_expr));
                                    self.indent -= 1;
                                    self.push_line("}");
                                    return Ok(());
                                }
                            }
                            let obj_expr = self.gen_expr(obj)?;
                            self.push_line(&format!("{}.{}({});", obj_expr, setter, val_expr));
                            return Ok(());
                        }
                    }
                }
                if let ExprKind::Name(name) = &obj.kind {
                    if let Some(global_name) = self
                        .class_attr_global(name, attr)
                        .map(|name| name.to_string())
                    {
                        let expected = self
                            .ctx
                            .classes
                            .get(name)
                            .and_then(|info| info.class_attrs.get(attr))
                            .map(|info| &info.ty);
                        let expr = self.gen_expr_with_expected(value, expected)?;
                        let expr = self.maybe_clone_list_expr(expr, value, expected);
                        if allow_let && !self.initialized_globals.contains(&global_name) {
                            let tmp = self.new_tmp();
                            let gname = self.global_name(&global_name);
                            self.push_line(&format!("let {} = {};", tmp, expr));
                            self.push_line(&format!(
                                "let _ = {}.get_or_init(|| Mutex::new({}));",
                                gname, tmp
                            ));
                            self.initialized_globals.insert(global_name.clone());
                            return Ok(());
                        }
                        let guard = self.new_tmp();
                        let current = self.new_tmp();
                        let expr =
                            self.with_global_override(&global_name, current.clone(), |this| {
                                let expr = this.gen_expr_with_expected(value, expected)?;
                                Ok(this.maybe_clone_list_expr(expr, value, expected))
                            })?;
                        self.push_line("{");
                        self.indent += 1;
                        self.push_line(&format!(
                            "let mut {} = {};",
                            guard,
                            self.global_lock_expr(&global_name)
                        ));
                        self.push_line(&format!("let {} = {}.clone();", current, guard));
                        self.push_line(&format!("*{} = {};", guard, expr));
                        self.indent -= 1;
                        self.push_line("}");
                        return Ok(());
                    }
                    if self.is_global(name) {
                        let expected = self.ctx.globals.get(name).cloned();
                        let val_expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        let val_expr =
                            self.maybe_clone_list_expr(val_expr, value, expected.as_ref());
                        let guard = self.new_tmp();
                        self.push_line("{");
                        self.indent += 1;
                        self.push_line(&format!(
                            "let mut {} = {};",
                            guard,
                            self.global_lock_expr(name)
                        ));
                        self.push_line(&format!("{}.{} = {};", guard, attr, val_expr));
                        self.indent -= 1;
                        self.push_line("}");
                        return Ok(());
                    }
                }
                let obj_expr = self.gen_expr(obj)?;
                let expected = match obj.ty.as_ref() {
                    Some(Type::Custom(class_name)) => self
                        .ctx
                        .classes
                        .get(class_name)
                        .and_then(|info| info.fields.get(attr))
                        .cloned(),
                    _ => None,
                };
                let val_expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                let val_expr = self.maybe_clone_list_expr(val_expr, value, expected.as_ref());
                self.push_line(&format!("{}.{} = {};", obj_expr, attr, val_expr));
            }
            AssignTarget::Index {
                value: container,
                index,
            } => {
                let expected = match container.ty.as_ref() {
                    Some(Type::List(inner)) | Some(Type::Set(inner)) => Some(inner.as_ref()),
                    Some(Type::Dict(_, val)) => Some(val.as_ref()),
                    Some(Type::Tuple(items)) => {
                        if let ExprKind::Literal(Literal::Int(idx)) = &index.kind {
                            if *idx >= 0 {
                                items.get(*idx as usize)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let val_expr = self.gen_expr_with_expected(value, expected)?;
                if let ExprKind::Name(name) = &container.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        let inner = self.new_tmp();
                        let dict_guard = self.new_tmp();
                        self.push_line("{");
                        self.indent += 1;
                        self.push_line(&format!(
                            "let mut {} = {};",
                            guard,
                            self.global_lock_expr(name)
                        ));
                        if let Some(Type::Dict(_, _)) = container.ty.as_ref() {
                            let idx_expr = self.gen_expr(index)?;
                            // Lock the inner dict before inserting.
                            self.push_line(&format!(
                                "let mut {} = {}.py_dict_guard();",
                                dict_guard, guard
                            ));
                            self.push_line(&format!(
                                "{}.insert({}, {});",
                                dict_guard, idx_expr, val_expr
                            ));
                        } else if matches!(container.ty.as_ref(), Some(Type::List(_))) {
                            let idx_raw = self.gen_expr(index)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            self.push_line(&format!(
                                "let mut {} = {}.py_list_guard();",
                                inner, guard
                            ));
                            self.push_line(&format!("let {} = {}.len();", len_tmp, inner));
                            self.push_line(&format!(
                                "let {} = {};",
                                idx_tmp,
                                self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                            ));
                            self.push_line(&format!("{}[{}] = {};", inner, idx_tmp, val_expr));
                        } else if matches!(container.ty.as_ref(), Some(Type::Tuple(_))) {
                            // Match CPython: tuple item assignment is a runtime TypeError.
                            self.push_line(&format!(
                                "{};",
                                self.wrap_result(
                                    "Err::<(), PyError>(PyError::TypeError(\"'tuple' object does not support item assignment\".into()))".to_string()
                                )
                            ));
                        }
                        self.indent -= 1;
                        self.push_line("}");
                        return Ok(());
                    }
                }
                let cont_expr = self.gen_expr(container)?;
                if let Some(Type::Dict(_, _)) = container.ty.as_ref() {
                    let idx_expr = self.gen_expr(index)?;
                    if matches!(self.dict_storage_for_expr(container), DictStorage::Local) {
                        // Local dicts are plain IndexMap values.
                        self.push_line(&format!(
                            "{}.insert({}, {});",
                            cont_expr, idx_expr, val_expr
                        ));
                    } else {
                        let guard = self.new_tmp();
                        // Scope the lock so dict mutations don't hold the mutex past this statement.
                        self.push_line("{");
                        self.indent += 1;
                        self.push_line(&format!(
                            "let mut {} = {}.py_dict_guard();",
                            guard, cont_expr
                        ));
                        self.push_line(&format!("{}.insert({}, {});", guard, idx_expr, val_expr));
                        self.indent -= 1;
                        self.push_line("}");
                    }
                } else if matches!(container.ty.as_ref(), Some(Type::List(_))) {
                    if let ExprKind::Name(name) = &container.kind {
                        if self.is_local_list_name(name) {
                            let idx_raw = self.gen_expr(index)?;
                            self.uses.py_index = true;
                            // Need to capture len first to avoid borrow conflict with mutable index
                            let len_tmp = self.new_tmp();
                            self.push_line(&format!("let {} = {}.len();", len_tmp, name));
                            let idx_expr =
                                self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp));
                            self.push_line(&format!("{}[{}] = {};", name, idx_expr, val_expr));
                            return Ok(());
                        }
                    }
                    let idx_raw = self.gen_expr(index)?;
                    self.uses.py_index = true;
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    // Scope the lock so list mutations don't hold the mutex past this statement.
                    self.push_line("{");
                    self.indent += 1;
                    self.push_line(&format!(
                        "let mut {} = {}.py_list_guard();",
                        guard, cont_expr
                    ));
                    self.push_line(&format!("let {} = {}.len();", len_tmp, guard));
                    self.push_line(&format!(
                        "let {} = {};",
                        idx_tmp,
                        self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                    ));
                    self.push_line(&format!("{}[{}] = {};", guard, idx_tmp, val_expr));
                    self.indent -= 1;
                    self.push_line("}");
                } else if matches!(container.ty.as_ref(), Some(Type::Tuple(_))) {
                    // Match CPython: tuple item assignment is a runtime TypeError.
                    self.push_line(&format!(
                        "{};",
                        self.wrap_result(
                            "Err::<(), PyError>(PyError::TypeError(\"'tuple' object does not support item assignment\".into()))".to_string()
                        )
                    ));
                } else {
                    let idx_expr = self.gen_expr(index)?;
                    self.push_line(&format!("{}[{}] = {};", cont_expr, idx_expr, val_expr));
                }
            }
            AssignTarget::Tuple(_) | AssignTarget::List(_) => {
                self.emit_unpack_assign(target, value, mut_counts)?;
            }
            AssignTarget::Starred(_) => {
                return Err(self.error(
                    value.span,
                    "Starred assignment target is only valid inside tuple/list unpacking",
                ));
            }
        }
        Ok(())
    }

    /// Decide whether local name rebinding should use `let` shadowing.
    ///
    /// This is only used for local variables to emulate Python's dynamic rebind
    /// behavior when the static Rust type changes incompatibly.
    fn should_shadow_on_type_change(current_ty: &Type, new_ty: &Type) -> bool {
        fn family(ty: &Type) -> &'static str {
            match ty {
                Type::Int => "int",
                Type::Float => "float",
                Type::Bool => "bool",
                Type::Str => "str",
                Type::Bytes => "bytes",
                Type::None => "none",
                Type::List(_) => "list",
                Type::Dict(_, _) => "dict",
                Type::Tuple(_) => "tuple",
                Type::Set(_) => "set",
                Type::Option(_) => "option",
                Type::Custom(_) => "custom",
                Type::Union(_) => "union",
                Type::Iterator(_) => "iter",
                Type::Lambda { .. } => "lambda",
                Type::Ref(_) => "ref",
                Type::MutRef(_) => "mutref",
                Type::Slice(_) => "slice",
                Type::Result(_, _) => "result",
                Type::Exception(_) => "exception",
                Type::Module(_) => "module",
                Type::StdlibFunction { .. } => "stdlibfn",
                Type::Unknown => "unknown",
            }
        }

        if family(current_ty) != family(new_ty) {
            return true;
        }
        match (current_ty, new_ty) {
            (Type::Unknown, Type::Unknown) => true,
            (Type::Unknown, _) | (_, Type::Unknown) => true,
            (Type::List(left), Type::List(right))
            | (Type::Set(left), Type::Set(right))
            | (Type::Option(left), Type::Option(right))
            | (Type::Ref(left), Type::Ref(right))
            | (Type::MutRef(left), Type::MutRef(right))
            | (Type::Slice(left), Type::Slice(right)) => {
                Self::should_shadow_on_type_change(left.as_ref(), right.as_ref())
            }
            (Type::Dict(left_k, left_v), Type::Dict(right_k, right_v)) => {
                Self::should_shadow_on_type_change(left_k.as_ref(), right_k.as_ref())
                    || Self::should_shadow_on_type_change(left_v.as_ref(), right_v.as_ref())
            }
            (Type::Tuple(left), Type::Tuple(right)) => {
                left.len() != right.len()
                    || left
                        .iter()
                        .zip(right.iter())
                        .any(|(l, r)| Self::should_shadow_on_type_change(l, r))
            }
            (
                Type::Lambda {
                    params: lp,
                    ret: lr,
                    ..
                },
                Type::Lambda {
                    params: rp,
                    ret: rr,
                    ..
                },
            ) => {
                lp.len() != rp.len()
                    || lp
                        .iter()
                        .zip(rp.iter())
                        .any(|(l, r)| Self::should_shadow_on_type_change(l, r))
                    || Self::should_shadow_on_type_change(lr.as_ref(), rr.as_ref())
            }
            (Type::Iterator(_), Type::Iterator(_)) => {
                // CPython-compat divergence:
                // Distinct iterator-producing expressions can lower to different
                // concrete Rust closure/adapter types even with the same logical
                // `Iterator[T]` shape. Shadowing keeps Python-style rebinding valid.
                true
            }
            (Type::Result(l_ok, l_err), Type::Result(r_ok, r_err)) => {
                Self::should_shadow_on_type_change(l_ok.as_ref(), r_ok.as_ref())
                    || Self::should_shadow_on_type_change(l_err.as_ref(), r_err.as_ref())
            }
            (Type::Custom(left), Type::Custom(right)) => left != right,
            (Type::Union(left), Type::Union(right)) => left != right,
            _ => false,
        }
    }

    /// Emit Python `del` for supported targets.
    ///
    /// Supported forms:
    /// - `del list[idx]`
    /// - `del dict[key]`
    /// - `del obj.prop` when `prop` has a deleter
    pub(crate) fn emit_delete_target(
        &mut self,
        target: &AssignTarget,
        span: Span,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Index {
                value: container,
                index,
            } => {
                if let ExprKind::Name(name) = &container.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        self.push_line("{");
                        self.indent += 1;
                        self.push_line(&format!(
                            "let mut {} = {};",
                            guard,
                            self.global_lock_expr(name)
                        ));
                        match container.ty.as_ref() {
                            Some(Type::Dict(_, _)) => {
                                let idx_expr = self.gen_expr(index)?;
                                let dict_guard = self.new_tmp();
                                self.push_line(&format!(
                                    "let mut {} = {}.py_dict_guard();",
                                    dict_guard, guard
                                ));
                                self.push_line(&format!(
                                    "{}.shift_remove(&{});",
                                    dict_guard, idx_expr
                                ));
                            }
                            Some(Type::List(_)) => {
                                let idx_raw = self.gen_expr(index)?;
                                self.uses.py_index = true;
                                let inner = self.new_tmp();
                                let len_tmp = self.new_tmp();
                                let idx_tmp = self.new_tmp();
                                self.push_line(&format!(
                                    "let mut {} = {}.py_list_guard();",
                                    inner, guard
                                ));
                                self.push_line(&format!("let {} = {}.len();", len_tmp, inner));
                                self.push_line(&format!(
                                    "let {} = {};",
                                    idx_tmp,
                                    self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                                ));
                                self.push_line(&format!("{}.remove({});", inner, idx_tmp));
                            }
                            _ => {
                                return Err(
                                    self.error(span, "del index requires list or dict container")
                                )
                            }
                        }
                        self.indent -= 1;
                        self.push_line("}");
                        return Ok(());
                    }
                }

                let cont_expr = self.gen_expr(container)?;
                match container.ty.as_ref() {
                    Some(Type::Dict(_, _)) => {
                        let idx_expr = self.gen_expr(index)?;
                        if matches!(self.dict_storage_for_expr(container), DictStorage::Local) {
                            self.push_line(&format!("{}.shift_remove(&{});", cont_expr, idx_expr));
                        } else {
                            let guard = self.new_tmp();
                            self.push_line("{");
                            self.indent += 1;
                            self.push_line(&format!(
                                "let mut {} = {}.py_dict_guard();",
                                guard, cont_expr
                            ));
                            self.push_line(&format!("{}.shift_remove(&{});", guard, idx_expr));
                            self.indent -= 1;
                            self.push_line("}");
                        }
                    }
                    Some(Type::List(_)) => {
                        let idx_raw = self.gen_expr(index)?;
                        self.uses.py_index = true;
                        if let ExprKind::Name(name) = &container.kind {
                            if self.is_local_list_name(name) {
                                // Resolve possibly-negative Python index before removal.
                                let len_tmp = self.new_tmp();
                                let idx_tmp = self.new_tmp();
                                self.push_line(&format!("let {} = {}.len();", len_tmp, name));
                                self.push_line(&format!(
                                    "let {} = {};",
                                    idx_tmp,
                                    self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                                ));
                                self.push_line(&format!("{}.remove({});", name, idx_tmp));
                                return Ok(());
                            }
                        }
                        let guard = self.new_tmp();
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        self.push_line("{");
                        self.indent += 1;
                        self.push_line(&format!(
                            "let mut {} = {}.py_list_guard();",
                            guard, cont_expr
                        ));
                        self.push_line(&format!("let {} = {}.len();", len_tmp, guard));
                        self.push_line(&format!(
                            "let {} = {};",
                            idx_tmp,
                            self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                        ));
                        self.push_line(&format!("{}.remove({});", guard, idx_tmp));
                        self.indent -= 1;
                        self.push_line("}");
                    }
                    _ => {
                        return Err(self.error(span, "del index requires list or dict container"));
                    }
                }
            }
            AssignTarget::Attr { value: obj, attr } => {
                if let Some(Type::Custom(class_name)) = obj.ty.as_ref() {
                    if let Some(prop) = self.class_property(class_name, attr).cloned() {
                        if let Some(deleter) = prop.deleter {
                            if let ExprKind::Name(name) = &obj.kind {
                                if self.is_global(name) {
                                    let guard = self.new_tmp();
                                    self.push_line("{");
                                    self.indent += 1;
                                    self.push_line(&format!(
                                        "let mut {} = {};",
                                        guard,
                                        self.global_lock_expr(name)
                                    ));
                                    self.push_line(&format!("{}.{}();", guard, deleter));
                                    self.indent -= 1;
                                    self.push_line("}");
                                    return Ok(());
                                }
                            }
                            let obj_expr = self.gen_expr(obj)?;
                            self.push_line(&format!("{}.{}();", obj_expr, deleter));
                            return Ok(());
                        }
                        return Err(self
                            .error(span, format!("Property {class_name}.{attr} has no deleter")));
                    }
                    if self
                        .ctx
                        .classes
                        .get(class_name)
                        .is_some_and(|info| info.fields.contains_key(attr))
                    {
                        if self
                            .ctx
                            .classes
                            .get(class_name)
                            .and_then(|info| info.fields.get(attr))
                            .is_some_and(|ty| matches!(ty, Type::Int))
                        {
                            // CPython-compat divergence:
                            // deletable int fields use `i64::MIN` as an internal tombstone.
                            if let ExprKind::Name(name) = &obj.kind {
                                if self.is_global(name) {
                                    let guard = self.new_tmp();
                                    self.push_line("{");
                                    self.indent += 1;
                                    self.push_line(&format!(
                                        "let mut {} = {};",
                                        guard,
                                        self.global_lock_expr(name)
                                    ));
                                    self.push_line(&format!("{}.{} = i64::MIN;", guard, attr));
                                    self.indent -= 1;
                                    self.push_line("}");
                                    return Ok(());
                                }
                            }
                            let obj_expr = self.gen_expr(obj)?;
                            self.push_line(&format!("{}.{} = i64::MIN;", obj_expr, attr));
                            return Ok(());
                        }
                        // Python deletes instance attributes at runtime. For non-int fields we
                        // keep the fixed-field struct model and treat deletion as a no-op.
                        return Ok(());
                    }
                }
                return Err(self.error(span, "del attribute requires a property with a deleter"));
            }
            AssignTarget::Name(_) => {
                return Err(self.error(
                    span,
                    "del name is not supported; only del index/attribute is supported",
                ))
            }
            AssignTarget::Tuple(_) | AssignTarget::List(_) | AssignTarget::Starred(_) => {
                return Err(self.error(span, "del unpacking targets are not supported"));
            }
        }
        Ok(())
    }

    /// Emit tuple/list unpacking assignments, evaluating the RHS once.
    pub(super) fn emit_unpack_assign(
        &mut self,
        target: &AssignTarget,
        value: &Expr,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        let value_expr = self.gen_expr(value)?;
        let tmp = self.new_tmp();
        self.push_line(&format!("let {} = {};", tmp, value_expr));
        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
            let storage = self.list_storage_for_expr(value);
            self.set_list_storage_for_temp(&tmp, storage);
        }
        let tmp_expr = Expr {
            kind: ExprKind::Name(tmp),
            span: value.span,
            ty: value.ty.clone(),
        };
        self.emit_unpack_from(&tmp_expr, target, mut_counts)
    }

    /// Recursively unpack tuple/list targets from a source expression.
    fn emit_unpack_from(
        &mut self,
        source: &Expr,
        target: &AssignTarget,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                let starred: Vec<usize> = items
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, item)| {
                        if matches!(item, AssignTarget::Starred(_)) {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                    .collect();
                if starred.len() > 1 {
                    return Err(
                        self.error(source.span, "Only one starred assignment target is allowed")
                    );
                }

                if let Some(star_idx) = starred.first().copied() {
                    let prefix_len = star_idx;
                    let suffix_len = items.len().saturating_sub(star_idx + 1);

                    match source.ty.as_ref() {
                        Some(Type::Tuple(tuple_types)) => {
                            if tuple_types.len() < (prefix_len + suffix_len) {
                                return Err(self.error(
                                    source.span,
                                    format!(
                                        "Unpacking expected at least {} values, got {}",
                                        prefix_len + suffix_len,
                                        tuple_types.len()
                                    ),
                                ));
                            }

                            for (idx, item_target) in items.iter().take(prefix_len).enumerate() {
                                let elem_ty =
                                    tuple_types.get(idx).cloned().unwrap_or(Type::Unknown);
                                let idx_expr = Expr {
                                    kind: ExprKind::Literal(Literal::Int(idx as i64)),
                                    span: source.span,
                                    ty: Some(Type::Int),
                                };
                                let elem_expr = Expr {
                                    kind: ExprKind::Index {
                                        value: Box::new(source.clone()),
                                        index: Box::new(idx_expr),
                                    },
                                    span: source.span,
                                    ty: Some(elem_ty),
                                };
                                self.emit_simple_assign(item_target, &elem_expr, mut_counts, true)?;
                            }

                            let middle_start = prefix_len;
                            let middle_end = tuple_types.len() - suffix_len;
                            let middle_ty = self
                                .merge_starred_elem_type(&tuple_types[middle_start..middle_end]);
                            let star_list_ty = Type::List(Box::new(middle_ty.clone()));
                            let tuple_src = self.gen_expr(source)?;
                            let middle_items: Vec<String> = (middle_start..middle_end)
                                .map(|idx| format!("{tuple_src}.{idx}.clone()"))
                                .collect();
                            let star_storage = ListStorage::SharedCell;
                            let star_base = if middle_items.is_empty() {
                                format!("Vec::<{}>::new()", self.rust_type(&middle_ty))
                            } else {
                                format!("vec![{}]", middle_items.join(", "))
                            };
                            let star_list_expr =
                                self.wrap_list_storage_expr(&star_base, star_storage);
                            let star_tmp = self.new_tmp();
                            self.set_list_storage_for_temp(&star_tmp, star_storage);
                            self.push_line(&format!("let {} = {};", star_tmp, star_list_expr));
                            let star_source = Expr {
                                kind: ExprKind::Name(star_tmp),
                                span: source.span,
                                ty: Some(star_list_ty),
                            };
                            if let AssignTarget::Starred(inner) = &items[star_idx] {
                                self.emit_unpack_from(&star_source, inner, mut_counts)?;
                            } else {
                                unreachable!("star index must point to AssignTarget::Starred");
                            }

                            for offset in 0..suffix_len {
                                let idx = tuple_types.len() - suffix_len + offset;
                                let elem_ty =
                                    tuple_types.get(idx).cloned().unwrap_or(Type::Unknown);
                                let idx_expr = Expr {
                                    kind: ExprKind::Literal(Literal::Int(idx as i64)),
                                    span: source.span,
                                    ty: Some(Type::Int),
                                };
                                let elem_expr = Expr {
                                    kind: ExprKind::Index {
                                        value: Box::new(source.clone()),
                                        index: Box::new(idx_expr),
                                    },
                                    span: source.span,
                                    ty: Some(elem_ty),
                                };
                                self.emit_simple_assign(
                                    &items[star_idx + 1 + offset],
                                    &elem_expr,
                                    mut_counts,
                                    true,
                                )?;
                            }
                        }
                        Some(Type::List(inner)) => {
                            let min_required = prefix_len + suffix_len;
                            if min_required > 0 {
                                let len_tmp = self.new_tmp();
                                let src_expr = self.gen_expr(source)?;
                                if matches!(self.list_storage_for_expr(source), ListStorage::Local)
                                {
                                    self.push_line(&format!(
                                        "let {} = {}.len();",
                                        len_tmp, src_expr
                                    ));
                                } else {
                                    self.push_line(&format!(
                                        "let {} = {}.py_list_guard().len();",
                                        len_tmp, src_expr
                                    ));
                                }
                                self.push_line(&format!(
                                    "if {} < {} {{ panic!(\"Unpacking expected at least {} values, got {{}}\", {}); }}",
                                    len_tmp, min_required, min_required, len_tmp
                                ));
                            }

                            for (idx, item_target) in items.iter().take(prefix_len).enumerate() {
                                let idx_expr = Expr {
                                    kind: ExprKind::Literal(Literal::Int(idx as i64)),
                                    span: source.span,
                                    ty: Some(Type::Int),
                                };
                                let elem_expr = Expr {
                                    kind: ExprKind::Index {
                                        value: Box::new(source.clone()),
                                        index: Box::new(idx_expr),
                                    },
                                    span: source.span,
                                    ty: Some(inner.as_ref().clone()),
                                };
                                self.emit_simple_assign(item_target, &elem_expr, mut_counts, true)?;
                            }

                            let start_expr = Some(Box::new(Expr {
                                kind: ExprKind::Literal(Literal::Int(prefix_len as i64)),
                                span: source.span,
                                ty: Some(Type::Int),
                            }));
                            let end_expr = if suffix_len == 0 {
                                None
                            } else {
                                Some(Box::new(Expr {
                                    kind: ExprKind::Literal(Literal::Int(-(suffix_len as i64))),
                                    span: source.span,
                                    ty: Some(Type::Int),
                                }))
                            };
                            let star_slice_expr = Expr {
                                kind: ExprKind::Slice {
                                    value: Box::new(source.clone()),
                                    start: start_expr,
                                    end: end_expr,
                                    step: None,
                                },
                                span: source.span,
                                ty: Some(Type::List(inner.clone())),
                            };
                            if let AssignTarget::Starred(inner_target) = &items[star_idx] {
                                self.emit_simple_assign(
                                    inner_target,
                                    &star_slice_expr,
                                    mut_counts,
                                    true,
                                )?;
                            } else {
                                unreachable!("star index must point to AssignTarget::Starred");
                            }

                            for offset in 0..suffix_len {
                                let from_end = suffix_len - offset;
                                let idx_expr = Expr {
                                    kind: ExprKind::Literal(Literal::Int(-(from_end as i64))),
                                    span: source.span,
                                    ty: Some(Type::Int),
                                };
                                let elem_expr = Expr {
                                    kind: ExprKind::Index {
                                        value: Box::new(source.clone()),
                                        index: Box::new(idx_expr),
                                    },
                                    span: source.span,
                                    ty: Some(inner.as_ref().clone()),
                                };
                                self.emit_simple_assign(
                                    &items[star_idx + 1 + offset],
                                    &elem_expr,
                                    mut_counts,
                                    true,
                                )?;
                            }
                        }
                        _ => {
                            return Err(self.error(
                                source.span,
                                "Unpacking assignment requires a tuple or list value",
                            ));
                        }
                    }
                    return Ok(());
                }

                let element_types =
                    self.unpack_element_types(source.ty.as_ref(), items.len(), source.span)?;
                for (idx, item) in items.iter().enumerate() {
                    let elem_ty = element_types.get(idx).cloned().unwrap_or(Type::Unknown);
                    let idx_expr = Expr {
                        kind: ExprKind::Literal(Literal::Int(idx as i64)),
                        span: source.span,
                        ty: Some(Type::Int),
                    };
                    let elem_expr = Expr {
                        kind: ExprKind::Index {
                            value: Box::new(source.clone()),
                            index: Box::new(idx_expr),
                        },
                        span: source.span,
                        ty: Some(elem_ty.clone()),
                    };
                    if matches!(item, AssignTarget::Tuple(_) | AssignTarget::List(_)) {
                        let nested_tmp = self.new_tmp();
                        let elem_str = self.gen_expr(&elem_expr)?;
                        self.push_line(&format!("let {} = {};", nested_tmp, elem_str));
                        let nested_expr = Expr {
                            kind: ExprKind::Name(nested_tmp),
                            span: source.span,
                            ty: Some(elem_ty),
                        };
                        self.emit_unpack_from(&nested_expr, item, mut_counts)?;
                    } else {
                        self.emit_simple_assign(item, &elem_expr, mut_counts, true)?;
                    }
                }
                Ok(())
            }
            AssignTarget::Starred(inner) => self.emit_unpack_from(source, inner, mut_counts),
            _ => self.emit_simple_assign(target, source, mut_counts, true),
        }
    }

    /// Determine element types when unpacking tuples/lists during codegen.
    fn unpack_element_types(
        &self,
        value_ty: Option<&Type>,
        count: usize,
        span: Span,
    ) -> Result<Vec<Type>, CompileError> {
        match value_ty {
            Some(Type::Tuple(items)) => {
                if items.len() != count {
                    return Err(self.error(
                        span,
                        format!("Unpacking expected {count} values, got {}", items.len()),
                    ));
                }
                Ok(items.clone())
            }
            Some(Type::List(inner)) => Ok(vec![inner.as_ref().clone(); count]),
            Some(Type::Unknown) | None => Ok(vec![Type::Unknown; count]),
            _ => Err(self.error(span, "Unpacking assignment requires a tuple or list value")),
        }
    }

    /// Merge element types for starred tuple unpacking.
    fn merge_starred_elem_type(&self, items: &[Type]) -> Type {
        if items.is_empty() {
            return Type::Unknown;
        }
        let first = items[0].clone();
        if items.iter().all(|t| t == &first) {
            return first;
        }
        if items
            .iter()
            .all(|t| matches!(t, Type::Int | Type::Float | Type::Bool))
        {
            if items.iter().any(|t| matches!(t, Type::Float)) {
                return Type::Float;
            }
            return Type::Int;
        }
        Type::Unknown
    }

    /// Sync list/dict storage metadata for a newly bound local from its RHS.
    fn sync_binding_container_storage_from_value(&mut self, name: &str, value: &Expr) {
        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
            let storage = self.list_storage_for_expr(value);
            self.set_list_storage_for_temp(name, storage);
        }
        if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
            let storage = self.dict_storage_for_expr(value);
            self.set_dict_storage_for_temp(name, storage);
        }
    }

    pub(super) fn gen_empty_list_with_hint(
        &mut self,
        name: &str,
        value: &Expr,
    ) -> Result<Option<(String, Type)>, CompileError> {
        let elem_ty = match self.list_elem_type_for_name(name) {
            Some(ty) if !matches!(ty, Type::Unknown) => ty.clone(),
            _ => return Ok(None),
        };
        let needs_hint = match value.ty.as_ref() {
            Some(Type::List(inner)) => matches!(inner.as_ref(), Type::Unknown),
            _ => false,
        };
        if !needs_hint {
            return Ok(None);
        }
        let is_empty_list = matches!(&value.kind, ExprKind::List(items) if items.is_empty());
        let is_empty_call = matches!(
            &value.kind,
            ExprKind::Call {
                func,
                args,
                keywords,
            }
                if args.is_empty()
                    && keywords.is_empty()
                    && matches!(&func.kind, ExprKind::Name(name) if name == "list" || name == "tuple")
        );
        if !is_empty_list && !is_empty_call {
            return Ok(None);
        }
        let storage = self.list_storage_for_name(name);
        let base = format!("Vec::<{}>::new()", self.rust_type(&elem_ty));
        let expr = self.wrap_list_storage_expr(&base, storage);
        Ok(Some((expr, elem_ty)))
    }

    /// Generate a list expression for a local Vec-backed list assignment.
    pub(super) fn gen_list_assignment_expr(
        &mut self,
        name: &str,
        value: &Expr,
    ) -> Result<Option<String>, CompileError> {
        if !matches!(value.ty.as_ref(), Some(Type::List(_))) {
            return Ok(None);
        }
        if !matches!(self.list_storage_for_name(name), ListStorage::Local) {
            return Ok(None);
        }
        self.gen_fresh_list_expr_with_storage(value, ListStorage::Local)
    }

    fn emit_inplace_list_add_assign(
        &mut self,
        name: &str,
        value: &Expr,
    ) -> Result<bool, CompileError> {
        let ExprKind::Binary {
            op: BinOp::Add,
            left,
            right,
        } = &value.kind
        else {
            return Ok(false);
        };
        let ExprKind::Name(left_name) = &left.kind else {
            return Ok(false);
        };
        if left_name != name {
            return Ok(false);
        }
        if !matches!(left.ty.as_ref(), Some(Type::List(_)))
            || !matches!(right.ty.as_ref(), Some(Type::List(_)))
        {
            return Ok(false);
        }

        let rhs_expr = self.gen_expr(right)?;
        let rhs_items = if matches!(self.list_storage_for_expr(right), ListStorage::Local) {
            format!("{}.iter().cloned().collect::<Vec<_>>()", rhs_expr)
        } else {
            let rhs_tmp = self.new_tmp();
            let rhs_guard = self.new_tmp();
            let rhs_init = if matches!(right.kind, ExprKind::Name(_)) {
                format!("{}.clone()", rhs_expr)
            } else {
                rhs_expr
            };
            format!(
                "{{ let {rhs_tmp} = {rhs_init}; let {rhs_guard} = {rhs_tmp}.py_list_guard(); {rhs_guard}.iter().cloned().collect::<Vec<_>>() }}",
                rhs_tmp = rhs_tmp,
                rhs_init = rhs_init,
                rhs_guard = rhs_guard
            )
        };

        if matches!(self.list_storage_for_name(name), ListStorage::Local) {
            let rhs_tmp = self.new_tmp();
            self.push_line(&format!("let {} = {};", rhs_tmp, rhs_items));
            self.push_line(&format!("{}.extend({});", name, rhs_tmp));
            return Ok(true);
        }

        let rhs_tmp = self.new_tmp();
        let target_guard = self.new_tmp();
        self.push_line("{");
        self.indent += 1;
        self.push_line(&format!("let {} = {};", rhs_tmp, rhs_items));
        self.push_line(&format!(
            "let mut {} = {}.py_list_guard();",
            target_guard, name
        ));
        self.push_line(&format!("{}.extend({});", target_guard, rhs_tmp));
        self.indent -= 1;
        self.push_line("}");
        Ok(true)
    }

    /// Generate a dict expression for a local IndexMap-backed dict assignment.
    pub(super) fn gen_dict_assignment_expr(
        &mut self,
        name: &str,
        value: &Expr,
    ) -> Result<Option<String>, CompileError> {
        if !matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
            return Ok(None);
        }
        if !matches!(self.dict_storage_for_name(name), DictStorage::Local) {
            return Ok(None);
        }
        match &value.kind {
            ExprKind::Dict(items) => {
                if items.is_empty()
                    && matches!(
                        value.ty.as_ref(),
                        Some(Type::Dict(key, val))
                            if matches!(key.as_ref(), Type::Unknown)
                                || matches!(val.as_ref(), Type::Unknown)
                    )
                {
                    self.uses.index_map = true;
                    if let Some((key_ty, val_ty)) = self.dict_kv_type_for_name(name) {
                        if !matches!(key_ty, Type::Unknown) && !matches!(val_ty, Type::Unknown) {
                            let key_ty = key_ty.clone();
                            let val_ty = val_ty.clone();
                            return Ok(Some(format!(
                                "IndexMap::<{}, {}>::new()",
                                self.rust_type(&key_ty),
                                self.rust_type(&val_ty)
                            )));
                        }
                    }
                    // Unconstrained empty dicts need a concrete fallback to avoid
                    // Rust E0282 in expressions that never reveal key/value types.
                    self.uses.py_repr = true;
                    return Ok(Some("IndexMap::<PyRepr, PyRepr>::new()".to_string()));
                }
                Ok(Some(self.gen_dict_expr_with_storage(
                    value,
                    items,
                    DictStorage::Local,
                )?))
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Name(call_name) = &func.kind {
                    if call_name == "dict" {
                        self.uses.index_map = true;
                        if args.is_empty() && keywords.is_empty() {
                            return Ok(Some("IndexMap::new()".to_string()));
                        }
                        if args.len() == 1 && keywords.is_empty() {
                            let arg = &args[0];
                            if matches!(arg.ty.as_ref(), Some(Type::Dict(_, _))) {
                                let arg_expr = self.gen_expr(arg)?;
                                if matches!(self.dict_storage_for_expr(arg), DictStorage::Local) {
                                    // Local dict copy is a simple IndexMap clone.
                                    return Ok(Some(format!("{}.clone()", arg_expr)));
                                }
                                let tmp = self.new_tmp();
                                let guard = self.new_tmp();
                                return Ok(Some(format!(
                                    "{{ let {tmp} = {arg}; let {guard} = {tmp}.py_dict_guard(); {guard}.clone() }}",
                                    tmp = tmp,
                                    arg = arg_expr,
                                    guard = guard
                                )));
                            }
                            let iter_src = self.gen_iter_source(arg)?;
                            let body = format!("({}).collect::<IndexMap<_, _>>()", iter_src.expr);
                            return Ok(Some(iter_src.wrap(body)));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
