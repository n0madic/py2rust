use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_stmt(
        &mut self,
        stmt: &mut Stmt,
        expected_ret: Option<&TypeRef>,
    ) -> Result<(), CompileError> {
        match &mut stmt.kind {
            StmtKind::Let { name, ann, value } => {
                if name == "__name__" {
                    return Err(self.error(stmt.span, "Assignment to __name__ is not supported"));
                }
                if self.in_function() {
                    if self.is_declared_global(name) {
                        let global_ty = self.ctx.globals.get(name).cloned().ok_or_else(|| {
                            self.error(
                                stmt.span,
                                format!("global `{name}` is not defined at module scope"),
                            )
                        })?;
                        let expected = if let Some(ann) = ann {
                            let ty = self.resolve_type_ref(ann, stmt.span)?;
                            if matches!(ty, Type::Iterator(_)) {
                                return Err(self.error(
                                    stmt.span,
                                    "Iterator[T] is only allowed as a return type",
                                ));
                            }
                            self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                            Some(ty)
                        } else {
                            None
                        };
                        let ty = self.check_expr(value, expected.as_ref().or(Some(&global_ty)))?;
                        if matches!(ty, Type::Unknown) && !matches!(global_ty, Type::Unknown) {
                            return Err(
                                self.error(stmt.span, "Unable to infer type; add annotation")
                            );
                        }
                        self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                        stmt.kind = StmtKind::Assign {
                            target: AssignTarget::Name(name.clone()),
                            value: value.clone(),
                        };
                        return Ok(());
                    }
                    if self.ctx.globals.contains_key(name) {
                        return Err(self.error(
                            stmt.span,
                            format!("Local binding shadows global variable `{name}`"),
                        ));
                    }
                }
                if let ExprKind::Lambda { params, .. } = &value.kind {
                    let placeholder = Type::Lambda {
                        params: vec![Type::Unknown; params.len()],
                        ret: Box::new(Type::Unknown),
                    };
                    self.insert_var(name, placeholder, stmt.span)?;
                    let expected = if let Some(ann) = ann {
                        let ty = self.resolve_type_ref(ann, stmt.span)?;
                        if matches!(ty, Type::Iterator(_)) {
                            return Err(self
                                .error(stmt.span, "Iterator[T] is only allowed as a return type"));
                        }
                        Some(ty)
                    } else {
                        None
                    };
                    let ty = self.check_expr(value, expected.as_ref())?;
                    if let Some(expected) = expected {
                        self.ensure_assignable(&ty, &expected, stmt.span)?;
                        self.insert_var(name, expected, stmt.span)?;
                    } else {
                        if matches!(ty, Type::Unknown) {
                            return Err(
                                self.error(stmt.span, "Unable to infer type; add annotation")
                            );
                        }
                        self.insert_var(name, ty, stmt.span)?;
                    }
                    return Ok(());
                }
                let expected = if let Some(ann) = ann {
                    let ty = self.resolve_type_ref(ann, stmt.span)?;
                    if matches!(ty, Type::Iterator(_)) {
                        return Err(
                            self.error(stmt.span, "Iterator[T] is only allowed as a return type")
                        );
                    }
                    Some(ty)
                } else {
                    None
                };
                let ty = self.check_expr(value, expected.as_ref())?;
                if let Some(expected) = expected {
                    self.ensure_assignable(&ty, &expected, stmt.span)?;
                    self.insert_var(name, expected, stmt.span)?;
                } else {
                    if matches!(ty, Type::Unknown) {
                        return Err(self.error(stmt.span, "Unable to infer type; add annotation"));
                    }
                    self.insert_var(name, ty, stmt.span)?;
                }
            }
            StmtKind::Assign { target, value } => {
                let ty = self.check_expr(value, None)?;
                let mut promote_to_let: Option<(String, Expr)> = None;
                match target {
                    AssignTarget::Name(name) => {
                        if name == "__name__" {
                            return Err(
                                self.error(stmt.span, "Assignment to __name__ is not supported")
                            );
                        }
                        if self.in_function() && self.is_declared_global(name) {
                            let global_ty =
                                self.ctx.globals.get(name).cloned().ok_or_else(|| {
                                    self.error(
                                        stmt.span,
                                        format!("global `{name}` is not defined at module scope"),
                                    )
                                })?;
                            if matches!(ty, Type::Unknown) && !matches!(global_ty, Type::Unknown) {
                                return Err(
                                    self.error(stmt.span, "Unable to infer type; add annotation")
                                );
                            }
                            self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                        } else {
                            if self.in_function() && self.ctx.globals.contains_key(name) {
                                return Err(self.error(
                                    stmt.span,
                                    format!("Local binding shadows global variable `{name}`"),
                                ));
                            }
                            if let Some(existing) = self.lookup_var(name) {
                                self.ensure_assignable(&ty, &existing, stmt.span)?;
                            } else {
                                if matches!(ty, Type::Unknown) {
                                    return Err(self
                                        .error(stmt.span, "Unable to infer type; add annotation"));
                                }
                                promote_to_let = Some((name.clone(), value.clone()));
                                self.insert_var(name, ty, stmt.span)?;
                            }
                        }
                    }
                    AssignTarget::Attr { value: obj, attr } => {
                        let obj_ty = self.check_expr(obj, None)?;
                        if let Type::Custom(class_name) = obj_ty {
                            let class_info =
                                self.ctx.classes.get(&class_name).ok_or_else(|| {
                                    self.error(stmt.span, format!("Unknown class: {class_name}"))
                                })?;
                            let field_ty = class_info.fields.get(attr).ok_or_else(|| {
                                self.error(stmt.span, format!("Unknown field {class_name}.{attr}"))
                            })?;
                            self.ensure_assignable(&ty, field_ty, stmt.span)?;
                        } else {
                            return Err(self.error(
                                stmt.span,
                                "Attribute assignment only allowed on class instances",
                            ));
                        }
                    }
                    AssignTarget::Index {
                        value: container,
                        index,
                    } => {
                        let container_ty = self.check_expr(container, None)?;
                        let index_ty = self.check_expr(index, None)?;
                        match container_ty {
                            Type::List(inner) => {
                                self.ensure_assignable(&index_ty, &Type::Int, stmt.span)?;
                                self.ensure_assignable(&ty, &inner, stmt.span)?;
                            }
                            Type::Dict(key_ty, val_ty) => {
                                self.ensure_assignable(&index_ty, &key_ty, stmt.span)?;
                                self.ensure_assignable(&ty, &val_ty, stmt.span)?;
                            }
                            _ => {
                                return Err(
                                    self.error(stmt.span, "Index assignment requires list or dict")
                                )
                            }
                        }
                    }
                }
                if let Some((name, value)) = promote_to_let {
                    stmt.kind = StmtKind::Let {
                        name,
                        ann: None,
                        value,
                    };
                }
            }
            StmtKind::Return { value } => {
                let ret_ann = expected_ret
                    .ok_or_else(|| self.error(stmt.span, "Return outside of function"))?;
                let expected = self.resolve_type_ref(ret_ann, stmt.span)?;
                if matches!(expected, Type::Unknown) {
                    if let Some(expr) = value {
                        let _ = self.check_expr(expr, None)?;
                    }
                    return Ok(());
                }
                match value {
                    Some(expr) => {
                        let actual = self.check_expr(expr, Some(&expected))?;
                        self.ensure_assignable(&actual, &expected, stmt.span)?;
                    }
                    None => {
                        if !matches!(expected, Type::None) {
                            return Err(self.error(stmt.span, "Return value required"));
                        }
                    }
                }
            }
            StmtKind::If { test, body, orelse } => {
                let cond_ty = self.check_expr(test, Some(&Type::Bool))?;
                self.ensure_assignable(&cond_ty, &Type::Bool, stmt.span)?;
                for stmt in body {
                    self.check_stmt(stmt, expected_ret)?;
                }
                for stmt in orelse {
                    self.check_stmt(stmt, expected_ret)?;
                }
            }
            StmtKind::While { test, body } => {
                let cond_ty = self.check_expr(test, Some(&Type::Bool))?;
                self.ensure_assignable(&cond_ty, &Type::Bool, stmt.span)?;
                for stmt in body {
                    self.check_stmt(stmt, expected_ret)?;
                }
            }
            StmtKind::For { target, iter, body } => {
                let iter_ty = self.check_expr(iter, None)?;
                let item_ty = self.iter_item_type(&iter_ty, stmt.span)?;
                if self.in_function() && self.is_declared_global(target) {
                    let global_ty = self.ctx.globals.get(target).cloned().ok_or_else(|| {
                        self.error(
                            stmt.span,
                            format!("global `{target}` is not defined at module scope"),
                        )
                    })?;
                    self.ensure_assignable(&item_ty, &global_ty, stmt.span)?;
                } else {
                    if self.in_function() && self.ctx.globals.contains_key(target) {
                        return Err(self.error(
                            stmt.span,
                            format!("Local binding shadows global variable `{target}`"),
                        ));
                    }
                    if let Some(existing) = self.lookup_var(target) {
                        self.ensure_assignable(&item_ty, &existing, stmt.span)?;
                    } else {
                        self.insert_var(target, item_ty, stmt.span)?;
                    }
                }
                for stmt in body {
                    self.check_stmt(stmt, expected_ret)?;
                }
            }
            StmtKind::Global { names } => {
                if !self.in_function() {
                    return Err(self.error(stmt.span, "global is only allowed inside functions"));
                }
                for name in names.iter() {
                    if name == "__name__" {
                        return Err(self.error(stmt.span, "global __name__ is not supported"));
                    }
                    self.declare_global(name, stmt.span)?;
                }
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) => {
                self.check_expr(expr, None)?;
            }
            StmtKind::Assert { test, msg } => {
                self.check_expr(test, Some(&Type::Bool))?;
                if let Some(msg) = msg {
                    self.check_expr(msg, Some(&Type::Str))?;
                }
            }
            StmtKind::Match { subject, cases } => {
                let subj_ty = self.check_expr(subject, None)?;
                let union_name = if let Type::Union(name) = subj_ty {
                    name
                } else {
                    return Err(self.error(stmt.span, "match requires a union type"));
                };
                let union_variants = self
                    .ctx
                    .unions
                    .get(&union_name)
                    .ok_or_else(|| self.error(stmt.span, format!("Unknown union: {union_name}")))?
                    .variants
                    .clone();
                for case in &mut *cases {
                    if !union_variants.contains(&case.variant) {
                        return Err(self.error(case.span, "Case variant not in union"));
                    }
                    let fields: Vec<(String, Type)> = self
                        .ctx
                        .classes
                        .get(&case.variant)
                        .ok_or_else(|| {
                            self.error(
                                case.span,
                                format!("Unknown variant class: {}", case.variant),
                            )
                        })?
                        .fields
                        .iter()
                        .map(|(name, ty)| (name.clone(), ty.clone()))
                        .collect();
                    if fields.len() != case.bindings.len() {
                        return Err(
                            self.error(case.span, "Case binding count does not match fields")
                        );
                    }
                    self.scopes.push(HashMap::new());
                    for (binding, (_, field_ty)) in case.bindings.iter().zip(fields.iter()) {
                        if let Some(existing) = self.lookup_var(binding) {
                            self.ensure_assignable(field_ty, &existing, case.span)?;
                        } else {
                            self.insert_var(binding, field_ty.clone(), case.span)?;
                        }
                    }
                    for stmt in &mut case.body {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                    self.scopes.pop();
                }
                // Check for match exhaustiveness
                let covered: HashSet<&String> = cases.iter().map(|c| &c.variant).collect();
                let missing: Vec<&String> = union_variants
                    .iter()
                    .filter(|v| !covered.contains(v))
                    .collect();
                if !missing.is_empty() {
                    let missing_str = missing
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(self.error(
                        stmt.span,
                        format!("non-exhaustive match: missing variants {}", missing_str),
                    ));
                }
            }
        }
        Ok(())
    }
}
