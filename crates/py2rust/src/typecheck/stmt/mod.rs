use super::*;
use crate::hir_visit::StmtVisitorMut;
use crate::stdlib::registry::{
    find_imported_member, find_stdlib_attribute, importable_members, method_spec, resolve_module,
};

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
            target: Box::new(target.clone()),
            value: value.clone(),
        })
    }

    fn visit_delete_mut(&mut self, target: &mut AssignTarget) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Delete {
            target: Box::new(target.clone()),
        })
    }

    fn visit_class_mut(&mut self, def: &mut ClassDef) -> Result<(), CompileError> {
        self.check_and_rewrite(StmtKind::Class { def: def.clone() })
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
                        target: Box::new(AssignTarget::Name(name.clone())),
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
                        target: Box::new(AssignTarget::Name(name.clone())),
                        value: value.clone(),
                    };
                    return Ok(());
                }
                if let ExprKind::Lambda {
                    params,
                    param_kinds,
                    ..
                } = &value.kind
                {
                    // Clone param metadata before mutable borrow of `value` in check_expr.
                    let param_names_owned = params.clone();
                    let param_kinds_owned = param_kinds.clone();
                    let placeholder = Type::Lambda {
                        param_names: param_names_owned.clone(),
                        params: vec![Type::Unknown; param_names_owned.len()],
                        param_kinds: Vec::new(),
                        has_defaults: Vec::new(),
                        ret: Box::new(Type::Unknown),
                    };
                    self.insert_var(name, placeholder, stmt.span)?;
                    let expected = if let Some(ann) = ann {
                        let ty = self.resolve_type_ref(ann, stmt.span)?;
                        if matches!(ty, Type::Iterator(_)) {
                            return Err(self
                                .error(stmt.span, "Iterator[T] is only allowed as a return type"));
                        }
                        Some(Self::normalize_lambda_expected(ty, &param_kinds_owned))
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
                    self.insert_var(name, declared.clone(), stmt.span)?;
                    self.lambda_defs.insert(name.clone(), value.clone());
                    // Second-pass refinement: when the initial unannotated body check
                    // refined one or more params (e.g., `v: Unknown → v: Value` via
                    // `v._field` attribute access), side effects that ran BEFORE the
                    // refinement (e.g., `visited.add(v)` before the for loop) saw the
                    // wrong (Unknown) type. Re-run the body with the now-concrete param
                    // types so those side effects can propagate the correct container
                    // element type into the outer scope.
                    //
                    // The guard prevents infinite re-entry for recursive nested functions
                    // (e.g., `build_topo` calling itself): the inner recursive call sees
                    // the guard already active and returns Ok(Unknown), allowing the outer
                    // re-check to complete and collect the correct side effects.
                    // Second-pass refinement: if the initial body check refined any param
                    // from Unknown to concrete (e.g., `v: Unknown → v: Value` via
                    // attribute access), re-run the body with those concrete params.
                    // This propagates side effects (e.g., `visited.add(v)`) that fired
                    // before the refinement to correctly update outer-scope container types.
                    if let Type::Lambda {
                        params: refined_params,
                        ..
                    } = &declared
                    {
                        let any_param_refined =
                            refined_params.iter().any(|p| !matches!(p, Type::Unknown));
                        if any_param_refined {
                            let expected_refined = Type::Lambda {
                                param_names: param_names_owned.clone(),
                                params: refined_params.clone(),
                                param_kinds: param_kinds_owned.clone(),
                                has_defaults: vec![false; param_names_owned.len()],
                                ret: Box::new(Type::Unknown),
                            };
                            let _ = self.with_lambda_inference_guard(name, stmt.span, |tc| {
                                let mut expr_clone = value.clone();
                                tc.check_expr(&mut expr_clone, Some(&expected_refined))
                            });
                        }
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
                    fn is_decorator_chain_expr(expr: &Expr) -> bool {
                        match &expr.kind {
                            ExprKind::Call { args, keywords, .. }
                                if keywords.is_empty() && args.len() == 1 =>
                            {
                                matches!(args[0].kind, ExprKind::Lambda { .. })
                                    || is_decorator_chain_expr(&args[0])
                            }
                            _ => false,
                        }
                    }
                    let assignable = self.ensure_assignable(&ty, &expected, stmt.span);
                    let relax_decorator_callable_shape = assignable.is_err()
                        && matches!(expected, Type::Lambda { .. })
                        && matches!(ty, Type::Lambda { .. })
                        && is_decorator_chain_expr(value);
                    if !relax_decorator_callable_shape {
                        assignable?;
                    }
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
                    // CPython-compat note:
                    // Decorator wrappers often return a callable whose static parameter model
                    // differs from the original function annotation (`*args/**kwargs` wrapper).
                    // Keep the concrete post-decoration callable shape in that case.
                    let declared = if relax_decorator_callable_shape {
                        ty.clone()
                    } else {
                        // Preserve explicit annotation constraints, but allow Unknown annotations
                        // (for example, wide inline unions) to refine from the initializer.
                        Self::merge_types(declared, ty.clone())
                    };
                    self.insert_var(name, declared, stmt.span)?;
                } else {
                    // Python allows local names to be rebound from dynamically-typed values.
                    // Keep Unknown when inference is impossible instead of hard-failing.
                    self.insert_var(name, ty, stmt.span)?;
                }
            }
            StmtKind::Assign { target, value } => {
                let mut ty = self.check_expr(value, None)?;
                if matches!(
                    target.as_ref(),
                    AssignTarget::Tuple(_) | AssignTarget::List(_)
                ) {
                    // Destructuring assignment: validate each leaf target against element types.
                    self.check_unpack_target(target, &ty, Some(value), stmt.span)?;
                } else {
                    let mut promote_to_let: Option<(String, Expr)> = None;
                    match target.as_mut() {
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
                                if self.lookup_local_var(name).is_some() {
                                    // Plain Python assignment rebinds local names and does not
                                    // require compatibility with prior runtime values.
                                    let rebound = self
                                        .lookup_local_var(name)
                                        .map(|existing| {
                                            Self::preserve_optional_binding(existing, ty.clone())
                                        })
                                        .unwrap_or_else(|| ty.clone());
                                    self.insert_var(name, rebound, stmt.span)?;
                                } else {
                                    promote_to_let = Some((name.clone(), value.clone()));
                                    self.insert_var(name, ty, stmt.span)?;
                                }
                            } else if self.lookup_var(name).is_some() {
                                // Module-scope rebinding follows the same Python semantics.
                                let rebound = self
                                    .lookup_var(name)
                                    .map(|existing| {
                                        Self::preserve_optional_binding(existing, ty.clone())
                                    })
                                    .unwrap_or_else(|| ty.clone());
                                self.insert_var(name, rebound, stmt.span)?;
                            } else {
                                promote_to_let = Some((name.clone(), value.clone()));
                                self.insert_var(name, ty, stmt.span)?;
                            }
                            if matches!(value.kind, ExprKind::Lambda { .. }) {
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
                                if !self.ctx.classes.contains_key(&class_name) {
                                    return Err(self
                                        .error(stmt.span, format!("Unknown class: {class_name}")));
                                }
                                let prop = self
                                    .ctx
                                    .classes
                                    .get(&class_name)
                                    .and_then(|info| info.properties.get(attr))
                                    .cloned();
                                if let Some(prop) = prop {
                                    if let Some(setter_name) = prop.setter.as_ref() {
                                        let expected = self
                                            .ctx
                                            .classes
                                            .get(&class_name)
                                            .and_then(|info| info.methods.get(setter_name))
                                            .and_then(|sig| sig.params.get(1))
                                            .cloned()
                                            .unwrap_or(Type::Unknown);
                                        self.ensure_assignable(&ty, &expected, stmt.span)?;
                                        if matches!(expected, Type::Unknown)
                                            && !matches!(ty, Type::Unknown)
                                        {
                                            if let Some(info) =
                                                self.ctx.classes.get_mut(&class_name)
                                            {
                                                if let Some(sig) = info.methods.get_mut(setter_name)
                                                {
                                                    if sig.params.len() >= 2 {
                                                        sig.params[1] = ty.clone();
                                                    }
                                                }
                                            }
                                        }
                                        return Ok(());
                                    }
                                    return Err(self.error(
                                        stmt.span,
                                        format!("Property {class_name}.{attr} has no setter"),
                                    ));
                                }
                                let field_ty = self
                                    .ctx
                                    .classes
                                    .get(&class_name)
                                    .and_then(|info| info.fields.get(attr))
                                    .cloned()
                                    .ok_or_else(|| {
                                        self.error(
                                            stmt.span,
                                            format!("Unknown field {class_name}.{attr}"),
                                        )
                                    })?;
                                self.ensure_assignable(&ty, &field_ty, stmt.span)?;
                                if matches!(field_ty, Type::Unknown) && !matches!(ty, Type::Unknown)
                                {
                                    if let Some(info) = self.ctx.classes.get_mut(&class_name) {
                                        if let Some(slot) = info.fields.get_mut(attr) {
                                            *slot = ty.clone();
                                        }
                                    }
                                }
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
                                    if let ExprKind::Name(dict_name) = &container.kind {
                                        let refined_key =
                                            Self::merge_types((*key_ty).clone(), index_ty.clone());
                                        let refined_val =
                                            Self::merge_types((*val_ty).clone(), ty.clone());
                                        self.set_var_type(
                                            dict_name,
                                            Type::Dict(
                                                Box::new(refined_key),
                                                Box::new(refined_val),
                                            ),
                                        );
                                    }
                                }
                                Type::Tuple(_) => {
                                    // CPython raises TypeError at runtime for tuple item
                                    // assignment; keep the statement type-checkable so
                                    // try/except handlers can observe that runtime error.
                                    self.ensure_assignable(&index_ty, &Type::Int, stmt.span)?;
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
            StmtKind::Delete { target } => match target.as_mut() {
                AssignTarget::Index {
                    value: container,
                    index,
                } => {
                    let container_ty = self.check_expr(container, None)?;
                    let index_ty = self.check_expr(index, None)?;
                    match container_ty {
                        Type::List(_) => {
                            self.ensure_assignable(&index_ty, &Type::Int, stmt.span)?;
                        }
                        Type::Dict(key_ty, _) => {
                            self.ensure_assignable(&index_ty, &key_ty, stmt.span)?;
                        }
                        _ => return Err(self.error(stmt.span, "del index requires list or dict")),
                    }
                }
                AssignTarget::Attr { value: obj, attr } => {
                    let obj_ty = self.check_expr(obj, None)?;
                    let Type::Custom(class_name) = obj_ty else {
                        return Err(self.error(
                            stmt.span,
                            "del attribute is only allowed on class instances",
                        ));
                    };
                    let class_info = self.ctx.classes.get(&class_name).ok_or_else(|| {
                        self.error(stmt.span, format!("Unknown class: {class_name}"))
                    })?;
                    if let Some(prop) = class_info.properties.get(attr) {
                        if prop.deleter.is_none() {
                            return Err(self.error(
                                stmt.span,
                                format!("Property {class_name}.{attr} has no deleter"),
                            ));
                        }
                    } else if !class_info.fields.contains_key(attr) {
                        return Err(self.error(
                            stmt.span,
                            format!("Unknown attribute {class_name}.{attr} for del"),
                        ));
                    }
                }
                AssignTarget::Name(_) => {
                    return Err(self.error(
                        stmt.span,
                        "del name is not supported; only index/attribute deletion is supported",
                    ))
                }
                AssignTarget::Tuple(_) | AssignTarget::List(_) | AssignTarget::Starred(_) => {
                    return Err(self.error(stmt.span, "del unpacking targets are not supported"))
                }
            },
            StmtKind::Class { def } => {
                if !self.in_function() {
                    return Err(self.error(
                        def.span,
                        "Class statements are only supported inside function bodies",
                    ));
                }
                if self.control_flow_depth > 0 {
                    return Err(self.error(
                        def.span,
                        "Local class definitions are only supported at function-body scope",
                    ));
                }
                self.register_local_class_signature(def)?;
                self.insert_var(&def.name, Type::Custom(def.name.clone()), stmt.span)?;
                let saved_floor = self.capture_scope_floor;
                self.capture_scope_floor = Some(self.scopes.len());
                let check_result = self.check_class(def);
                self.capture_scope_floor = saved_floor;
                check_result?;
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
                // isinstance narrowing for InlineUnion types.
                // `isinstance(x, T)` where x: InlineUnion → narrow true-branch to T.
                let isinstance_inline_narrowing = |tc: &TypeChecker<'_>, test_expr: &Expr| -> Option<(String, Type, Type)> {
                    let call_expr = match &test_expr.kind {
                        ExprKind::Call { func, args, .. } => {
                            if let ExprKind::Name(n) = &func.kind {
                                if n == "isinstance" && args.len() == 2 { Some(args.as_slice()) } else { None }
                            } else { None }
                        }
                        _ => None,
                    }?;
                    let ExprKind::Name(var_name) = &call_expr[0].kind else { return None; };
                    let var_ty = tc.lookup_var(var_name)?.clone();
                    let members = match &var_ty {
                        Type::InlineUnion(m) => m.clone(),
                        Type::Option(inner) => {
                            if let Type::InlineUnion(m) = inner.as_ref() { m.clone() } else { return None; }
                        }
                        _ => return None,
                    };
                    let ExprKind::Name(type_name) = &call_expr[1].kind else { return None; };
                    let target = Type::from_isinstance_name(type_name)?;
                    let matched_ty = members.iter().find(|m| std::mem::discriminant(*m) == std::mem::discriminant(&target))?.clone();
                    // False branch: the remaining members (remove matched type from union).
                    let remaining: Vec<Type> = members.iter().filter(|m| std::mem::discriminant(*m) != std::mem::discriminant(&matched_ty)).cloned().collect();
                    let false_ty = match remaining.len() {
                        0 => var_ty.clone(),
                        1 => remaining.into_iter().next().unwrap(),
                        _ => Type::InlineUnion(remaining),
                    };
                    Some((var_name.clone(), matched_ty.clone(), false_ty))
                };
                if narrowed.is_none() {
                    // Direct isinstance(x, T)
                    if let Some((var_name, true_ty, false_ty)) = isinstance_inline_narrowing(self, test) {
                        narrowed = Some((var_name, true_ty, false_ty));
                    }
                }
                if narrowed.is_none() {
                    // `not isinstance(x, T)` — swap branches.
                    if let ExprKind::Unary { op: UnaryOp::Not, expr: inner } = &test.kind {
                        if let Some((var_name, true_ty, false_ty)) = isinstance_inline_narrowing(self, inner) {
                            narrowed = Some((var_name, false_ty, true_ty));
                        }
                    }
                }
                // Collect multi-variable narrowings for `not (A and B)` pattern.
                // When the test is `not (isinstance(x,T) and isinstance(y,U))`,
                // the else branch (when `not (...)` is false) has x:T and y:U.
                let mut extra_else_narrowings: Vec<(String, Type)> = Vec::new();
                if narrowed.is_none() {
                    if let ExprKind::Unary { op: UnaryOp::Not, expr: inner } = &test.kind {
                        if let ExprKind::BoolOp { op: BoolOp::And, values } = &inner.kind {
                            // Collect narrowings from each component.
                            for value in values {
                                if let Some((var_name, true_ty, _)) = isinstance_inline_narrowing(self, value) {
                                    extra_else_narrowings.push((var_name, true_ty));
                                }
                            }
                        }
                    }
                }
                self.with_control_flow_depth(|tc| {
                    if let Some((name, true_ty, false_ty)) = narrowed {
                        let mut true_end_ty = true_ty.clone();
                        tc.scopes.push(HashMap::new());
                        if let Some(scope) = tc.scopes.last_mut() {
                            scope.insert(name.clone(), true_ty);
                        }
                        for stmt in body {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        if let Some(scope) = tc.scopes.last() {
                            if let Some(ty) = scope.get(&name) {
                                true_end_ty = ty.clone();
                            }
                        }
                        tc.scopes.pop();

                        let mut false_end_ty = false_ty.clone();
                        tc.scopes.push(HashMap::new());
                        if let Some(scope) = tc.scopes.last_mut() {
                            scope.insert(name.clone(), false_ty);
                        }
                        for stmt in orelse {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        if let Some(scope) = tc.scopes.last() {
                            if let Some(ty) = scope.get(&name) {
                                false_end_ty = ty.clone();
                            }
                        }
                        tc.scopes.pop();

                        // Merge branch-local refinements back into the outer binding.
                        // This keeps Optional narrowing + reassignment flows usable after `if`.
                        let merged_after_if = match (&true_end_ty, &false_end_ty) {
                            (Type::None, Type::Unknown) | (Type::Unknown, Type::None) => {
                                Type::Option(Box::new(Type::Unknown))
                            }
                            (Type::None, other) => Type::Option(Box::new(other.clone())),
                            (other, Type::None) => Type::Option(Box::new(other.clone())),
                            (Type::Unknown, other) => other.clone(),
                            (other, Type::Unknown) => other.clone(),
                            _ => Self::merge_types(true_end_ty.clone(), false_end_ty.clone()),
                        };

                        let mut updated = false;
                        for scope in tc.scopes.iter_mut().rev() {
                            if scope.contains_key(&name) {
                                scope.insert(name.clone(), merged_after_if.clone());
                                updated = true;
                                break;
                            }
                        }
                        if !updated {
                            tc.set_var_type(&name, merged_after_if);
                        }
                    } else if !extra_else_narrowings.is_empty() {
                        // Multi-variable narrowing from `not (A and B)` etc.
                        for stmt in body {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        // Apply all collected narrowings in the else scope.
                        tc.scopes.push(HashMap::new());
                        for (var_name, ty) in &extra_else_narrowings {
                            if let Some(scope) = tc.scopes.last_mut() {
                                scope.insert(var_name.clone(), ty.clone());
                            }
                        }
                        for stmt in orelse {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        tc.scopes.pop();
                    } else {
                        for stmt in body {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        for stmt in orelse {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                    }
                    Ok(())
                })?;
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
                    self.with_control_flow_depth(|tc| {
                        tc.scopes.push(HashMap::new());
                        if let Some(scope) = tc.scopes.last_mut() {
                            scope.insert(name, narrowed_ty);
                        }
                        for stmt in body {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        tc.scopes.pop();
                        Ok(())
                    })?;
                } else {
                    self.with_control_flow_depth(|tc| {
                        for stmt in body {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        Ok(())
                    })?;
                }
            }
            StmtKind::For { target, iter, body } => {
                let iter_ty = self.check_expr(iter, None)?;
                let item_ty = self.iter_item_type(&iter_ty, stmt.span)?;
                let iter_name = if let ExprKind::Name(name) = &iter.kind {
                    Some(name.clone())
                } else {
                    None
                };

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

                self.with_control_flow_depth(|tc| {
                    for stmt in body {
                        tc.check_stmt(stmt, expected_ret)?;
                    }
                    Ok(())
                })?;

                if matches!(iter_ty, Type::Unknown) {
                    if let Some(iter_name) = iter_name {
                        let inferred_item = match target {
                            ForTarget::Name(name) => self.lookup_var(name).unwrap_or(Type::Unknown),
                            ForTarget::Tuple(names) => {
                                let mut items = Vec::with_capacity(names.len());
                                let mut has_unknown = false;
                                for name in names {
                                    let ty = self.lookup_var(name).unwrap_or(Type::Unknown);
                                    if matches!(ty, Type::Unknown) {
                                        has_unknown = true;
                                        break;
                                    }
                                    items.push(ty);
                                }
                                if has_unknown {
                                    Type::Unknown
                                } else {
                                    Type::Tuple(items)
                                }
                            }
                        };
                        if !matches!(inferred_item, Type::Unknown) {
                            // CPython-compat divergence:
                            // Unknown loop iterables are refined from target usage so
                            // nested defs/lambdas gain concrete iterator parameter types.
                            self.set_var_type(&iter_name, Type::Iterator(Box::new(inferred_item)));
                        }
                    }
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
                        // Virtual stdlib module import.
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
                        let has_star = names.iter().any(|binding| binding.name == "*");
                        if has_star {
                            if names.len() != 1 || names[0].alias.is_some() {
                                return Err(self.error(
                                    stmt.span,
                                    "from <stdlib> import * does not support aliases or extra names",
                                ));
                            }
                            let expanded: Vec<ImportFromBinding> = importable_members(module_id)
                                .iter()
                                .map(|name| ImportFromBinding {
                                    name: (*name).to_string(),
                                    alias: None,
                                })
                                .collect();
                            // Rewrite `*` to explicit names so later phases remain simple.
                            *names = expanded;
                        }

                        for binding in names.iter() {
                            let bound_name =
                                binding.alias.as_deref().unwrap_or(binding.name.as_str());
                            if let Some(method_id) =
                                find_imported_member(module_id, binding.name.as_str())
                            {
                                let spec = method_spec(method_id);
                                self.insert_var(
                                    bound_name,
                                    Type::StdlibFunction {
                                        module: spec.module_name.to_string(),
                                        method: spec.method_name.to_string(),
                                    },
                                    stmt.span,
                                )?;
                                continue;
                            }
                            if let Some(attr_spec) =
                                find_stdlib_attribute(module_id, binding.name.as_str())
                            {
                                self.insert_var(
                                    bound_name,
                                    (attr_spec.type_resolver)(),
                                    stmt.span,
                                )?;
                                continue;
                            }
                            return Err(self.error(
                                stmt.span,
                                format!("{module} has no supported member '{}'", binding.name),
                            ));
                        }
                    } else {
                        // User-module from-imports are rewritten by the import resolver.
                        // Keep unresolved names permissive to avoid false negatives.
                        for binding in names.iter() {
                            if binding.name == "*" {
                                return Err(self.error(
                                    stmt.span,
                                    format!("from {module} import * is only supported for stdlib modules"),
                                ));
                            }
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

                    self.with_control_flow_depth(|tc| {
                        tc.scopes.push(HashMap::new());
                        for (binding, (_, field_ty)) in case.bindings.iter().zip(fields.iter()) {
                            if let Some(existing) = tc.lookup_var(binding) {
                                tc.ensure_assignable(field_ty, &existing, case.span)?;
                            } else {
                                tc.insert_var(binding, field_ty.clone(), case.span)?;
                            }
                        }
                        for stmt in &mut case.body {
                            tc.check_stmt(stmt, expected_ret)?;
                        }
                        tc.scopes.pop();
                        Ok(())
                    })?;
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
            } => {
                self.with_control_flow_depth(|tc| {
                    tc.check_try_stmt(body, handlers, orelse, finalbody, expected_ret)
                })?;
            }
            StmtKind::Raise { exc, cause } => self.check_raise_stmt(exc, cause, stmt.span)?,
        }
        Ok(())
    }

    /// Normalize lambda annotation parameter types against the lowered parameter kinds.
    ///
    /// `TypeRef::Lambda` stores annotation payload types, while variadic parameters in Python
    /// describe element/value types (`*args: T`, `**kwargs: T`). This helper wraps those payloads
    /// into concrete container types so nested variadic defs type-check like top-level defs.
    fn normalize_lambda_expected(expected: Type, param_kinds: &[ParamKind]) -> Type {
        let Type::Lambda {
            param_names,
            mut params,
            param_kinds: mut expected_kinds,
            mut has_defaults,
            ret,
        } = expected
        else {
            return expected;
        };

        if params.len() != param_kinds.len() {
            return Type::Lambda {
                param_names,
                params,
                param_kinds: expected_kinds,
                has_defaults,
                ret,
            };
        }

        for (idx, kind) in param_kinds.iter().enumerate() {
            let current = params[idx].clone();
            params[idx] = match kind {
                ParamKind::VarArgs => match current {
                    Type::List(_) => current,
                    other => Type::List(Box::new(other)),
                },
                ParamKind::VarKeywords => match current {
                    Type::Dict(key, value) if matches!(key.as_ref(), Type::Str) => {
                        Type::Dict(key, value)
                    }
                    other => Type::Dict(Box::new(Type::Str), Box::new(other)),
                },
                _ => current,
            };
        }

        if expected_kinds.len() != param_kinds.len() {
            expected_kinds = param_kinds.to_vec();
        }
        if has_defaults.len() != param_kinds.len() {
            has_defaults = vec![false; param_kinds.len()];
        }

        Type::Lambda {
            param_names,
            params,
            param_kinds: expected_kinds,
            has_defaults,
            ret,
        }
    }

    /// Run nested statement checking under increased control-flow depth.
    fn with_control_flow_depth<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        self.control_flow_depth += 1;
        let result = f(self);
        self.control_flow_depth -= 1;
        result
    }

    /// Keep `Optional` bindings stable across plain rebinding assignments.
    fn preserve_optional_binding(existing: Type, assigned: Type) -> Type {
        match existing {
            Type::Option(inner) => match assigned {
                Type::None => Type::Option(inner),
                Type::Option(rhs_inner) => {
                    Type::Option(Box::new(Self::merge_types(*inner, *rhs_inner)))
                }
                other => Type::Option(Box::new(Self::merge_types(*inner, other))),
            },
            _ => assigned,
        }
    }

    /// Register a function-local class signature in the type context.
    ///
    /// Local classes share the same signature shape as top-level classes, but
    /// remain scoped to their owner function.
    fn register_local_class_signature(&mut self, class_def: &ClassDef) -> Result<(), CompileError> {
        if self.ctx.classes.contains_key(&class_def.name) {
            return Err(self.error(
                class_def.span,
                format!(
                    "Class `{}` conflicts with an existing class symbol",
                    class_def.name
                ),
            ));
        }
        if self.lookup_local_var(&class_def.name).is_some() {
            return Err(self.error(
                class_def.span,
                format!(
                    "Class `{}` conflicts with an existing local symbol",
                    class_def.name
                ),
            ));
        }
        if let Some(base) = class_def.base.as_ref() {
            if !self.is_visible_class(base) && !Self::is_builtin_exception_name(base) {
                return Err(self.error(class_def.span, format!("Unknown base class: {base}")));
            }
        }
        let owner_scope = self.function_scopes.last().copied();
        self.ctx.classes.insert(
            class_def.name.clone(),
            ClassInfo {
                name: class_def.name.clone(),
                owner_scope,
                base: None,
                fields: IndexMap::new(),
                class_attrs: IndexMap::new(),
                methods: HashMap::new(),
                method_kinds: HashMap::new(),
                properties: HashMap::new(),
                init: None,
                iter_return: None,
                iter_item: None,
                next_item: None,
                match_args: class_def.match_args.clone(),
                shared_mutable_fields: HashSet::new(),
            },
        );

        let mut fields = IndexMap::new();
        for field in &class_def.fields {
            let ty = self.resolve_type_ref(&field.ty, field.span)?;
            if matches!(ty, Type::Iterator(_)) {
                return Err(self.error(field.span, "Iterator[T] is only allowed as a return type"));
            }
            fields.insert(field.name.clone(), ty);
        }

        let mut class_attrs = IndexMap::new();
        for attr in &class_def.class_attrs {
            let ty = if let Some(ann) = &attr.ann {
                let ty = self.resolve_type_ref(ann, attr.span)?;
                if matches!(ty, Type::Iterator(_)) {
                    return Err(
                        self.error(attr.span, "Iterator[T] is only allowed as a return type")
                    );
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
                deleter: None,
                ty: Type::Unknown,
            });
            if !prop.getter.is_empty() {
                entry.getter = prop.getter.clone();
            }
            if prop.setter.is_some() {
                entry.setter = prop.setter.clone();
            }
            if prop.deleter.is_some() {
                entry.deleter = prop.deleter.clone();
            }
        }

        let mut init = None;
        let mut iter_return = None;
        let mut iter_item = None;
        let mut next_item = None;
        for method in &class_def.methods {
            let mut params = self.resolve_params(&method.params)?;
            if method.name == "__exit__" {
                for param_ty in params.iter_mut().skip(1) {
                    if matches!(param_ty, Type::Unknown) {
                        *param_ty = Type::Int;
                    }
                }
            }
            let ret = self.resolve_type_ref(&method.ret, method.span)?;
            let defaults = method.params.iter().filter(|p| p.default.is_some()).count();
            let sig = FunctionSig {
                param_names: method.params.iter().map(|p| p.name.clone()).collect(),
                param_kinds: method.params.iter().map(|p| p.kind).collect(),
                has_defaults: method.params.iter().map(|p| p.default.is_some()).collect(),
                params,
                ret: ret.clone(),
                span: method.span,
                is_generator: false,
                can_throw: false,
                thrown_exceptions: Vec::new(),
                defaults,
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

        for info in properties.values_mut() {
            if !info.getter.is_empty() {
                if let Some(sig) = methods.get(&info.getter) {
                    info.ty = sig.ret.clone();
                    continue;
                }
            }
            if let Some(setter) = info.setter.as_ref() {
                if let Some(sig) = methods.get(setter) {
                    let setter_shape_ok = sig.params.len() == 2
                        && sig.param_names.len() == 2
                        && sig.param_names[0] == "self"
                        && sig.param_kinds.len() == 2
                        && matches!(sig.param_kinds[0], ParamKind::PositionalOrKeyword)
                        && matches!(sig.param_kinds[1], ParamKind::PositionalOrKeyword)
                        && sig.has_defaults.len() == 2
                        && !sig.has_defaults[1];
                    if !setter_shape_ok {
                        return Err(self.error(
                            sig.span,
                            "Property setter must have signature (self, value)",
                        ));
                    }
                    info.ty = sig.params[1].clone();
                }
            }
            if let Some(deleter) = info.deleter.as_ref() {
                if let Some(sig) = methods.get(deleter) {
                    let deleter_shape_ok = sig.params.len() == 1
                        && sig.param_names.len() == 1
                        && sig.param_names[0] == "self"
                        && sig.param_kinds.len() == 1
                        && matches!(sig.param_kinds[0], ParamKind::PositionalOrKeyword)
                        && sig.has_defaults.len() == 1;
                    if !deleter_shape_ok {
                        return Err(
                            self.error(sig.span, "Property deleter must have signature (self)")
                        );
                    }
                }
            }
        }

        let mut merged_fields = fields;
        let mut merged_class_attrs = class_attrs;
        let mut merged_methods = methods;
        let mut merged_method_kinds = method_kinds;
        let mut merged_properties = properties;
        let mut merged_init = init;
        let mut merged_iter_return = iter_return;
        let mut merged_iter_item = iter_item;
        let mut merged_next_item = next_item;

        if let Some(base) = class_def.base.as_ref() {
            if Self::is_builtin_exception_name(base) {
                if merged_init.is_none() {
                    merged_init = Some(FunctionSig {
                        param_names: vec!["self".to_string(), "message".to_string()],
                        param_kinds: vec![
                            ParamKind::PositionalOrKeyword,
                            ParamKind::PositionalOrKeyword,
                        ],
                        has_defaults: vec![false, true],
                        params: vec![Type::Custom(class_def.name.clone()), Type::Str],
                        ret: Type::None,
                        span: class_def.span,
                        is_generator: false,
                        can_throw: false,
                        thrown_exceptions: Vec::new(),
                        defaults: 1,
                    });
                }
            } else {
                let base_info = self.ctx.classes.get(base).cloned().ok_or_else(|| {
                    self.error(class_def.span, format!("Unknown base class: {base}"))
                })?;

                let mut inherited_fields = base_info.fields;
                for (name, ty) in merged_fields {
                    inherited_fields.insert(name, ty);
                }
                merged_fields = inherited_fields;

                let mut inherited_class_attrs = base_info.class_attrs;
                for (name, info) in merged_class_attrs {
                    inherited_class_attrs.insert(name, info);
                }
                merged_class_attrs = inherited_class_attrs;

                let mut inherited_methods = base_info.methods;
                for (name, sig) in merged_methods {
                    inherited_methods.insert(name, sig);
                }
                merged_methods = inherited_methods;

                let mut inherited_kinds = base_info.method_kinds;
                for (name, kind) in merged_method_kinds {
                    inherited_kinds.insert(name, kind);
                }
                merged_method_kinds = inherited_kinds;

                let mut inherited_properties = base_info.properties;
                for (name, prop) in merged_properties {
                    inherited_properties.insert(name, prop);
                }
                merged_properties = inherited_properties;

                if merged_init.is_none() {
                    merged_init = base_info.init;
                }
                if merged_iter_return.is_none() {
                    merged_iter_return = base_info.iter_return;
                }
                if merged_iter_item.is_none() {
                    merged_iter_item = base_info.iter_item;
                }
                if merged_next_item.is_none() {
                    merged_next_item = base_info.next_item;
                }
            }
        }

        if let Some(info) = self.ctx.classes.get_mut(&class_def.name) {
            info.base = class_def.base.clone();
            info.fields = merged_fields;
            info.class_attrs = merged_class_attrs;
            info.methods = merged_methods;
            info.method_kinds = merged_method_kinds;
            info.properties = merged_properties;
            info.init = merged_init;
            info.iter_return = merged_iter_return;
            info.iter_item = merged_iter_item;
            info.next_item = merged_next_item;
            info.match_args = class_def.match_args.clone();
        }

        Ok(())
    }
}
