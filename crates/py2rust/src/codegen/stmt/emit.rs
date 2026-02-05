// Main statement emission logic.

use super::super::util::{collect_assign_counts, mut_kw_for_name};
use super::super::*;

impl<'a> Codegen<'a> {
    /// Generate a Rust pattern for a for loop target.
    fn gen_for_target(&self, target: &ForTarget) -> String {
        match target {
            ForTarget::Name(name) => name.clone(),
            ForTarget::Tuple(names) => format!("({})", names.join(", ")),
        }
    }

    /// Update local variable types for a for loop target.
    fn insert_for_target_vars(
        &mut self,
        target: &ForTarget,
        item_ty: &Type,
        scoped_locals: &mut HashMap<String, Type>,
    ) {
        match target {
            ForTarget::Name(name) => {
                scoped_locals.insert(name.clone(), item_ty.clone());
            }
            ForTarget::Tuple(names) => {
                if let Type::Tuple(elem_types) = item_ty {
                    for (name, ty) in names.iter().zip(elem_types.iter()) {
                        scoped_locals.insert(name.clone(), ty.clone());
                    }
                } else {
                    // Fallback: insert Unknown for all names.
                    for name in names {
                        scoped_locals.insert(name.clone(), Type::Unknown);
                    }
                }
            }
        }
    }

    /// Emit default argument and class attribute initializers at the top of main.
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
    fn wrap_global_value(&mut self, expr: String, value: &Expr, expected: Option<&Type>) -> String {
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
                        let expr = if let Some(local_expr) =
                            self.gen_list_assignment_expr(name, value)?
                        {
                            local_expr
                        } else if let Some(local_expr) =
                            self.gen_dict_assignment_expr(name, value)?
                        {
                            local_expr
                        } else {
                            let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                            self.maybe_clone_list_expr(expr, value.ty.as_ref(), expected.as_ref())
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
                    self.push_line(&format!("let {} = {}.borrow().clone();", current, name));
                    let expr = self.with_name_override(name, current, |this| {
                        if let Some(local_expr) = this.gen_list_assignment_expr(name, value)? {
                            return Ok(local_expr);
                        }
                        if let Some(local_expr) = this.gen_dict_assignment_expr(name, value)? {
                            return Ok(local_expr);
                        }
                        let expr = this.gen_expr_with_expected(value, expected.as_ref())?;
                        Ok(this.maybe_clone_list_expr(expr, value.ty.as_ref(), expected.as_ref()))
                    })?;
                    self.push_line(&format!("*{}.borrow_mut() = {};", name, expr));
                    return Ok(());
                }
                // Global assignment uses OnceLock + Mutex for initialization and mutation.
                if self.is_global(name) {
                    let expected = self.ctx.globals.get(name).cloned();
                    if allow_let
                        && self.current_function.is_none()
                        && !self.initialized_globals.contains(name)
                    {
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        let expr =
                            self.maybe_clone_list_expr(expr, value.ty.as_ref(), expected.as_ref());
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
                        let expr =
                            this.maybe_clone_list_expr(expr, value.ty.as_ref(), expected.as_ref());
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
                        return Ok(());
                    }
                    if let Some(expr) = self.gen_list_assignment_expr(name, value)? {
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        if let Some(ty) = value.ty.clone() {
                            self.set_local_var_type(name, ty);
                        }
                        return Ok(());
                    }
                    if let Some(expr) = self.gen_dict_assignment_expr(name, value)? {
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        if let Some(ty) = value.ty.clone() {
                            self.set_local_var_type(name, ty);
                        }
                        return Ok(());
                    }
                    let expr = self.gen_expr(value)?;
                    let expr = self.maybe_clone_list_expr(expr, value.ty.as_ref(), None);
                    let mut_kw = mut_kw_for_name(name, mut_counts);
                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                    }
                } else {
                    let expected = self.local_var_type(name).cloned();
                    let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                    if let Some(local_expr) = self.gen_list_assignment_expr(name, value)? {
                        self.push_line(&format!("{} = {};", name, local_expr));
                        return Ok(());
                    }
                    if let Some(local_expr) = self.gen_dict_assignment_expr(name, value)? {
                        self.push_line(&format!("{} = {};", name, local_expr));
                        return Ok(());
                    }
                    let expr =
                        self.maybe_clone_list_expr(expr, value.ty.as_ref(), expected.as_ref());
                    self.push_line(&format!("{} = {};", name, expr));
                }
            }
            AssignTarget::Attr { value: obj, attr } => {
                if let Some(Type::Custom(class_name)) = obj.ty.as_ref() {
                    if let Some(prop) = self.class_property(class_name, attr).cloned() {
                        if let Some(setter) = prop.setter {
                            let expected = Some(&prop.ty);
                            let val_expr = self.gen_expr_with_expected(value, expected)?;
                            let val_expr =
                                self.maybe_clone_list_expr(val_expr, value.ty.as_ref(), expected);
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
                        let expr = self.maybe_clone_list_expr(expr, value.ty.as_ref(), expected);
                        if allow_let
                            && self.current_function.is_none()
                            && !self.initialized_globals.contains(&global_name)
                        {
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
                                Ok(this.maybe_clone_list_expr(expr, value.ty.as_ref(), expected))
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
                        let val_expr = self.maybe_clone_list_expr(
                            val_expr,
                            value.ty.as_ref(),
                            expected.as_ref(),
                        );
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
                let val_expr =
                    self.maybe_clone_list_expr(val_expr, value.ty.as_ref(), expected.as_ref());
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
                                "let mut {} = {}.lock().expect(\"dict mutex poisoned\");",
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
                                "let mut {} = {}.lock().expect(\"list mutex poisoned\");",
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
                            let idx_raw = self.gen_expr(index)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            self.push_line(&format!("let {} = {}.len();", len_tmp, guard));
                            self.push_line(&format!(
                                "let {} = {};",
                                idx_tmp,
                                self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                            ));
                            self.push_line(&format!("{}[{}] = {};", guard, idx_tmp, val_expr));
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
                        // Local dicts are plain HashMap values.
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
                            "let mut {} = {}.lock().expect(\"dict mutex poisoned\");",
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
                        "let mut {} = {}.lock().expect(\"list mutex poisoned\");",
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
                    let idx_raw = self.gen_expr(index)?;
                    self.uses.py_index = true;
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    self.push_line(&format!("let {} = {}.len();", len_tmp, cont_expr));
                    self.push_line(&format!(
                        "let {} = {};",
                        idx_tmp,
                        self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp))
                    ));
                    self.push_line(&format!("{}[{}] = {};", cont_expr, idx_tmp, val_expr));
                } else {
                    let idx_expr = self.gen_expr(index)?;
                    self.push_line(&format!("{}[{}] = {};", cont_expr, idx_expr, val_expr));
                }
            }
            AssignTarget::Tuple(_) | AssignTarget::List(_) => {
                self.emit_unpack_assign(target, value, mut_counts)?;
            }
        }
        Ok(())
    }

    /// Emit tuple/list unpacking assignments, evaluating the RHS once.
    fn emit_unpack_assign(
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

    /// Emit a statement into the output buffer.
    pub(crate) fn emit_stmt(
        &mut self,
        stmt: &Stmt,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Let { name, ann, value } => {
                if self.is_global(name) {
                    let expected = self.ctx.globals.get(name).cloned();
                    let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                    let expr = self.wrap_global_value(expr, value, expected.as_ref());
                    let gname = self.global_name(name);
                    let tmp = self.new_tmp();
                    self.push_line(&format!("let {} = {};", tmp, expr));
                    self.push_line(&format!(
                        "let _ = {}.get_or_init(|| Mutex::new({}));",
                        gname, tmp
                    ));
                    self.initialized_globals.insert(name.clone());
                    return Ok(());
                }
                if self.is_cell_local(name) {
                    if ann.is_none() {
                        if let Some((expr, elem_ty)) = self.gen_empty_list_with_hint(name, value)? {
                            let expr = format!("Rc::new(RefCell::new({}))", expr);
                            let mut_kw = mut_kw_for_name(name, mut_counts);
                            self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                            self.set_local_var_type(name, Type::List(Box::new(elem_ty)));
                            return Ok(());
                        }
                    }

                    let expected = if let Some(ann) = ann {
                        Some(self.resolve_type_ref(ann, stmt.span)?)
                    } else {
                        None
                    };
                    let declared =
                        if let (Some(Type::Tuple(exp_items)), Some(Type::Tuple(actual_items))) =
                            (expected.as_ref(), value.ty.as_ref())
                        {
                            if exp_items.len() != actual_items.len() {
                                Some(Type::Tuple(actual_items.clone()))
                            } else {
                                expected.clone()
                            }
                        } else {
                            expected.clone()
                        };
                    let expr = if let Some(local_expr) =
                        self.gen_list_assignment_expr(name, value)?
                    {
                        local_expr
                    } else if let Some(local_expr) = self.gen_dict_assignment_expr(name, value)? {
                        local_expr
                    } else {
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        self.maybe_clone_list_expr(expr, value.ty.as_ref(), declared.as_ref())
                    };
                    let expr = format!("Rc::new(RefCell::new({}))", expr);
                    let mut_kw = mut_kw_for_name(name, mut_counts);
                    if let Some(declared) = declared.clone() {
                        // Choose a storage-aware type for lists/dicts; everything else uses rust_type().
                        let ty_str = match declared {
                            Type::List(_) => {
                                let storage = self.list_storage_for_name(name);
                                self.rust_type_for_list_storage(&declared, storage)
                            }
                            Type::Dict(_, _) => {
                                let storage = self.dict_storage_for_name(name);
                                self.rust_type_for_dict_storage(&declared, storage)
                            }
                            _ => self.rust_type(&declared),
                        };
                        let wrapped = format!("Rc<RefCell<{}>>", ty_str);
                        self.push_line(&format!("let {}{}: {} = {};", mut_kw, name, wrapped, expr));
                        self.set_local_var_type(name, declared);
                    } else {
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        if let Some(ty) = value.ty.clone() {
                            self.set_local_var_type(name, ty);
                        }
                    }
                    return Ok(());
                }
                if ann.is_none() {
                    if let Some((expr, elem_ty)) = self.gen_empty_list_with_hint(name, value)? {
                        let mut_kw = mut_kw_for_name(name, mut_counts);
                        self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                        self.set_local_var_type(name, Type::List(Box::new(elem_ty)));
                        return Ok(());
                    }
                }
                if let ExprKind::Lambda { params, body } = &value.kind {
                    if let ExprKind::Block { stmts } = &body.kind {
                        fn expr_mentions_name(expr: &Expr, target: &str) -> bool {
                            match &expr.kind {
                                ExprKind::Name(name) => name == target,
                                ExprKind::Call {
                                    func,
                                    args,
                                    keywords,
                                } => {
                                    expr_mentions_name(func, target)
                                        || args.iter().any(|arg| expr_mentions_name(arg, target))
                                        || keywords
                                            .iter()
                                            .any(|kw| expr_mentions_name(&kw.value, target))
                                }
                                ExprKind::Starred { value } => expr_mentions_name(value, target),
                                ExprKind::Attr { value, .. } => expr_mentions_name(value, target),
                                ExprKind::Binary { left, right, .. }
                                | ExprKind::Compare { left, right, .. } => {
                                    expr_mentions_name(left, target)
                                        || expr_mentions_name(right, target)
                                }
                                ExprKind::CompareChain {
                                    left, comparators, ..
                                } => {
                                    expr_mentions_name(left, target)
                                        || comparators
                                            .iter()
                                            .any(|expr| expr_mentions_name(expr, target))
                                }
                                ExprKind::Unary { expr, .. } => expr_mentions_name(expr, target),
                                ExprKind::BoolOp { values, .. }
                                | ExprKind::List(values)
                                | ExprKind::Tuple(values)
                                | ExprKind::Set(values) => {
                                    values.iter().any(|expr| expr_mentions_name(expr, target))
                                }
                                ExprKind::Dict(items) => items.iter().any(|(k, v)| {
                                    expr_mentions_name(k, target) || expr_mentions_name(v, target)
                                }),
                                ExprKind::Index { value, index } => {
                                    expr_mentions_name(value, target)
                                        || expr_mentions_name(index, target)
                                }
                                ExprKind::Slice {
                                    value,
                                    start,
                                    end,
                                    step,
                                } => {
                                    expr_mentions_name(value, target)
                                        || start
                                            .as_deref()
                                            .is_some_and(|e| expr_mentions_name(e, target))
                                        || end
                                            .as_deref()
                                            .is_some_and(|e| expr_mentions_name(e, target))
                                        || step
                                            .as_deref()
                                            .is_some_and(|e| expr_mentions_name(e, target))
                                }
                                ExprKind::ListComp { elt, iter, ifs, .. }
                                | ExprKind::SetComp { elt, iter, ifs, .. } => {
                                    expr_mentions_name(elt, target)
                                        || expr_mentions_name(iter, target)
                                        || ifs.iter().any(|e| expr_mentions_name(e, target))
                                }
                                ExprKind::UnionCtor { inner, .. } => {
                                    expr_mentions_name(inner, target)
                                }
                                ExprKind::Lambda { .. } => false,
                                ExprKind::IfExpr { test, body, orelse } => {
                                    expr_mentions_name(test, target)
                                        || expr_mentions_name(body, target)
                                        || expr_mentions_name(orelse, target)
                                }
                                ExprKind::Block { stmts } => {
                                    stmts.iter().any(|stmt| stmt_mentions_name(stmt, target))
                                }
                                ExprKind::Literal(_) => false,
                            }
                        }

                        fn stmt_mentions_name(stmt: &Stmt, target: &str) -> bool {
                            match &stmt.kind {
                                StmtKind::Let { value, .. } | StmtKind::Expr(value) => {
                                    expr_mentions_name(value, target)
                                }
                                StmtKind::Assign { value, .. } => expr_mentions_name(value, target),
                                StmtKind::Return { value } => value
                                    .as_ref()
                                    .is_some_and(|expr| expr_mentions_name(expr, target)),
                                StmtKind::If { test, body, orelse } => {
                                    expr_mentions_name(test, target)
                                        || body.iter().any(|s| stmt_mentions_name(s, target))
                                        || orelse.iter().any(|s| stmt_mentions_name(s, target))
                                }
                                StmtKind::While { test, body } => {
                                    expr_mentions_name(test, target)
                                        || body.iter().any(|s| stmt_mentions_name(s, target))
                                }
                                StmtKind::For { iter, body, .. } => {
                                    expr_mentions_name(iter, target)
                                        || body.iter().any(|s| stmt_mentions_name(s, target))
                                }
                                StmtKind::Assert { test, msg } => {
                                    expr_mentions_name(test, target)
                                        || msg
                                            .as_ref()
                                            .is_some_and(|expr| expr_mentions_name(expr, target))
                                }
                                StmtKind::Match { subject, cases } => {
                                    expr_mentions_name(subject, target)
                                        || cases.iter().any(|case| {
                                            case.body
                                                .iter()
                                                .any(|stmt| stmt_mentions_name(stmt, target))
                                        })
                                }
                                StmtKind::Try {
                                    body,
                                    handlers,
                                    orelse,
                                    finalbody,
                                } => {
                                    body.iter().any(|s| stmt_mentions_name(s, target))
                                        || handlers.iter().any(|h| {
                                            h.body
                                                .iter()
                                                .any(|stmt| stmt_mentions_name(stmt, target))
                                        })
                                        || orelse.iter().any(|s| stmt_mentions_name(s, target))
                                        || finalbody.iter().any(|s| stmt_mentions_name(s, target))
                                }
                                StmtKind::Raise { exc, cause } => {
                                    exc.as_ref()
                                        .is_some_and(|expr| expr_mentions_name(expr, target))
                                        || cause
                                            .as_ref()
                                            .is_some_and(|expr| expr_mentions_name(expr, target))
                                }
                                StmtKind::Global { .. }
                                | StmtKind::Nonlocal { .. }
                                | StmtKind::Break
                                | StmtKind::Continue => false,
                            }
                        }

                        fn contains_nonlocal_decl(stmt: &Stmt) -> bool {
                            match &stmt.kind {
                                StmtKind::Nonlocal { .. } => true,
                                StmtKind::If { body, orelse, .. } => {
                                    body.iter().any(contains_nonlocal_decl)
                                        || orelse.iter().any(contains_nonlocal_decl)
                                }
                                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                                    body.iter().any(contains_nonlocal_decl)
                                }
                                StmtKind::Match { cases, .. } => cases
                                    .iter()
                                    .any(|case| case.body.iter().any(contains_nonlocal_decl)),
                                StmtKind::Try {
                                    body,
                                    handlers,
                                    orelse,
                                    finalbody,
                                } => {
                                    body.iter().any(contains_nonlocal_decl)
                                        || handlers
                                            .iter()
                                            .any(|h| h.body.iter().any(contains_nonlocal_decl))
                                        || orelse.iter().any(contains_nonlocal_decl)
                                        || finalbody.iter().any(contains_nonlocal_decl)
                                }
                                _ => false,
                            }
                        }

                        let has_nonlocal_decl = stmts.iter().any(contains_nonlocal_decl);
                        let has_unknown_sig = matches!(
                            value.ty.as_ref(),
                            Some(Type::Lambda { params, ret })
                                if params.iter().any(|ty| matches!(ty, Type::Unknown))
                                    || matches!(ret.as_ref(), Type::Unknown)
                        );
                        let is_recursive_nested =
                            stmts.iter().any(|stmt| stmt_mentions_name(stmt, name));
                        // Nested def: inside a function, emit a closure to allow captures.
                        if self.current_function.is_some() && !is_recursive_nested {
                            let expected = if let Some(ann) = ann {
                                Some(self.resolve_type_ref(ann, stmt.span)?)
                            } else {
                                None
                            };
                            let _ = has_unknown_sig;
                            let mut expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                            if has_nonlocal_decl {
                                // `nonlocal` closures must borrow outer cells instead of moving them,
                                // otherwise the outer binding is moved and becomes unusable afterwards.
                                if let Some(stripped) = expr.strip_prefix("move ") {
                                    expr = stripped.to_string();
                                }
                            }
                            let mut_kw = mut_kw_for_name(name, mut_counts);
                            self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                            if let Some(ty) = expected.or_else(|| value.ty.clone()) {
                                self.set_local_var_type(name, ty);
                            }
                            return Ok(());
                        }

                        let mut param_parts = Vec::new();
                        let mut ret_ty = Type::Unknown;
                        if let Some(Type::Lambda {
                            params: param_tys,
                            ret,
                        }) = value.ty.as_ref()
                        {
                            ret_ty = (**ret).clone();
                            for (param, ty) in params.iter().zip(param_tys.iter()) {
                                let ty_str = if matches!(ty, Type::Unknown) {
                                    "()".to_string()
                                } else {
                                    self.rust_type(ty)
                                };
                                param_parts.push(format!("{}: {}", param, ty_str));
                            }
                        } else {
                            for param in params {
                                param_parts.push(format!("{}: ()", param));
                            }
                        }
                        let ret_str = if matches!(ret_ty, Type::Unknown) {
                            "()".to_string()
                        } else {
                            self.rust_type(&ret_ty)
                        };
                        self.push_line(&format!(
                            "fn {}({}) -> {} {{",
                            name,
                            param_parts.join(", "),
                            ret_str
                        ));
                        self.indent += 1;
                        let mut_counts = collect_assign_counts(stmts);
                        for stmt in stmts {
                            self.emit_stmt(stmt, &mut_counts)?;
                        }
                        self.indent -= 1;
                        self.push_line("}");
                        return Ok(());
                    }
                }
                let expected = if let Some(ann) = ann {
                    Some(self.resolve_type_ref(ann, stmt.span)?)
                } else {
                    None
                };
                let declared =
                    if let (Some(Type::Tuple(exp_items)), Some(Type::Tuple(actual_items))) =
                        (expected.as_ref(), value.ty.as_ref())
                    {
                        if exp_items.len() != actual_items.len() {
                            Some(Type::Tuple(actual_items.clone()))
                        } else {
                            expected.clone()
                        }
                    } else {
                        expected.clone()
                    };
                let expr = if let Some(local_expr) = self.gen_list_assignment_expr(name, value)? {
                    local_expr
                } else if let Some(local_expr) = self.gen_dict_assignment_expr(name, value)? {
                    local_expr
                } else {
                    let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                    self.maybe_clone_list_expr(expr, value.ty.as_ref(), declared.as_ref())
                };
                let mut_kw = mut_kw_for_name(name, mut_counts);
                if ann.is_some() {
                    let ty = declared.expect("resolved above");
                    // Choose a storage-aware type for lists/dicts; everything else uses rust_type().
                    let ty_str = match ty {
                        Type::List(_) => {
                            let storage = self.list_storage_for_name(name);
                            self.rust_type_for_list_storage(&ty, storage)
                        }
                        Type::Dict(_, _) => {
                            let storage = self.dict_storage_for_name(name);
                            self.rust_type_for_dict_storage(&ty, storage)
                        }
                        _ => self.rust_type(&ty),
                    };
                    self.push_line(&format!("let {}{}: {} = {};", mut_kw, name, ty_str, expr));
                    self.set_local_var_type(name, ty);
                } else {
                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                    }
                }
            }
            StmtKind::Assign { target, value } => {
                if matches!(target, AssignTarget::Tuple(_) | AssignTarget::List(_)) {
                    self.emit_unpack_assign(target, value, mut_counts)?;
                } else {
                    self.emit_simple_assign(target, value, mut_counts, false)?;
                }
            }
            StmtKind::Return { value } => {
                // Check if we're in a throwing function or inside a try block with value return.
                let in_throwing_fn = self.current_function_throws();
                let in_try_with_value = self.try_block_return_type.is_some();

                // Inside try block with value returns, always wrap in Ok.
                let wrap_in_ok = in_throwing_fn || in_try_with_value;

                if let Some(expr) = value {
                    let expected = if let Some(lambda_ret) =
                        self.lambda_return_types.last().and_then(|ret| ret.as_ref())
                    {
                        Some(lambda_ret.clone())
                    } else {
                        self.current_function_ret.as_ref().map(|ty| {
                            if let Some((ok, _)) = ty.unwrap_result() {
                                ok.clone()
                            } else {
                                ty.clone()
                            }
                        })
                    };
                    let mut expr_str = self.gen_expr_with_expected(expr, expected.as_ref())?;
                    if matches!(expr.ty.as_ref(), Some(Type::Lambda { .. })) {
                        if let Some(expected_ty) = expected.as_ref() {
                            if matches!(expected_ty, Type::Lambda { .. }) {
                                // Closure returns inside higher-order contexts need explicit
                                // boxing and trait-object coercion.
                                let boxed_ty = self.rust_type_for_closure_param(expected_ty);
                                expr_str = format!("(Box::new({}) as {})", expr_str, boxed_ty);
                            }
                        }
                    }
                    if wrap_in_ok {
                        self.push_line(&format!("return Ok({});", expr_str));
                    } else {
                        self.push_line(&format!("return {};", expr_str));
                    }
                } else if wrap_in_ok {
                    self.push_line("return Ok(());");
                } else {
                    self.push_line("return;");
                }
            }
            StmtKind::If { test, body, orelse } => {
                if body.len() == 1 && orelse.len() == 1 {
                    let extract = |stmt: &Stmt| -> Option<(String, Option<TypeRef>, Expr, bool)> {
                        match &stmt.kind {
                            StmtKind::Let { name, ann, value } => {
                                Some((name.clone(), ann.clone(), value.clone(), true))
                            }
                            StmtKind::Assign {
                                target: AssignTarget::Name(name),
                                value,
                            } => Some((name.clone(), None, value.clone(), false)),
                            _ => None,
                        }
                    };
                    if let (
                        Some((name_left, ann_left, val_left, left_is_let)),
                        Some((name_right, ann_right, val_right, right_is_let)),
                    ) = (extract(&body[0]), extract(&orelse[0]))
                    {
                        if name_left == name_right && (left_is_let || right_is_let) {
                            let test_expr = self.gen_expr(test)?;
                            let left_expr = self.gen_expr(&val_left)?;
                            let right_expr = self.gen_expr(&val_right)?;
                            let mut_kw = mut_kw_for_name(&name_left, mut_counts);
                            let ann = ann_left.or(ann_right);
                            if let Some(ann) = ann {
                                let ty = self.resolve_type_ref(&ann, stmt.span)?;
                                let ty_str = self.rust_type(&ty);
                                let left_expr =
                                    self.gen_expr_with_expected(&val_left, Some(&ty))?;
                                let right_expr =
                                    self.gen_expr_with_expected(&val_right, Some(&ty))?;
                                self.push_line(&format!(
                                    "let {}{}: {} = if {} {{ {} }} else {{ {} }};",
                                    mut_kw, name_left, ty_str, test_expr, left_expr, right_expr
                                ));
                            } else {
                                self.push_line(&format!(
                                    "let {}{} = if {} {{ {} }} else {{ {} }};",
                                    mut_kw, name_left, test_expr, left_expr, right_expr
                                ));
                            }
                            return Ok(());
                        }
                    }
                }
                let test_expr = self.gen_expr(test)?;
                self.push_line(&format!("if {} {{", test_expr));
                self.indent += 1;
                for stmt in body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.indent -= 1;
                if orelse.is_empty() {
                    self.push_line("}");
                } else {
                    self.push_line("} else {");
                    self.indent += 1;
                    for stmt in orelse {
                        self.emit_stmt(stmt, mut_counts)?;
                    }
                    self.indent -= 1;
                    self.push_line("}");
                }
            }
            StmtKind::While { test, body } => {
                let test_expr = self.gen_expr(test)?;
                self.push_line(&format!("while {} {{", test_expr));
                self.indent += 1;
                for stmt in body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.indent -= 1;
                self.push_line("}");
            }
            StmtKind::For { target, iter, body } => {
                let target_pattern = self.gen_for_target(target);
                let item_ty = iter
                    .ty
                    .as_ref()
                    .and_then(|ty| self.iter_item_type_hint(ty))
                    .unwrap_or(Type::Unknown);

                // Optimize local Vec iteration with while loop for simple name targets.
                if matches!(target, ForTarget::Name(_))
                    && matches!(iter.ty.as_ref(), Some(Type::List(inner)) if matches!(self.list_storage_for_expr(iter), ListStorage::Local))
                {
                    if let Some(Type::List(inner)) = iter.ty.as_ref() {
                        let iter_expr = self.gen_expr(iter)?;
                        let idx = self.new_tmp();
                        let item_expr = if self.is_copy_type(inner) {
                            format!("{iter}[{idx}]", iter = iter_expr, idx = idx)
                        } else {
                            format!("{iter}[{idx}].clone()", iter = iter_expr, idx = idx)
                        };
                        self.push_line(&format!("let mut {}: usize = 0;", idx));
                        self.push_line(&format!("while {} < {}.len() {{", idx, iter_expr));
                        self.indent += 1;
                        self.push_line(&format!("let {} = {};", target_pattern, item_expr));
                        self.push_line(&format!("{} += 1;", idx));
                        let saved_locals = self.local_vars.clone();
                        let mut scoped_locals = saved_locals.clone().unwrap_or_default();
                        self.insert_for_target_vars(target, &item_ty, &mut scoped_locals);
                        self.local_vars = Some(scoped_locals);
                        for stmt in body {
                            self.emit_stmt(stmt, mut_counts)?;
                        }
                        self.local_vars = saved_locals;
                        self.indent -= 1;
                        self.push_line("}");
                        return Ok(());
                    }
                }

                // General for loop with iterator.
                let IterSource { setup, expr } = self.gen_iter_source(iter)?;
                // Keep list/dict lock guards alive for the duration of the loop body.
                for line in setup {
                    self.push_line(&format!("{};", line));
                }
                let iter_src = expr;
                self.push_line(&format!("for {} in {} {{", target_pattern, iter_src));
                self.indent += 1;
                let saved_locals = self.local_vars.clone();
                let mut scoped_locals = saved_locals.clone().unwrap_or_default();
                self.insert_for_target_vars(target, &item_ty, &mut scoped_locals);
                self.local_vars = Some(scoped_locals);
                for stmt in body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.local_vars = saved_locals;
                self.indent -= 1;
                self.push_line("}");
            }
            StmtKind::Global { .. } | StmtKind::Nonlocal { .. } => {}
            StmtKind::Break => self.push_line("break;"),
            StmtKind::Continue => self.push_line("continue;"),
            StmtKind::Assert { test, msg } => {
                let test_expr = self.gen_expr(test)?;
                if let Some(msg) = msg {
                    let msg_expr = self.gen_expr(msg)?;
                    self.push_line(&format!("assert!({}, \"{{}}\", {});", test_expr, msg_expr));
                } else {
                    self.push_line(&format!("assert!({});", test_expr));
                }
            }
            StmtKind::Expr(expr) => {
                let expr_str = self.gen_expr(expr)?;
                self.push_line(&format!("{};", expr_str));
            }
            StmtKind::Match { subject, cases } => {
                let subj_expr = self.gen_expr(subject)?;
                self.push_line(&format!("match {} {{", subj_expr));
                self.indent += 1;
                for case in cases {
                    self.emit_match_case(case)?;
                }
                self.indent -= 1;
                self.push_line("}");
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                self.emit_try_stmt(body, handlers, orelse, finalbody, mut_counts)?;
            }
            StmtKind::Raise { exc, cause } => {
                self.emit_raise_stmt(exc.as_ref(), cause.as_ref(), stmt.span)?;
            }
        }
        Ok(())
    }

    /// Generate an empty list expression using inferred element type hints.
    fn gen_empty_list_with_hint(
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
    fn gen_list_assignment_expr(
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
        match &value.kind {
            ExprKind::List(items) => Ok(Some(self.gen_list_expr_with_storage(
                value,
                items,
                ListStorage::Local,
            )?)),
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
            } => Ok(Some(self.gen_list_comp_expr_with_storage(
                elt,
                target,
                iter,
                ifs,
                ListStorage::Local,
            )?)),
            _ => Ok(None),
        }
    }

    /// Generate a dict expression for a local HashMap-backed dict assignment.
    fn gen_dict_assignment_expr(
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
            ExprKind::Dict(items) => Ok(Some(self.gen_dict_expr_with_storage(
                value,
                items,
                DictStorage::Local,
            )?)),
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Name(call_name) = &func.kind {
                    if call_name == "dict" {
                        self.uses.hash_map = true;
                        if args.is_empty() && keywords.is_empty() {
                            return Ok(Some("HashMap::new()".to_string()));
                        }
                        if args.len() == 1 && keywords.is_empty() {
                            let arg = &args[0];
                            if matches!(arg.ty.as_ref(), Some(Type::Dict(_, _))) {
                                let arg_expr = self.gen_expr(arg)?;
                                if matches!(self.dict_storage_for_expr(arg), DictStorage::Local) {
                                    // Local dict copy is a simple HashMap clone.
                                    return Ok(Some(format!("{}.clone()", arg_expr)));
                                }
                                let tmp = self.new_tmp();
                                let guard = self.new_tmp();
                                return Ok(Some(format!(
                                    "{{ let {tmp} = {arg}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.clone() }}",
                                    tmp = tmp,
                                    arg = arg_expr,
                                    guard = guard
                                )));
                            }
                            let iter_src = self.gen_iter_source(arg)?;
                            let body = format!("({}).collect::<HashMap<_, _>>()", iter_src.expr);
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
