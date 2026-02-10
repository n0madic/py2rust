// List element-type inference analysis for code generation.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Collect list element type hints for a block of statements.
    pub(in crate::codegen) fn collect_list_elem_types_for_stmts(
        &self,
        stmts: &[Stmt],
    ) -> HashMap<String, Type> {
        let mut inferred = HashMap::new();
        self.collect_list_elem_types_in_stmts(stmts, &mut inferred);
        inferred
    }

    /// Collect list element hints from statement references without cloning.
    pub(in crate::codegen) fn collect_list_elem_types_for_stmt_refs(
        &self,
        stmts: &[&Stmt],
    ) -> HashMap<String, Type> {
        let mut inferred = HashMap::new();
        for stmt in stmts {
            self.collect_list_elem_types_in_stmts(std::slice::from_ref(*stmt), &mut inferred);
        }
        inferred
    }

    /// Walk statements and record list element types inferred from assignments and calls.
    fn collect_list_elem_types_in_stmts(
        &self,
        stmts: &[Stmt],
        inferred: &mut HashMap<String, Type>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    self.note_list_assignment(name, value, inferred);
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target.as_ref() {
                        self.note_list_assignment(name, value, inferred);
                    }
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
                StmtKind::Delete { target } => {
                    self.collect_list_elem_types_in_target(target, inferred);
                }
                StmtKind::Class { def } => {
                    for attr in &def.class_attrs {
                        self.collect_list_elem_types_in_expr(&attr.value, inferred);
                    }
                    for method in &def.methods {
                        self.collect_list_elem_types_in_stmts(&method.body, inferred);
                    }
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_list_elem_types_in_expr(test, inferred);
                    self.collect_list_elem_types_in_stmts(body, inferred);
                    self.collect_list_elem_types_in_stmts(orelse, inferred);
                }
                StmtKind::While { test, body } => {
                    self.collect_list_elem_types_in_expr(test, inferred);
                    self.collect_list_elem_types_in_stmts(body, inferred);
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_list_elem_types_in_expr(iter, inferred);
                    self.collect_list_elem_types_in_stmts(body, inferred);
                }
                StmtKind::Expr(expr) => {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_list_elem_types_in_expr(test, inferred);
                    if let Some(expr) = msg {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_list_elem_types_in_expr(subject, inferred);
                    for case in cases {
                        self.collect_list_elem_types_in_stmts(&case.body, inferred);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_list_elem_types_in_stmts(body, inferred);
                    for handler in handlers {
                        self.collect_list_elem_types_in_stmts(&handler.body, inferred);
                    }
                    self.collect_list_elem_types_in_stmts(orelse, inferred);
                    self.collect_list_elem_types_in_stmts(finalbody, inferred);
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                    if let Some(expr) = cause {
                        self.collect_list_elem_types_in_expr(expr, inferred);
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

    /// Track list element type assignments from direct list expressions.
    fn note_list_assignment(&self, name: &str, value: &Expr, inferred: &mut HashMap<String, Type>) {
        if let Some(Type::List(inner)) = value.ty.as_ref() {
            if !matches!(inner.as_ref(), Type::Unknown) && !inferred.contains_key(name) {
                inferred.insert(name.to_string(), (*inner.as_ref()).clone());
            }
        }
    }

    /// Collect list-type hints from assignment-like targets that carry expressions.
    fn collect_list_elem_types_in_target(
        &self,
        target: &AssignTarget,
        inferred: &mut HashMap<String, Type>,
    ) {
        match target {
            AssignTarget::Attr { value, .. } => {
                self.collect_list_elem_types_in_expr(value, inferred)
            }
            AssignTarget::Index { value, index } => {
                self.collect_list_elem_types_in_expr(value, inferred);
                self.collect_list_elem_types_in_expr(index, inferred);
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                for item in items {
                    self.collect_list_elem_types_in_target(item, inferred);
                }
            }
            AssignTarget::Starred(inner) => {
                self.collect_list_elem_types_in_target(inner, inferred);
            }
            AssignTarget::Name(_) => {}
        }
    }

    /// Walk expressions and record list element types inferred from list method calls.
    fn collect_list_elem_types_in_expr(&self, expr: &Expr, inferred: &mut HashMap<String, Type>) {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Attr { value, attr } = &func.kind {
                    if let ExprKind::Name(name) = &value.kind {
                        let elem_ty = match attr.as_str() {
                            "append" | "index" | "count" => {
                                args.first().and_then(|arg| arg.ty.clone())
                            }
                            "insert" => args.get(1).and_then(|arg| arg.ty.clone()),
                            "extend" => args
                                .first()
                                .and_then(|arg| arg.ty.as_ref())
                                .and_then(|ty| self.iter_item_type_hint(ty)),
                            _ => None,
                        };
                        if let Some(elem_ty) = elem_ty {
                            if !matches!(elem_ty, Type::Unknown) && !inferred.contains_key(name) {
                                inferred.insert(name.clone(), elem_ty);
                            }
                        }
                    }
                }
                self.collect_list_elem_types_in_expr(func, inferred);
                for arg in args {
                    self.collect_list_elem_types_in_expr(arg, inferred);
                }
                for kw in keywords {
                    self.collect_list_elem_types_in_expr(&kw.value, inferred);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_list_elem_types_in_expr(value, inferred);
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
            }
            ExprKind::Attr { value, .. } => {
                self.collect_list_elem_types_in_expr(value, inferred);
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_list_elem_types_in_expr(left, inferred);
                self.collect_list_elem_types_in_expr(right, inferred);
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.collect_list_elem_types_in_expr(inner, inferred);
            }
            ExprKind::Compare { left, right, .. } => {
                self.collect_list_elem_types_in_expr(left, inferred);
                self.collect_list_elem_types_in_expr(right, inferred);
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.collect_list_elem_types_in_expr(left, inferred);
                for cmp in comparators {
                    self.collect_list_elem_types_in_expr(cmp, inferred);
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for value in values {
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_list_elem_types_in_expr(item, inferred);
                }
            }
            ExprKind::Dict(items) => {
                for entry in items {
                    match entry {
                        DictEntry::Item { key, value } => {
                            self.collect_list_elem_types_in_expr(key, inferred);
                            self.collect_list_elem_types_in_expr(value, inferred);
                        }
                        DictEntry::Unpack { value } => {
                            self.collect_list_elem_types_in_expr(value, inferred);
                        }
                    }
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_list_elem_types_in_expr(value, inferred);
                self.collect_list_elem_types_in_expr(index, inferred);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_list_elem_types_in_expr(value, inferred);
                if let Some(expr) = start {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
                if let Some(expr) = end {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
                if let Some(expr) = step.as_deref() {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_list_elem_types_in_expr(iter, inferred);
                self.collect_list_elem_types_in_expr(elt, inferred);
                for cond in ifs {
                    self.collect_list_elem_types_in_expr(cond, inferred);
                }
            }
            ExprKind::Lambda { body, .. } => {
                self.collect_list_elem_types_in_expr(body, inferred);
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_list_elem_types_in_expr(test, inferred);
                self.collect_list_elem_types_in_expr(body, inferred);
                self.collect_list_elem_types_in_expr(orelse, inferred);
            }
            ExprKind::Block { stmts } => {
                self.collect_list_elem_types_in_stmts(stmts, inferred);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_list_elem_types_in_expr(inner, inferred);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
    }
}
