// Emission for unions, classes, and iterator adapters.

use super::super::util::{collect_assign_counts, mut_kw_for_param};
use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit a union (enum) definition.
    ///
    /// Python unions (created via `Status = Success | Failure`) map to Rust enums.
    /// Each variant wraps the corresponding class type.
    ///
    /// Example:
    /// Python: `Result = Ok | Err`
    /// Rust:   `enum Result { Ok(Ok), Err(Err) }`
    ///
    /// The variant names are the same as the class names they wrap.
    pub(crate) fn emit_union(&mut self, def: &UnionDef) -> Result<(), CompileError> {
        self.push_line("#[derive(Debug, Clone)]");
        self.push_line(&format!("pub enum {} {{", def.name));
        self.indent += 1;
        for variant in &def.variants {
            self.push_line(&format!("{}({}),", variant, variant));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");
        Ok(())
    }

    /// Emit a class definition, its methods, and iterator impls when needed.
    pub(crate) fn emit_class(&mut self, class_def: &ClassDef) -> Result<(), CompileError> {
        let class_info = self.ctx.classes.get(&class_def.name).ok_or_else(|| {
            self.error(class_def.span, format!("Unknown class: {}", class_def.name))
        })?;

        self.push_line("#[derive(Debug, Clone)]");
        self.push_line(&format!("pub struct {} {{", class_def.name));
        self.indent += 1;
        for (field, ty) in &class_info.fields {
            let ty_str = self.rust_type(ty);
            self.push_line(&format!("pub {}: {},", field, ty_str));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line(&format!("impl {} {{", class_def.name));
        self.indent += 1;
        if let Some(init) = class_def.methods.iter().find(|m| m.name == "__init__") {
            self.emit_constructor(class_def, init)?;
        } else if class_info.fields.is_empty() {
            self.push_line(&format!("pub fn new() -> {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("{} {{}}", class_def.name));
            self.indent -= 1;
            self.push_line("}");
        } else {
            self.push_line("// no __init__ defined");
        }

        // Emit all visible methods, including inherited ones, so instance/class
        // dispatch on subclasses can resolve base-method symbols in Rust.
        let mut method_names: Vec<String> = class_info.methods.keys().cloned().collect();
        method_names.sort();
        for method_name in method_names {
            if method_name == "__init__" {
                continue;
            }
            if method_name == "next" && class_info.next_item.is_some() {
                continue;
            }
            let method = self
                .method_def(&class_def.name, method_name.as_str())
                .cloned()
                .ok_or_else(|| {
                    self.error(
                        class_def.span,
                        format!("Unknown method {}.{}", class_def.name, method_name),
                    )
                })?;
            if let Some((prop_name, _)) = class_info.properties.iter().find(|(name, prop)| {
                (prop.getter == method.name || name.as_str() == method.name)
                    && prop.deleter.is_some()
            }) {
                if let Some(field) =
                    self.deletable_property_backing_int_field(class_def.name.as_str(), prop_name)
                {
                    self.emit_deletable_int_property_getter(&method, field.as_str())?;
                    continue;
                }
            }
            self.emit_function(&method, Some(class_def))?;
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        if let Some(item_ty) = &class_info.next_item {
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl Iterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            let next_method = class_def
                .methods
                .iter()
                .find(|m| m.name == "next")
                .ok_or_else(|| self.error(class_def.span, "Iterator class missing next method"))?;
            let ret_ty = self.resolve_type_ref(&next_method.ret, next_method.span)?;
            let ret_str = self.rust_type(&ret_ty);
            self.push_line(&format!("fn next(&mut self) -> {} {{", ret_str));
            self.indent += 1;
            let mut_counts = collect_assign_counts(&next_method.body);
            for stmt in &next_method.body {
                self.emit_stmt(stmt, &mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        }

        if class_info.iter_return.is_some() || class_info.iter_item.is_some() {
            self.emit_into_iter(class_def, class_info)?;
        }

        Ok(())
    }

    fn emit_constructor(
        &mut self,
        class_def: &ClassDef,
        init: &Function,
    ) -> Result<(), CompileError> {
        let class_info = self
            .ctx
            .classes
            .get(&class_def.name)
            .ok_or_else(|| self.error(class_def.span, "Unknown class"))?;
        // Constructor parameters can be reassigned inside __init__ (Python semantics),
        // so we mark parameters as mutable when assignment analysis detects mutations.
        let mut_counts = collect_assign_counts(&init.body);
        let inferred_params = class_info
            .init
            .as_ref()
            .map(|sig| sig.params.clone())
            .unwrap_or_default();
        let prev_current_function = self.current_function.clone();
        let prev_current_function_ret = self.current_function_ret.clone();
        let prev_local_vars = self.local_vars.clone();
        let prev_nonlocal_decls = self.nonlocal_decls.clone();
        let prev_cell_locals = self.cell_locals.clone();
        self.current_function = Some(format!("{}.__init__", class_def.name));
        self.current_function_ret = Some(Type::Custom(class_def.name.clone()));

        let mut ctor_locals = HashMap::new();
        ctor_locals.insert("self".to_string(), Type::Custom(class_def.name.clone()));
        let mut params = Vec::new();
        for (idx, param) in init.params.iter().enumerate().skip(1) {
            let ty = inferred_params
                .get(idx)
                .cloned()
                .unwrap_or(self.resolve_type_ref(&param.ann, param.span)?);
            ctor_locals.insert(param.name.clone(), ty.clone());
            let ty_str = if matches!(ty, Type::Unknown) {
                // CPython-compat divergence:
                // unknown constructor parameters are erased to unit for Rust
                // signature stability; runtime dynamic typing is not preserved here.
                "()".to_string()
            } else {
                self.rust_type(&ty)
            };
            let mut_kw = mut_kw_for_param(&param.name, &mut_counts);
            params.push(format!("{}{}: {}", mut_kw, param.name, ty_str));
        }
        self.local_vars = Some(ctor_locals);
        self.nonlocal_decls = None;
        self.cell_locals = None;
        let sig = format!("({}) -> {}", params.join(", "), class_def.name);
        self.push_line(&format!("pub fn new{} {{", sig));
        self.indent += 1;
        let mut field_inits: HashMap<String, String> = HashMap::new();
        for stmt in &init.body {
            match &stmt.kind {
                StmtKind::Assign { target, value } => {
                    match target.as_ref() {
                        AssignTarget::Attr { value: obj, attr } => {
                            if matches!(&obj.kind, ExprKind::Name(n) if n == "self") {
                                self.record_field_init(&mut field_inits, class_info, attr, value)?;
                            } else if let ExprKind::Name(name) = &obj.kind {
                                if self.class_attr_global(name, attr).is_some() {
                                    self.emit_simple_assign(target, value, &mut_counts, false)?;
                                    continue;
                                }
                                return Err(self
                                    .error(stmt.span, "__init__ may only assign to self fields"));
                            } else {
                                return Err(self
                                    .error(stmt.span, "__init__ may only assign to self fields"));
                            }
                        }
                        AssignTarget::Name(_) => {
                            // Local name rebinding inside __init__ (for example, default-fallback
                            // preprocessing) is emitted as regular assignment code.
                            self.emit_simple_assign(target, value, &mut_counts, false)?;
                        }
                        _ => {
                            return Err(
                                self.error(stmt.span, "__init__ may only assign to self fields")
                            );
                        }
                    }
                }
                StmtKind::If { body, orelse, .. } => {
                    if Self::stmts_assign_self_field(body) || Self::stmts_assign_self_field(orelse)
                    {
                        // CPython-compat divergence:
                        // CPython allows conditional field writes in __init__, but our
                        // constructor lowering materializes a single struct literal from
                        // collected field expressions. To keep this lowering simple and
                        // predictable, we currently reject conditional self-field writes.
                        return Err(self.error(
                            stmt.span,
                            "Conditional self field assignments in __init__ are not supported",
                        ));
                    }
                    // Allow control flow that only mutates local constructor variables.
                    self.emit_stmt(stmt, &mut_counts)?;
                }
                StmtKind::Expr(expr) => {
                    // Allow None literals and docstrings (string literals).
                    if matches!(
                        expr.kind,
                        ExprKind::Literal(Literal::None | Literal::Str(_))
                    ) {
                        continue;
                    }
                    let super_args = match &expr.kind {
                        ExprKind::Call { func, args, .. } => {
                            if let ExprKind::Attr { value, attr } = &func.kind {
                                if attr == "__init__" {
                                    if let ExprKind::Call {
                                        func, args: s_args, ..
                                    } = &value.kind
                                    {
                                        if matches!(&func.kind, ExprKind::Name(n) if n == "super")
                                            && s_args.is_empty()
                                        {
                                            Some(args.as_slice())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(args) = super_args {
                        let base = class_def.base.as_ref().ok_or_else(|| {
                            self.error(stmt.span, "super().__init__ used without base class")
                        })?;
                        let (base_init, param_types) = {
                            let base_def = self.class_defs.get(base).ok_or_else(|| {
                                self.error(stmt.span, format!("Unknown base class: {base}"))
                            })?;
                            let base_init = base_def
                                .methods
                                .iter()
                                .find(|m| m.name == "__init__")
                                .cloned()
                                .ok_or_else(|| {
                                    self.error(stmt.span, "Base class missing __init__")
                                })?;
                            let base_sig = self
                                .ctx
                                .classes
                                .get(base)
                                .and_then(|info| info.init.clone())
                                .ok_or_else(|| {
                                    self.error(stmt.span, "Base class missing __init__ signature")
                                })?;
                            let param_types: Vec<Type> =
                                base_sig.params.into_iter().skip(1).collect();
                            (base_init, param_types)
                        };
                        let full_args = self.resolve_call_args(
                            args,
                            &[],
                            &base_init.params[1..],
                            &param_types,
                            (Some(base), "__init__"),
                            false,
                        )?;
                        let mut bindings = Vec::new();
                        for (idx, arg) in full_args.iter().enumerate() {
                            let tmp = self.new_tmp();
                            let expected = param_types.get(idx);
                            let expr = if let Some(expected) = expected {
                                self.gen_expr_with_expected(arg, Some(expected))?
                            } else {
                                self.gen_expr(arg)?
                            };
                            let expr = self.maybe_clone_list_expr(expr, arg, expected);
                            self.push_line(&format!("let {} = {};", tmp, expr));
                            bindings.push((base_init.params[idx + 1].name.clone(), tmp));
                        }
                        for (name, tmp) in &bindings {
                            self.name_overrides.push((name.clone(), tmp.clone()));
                        }
                        for stmt in &base_init.body {
                            match &stmt.kind {
                                StmtKind::Assign { target, value } => {
                                    if let AssignTarget::Attr { value: obj, attr } = target.as_ref()
                                    {
                                        if matches!(&obj.kind, ExprKind::Name(n) if n == "self") {
                                            self.record_field_init(
                                                &mut field_inits,
                                                class_info,
                                                attr,
                                                value,
                                            )?;
                                        } else if let ExprKind::Name(name) = &obj.kind {
                                            if self.class_attr_global(name, attr).is_some() {
                                                self.emit_simple_assign(
                                                    target,
                                                    value,
                                                    &mut_counts,
                                                    false,
                                                )?;
                                                continue;
                                            }
                                        } else {
                                            return Err(self.error(
                                                stmt.span,
                                                "__init__ may only assign to self fields",
                                            ));
                                        }
                                    } else {
                                        return Err(self.error(
                                            stmt.span,
                                            "__init__ may only assign to self fields",
                                        ));
                                    }
                                }
                                StmtKind::Expr(expr) => {
                                    // Allow None literals and docstrings (string literals).
                                    if matches!(
                                        expr.kind,
                                        ExprKind::Literal(Literal::None | Literal::Str(_))
                                    ) {
                                        continue;
                                    }
                                    return Err(self.error(
                                        stmt.span,
                                        "__init__ may only contain field assignments",
                                    ));
                                }
                                _ => {
                                    return Err(self.error(
                                        stmt.span,
                                        "__init__ may only contain field assignments",
                                    ))
                                }
                            }
                        }
                        for _ in &bindings {
                            self.name_overrides.pop();
                        }
                        continue;
                    }
                    return Err(
                        self.error(stmt.span, "__init__ may only contain field assignments")
                    );
                }
                _ => {
                    return Err(self.error(stmt.span, "__init__ may only contain field assignments"))
                }
            }
        }
        for field in class_info.fields.keys() {
            if !field_inits.contains_key(field) {
                return Err(self.error(
                    init.span,
                    format!("Field {field} not initialized in __init__"),
                ));
            }
        }
        self.push_line(&format!("{} {{", class_def.name));
        self.indent += 1;
        for (field, _) in &class_info.fields {
            // Safe: missing fields are rejected above during __init__ validation.
            let expr = field_inits
                .get(field)
                .expect("field init missing after __init__ validation");
            self.push_line(&format!("{}: {},", field, expr));
        }
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.current_function = prev_current_function;
        self.current_function_ret = prev_current_function_ret;
        self.local_vars = prev_local_vars;
        self.nonlocal_decls = prev_nonlocal_decls;
        self.cell_locals = prev_cell_locals;
        Ok(())
    }

    /// Check whether a statement list contains assignments to `self.<field>`.
    ///
    /// Constructor lowering currently records field values as final expressions for
    /// struct literal construction, so conditional field writes are rejected.
    /// This is a CPython behavior mismatch accepted as a current simplification.
    fn stmts_assign_self_field(stmts: &[Stmt]) -> bool {
        stmts.iter().any(Self::stmt_assigns_self_field)
    }

    /// Recursively detect `self.<field> = ...` writes inside a statement tree.
    fn stmt_assigns_self_field(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Assign { target, .. } => Self::target_assigns_self_field(target),
            StmtKind::If { body, orelse, .. } => {
                Self::stmts_assign_self_field(body) || Self::stmts_assign_self_field(orelse)
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                Self::stmts_assign_self_field(body)
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                Self::stmts_assign_self_field(body)
                    || handlers
                        .iter()
                        .any(|handler| Self::stmts_assign_self_field(&handler.body))
                    || Self::stmts_assign_self_field(orelse)
                    || Self::stmts_assign_self_field(finalbody)
            }
            StmtKind::Match { cases, .. } => cases
                .iter()
                .any(|case| Self::stmts_assign_self_field(&case.body)),
            _ => false,
        }
    }

    /// Recursively detect whether an assignment target writes a `self` field.
    fn target_assigns_self_field(target: &AssignTarget) -> bool {
        match target {
            AssignTarget::Attr { value, .. } => {
                matches!(&value.kind, ExprKind::Name(name) if name == "self")
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                items.iter().any(Self::target_assigns_self_field)
            }
            AssignTarget::Starred(inner) => Self::target_assigns_self_field(inner),
            AssignTarget::Name(_) | AssignTarget::Index { .. } => false,
        }
    }

    /// Record a field initializer expression, applying expected typing and list cloning.
    fn record_field_init(
        &mut self,
        field_inits: &mut HashMap<String, String>,
        class_info: &ClassInfo,
        attr: &str,
        value: &Expr,
    ) -> Result<(), CompileError> {
        let expected = class_info.fields.get(attr);
        let expr = self.gen_expr_with_expected(value, expected)?;
        let expr = self.maybe_clone_list_expr(expr, value, expected);
        field_inits.insert(attr.to_string(), expr);
        Ok(())
    }

    /// Emit a property getter that maps deleted backing-field sentinel to AttributeError.
    fn emit_deletable_int_property_getter(
        &mut self,
        method: &Function,
        field: &str,
    ) -> Result<(), CompileError> {
        self.push_line(&format!(
            "pub fn {}(&self) -> Result<i64, PyError> {{",
            method.name
        ));
        self.indent += 1;
        // CPython-compat divergence:
        // deleted int-backed attributes use `i64::MIN` sentinel in this runtime model.
        self.push_line(&format!("if self.{field} == i64::MIN {{", field = field));
        self.indent += 1;
        self.push_line("return Err(PyError::AttributeError(\"AttributeError\".into()));");
        self.indent -= 1;
        self.push_line("}");
        self.push_line(&format!("Ok(self.{field})", field = field));
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");
        Ok(())
    }

    fn emit_into_iter(
        &mut self,
        class_def: &ClassDef,
        info: &ClassInfo,
    ) -> Result<(), CompileError> {
        if let Some(iter_return) = &info.iter_return {
            let iter_info = self.ctx.classes.get(iter_return).ok_or_else(|| {
                self.error(
                    class_def.span,
                    format!("Unknown iterator class: {iter_return}"),
                )
            })?;
            let item_ty = iter_info
                .next_item
                .as_ref()
                .ok_or_else(|| self.error(class_def.span, "Iterator class missing next()"))?;
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl IntoIterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            self.push_line(&format!("type IntoIter = {};", iter_return));
            self.push_line("fn into_iter(self) -> Self::IntoIter {");
            self.indent += 1;
            self.push_line("self.__iter__()");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        } else if let Some(item_ty) = &info.iter_item {
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl IntoIterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            self.push_line("type IntoIter = Box<dyn Iterator<Item = Self::Item>>;");
            self.push_line("fn into_iter(self) -> Self::IntoIter {");
            self.indent += 1;
            self.push_line("Box::new(self.__iter__())");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        }
        Ok(())
    }
}
