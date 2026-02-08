// Dict key/value-type inference analysis for code generation.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Collect dict key/value hints for a block of statements.
    pub(in crate::codegen) fn collect_dict_kv_types_for_stmts(
        &self,
        stmts: &[Stmt],
    ) -> HashMap<String, (Type, Type)> {
        let mut inferred = HashMap::new();
        self.collect_dict_kv_types_in_stmts(stmts, &mut inferred);
        inferred
    }

    fn merge_hint(existing: &Type, new: &Type) -> Type {
        match (existing, new) {
            (Type::Unknown, ty) => ty.clone(),
            (ty, Type::Unknown) => ty.clone(),
            (left, right) if left == right => left.clone(),
            _ => Type::Unknown,
        }
    }

    fn note_dict_hint(
        &self,
        name: &str,
        key_ty: Type,
        val_ty: Type,
        inferred: &mut HashMap<String, (Type, Type)>,
    ) {
        if matches!(key_ty, Type::Unknown) && matches!(val_ty, Type::Unknown) {
            return;
        }
        inferred
            .entry(name.to_string())
            .and_modify(|(existing_key, existing_val)| {
                *existing_key = Self::merge_hint(existing_key, &key_ty);
                *existing_val = Self::merge_hint(existing_val, &val_ty);
            })
            .or_insert((key_ty, val_ty));
    }

    fn note_dict_assignment(
        &self,
        name: &str,
        value: &Expr,
        inferred: &mut HashMap<String, (Type, Type)>,
    ) {
        if let Some(Type::Dict(key_ty, val_ty)) = value.ty.as_ref() {
            self.note_dict_hint(
                name,
                (*key_ty.as_ref()).clone(),
                (*val_ty.as_ref()).clone(),
                inferred,
            );
        }
    }

    fn collect_dict_kv_types_in_stmts(
        &self,
        stmts: &[Stmt],
        inferred: &mut HashMap<String, (Type, Type)>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    self.note_dict_assignment(name, value, inferred);
                    self.collect_dict_kv_types_in_expr(value, inferred);
                }
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target.as_ref() {
                        self.note_dict_assignment(name, value, inferred);
                    }
                    if let AssignTarget::Index {
                        value: container,
                        index,
                    } = target.as_ref()
                    {
                        if let ExprKind::Name(name) = &container.kind {
                            self.note_dict_hint(
                                name,
                                index.ty.clone().unwrap_or(Type::Unknown),
                                value.ty.clone().unwrap_or(Type::Unknown),
                                inferred,
                            );
                        }
                    }
                    self.collect_dict_kv_types_in_expr(value, inferred);
                }
                StmtKind::Delete { target } => {
                    self.collect_dict_kv_types_in_target(target, inferred);
                }
                StmtKind::Class { def } => {
                    for attr in &def.class_attrs {
                        self.collect_dict_kv_types_in_expr(&attr.value, inferred);
                    }
                    for method in &def.methods {
                        self.collect_dict_kv_types_in_stmts(&method.body, inferred);
                    }
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_dict_kv_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_dict_kv_types_in_expr(test, inferred);
                    self.collect_dict_kv_types_in_stmts(body, inferred);
                    self.collect_dict_kv_types_in_stmts(orelse, inferred);
                }
                StmtKind::While { test, body } => {
                    self.collect_dict_kv_types_in_expr(test, inferred);
                    self.collect_dict_kv_types_in_stmts(body, inferred);
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_dict_kv_types_in_expr(iter, inferred);
                    self.collect_dict_kv_types_in_stmts(body, inferred);
                }
                StmtKind::Expr(expr) => {
                    self.collect_dict_kv_types_in_expr(expr, inferred);
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_dict_kv_types_in_expr(test, inferred);
                    if let Some(expr) = msg {
                        self.collect_dict_kv_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_dict_kv_types_in_expr(subject, inferred);
                    for case in cases {
                        self.collect_dict_kv_types_in_stmts(&case.body, inferred);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_dict_kv_types_in_stmts(body, inferred);
                    for handler in handlers {
                        self.collect_dict_kv_types_in_stmts(&handler.body, inferred);
                    }
                    self.collect_dict_kv_types_in_stmts(orelse, inferred);
                    self.collect_dict_kv_types_in_stmts(finalbody, inferred);
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_dict_kv_types_in_expr(expr, inferred);
                    }
                    if let Some(expr) = cause {
                        self.collect_dict_kv_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::Import { .. }
                | StmtKind::ImportFrom { .. }
                | StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
            }
        }
    }

    fn collect_dict_kv_types_in_target(
        &self,
        target: &AssignTarget,
        inferred: &mut HashMap<String, (Type, Type)>,
    ) {
        match target {
            AssignTarget::Attr { value, .. } => {
                self.collect_dict_kv_types_in_expr(value, inferred);
            }
            AssignTarget::Index { value, index } => {
                self.collect_dict_kv_types_in_expr(value, inferred);
                self.collect_dict_kv_types_in_expr(index, inferred);
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                for item in items {
                    self.collect_dict_kv_types_in_target(item, inferred);
                }
            }
            AssignTarget::Starred(inner) => {
                self.collect_dict_kv_types_in_target(inner, inferred);
            }
            AssignTarget::Name(_) => {}
        }
    }

    fn collect_dict_kv_types_in_expr(
        &self,
        expr: &Expr,
        inferred: &mut HashMap<String, (Type, Type)>,
    ) {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Attr { value, attr } = &func.kind {
                    if let ExprKind::Name(name) = &value.kind {
                        if attr == "setdefault" {
                            let key_ty = args
                                .first()
                                .and_then(|arg| arg.ty.clone())
                                .unwrap_or(Type::Unknown);
                            let val_ty = args
                                .get(1)
                                .and_then(|arg| arg.ty.clone())
                                .unwrap_or(Type::Unknown);
                            self.note_dict_hint(name, key_ty, val_ty, inferred);
                        } else if attr == "update" {
                            if let Some(Type::Dict(key_ty, val_ty)) =
                                args.first().and_then(|arg| arg.ty.as_ref())
                            {
                                self.note_dict_hint(
                                    name,
                                    (*key_ty.as_ref()).clone(),
                                    (*val_ty.as_ref()).clone(),
                                    inferred,
                                );
                            }
                        }
                    }
                }
                self.collect_dict_kv_types_in_expr(func, inferred);
                for arg in args {
                    self.collect_dict_kv_types_in_expr(arg, inferred);
                }
                for kw in keywords {
                    self.collect_dict_kv_types_in_expr(&kw.value, inferred);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_dict_kv_types_in_expr(value, inferred);
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    self.collect_dict_kv_types_in_expr(value, inferred);
                }
            }
            ExprKind::Attr { value, .. } => {
                self.collect_dict_kv_types_in_expr(value, inferred);
            }
            ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                self.collect_dict_kv_types_in_expr(left, inferred);
                self.collect_dict_kv_types_in_expr(right, inferred);
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.collect_dict_kv_types_in_expr(left, inferred);
                for cmp in comparators {
                    self.collect_dict_kv_types_in_expr(cmp, inferred);
                }
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.collect_dict_kv_types_in_expr(inner, inferred);
            }
            ExprKind::BoolOp { values, .. }
            | ExprKind::List(values)
            | ExprKind::Tuple(values)
            | ExprKind::Set(values) => {
                for value in values {
                    self.collect_dict_kv_types_in_expr(value, inferred);
                }
            }
            ExprKind::Dict(items) => {
                for entry in items {
                    match entry {
                        DictEntry::Item { key, value } => {
                            self.collect_dict_kv_types_in_expr(key, inferred);
                            self.collect_dict_kv_types_in_expr(value, inferred);
                        }
                        DictEntry::Unpack { value } => {
                            self.collect_dict_kv_types_in_expr(value, inferred);
                        }
                    }
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_dict_kv_types_in_expr(value, inferred);
                self.collect_dict_kv_types_in_expr(index, inferred);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_dict_kv_types_in_expr(value, inferred);
                if let Some(expr) = start {
                    self.collect_dict_kv_types_in_expr(expr, inferred);
                }
                if let Some(expr) = end {
                    self.collect_dict_kv_types_in_expr(expr, inferred);
                }
                if let Some(expr) = step.as_deref() {
                    self.collect_dict_kv_types_in_expr(expr, inferred);
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_dict_kv_types_in_expr(iter, inferred);
                self.collect_dict_kv_types_in_expr(elt, inferred);
                for cond in ifs {
                    self.collect_dict_kv_types_in_expr(cond, inferred);
                }
            }
            ExprKind::Lambda { body, .. } => {
                self.collect_dict_kv_types_in_expr(body, inferred);
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_dict_kv_types_in_expr(test, inferred);
                self.collect_dict_kv_types_in_expr(body, inferred);
                self.collect_dict_kv_types_in_expr(orelse, inferred);
            }
            ExprKind::Block { stmts } => {
                self.collect_dict_kv_types_in_stmts(stmts, inferred);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_dict_kv_types_in_expr(inner, inferred);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
    }
}
