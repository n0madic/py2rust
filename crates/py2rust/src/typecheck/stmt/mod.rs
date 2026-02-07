use super::*;
use crate::hir_visit::StmtVisitorMut;
use crate::stdlib::registry::{find_imported_member, method_spec, resolve_module};

mod exceptions;
mod unpack;

/// Mutable statement visitor that reuses the legacy statement checker logic.
///
/// The visitor is responsible for top-level `StmtKind` dispatch so that
/// `TypeChecker::check_stmt` no longer has a large manual `match`.
struct CheckStmtVisitor<'tc, 'a, 'e> {
    tc: &'tc mut TypeChecker<'a>,
    expected_ret: Option<&'e TypeRef>,
    span: Span,
    rewrite: Option<StmtKind>,
}

impl<'tc, 'a, 'e> CheckStmtVisitor<'tc, 'a, 'e> {
    /// Run legacy statement checking for one cloned statement shape and keep the full rewrite.
    fn check_and_rewrite(&mut self, kind: StmtKind) -> Result<(), CompileError> {
        let mut tmp = Stmt {
            kind,
            span: self.span,
        };
        self.tc.check_stmt_via_match(&mut tmp, self.expected_ret)?;
        // Always replace the caller statement with the checked/rewritten kind.
        self.rewrite = Some(tmp.kind);
        Ok(())
    }
}

impl<'tc, 'a, 'e> StmtVisitorMut<Result<(), CompileError>> for CheckStmtVisitor<'tc, 'a, 'e> {
    fn visit_let_mut(
        &mut self,
        name: &mut String,
        ann: &mut Option<TypeRef>,
        value: &mut Expr,
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Let {
            name: name.clone(),
            ann: ann.clone(),
            value: value.clone(),
        })
    }

    fn visit_assign_mut(
        &mut self,
        target: &mut AssignTarget,
        value: &mut Expr,
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Assign {
            target: target.clone(),
            value: value.clone(),
        })
    }

    fn visit_return_mut(&mut self, value: &mut Option<Expr>) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Return {
            value: value.clone(),
        })
    }

    fn visit_if_mut(
        &mut self,
        test: &mut Expr,
        body: &mut [Stmt],
        orelse: &mut [Stmt],
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::If {
            test: test.clone(),
            body: body.to_vec(),
            orelse: orelse.to_vec(),
        })
    }

    fn visit_while_mut(&mut self, test: &mut Expr, body: &mut [Stmt]) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::While {
            test: test.clone(),
            body: body.to_vec(),
        })
    }

    fn visit_for_mut(
        &mut self,
        target: &mut ForTarget,
        iter: &mut Expr,
        body: &mut [Stmt],
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::For {
            target: target.clone(),
            iter: iter.clone(),
            body: body.to_vec(),
        })
    }

    fn visit_import_mut(&mut self, names: &mut [ImportBinding]) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Import {
            names: names.to_vec(),
        })
    }

    fn visit_import_from_mut(
        &mut self,
        module: &mut String,
        names: &mut [ImportFromBinding],
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::ImportFrom {
            module: module.clone(),
            names: names.to_vec(),
        })
    }

    fn visit_global_mut(&mut self, names: &mut [String]) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Global {
            names: names.to_vec(),
        })
    }

    fn visit_nonlocal_mut(&mut self, names: &mut [String]) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Nonlocal {
            names: names.to_vec(),
        })
    }

    fn visit_break_mut(&mut self) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Break)
    }

    fn visit_continue_mut(&mut self) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Continue)
    }

    fn visit_expr_stmt_mut(&mut self, expr: &mut Expr) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Expr(expr.clone()))
    }

    fn visit_assert_mut(
        &mut self,
        test: &mut Expr,
        msg: &mut Option<Expr>,
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Assert {
            test: test.clone(),
            msg: msg.clone(),
        })
    }

    fn visit_match_mut(
        &mut self,
        subject: &mut Expr,
        cases: &mut [MatchCase],
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Match {
            subject: subject.clone(),
            cases: cases.to_vec(),
        })
    }

    fn visit_try_mut(
        &mut self,
        body: &mut [Stmt],
        handlers: &mut [ExceptHandler],
        orelse: &mut [Stmt],
        finalbody: &mut [Stmt],
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Try {
            body: body.to_vec(),
            handlers: handlers.to_vec(),
            orelse: orelse.to_vec(),
            finalbody: finalbody.to_vec(),
        })
    }

    fn visit_raise_mut(
        &mut self,
        exc: &mut Option<Expr>,
        cause: &mut Option<Expr>,
    ) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Raise {
            exc: exc.clone(),
            cause: cause.clone(),
        })
    }
}

/// Statement type checking.
///
/// Statements are the imperative building blocks that modify state.
/// Key responsibilities:
/// 1. Type check variable assignments (Let and Assign)
/// 2. Validate control flow (if/while/for/match)
/// 3. Check return statements against function signature
/// 4. Handle exception statements (try/except/raise)
/// 5. Transform Let to Assign for global variables
///
/// Design decisions:
/// - Let creates new variable, Assign modifies existing
/// - Global variables in functions are detected and transformed to Assign
/// - Lambda assignments need forward declaration for recursion
/// - Return type must match function signature
/// - Iterator[T] is only allowed as return type, not variable type
impl<'a> TypeChecker<'a> {
    /// Type check a statement.
    ///
    /// expected_ret is the function's return type annotation, used to
    /// validate return statements.
    pub(super) fn check_stmt(
        &mut self,
        stmt: &mut Stmt,
        expected_ret: Option<&TypeRef>,
    ) -> Result<(), CompileError> {
        let mut visitor = CheckStmtVisitor {
            tc: self,
            expected_ret,
            span: stmt.span,
            rewrite: None,
        };
        stmt.accept_mut(&mut visitor)?;
        if let Some(new_kind) = visitor.rewrite {
            stmt.kind = new_kind;
        }
        Ok(())
    }

    /// Legacy statement checker implementation kept as the semantic source of truth.
    fn check_stmt_via_match(
        &mut self,
        stmt: &mut Stmt,
        expected_ret: Option<&TypeRef>,
    ) -> Result<(), CompileError> {
        match &mut stmt.kind {
            StmtKind::Let { name, ann, value } => {
                if name == "__name__" {
                    return Err(self.error(stmt.span, "Assignment to __name__ is not supported"));
                }
                if self.in_function() && self.is_declared_nonlocal(name) {
                    let outer_ty = self
                        .lookup_nonlocal_var(name)
                        .ok_or_else(|| self.error(stmt.span, "nonlocal binding not found"))?;
                    let expected = if let Some(ann) = ann {
                        let ty = self.resolve_type_ref(ann, stmt.span)?;
                        if matches!(ty, Type::Iterator(_)) {
                            return Err(self
                                .error(stmt.span, "Iterator[T] is only allowed as a return type"));
                        }
                        self.ensure_assignable(&ty, &outer_ty, stmt.span)?;
                        Some(ty)
                    } else {
                        None
                    };
                    let ty = self.check_expr(value, expected.as_ref().or(Some(&outer_ty)))?;
                    if matches!(ty, Type::Unknown) && !matches!(outer_ty, Type::Unknown) {
                        return Err(self.error(stmt.span, "Unable to infer type; add annotation"));
                    }
                    self.ensure_assignable(&ty, &outer_ty, stmt.span)?;
                    stmt.kind = StmtKind::Assign {
                        target: AssignTarget::Name(name.clone()),
                        value: value.clone(),
                    };
                    return Ok(());
                }
                if self.in_function() && self.is_declared_global(name) {
                    let global_ty = self.ctx.globals.get(name).cloned().ok_or_else(|| {
                        self.error(
                            stmt.span,
                            format!("global `{name}` is not defined at module scope"),
                        )
                    })?;
                    let expected = if let Some(ann) = ann {
                        let ty = self.resolve_type_ref(ann, stmt.span)?;
                        if matches!(ty, Type::Iterator(_)) {
                            return Err(self
                                .error(stmt.span, "Iterator[T] is only allowed as a return type"));
                        }
                        self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                        Some(ty)
                    } else {
                        None
                    };
                    let ty = self.check_expr(value, expected.as_ref().or(Some(&global_ty)))?;
                    if matches!(ty, Type::Unknown) && !matches!(global_ty, Type::Unknown) {
                        return Err(self.error(stmt.span, "Unable to infer type; add annotation"));
                    }
                    self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                    stmt.kind = StmtKind::Assign {
                        target: AssignTarget::Name(name.clone()),
                        value: value.clone(),
                    };
                    return Ok(());
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
                    let declared = if let Some(expected) = expected {
                        self.ensure_assignable(&ty, &expected, stmt.span)?;
                        // Keep explicit annotation constraints, but allow inferred lambda
                        // details to fill Unknown parts from unannotated nested defs.
                        Self::merge_types(expected, ty.clone())
                    } else {
                        if matches!(ty, Type::Unknown) {
                            return Err(
                                self.error(stmt.span, "Unable to infer type; add annotation")
                            );
                        }
                        ty
                    };
                    self.insert_var(name, declared, stmt.span)?;
                    if !self.in_function() {
                        self.lambda_defs.insert(name.clone(), value.clone());
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
                    // Tuple annotations with homogeneous element types accept any length.
                    // Store the actual tuple length to avoid codegen length mismatches.
                    let declared = if let (Type::Tuple(exp_items), Type::Tuple(actual_items)) =
                        (&expected, &ty)
                    {
                        if exp_items.len() != actual_items.len()
                            && exp_items
                                .first()
                                .is_some_and(|first| exp_items.iter().all(|t| t == first))
                        {
                            Type::Tuple(actual_items.clone())
                        } else {
                            expected.clone()
                        }
                    } else {
                        expected.clone()
                    };
                    // Preserve explicit annotation constraints, but allow Unknown annotations
                    // (for example, wide inline unions) to refine from the initializer.
                    let declared = Self::merge_types(declared, ty.clone());
                    self.insert_var(name, declared, stmt.span)?;
                } else {
                    if matches!(ty, Type::Unknown) {
                        return Err(self.error(stmt.span, "Unable to infer type; add annotation"));
                    }
                    self.insert_var(name, ty, stmt.span)?;
                }
            }
            StmtKind::Assign { target, value } => {
                let mut ty = self.check_expr(value, None)?;
                if matches!(target, AssignTarget::Tuple(_) | AssignTarget::List(_)) {
                    // Destructuring assignment: validate each leaf target against element types.
                    self.check_unpack_target(target, &ty, Some(value), stmt.span)?;
                } else {
                    let mut promote_to_let: Option<(String, Expr)> = None;
                    match target {
                        AssignTarget::Name(name) => {
                            if name == "__name__" {
                                return Err(self
                                    .error(stmt.span, "Assignment to __name__ is not supported"));
                            }
                            if self.in_function() && self.is_declared_nonlocal(name) {
                                let outer_ty = self.lookup_nonlocal_var(name).ok_or_else(|| {
                                    self.error(stmt.span, "nonlocal binding not found")
                                })?;
                                if ty.contains_unknown() && !outer_ty.contains_unknown() {
                                    ty = self.check_expr(value, Some(&outer_ty))?;
                                }
                                if ty.contains_unknown() && !outer_ty.contains_unknown() {
                                    return Err(self
                                        .error(stmt.span, "Unable to infer type; add annotation"));
                                }
                                self.ensure_assignable(&ty, &outer_ty, stmt.span)?;
                            } else if self.in_function() && self.is_declared_global(name) {
                                let global_ty =
                                    self.ctx.globals.get(name).cloned().ok_or_else(|| {
                                        self.error(
                                            stmt.span,
                                            format!(
                                                "global `{name}` is not defined at module scope"
                                            ),
                                        )
                                    })?;
                                if ty.contains_unknown() && !global_ty.contains_unknown() {
                                    ty = self.check_expr(value, Some(&global_ty))?;
                                }
                                if ty.contains_unknown() && !global_ty.contains_unknown() {
                                    return Err(self
                                        .error(stmt.span, "Unable to infer type; add annotation"));
                                }
                                self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                            } else if self.in_function()
                                && !self.is_declared_global(name)
                                && !self.is_declared_nonlocal(name)
                            {
                                if let Some(existing) = self.lookup_local_var(name) {
                                    if ty.contains_unknown() && !existing.contains_unknown() {
                                        ty = self.check_expr(value, Some(&existing))?;
                                    }
                                    if ty.contains_unknown() && !existing.contains_unknown() {
                                        return Err(self.error(
                                            stmt.span,
                                            "Unable to infer type; add annotation",
                                        ));
                                    }
                                    self.ensure_assignable(&ty, &existing, stmt.span)?;
                                } else {
                                    if matches!(ty, Type::Unknown) {
                                        return Err(self.error(
                                            stmt.span,
                                            "Unable to infer type; add annotation",
                                        ));
                                    }
                                    promote_to_let = Some((name.clone(), value.clone()));
                                    self.insert_var(name, ty, stmt.span)?;
                                }
                            } else if let Some(existing) = self.lookup_var(name) {
                                if ty.contains_unknown() && !existing.contains_unknown() {
                                    ty = self.check_expr(value, Some(&existing))?;
                                }
                                if ty.contains_unknown() && !existing.contains_unknown() {
                                    return Err(self
                                        .error(stmt.span, "Unable to infer type; add annotation"));
                                }
                                self.ensure_assignable(&ty, &existing, stmt.span)?;
                            } else {
                                if matches!(ty, Type::Unknown) {
                                    return Err(self
                                        .error(stmt.span, "Unable to infer type; add annotation"));
                                }
                                promote_to_let = Some((name.clone(), value.clone()));
                                self.insert_var(name, ty, stmt.span)?;
                            }
                            if !self.in_function() && matches!(value.kind, ExprKind::Lambda { .. })
                            {
                                self.lambda_defs.insert(name.clone(), value.clone());
                            }
                        }
                        AssignTarget::Attr { value: obj, attr } => {
                            let obj_ty = self.check_expr(obj, None)?;
                            if let ExprKind::Name(name) = &obj.kind {
                                if let Some(class_info) = self.ctx.classes.get(name) {
                                    if let Some(attr_info) = class_info.class_attrs.get(attr) {
                                        self.ensure_assignable(&ty, &attr_info.ty, stmt.span)?;
                                        return Ok(());
                                    }
                                }
                            }
                            if let Type::Custom(class_name) = obj_ty {
                                let class_info =
                                    self.ctx.classes.get(&class_name).ok_or_else(|| {
                                        self.error(
                                            stmt.span,
                                            format!("Unknown class: {class_name}"),
                                        )
                                    })?;
                                if let Some(prop) = class_info.properties.get(attr) {
                                    if let Some(setter_name) = &prop.setter {
                                        if let Some(sig) = class_info.methods.get(setter_name) {
                                            if sig.params.len() >= 2 {
                                                let expected = sig.params[1].clone();
                                                self.ensure_assignable(&ty, &expected, stmt.span)?;
                                            }
                                            return Ok(());
                                        }
                                    }
                                    return Err(self.error(
                                        stmt.span,
                                        format!("Property {class_name}.{attr} has no setter"),
                                    ));
                                }
                                let field_ty = class_info.fields.get(attr).ok_or_else(|| {
                                    self.error(
                                        stmt.span,
                                        format!("Unknown field {class_name}.{attr}"),
                                    )
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
                                    return Err(self.error(
                                        stmt.span,
                                        "Index assignment requires list or dict",
                                    ))
                                }
                            }
                        }
                        AssignTarget::Tuple(_) | AssignTarget::List(_) => {}
                        AssignTarget::Starred(_) => return Err(self.error(
                            stmt.span,
                            "Starred assignment target is only valid inside tuple/list unpacking",
                        )),
                    }
                    if let Some((name, value)) = promote_to_let {
                        stmt.kind = StmtKind::Let {
                            name,
                            ann: None,
                            value,
                        };
                    }
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
                // Python if-conditions use truthiness, not strict bool typing.
                let _cond_ty = self.check_expr(test, Some(&Type::Bool))?;
                let mut narrowed: Option<(String, Type, Type)> = None;
                // Optional narrowing for identity and truthiness checks.
                let narrow_none_compare = |name_expr: &Expr,
                                           none_expr: &Expr,
                                           op: &CmpOp|
                 -> Option<(String, Type, Type)> {
                    if let ExprKind::Name(name) = &name_expr.kind {
                        if matches!(none_expr.kind, ExprKind::Literal(Literal::None)) {
                            if let Some(orig_ty) = self.lookup_var(name) {
                                if let Type::Option(inner) = orig_ty.clone() {
                                    let some_ty = *inner.clone();
                                    let none_ty = Type::None;
                                    match op {
                                        CmpOp::IsNot => {
                                            return Some((name.clone(), some_ty, none_ty));
                                        }
                                        CmpOp::Is => {
                                            return Some((name.clone(), none_ty, some_ty));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    None
                };

                if let ExprKind::Compare { op, left, right } = &test.kind {
                    let (name_expr, none_expr) = match (&left.kind, &right.kind) {
                        (ExprKind::Name(_), ExprKind::Literal(Literal::None)) => (left, right),
                        (ExprKind::Literal(Literal::None), ExprKind::Name(_)) => (right, left),
                        _ => (left, right),
                    };
                    narrowed = narrow_none_compare(name_expr, none_expr, op);
                }
                if narrowed.is_none() {
                    if let ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: inner,
                    } = &test.kind
                    {
                        if let ExprKind::Compare { op, left, right } = &inner.kind {
                            // `not (x is None)` and `not (x is not None)` invert branches.
                            let inverted = match op {
                                CmpOp::Is => Some(CmpOp::IsNot),
                                CmpOp::IsNot => Some(CmpOp::Is),
                                _ => None,
                            };
                            if let Some(inverted) = inverted.as_ref() {
                                let (name_expr, none_expr) = match (&left.kind, &right.kind) {
                                    (ExprKind::Name(_), ExprKind::Literal(Literal::None)) => {
                                        (left, right)
                                    }
                                    (ExprKind::Literal(Literal::None), ExprKind::Name(_)) => {
                                        (right, left)
                                    }
                                    _ => (left, right),
                                };
                                narrowed = narrow_none_compare(name_expr, none_expr, inverted);
                            }
                        }
                    }
                }
                if narrowed.is_none() {
                    if let ExprKind::Name(name) = &test.kind {
                        if let Some(Type::Option(inner)) = self.lookup_var(name) {
                            // Truthy branch excludes None; falsy branch keeps the original union.
                            narrowed = Some((name.clone(), *inner.clone(), Type::Option(inner)));
                        }
                    }
                }
                if narrowed.is_none() {
                    if let ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: inner,
                    } = &test.kind
                    {
                        if let ExprKind::Name(name) = &inner.kind {
                            if let Some(Type::Option(inner_ty)) = self.lookup_var(name) {
                                // `if not x:` keeps Optional in then-branch and narrows else-branch.
                                narrowed = Some((
                                    name.clone(),
                                    Type::Option(inner_ty.clone()),
                                    *inner_ty.clone(),
                                ));
                            }
                        }
                    }
                }
                if let Some((name, true_ty, false_ty)) = narrowed {
                    self.scopes.push(HashMap::new());
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert(name.clone(), true_ty);
                    }
                    for stmt in body {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                    self.scopes.pop();
                    self.scopes.push(HashMap::new());
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert(name.clone(), false_ty);
                    }
                    for stmt in orelse {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                    self.scopes.pop();
                } else {
                    for stmt in body {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                    for stmt in orelse {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                }
            }
            StmtKind::While { test, body } => {
                // Python while-conditions use truthiness, not strict bool typing.
                let _cond_ty = self.check_expr(test, Some(&Type::Bool))?;
                let mut narrowed: Option<(String, Type)> = None;
                if let ExprKind::Name(name) = &test.kind {
                    if let Some(Type::Option(inner)) = self.lookup_var(name) {
                        // Truthy while-branch excludes None for Optional values.
                        narrowed = Some((name.clone(), *inner.clone()));
                    }
                }
                if let Some((name, narrowed_ty)) = narrowed {
                    self.scopes.push(HashMap::new());
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert(name, narrowed_ty);
                    }
                    for stmt in body {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                    self.scopes.pop();
                } else {
                    for stmt in body {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                }
            }
            StmtKind::For { target, iter, body } => {
                let iter_ty = self.check_expr(iter, None)?;
                let item_ty = self.iter_item_type(&iter_ty, stmt.span)?;

                // Handle different target patterns.
                match target {
                    ForTarget::Name(name) => {
                        if self.in_function() && self.is_declared_nonlocal(name) {
                            let outer_ty = self.lookup_nonlocal_var(name).ok_or_else(|| {
                                self.error(stmt.span, "nonlocal binding not found")
                            })?;
                            self.ensure_assignable(&item_ty, &outer_ty, stmt.span)?;
                        } else if self.in_function() && self.is_declared_global(name) {
                            let global_ty =
                                self.ctx.globals.get(name).cloned().ok_or_else(|| {
                                    self.error(
                                        stmt.span,
                                        format!("global `{name}` is not defined at module scope"),
                                    )
                                })?;
                            self.ensure_assignable(&item_ty, &global_ty, stmt.span)?;
                        } else if self.in_function() {
                            if let Some(existing) = self.lookup_local_var(name) {
                                self.set_var_type(
                                    name,
                                    Self::merge_types(existing, item_ty.clone()),
                                );
                            } else {
                                self.insert_var(name, item_ty, stmt.span)?;
                            }
                        } else if let Some(existing) = self.lookup_var(name) {
                            self.set_var_type(name, Self::merge_types(existing, item_ty.clone()));
                        } else {
                            self.insert_var(name, item_ty, stmt.span)?;
                        }
                    }
                    ForTarget::Tuple(names) => {
                        // Extract element types from tuple item type.
                        let elem_types = if let Type::Tuple(items) = &item_ty {
                            if items.len() != names.len() {
                                return Err(self.error(
                                    stmt.span,
                                    format!(
                                        "For loop unpacking expected {} values, got {}",
                                        names.len(),
                                        items.len()
                                    ),
                                ));
                            }
                            items.clone()
                        } else {
                            return Err(self.error(
                                stmt.span,
                                "For loop tuple unpacking requires iterable of tuples",
                            ));
                        };

                        // Bind each name to its corresponding element type.
                        for (name, ty) in names.iter().zip(elem_types.iter()) {
                            if self.in_function() && self.is_declared_nonlocal(name) {
                                let outer_ty = self.lookup_nonlocal_var(name).ok_or_else(|| {
                                    self.error(stmt.span, "nonlocal binding not found")
                                })?;
                                self.ensure_assignable(ty, &outer_ty, stmt.span)?;
                            } else if self.in_function() && self.is_declared_global(name) {
                                let global_ty =
                                    self.ctx.globals.get(name).cloned().ok_or_else(|| {
                                        self.error(
                                            stmt.span,
                                            format!(
                                                "global `{name}` is not defined at module scope"
                                            ),
                                        )
                                    })?;
                                self.ensure_assignable(ty, &global_ty, stmt.span)?;
                            } else if self.in_function() {
                                if let Some(existing) = self.lookup_local_var(name) {
                                    self.set_var_type(
                                        name,
                                        Self::merge_types(existing, ty.clone()),
                                    );
                                } else {
                                    self.insert_var(name, ty.clone(), stmt.span)?;
                                }
                            } else if let Some(existing) = self.lookup_var(name) {
                                self.set_var_type(name, Self::merge_types(existing, ty.clone()));
                            } else {
                                self.insert_var(name, ty.clone(), stmt.span)?;
                            }
                        }
                    }
                }

                for stmt in body {
                    self.check_stmt(stmt, expected_ret)?;
                }
            }
            StmtKind::Import { names } => {
                for binding in names {
                    if binding.module == "typing" {
                        // Typing imports are annotation-only and have no runtime binding.
                        continue;
                    }
                    let bound_name = binding.alias.as_deref().unwrap_or(binding.module.as_str());
                    if resolve_module(binding.module.as_str()).is_some() {
                        // Virtual stdlib module import (os/sys/re/json).
                        self.insert_var(
                            bound_name,
                            Type::Module(binding.module.clone()),
                            stmt.span,
                        )?;
                    } else {
                        // User-module imports are resolved by the import pass.
                        // Keep a permissive placeholder for remaining dynamic uses.
                        self.insert_var(bound_name, Type::Unknown, stmt.span)?;
                    }
                }
            }
            StmtKind::ImportFrom { module, names } => {
                if module != "typing" {
                    if let Some(module_id) = resolve_module(module.as_str()) {
                        for binding in names {
                            let method_id = find_imported_member(module_id, binding.name.as_str())
                                .ok_or_else(|| {
                                    self.error(
                                        stmt.span,
                                        format!(
                                            "{module} has no supported member '{}'",
                                            binding.name
                                        ),
                                    )
                                })?;
                            let spec = method_spec(method_id);
                            let bound_name =
                                binding.alias.as_deref().unwrap_or(binding.name.as_str());
                            self.insert_var(
                                bound_name,
                                Type::StdlibFunction {
                                    module: spec.module_name.to_string(),
                                    method: spec.method_name.to_string(),
                                },
                                stmt.span,
                            )?;
                        }
                    } else {
                        // User-module from-imports are rewritten by the import resolver.
                        // Keep unresolved names permissive to avoid false negatives.
                        for binding in names {
                            let bound_name =
                                binding.alias.as_deref().unwrap_or(binding.name.as_str());
                            self.insert_var(bound_name, Type::Unknown, stmt.span)?;
                        }
                    }
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
            StmtKind::Nonlocal { names } => {
                if !self.in_function() {
                    return Err(self.error(stmt.span, "nonlocal is only allowed inside functions"));
                }
                for name in names.iter() {
                    if name == "__name__" {
                        return Err(self.error(stmt.span, "nonlocal __name__ is not supported"));
                    }
                    self.declare_nonlocal(name, stmt.span)?;
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
                let mut seen_variants = HashSet::new();
                for case in &mut *cases {
                    // Reject duplicate variant branches early so coverage diagnostics stay honest.
                    if !seen_variants.insert(case.variant.clone()) {
                        return Err(self.error(
                            case.span,
                            format!("Duplicate match case for variant '{}'", case.variant),
                        ));
                    }
                    if !union_variants.contains(&case.variant) {
                        return Err(self.error(case.span, "Case variant not in union"));
                    }
                    let class_info = self.ctx.classes.get(&case.variant).ok_or_else(|| {
                        self.error(
                            case.span,
                            format!("Unknown variant class: {}", case.variant),
                        )
                    })?;

                    let fields: Vec<(String, Type)> =
                        if let Some(binding_fields) = &case.binding_fields {
                            // Keyword patterns bind by explicit field names, independent of
                            // __match_args__ ordering.
                            if binding_fields.len() != case.bindings.len() {
                                return Err(self.error(
                                    case.span,
                                    "Case keyword field count does not match bindings",
                                ));
                            }
                            let mut seen = HashSet::new();
                            let mut resolved = Vec::new();
                            for field_name in binding_fields {
                                if !seen.insert(field_name.clone()) {
                                    return Err(
                                        self.error(case.span, "Duplicate keyword field in pattern")
                                    );
                                }
                                let field_ty =
                                    class_info.fields.get(field_name).ok_or_else(|| {
                                        self.error(
                                            case.span,
                                            format!("Unknown field in pattern: {}", field_name),
                                        )
                                    })?;
                                resolved.push((field_name.clone(), field_ty.clone()));
                            }
                            resolved
                        } else {
                            // Positional patterns follow __match_args__ when present, otherwise
                            // declaration order.
                            let field_order: Vec<String> =
                                if let Some(ref match_args) = class_info.match_args {
                                    match_args.clone()
                                } else {
                                    class_info.fields.keys().cloned().collect()
                                };
                            let resolved: Vec<(String, Type)> = field_order
                                .iter()
                                .filter_map(|name| {
                                    class_info
                                        .fields
                                        .get(name)
                                        .map(|ty| (name.clone(), ty.clone()))
                                })
                                .collect();
                            if resolved.len() != case.bindings.len() {
                                return Err(self
                                    .error(case.span, "Case binding count does not match fields"));
                            }
                            resolved
                        };

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
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => self.check_try_stmt(body, handlers, orelse, finalbody, expected_ret)?,
            StmtKind::Raise { exc, cause } => self.check_raise_stmt(exc, cause, stmt.span)?,
        }
        Ok(())
    }
}
