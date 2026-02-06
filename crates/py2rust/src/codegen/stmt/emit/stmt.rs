// General statement emission and control-flow helpers.

use super::super::super::util::{collect_assign_counts, mut_kw_for_name};
use super::super::super::*;

impl<'a> Codegen<'a> {
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

    fn gen_condition_expr(&mut self, test: &Expr) -> Result<String, CompileError> {
        let test_expr = self.gen_expr(test)?;
        let rendered = match test.ty.as_ref() {
            Some(Type::Bool) => test_expr,
            Some(Type::None) => "false".to_string(),
            Some(Type::Option(inner)) => match inner.as_ref() {
                // Option truthiness follows Python semantics: Some(v) is truthy
                // only when v itself is truthy.
                Type::Bool => format!("{}.as_ref().is_some_and(|v| *v)", test_expr),
                Type::Int => format!("{}.as_ref().is_some_and(|v| *v != 0)", test_expr),
                Type::Float => format!("{}.as_ref().is_some_and(|v| *v != 0.0)", test_expr),
                Type::Str => format!("{}.as_ref().is_some_and(|v| !v.is_empty())", test_expr),
                Type::List(_) => format!(
                    "{}.as_ref().is_some_and(|v| !v.lock().expect(\"list mutex poisoned\").is_empty())",
                    test_expr
                ),
                Type::Dict(_, _) => format!(
                    "{}.as_ref().is_some_and(|v| !v.lock().expect(\"dict mutex poisoned\").is_empty())",
                    test_expr
                ),
                Type::Set(_) => format!("{}.as_ref().is_some_and(|v| !v.is_empty())", test_expr),
                Type::Tuple(items) => {
                    if items.is_empty() {
                        "false".to_string()
                    } else {
                        format!("{}.is_some()", test_expr)
                    }
                }
                Type::None => "false".to_string(),
                _ => format!("{}.is_some()", test_expr),
            },
            Some(Type::Int) => format!("({} != 0)", test_expr),
            Some(Type::Float) => format!("({} != 0.0)", test_expr),
            Some(Type::Str) => format!("!{}.is_empty()", test_expr),
            Some(Type::List(_)) => {
                if matches!(self.list_storage_for_expr(test), ListStorage::Local) {
                    format!("!{}.is_empty()", test_expr)
                } else {
                    format!(
                        "!{}.lock().expect(\"list mutex poisoned\").is_empty()",
                        test_expr
                    )
                }
            }
            Some(Type::Dict(_, _)) => {
                if matches!(self.dict_storage_for_expr(test), DictStorage::Local) {
                    format!("!{}.is_empty()", test_expr)
                } else {
                    format!(
                        "!{}.lock().expect(\"dict mutex poisoned\").is_empty()",
                        test_expr
                    )
                }
            }
            Some(Type::Set(_)) => format!("!{}.is_empty()", test_expr),
            Some(Type::Tuple(items)) => {
                if items.is_empty() {
                    "false".to_string()
                } else {
                    "true".to_string()
                }
            }
            _ => test_expr,
        };
        Ok(rendered)
    }

    /// Detect Optional narrowing implied by a condition expression.
    ///
    /// Returns `(name, then_ty, else_ty)` when a simple Optional narrowing applies.
    fn optional_narrowing_from_test(&self, test: &Expr) -> Option<(String, Type, Type)> {
        let lookup_optional_inner = |name: &str| -> Option<Type> {
            let var_ty = self.local_var_type(name).cloned().or_else(|| {
                if self.is_global(name) {
                    self.ctx.globals.get(name).cloned()
                } else {
                    None
                }
            })?;
            if let Type::Option(inner) = var_ty {
                Some(*inner)
            } else {
                None
            }
        };

        let narrow_none_compare =
            |name_expr: &Expr, none_expr: &Expr, op: &CmpOp| -> Option<(String, Type, Type)> {
                if !matches!(none_expr.kind, ExprKind::Literal(Literal::None)) {
                    return None;
                }
                let ExprKind::Name(name) = &name_expr.kind else {
                    return None;
                };
                let inner_ty = lookup_optional_inner(name)?;
                match op {
                    CmpOp::IsNot => Some((name.clone(), inner_ty, Type::None)),
                    CmpOp::Is => Some((name.clone(), Type::None, inner_ty)),
                    _ => None,
                }
            };

        if let ExprKind::Compare { op, left, right } = &test.kind {
            let (name_expr, none_expr) = match (&left.kind, &right.kind) {
                (ExprKind::Name(_), ExprKind::Literal(Literal::None)) => {
                    (left.as_ref(), right.as_ref())
                }
                (ExprKind::Literal(Literal::None), ExprKind::Name(_)) => {
                    (right.as_ref(), left.as_ref())
                }
                _ => (left.as_ref(), right.as_ref()),
            };
            if let Some(narrowed) = narrow_none_compare(name_expr, none_expr, op) {
                return Some(narrowed);
            }
        }

        if let ExprKind::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } = &test.kind
        {
            if let ExprKind::Compare { op, left, right } = &inner.kind {
                let inverted = match op {
                    CmpOp::Is => Some(CmpOp::IsNot),
                    CmpOp::IsNot => Some(CmpOp::Is),
                    _ => None,
                }?;
                let (name_expr, none_expr) = match (&left.kind, &right.kind) {
                    (ExprKind::Name(_), ExprKind::Literal(Literal::None)) => {
                        (left.as_ref(), right.as_ref())
                    }
                    (ExprKind::Literal(Literal::None), ExprKind::Name(_)) => {
                        (right.as_ref(), left.as_ref())
                    }
                    _ => (left.as_ref(), right.as_ref()),
                };
                if let Some(narrowed) = narrow_none_compare(name_expr, none_expr, &inverted) {
                    return Some(narrowed);
                }
            }
        }

        if let ExprKind::Name(name) = &test.kind {
            if let Some(inner_ty) = lookup_optional_inner(name) {
                return Some((
                    name.clone(),
                    inner_ty.clone(),
                    Type::Option(Box::new(inner_ty)),
                ));
            }
        }

        if let ExprKind::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } = &test.kind
        {
            if let ExprKind::Name(name) = &inner.kind {
                if let Some(inner_ty) = lookup_optional_inner(name) {
                    return Some((
                        name.clone(),
                        Type::Option(Box::new(inner_ty.clone())),
                        inner_ty,
                    ));
                }
            }
        }

        None
    }

    /// Build a name override expression that unwraps an Optional variable.
    fn optional_unwrap_override_expr(&self, name: &str, narrowed_ty: &Type) -> Option<String> {
        let var_ty = self.local_var_type(name).cloned().or_else(|| {
            if self.is_global(name) {
                self.ctx.globals.get(name).cloned()
            } else {
                None
            }
        })?;
        let Type::Option(inner) = var_ty else {
            return None;
        };
        if inner.as_ref() != narrowed_ty {
            return None;
        }
        let base = if let Some(override_expr) = self.name_override(name) {
            override_expr.to_string()
        } else if self.is_cell_local(name) || self.is_nonlocal_decl(name) {
            format!("{}.borrow().clone()", name)
        } else if self.is_global(name) {
            format!("{}.clone()", self.global_lock_expr(name))
        } else {
            name.to_string()
        };
        Some(format!(
            "({}).as_ref().expect(\"optional value '{}' is None\").clone()",
            base, name
        ))
    }

    /// Emit a list of statements with an optional variable-read override.
    fn emit_stmts_with_optional_override(
        &mut self,
        narrowed: Option<(&str, &Type)>,
        stmts: &[Stmt],
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        if let Some((name, ty)) = narrowed {
            if let Some(override_expr) = self.optional_unwrap_override_expr(name, ty) {
                return self.with_name_override(name, override_expr, |this| {
                    for stmt in stmts {
                        this.emit_stmt(stmt, mut_counts)?;
                    }
                    Ok(())
                });
            }
        }
        for stmt in stmts {
            self.emit_stmt(stmt, mut_counts)?;
        }
        Ok(())
    }

    /// Emit a statement into the output buffer.
    pub(crate) fn emit_stmt(
        &mut self,
        stmt: &Stmt,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Let { name, ann, value } => {
                // Function-local Let bindings should always stay local; global writes
                // inside functions are normalized to Assign during type checking.
                if self.current_function.is_none() && self.is_global(name) {
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
                    let declared_is_optional_collection = matches!(
                        declared.as_ref(),
                        Some(Type::Option(inner))
                            if matches!(inner.as_ref(), Type::List(_) | Type::Dict(_, _))
                    );
                    let (expr, used_collection_fast_path) = if !declared_is_optional_collection {
                        if let Some(local_expr) = self.gen_list_assignment_expr(name, value)? {
                            (local_expr, true)
                        } else if let Some(local_expr) =
                            self.gen_dict_assignment_expr(name, value)?
                        {
                            (local_expr, true)
                        } else {
                            let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                            (
                                self.maybe_clone_list_expr(
                                    expr,
                                    value.ty.as_ref(),
                                    declared.as_ref(),
                                ),
                                false,
                            )
                        }
                    } else {
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        (
                            self.maybe_clone_list_expr(expr, value.ty.as_ref(), declared.as_ref()),
                            false,
                        )
                    };
                    let expr = if let Some(Type::Option(inner)) = declared.as_ref() {
                        // Keep Option wrapping when collection assignment used the
                        // specialized fast path (which returns bare collection values).
                        if matches!(inner.as_ref(), Type::List(_) | Type::Dict(_, _))
                            && used_collection_fast_path
                        {
                            format!("Some({})", expr)
                        } else {
                            expr
                        }
                    } else {
                        expr
                    };
                    let expr = format!("Rc::new(RefCell::new({}))", expr);
                    let mut_kw = mut_kw_for_name(name, mut_counts);
                    if let Some(declared) = declared.clone() {
                        // Wide inline unions resolve to Unknown during annotation lowering;
                        // only for those annotations we refine from the RHS.
                        let union_ann_unknown =
                            ann.as_ref().is_some_and(|a| matches!(a, TypeRef::Union(_)));
                        let declared = if matches!(declared, Type::Unknown) && union_ann_unknown {
                            value.ty.clone().unwrap_or(declared)
                        } else {
                            declared
                        };
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
                                StmtKind::Import { .. }
                                | StmtKind::ImportFrom { .. }
                                | StmtKind::Global { .. }
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
                let declared_is_optional_collection = matches!(
                    declared.as_ref(),
                    Some(Type::Option(inner))
                        if matches!(inner.as_ref(), Type::List(_) | Type::Dict(_, _))
                );
                let (expr, used_collection_fast_path) = if !declared_is_optional_collection {
                    if let Some(local_expr) = self.gen_list_assignment_expr(name, value)? {
                        (local_expr, true)
                    } else if let Some(local_expr) = self.gen_dict_assignment_expr(name, value)? {
                        (local_expr, true)
                    } else {
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        (
                            self.maybe_clone_list_expr(expr, value.ty.as_ref(), declared.as_ref()),
                            false,
                        )
                    }
                } else {
                    let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                    (
                        self.maybe_clone_list_expr(expr, value.ty.as_ref(), declared.as_ref()),
                        false,
                    )
                };
                let expr = if let Some(Type::Option(inner)) = declared.as_ref() {
                    // Keep Option wrapping when collection assignment used the
                    // specialized fast path (which returns bare collection values).
                    if matches!(inner.as_ref(), Type::List(_) | Type::Dict(_, _))
                        && used_collection_fast_path
                    {
                        format!("Some({})", expr)
                    } else {
                        expr
                    }
                } else {
                    expr
                };
                let mut_kw = mut_kw_for_name(name, mut_counts);
                if ann.is_some() {
                    let ty = declared.expect("resolved above");
                    // Wide inline unions resolve to Unknown during annotation lowering;
                    // only for those annotations we refine from the RHS.
                    let union_ann_unknown =
                        ann.as_ref().is_some_and(|a| matches!(a, TypeRef::Union(_)));
                    let ty = if matches!(ty, Type::Unknown) && union_ann_unknown {
                        value.ty.clone().unwrap_or(ty)
                    } else {
                        ty
                    };
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
                let try_uses_option = self.try_block_returns_option;

                // Inside try blocks with value returns, wrap as `Ok(Some(..))` so
                // non-returning branches can still fall through via `Ok(None)`.
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
                    if in_try_with_value && try_uses_option {
                        self.push_line(&format!("return Ok(Some({}));", expr_str));
                    } else if wrap_in_ok {
                        self.push_line(&format!("return Ok({});", expr_str));
                    } else {
                        self.push_line(&format!("return {};", expr_str));
                    }
                } else if wrap_in_ok {
                    if in_try_with_value && try_uses_option {
                        self.push_line("return Ok(None);");
                    } else {
                        self.push_line("return Ok(());");
                    }
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
                            let test_expr = self.gen_condition_expr(test)?;
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
                let narrowed = self.optional_narrowing_from_test(test);
                let test_expr = self.gen_condition_expr(test)?;
                self.push_line(&format!("if {} {{", test_expr));
                self.indent += 1;
                let true_narrow = narrowed
                    .as_ref()
                    .map(|(name, true_ty, _)| (name.as_str(), true_ty));
                self.emit_stmts_with_optional_override(true_narrow, body, mut_counts)?;
                self.indent -= 1;
                if orelse.is_empty() {
                    self.push_line("}");
                } else {
                    self.push_line("} else {");
                    self.indent += 1;
                    let false_narrow = narrowed
                        .as_ref()
                        .map(|(name, _, false_ty)| (name.as_str(), false_ty));
                    self.emit_stmts_with_optional_override(false_narrow, orelse, mut_counts)?;
                    self.indent -= 1;
                    self.push_line("}");
                }
            }
            StmtKind::While { test, body } => {
                let test_expr = self.gen_condition_expr(test)?;
                self.push_line(&format!("while {} {{", test_expr));
                self.indent += 1;
                let narrowed: Option<(String, Type)> = if let ExprKind::Name(name) = &test.kind {
                    self.local_var_type(name).and_then(|ty| {
                        if let Type::Option(inner) = ty {
                            Some((name.clone(), inner.as_ref().clone()))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
                let narrowed_ref = narrowed.as_ref().map(|(name, ty)| (name.as_str(), ty));
                self.emit_stmts_with_optional_override(narrowed_ref, body, mut_counts)?;
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
            StmtKind::Import { .. }
            | StmtKind::ImportFrom { .. }
            | StmtKind::Global { .. }
            | StmtKind::Nonlocal { .. } => {}
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
}
