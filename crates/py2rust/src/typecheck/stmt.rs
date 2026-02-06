use super::*;
use crate::stdlib::registry::{method_spec, resolve_member, resolve_module};

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
                let ty = self.check_expr(value, None)?;
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
                                if matches!(ty, Type::Unknown) && !matches!(outer_ty, Type::Unknown)
                                {
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
                                if matches!(ty, Type::Unknown)
                                    && !matches!(global_ty, Type::Unknown)
                                {
                                    return Err(self
                                        .error(stmt.span, "Unable to infer type; add annotation"));
                                }
                                self.ensure_assignable(&ty, &global_ty, stmt.span)?;
                            } else if self.in_function()
                                && !self.is_declared_global(name)
                                && !self.is_declared_nonlocal(name)
                            {
                                if let Some(existing) = self.lookup_local_var(name) {
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
                                self.ensure_assignable(&item_ty, &existing, stmt.span)?;
                            } else {
                                self.insert_var(name, item_ty, stmt.span)?;
                            }
                        } else if let Some(existing) = self.lookup_var(name) {
                            self.ensure_assignable(&item_ty, &existing, stmt.span)?;
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
                                    self.ensure_assignable(ty, &existing, stmt.span)?;
                                } else {
                                    self.insert_var(name, ty.clone(), stmt.span)?;
                                }
                            } else if let Some(existing) = self.lookup_var(name) {
                                self.ensure_assignable(ty, &existing, stmt.span)?;
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
                    let _module_id = resolve_module(binding.module.as_str())
                        .ok_or_else(|| self.error(stmt.span, "Unsupported import"))?;
                    let bound_name = binding.alias.as_deref().unwrap_or(binding.module.as_str());
                    self.insert_var(bound_name, Type::Module(binding.module.clone()), stmt.span)?;
                }
            }
            StmtKind::ImportFrom { module, names } => {
                if module != "typing" {
                    let module_id = resolve_module(module.as_str())
                        .ok_or_else(|| self.error(stmt.span, "Unsupported import"))?;
                    for binding in names {
                        let method_id = resolve_member(module_id, binding.name.as_str())
                            .ok_or_else(|| {
                                self.error(
                                    stmt.span,
                                    format!("{module} has no supported member '{}'", binding.name),
                                )
                            })?;
                        let spec = method_spec(method_id);
                        let bound_name = binding.alias.as_deref().unwrap_or(binding.name.as_str());
                        self.insert_var(
                            bound_name,
                            Type::StdlibFunction {
                                module: spec.module_name.to_string(),
                                method: spec.method_name.to_string(),
                            },
                            stmt.span,
                        )?;
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
                for case in &mut *cases {
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
            } => {
                // Check try body
                for stmt in body {
                    self.check_stmt(stmt, expected_ret)?;
                }

                // Check exception handlers
                for handler in handlers {
                    self.check_except_handler(handler, expected_ret)?;
                }

                // Check else and finally clauses
                for stmt in orelse {
                    self.check_stmt(stmt, expected_ret)?;
                }
                for stmt in finalbody {
                    self.check_stmt(stmt, expected_ret)?;
                }
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(exc_expr) = exc {
                    // Special handling for built-in exception constructors
                    if let ExprKind::Call {
                        func,
                        args,
                        keywords: _,
                    } = &mut exc_expr.kind
                    {
                        if let ExprKind::Name(exc_name) = &func.kind {
                            if self.is_builtin_exception(exc_name) {
                                // Validate arguments (should be string message)
                                if !args.is_empty() {
                                    self.check_expr(&mut args[0], Some(&Type::Str))?;
                                }
                                // Set the exception type
                                exc_expr.ty = Some(Type::Exception(exc_name.clone()));
                            } else {
                                // Not a built-in, check normally
                                self.check_expr(exc_expr, None)?;
                            }
                        } else {
                            self.check_expr(exc_expr, None)?;
                        }
                    } else {
                        self.check_expr(exc_expr, None)?;
                    }

                    let exc_ty = exc_expr
                        .ty
                        .as_ref()
                        .ok_or_else(|| self.error(stmt.span, "Exception type unknown"))?;
                    self.validate_exception_type(exc_ty, stmt.span)?;

                    if let Some(cause_expr) = cause {
                        // Similar handling for cause
                        if let ExprKind::Call {
                            func,
                            args,
                            keywords: _,
                        } = &mut cause_expr.kind
                        {
                            if let ExprKind::Name(exc_name) = &func.kind {
                                if self.is_builtin_exception(exc_name) {
                                    if !args.is_empty() {
                                        self.check_expr(&mut args[0], Some(&Type::Str))?;
                                    }
                                    cause_expr.ty = Some(Type::Exception(exc_name.clone()));
                                } else {
                                    self.check_expr(cause_expr, None)?;
                                }
                            } else {
                                self.check_expr(cause_expr, None)?;
                            }
                        } else {
                            self.check_expr(cause_expr, None)?;
                        }

                        let cause_ty = cause_expr
                            .ty
                            .as_ref()
                            .ok_or_else(|| self.error(stmt.span, "Cause type unknown"))?;
                        self.validate_exception_type(cause_ty, stmt.span)?;
                    }
                } else {
                    // Re-raise: must be in except handler
                    if self.except_handler_depth == 0 {
                        return Err(
                            self.error(stmt.span, "Re-raise not allowed outside except handler")
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate and register targets for tuple/list destructuring assignments.
    ///
    /// This mirrors normal assignment checks but walks each leaf in the pattern,
    /// ensuring types line up and creating new bindings when needed.
    fn check_unpack_target(
        &mut self,
        target: &mut AssignTarget,
        value_ty: &Type,
        value_expr: Option<&Expr>,
        span: Span,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Name(name) => {
                if name == "__name__" {
                    return Err(self.error(span, "Assignment to __name__ is not supported"));
                }
                if self.in_function() && self.is_declared_nonlocal(name) {
                    let outer_ty = self
                        .lookup_nonlocal_var(name)
                        .ok_or_else(|| self.error(span, "nonlocal binding not found"))?;
                    if matches!(value_ty, Type::Unknown) && !matches!(outer_ty, Type::Unknown) {
                        return Err(self.error(span, "Unable to infer type; add annotation"));
                    }
                    self.ensure_assignable(value_ty, &outer_ty, span)?;
                } else if self.in_function() && self.is_declared_global(name) {
                    let global_ty = self.ctx.globals.get(name).cloned().ok_or_else(|| {
                        self.error(
                            span,
                            format!("global `{name}` is not defined at module scope"),
                        )
                    })?;
                    if matches!(value_ty, Type::Unknown) && !matches!(global_ty, Type::Unknown) {
                        return Err(self.error(span, "Unable to infer type; add annotation"));
                    }
                    self.ensure_assignable(value_ty, &global_ty, span)?;
                } else if let Some(existing) = self.lookup_var(name) {
                    self.ensure_assignable(value_ty, &existing, span)?;
                } else {
                    if matches!(value_ty, Type::Unknown) {
                        return Err(self.error(span, "Unable to infer type; add annotation"));
                    }
                    self.insert_var(name, value_ty.clone(), span)?;
                }
                // Preserve top-level lambda inference when unpacking literal tuples/lists.
                if !self.in_function()
                    && value_expr.is_some_and(|expr| matches!(expr.kind, ExprKind::Lambda { .. }))
                {
                    if let Some(expr) = value_expr {
                        self.lambda_defs.insert(name.clone(), expr.clone());
                    }
                }
            }
            AssignTarget::Attr { value: obj, attr } => {
                let obj_ty = self.check_expr(obj, None)?;
                if let ExprKind::Name(name) = &obj.kind {
                    if let Some(class_info) = self.ctx.classes.get(name) {
                        if let Some(attr_info) = class_info.class_attrs.get(attr) {
                            self.ensure_assignable(value_ty, &attr_info.ty, span)?;
                            return Ok(());
                        }
                    }
                }
                if let Type::Custom(class_name) = obj_ty {
                    let class_info =
                        self.ctx.classes.get(&class_name).ok_or_else(|| {
                            self.error(span, format!("Unknown class: {class_name}"))
                        })?;
                    if let Some(prop) = class_info.properties.get(attr) {
                        if let Some(setter_name) = &prop.setter {
                            if let Some(sig) = class_info.methods.get(setter_name) {
                                if sig.params.len() >= 2 {
                                    let expected = sig.params[1].clone();
                                    self.ensure_assignable(value_ty, &expected, span)?;
                                }
                                return Ok(());
                            }
                        }
                        return Err(
                            self.error(span, format!("Property {class_name}.{attr} has no setter"))
                        );
                    }
                    let field_ty = class_info.fields.get(attr).ok_or_else(|| {
                        self.error(span, format!("Unknown field {class_name}.{attr}"))
                    })?;
                    self.ensure_assignable(value_ty, field_ty, span)?;
                } else {
                    return Err(
                        self.error(span, "Attribute assignment only allowed on class instances")
                    );
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
                        self.ensure_assignable(&index_ty, &Type::Int, span)?;
                        self.ensure_assignable(value_ty, &inner, span)?;
                    }
                    Type::Dict(key_ty, val_ty) => {
                        self.ensure_assignable(&index_ty, &key_ty, span)?;
                        self.ensure_assignable(value_ty, &val_ty, span)?;
                    }
                    _ => {
                        return Err(self.error(span, "Index assignment requires list or dict"));
                    }
                }
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                let starred_indices: Vec<usize> = items
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
                if starred_indices.len() > 1 {
                    return Err(self.error(span, "Only one starred assignment target is allowed"));
                }
                if let Some(star_idx) = starred_indices.first().copied() {
                    let prefix_len = star_idx;
                    let suffix_len = items.len().saturating_sub(star_idx + 1);
                    match value_ty {
                        Type::Tuple(values) => {
                            if values.len() < (prefix_len + suffix_len) {
                                return Err(self.error(
                                    span,
                                    format!(
                                        "Unpacking expected at least {} values, got {}",
                                        prefix_len + suffix_len,
                                        values.len()
                                    ),
                                ));
                            }
                            for idx in 0..prefix_len {
                                let elem_expr = self.unpack_expr_at(value_expr, idx);
                                self.check_unpack_target(
                                    &mut items[idx],
                                    &values[idx],
                                    elem_expr,
                                    span,
                                )?;
                            }

                            let middle_start = prefix_len;
                            let middle_end = values.len() - suffix_len;
                            let middle_ty =
                                self.merge_many_types(&values[middle_start..middle_end]);
                            let starred_ty = Type::List(Box::new(middle_ty));
                            if let AssignTarget::Starred(inner) = &mut items[star_idx] {
                                self.check_unpack_target(inner, &starred_ty, None, span)?;
                            } else {
                                unreachable!("starred index must point to AssignTarget::Starred");
                            }

                            for offset in 0..suffix_len {
                                let idx = values.len() - suffix_len + offset;
                                let elem_expr = self.unpack_expr_at(value_expr, idx);
                                self.check_unpack_target(
                                    &mut items[star_idx + 1 + offset],
                                    &values[idx],
                                    elem_expr,
                                    span,
                                )?;
                            }
                        }
                        Type::List(inner) => {
                            if let Some(len) = self.unpack_expr_len(value_expr) {
                                if len < (prefix_len + suffix_len) {
                                    return Err(self.error(
                                        span,
                                        format!(
                                            "Unpacking expected at least {} values, got {}",
                                            prefix_len + suffix_len,
                                            len
                                        ),
                                    ));
                                }
                            }
                            for (idx, item_target) in items.iter_mut().take(prefix_len).enumerate()
                            {
                                let elem_expr = self.unpack_expr_at(value_expr, idx);
                                self.check_unpack_target(
                                    item_target,
                                    inner.as_ref(),
                                    elem_expr,
                                    span,
                                )?;
                            }
                            if let AssignTarget::Starred(inner_target) = &mut items[star_idx] {
                                self.check_unpack_target(
                                    inner_target,
                                    &Type::List(inner.clone()),
                                    None,
                                    span,
                                )?;
                            } else {
                                unreachable!("starred index must point to AssignTarget::Starred");
                            }
                            for offset in 0..suffix_len {
                                let elem_expr =
                                    self.unpack_expr_at_from_end(value_expr, suffix_len - offset);
                                self.check_unpack_target(
                                    &mut items[star_idx + 1 + offset],
                                    inner.as_ref(),
                                    elem_expr,
                                    span,
                                )?;
                            }
                        }
                        Type::Unknown => {
                            return Err(self.error(span, "Unable to infer type; add annotation"));
                        }
                        _ => {
                            return Err(self.error(
                                span,
                                "Unpacking assignment requires a tuple or list value",
                            ));
                        }
                    }
                } else {
                    // Unpack element types from the RHS and recurse into each element target.
                    let element_types = self.unpack_element_types(value_ty, items.len(), span)?;
                    let element_exprs = self.unpack_element_exprs(value_expr, items.len());
                    for ((item, elem_ty), elem_expr) in
                        items.iter_mut().zip(element_types).zip(element_exprs)
                    {
                        self.check_unpack_target(item, &elem_ty, elem_expr, span)?;
                    }
                }
            }
            AssignTarget::Starred(inner) => {
                if !matches!(value_ty, Type::List(_)) {
                    return Err(self.error(span, "Starred assignment target expects a list value"));
                }
                self.check_unpack_target(inner, value_ty, value_expr, span)?;
            }
        }
        Ok(())
    }

    /// Merge a sequence of types into one element type for starred unpacking.
    fn merge_many_types(&self, items: &[Type]) -> Type {
        if items.is_empty() {
            return Type::Unknown;
        }
        let mut acc = items[0].clone();
        for ty in &items[1..] {
            acc = Self::merge_types(acc, ty.clone());
        }
        acc
    }

    /// Read unpack source length when it's a literal tuple/list expression.
    fn unpack_expr_len(&self, value_expr: Option<&Expr>) -> Option<usize> {
        let expr = value_expr?;
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => Some(items.len()),
            _ => None,
        }
    }

    /// Extract unpack source element by absolute index for literal tuple/list RHS.
    fn unpack_expr_at<'b>(&self, value_expr: Option<&'b Expr>, idx: usize) -> Option<&'b Expr> {
        let expr = value_expr?;
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => items.get(idx),
            _ => None,
        }
    }

    /// Extract unpack source element by 1-based index from end for literal tuple/list RHS.
    fn unpack_expr_at_from_end<'b>(
        &self,
        value_expr: Option<&'b Expr>,
        from_end: usize,
    ) -> Option<&'b Expr> {
        let expr = value_expr?;
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                if from_end == 0 || from_end > items.len() {
                    None
                } else {
                    items.get(items.len() - from_end)
                }
            }
            _ => None,
        }
    }

    /// Compute the element types for tuple/list unpacking.
    fn unpack_element_types(
        &self,
        value_ty: &Type,
        count: usize,
        span: Span,
    ) -> Result<Vec<Type>, CompileError> {
        match value_ty {
            Type::Tuple(items) => {
                if items.len() != count {
                    return Err(self.error(
                        span,
                        format!("Unpacking expected {count} values, got {}", items.len()),
                    ));
                }
                Ok(items.clone())
            }
            Type::List(inner) => Ok(vec![inner.as_ref().clone(); count]),
            Type::Unknown => Err(self.error(span, "Unable to infer type; add annotation")),
            _ => Err(self.error(span, "Unpacking assignment requires a tuple or list value")),
        }
    }

    /// Extract element expressions when unpacking from a literal tuple/list.
    fn unpack_element_exprs<'b>(
        &self,
        value_expr: Option<&'b Expr>,
        count: usize,
    ) -> Vec<Option<&'b Expr>> {
        if let Some(expr) = value_expr {
            match &expr.kind {
                ExprKind::Tuple(items) | ExprKind::List(items) if items.len() == count => {
                    return items.iter().map(Some).collect();
                }
                _ => {}
            }
        }
        vec![None; count]
    }

    fn check_except_handler(
        &mut self,
        handler: &mut ExceptHandler,
        expected_return: Option<&TypeRef>,
    ) -> Result<(), CompileError> {
        if let Some(exc_type_name) = &handler.exc_type {
            self.validate_exception_name(exc_type_name, handler.span)?;
        }

        // Bind exception to name if present
        if let Some(name) = &handler.name {
            let exc_type = handler
                .exc_type
                .as_ref()
                .map(|t| Type::Exception(t.clone()))
                .unwrap_or(Type::Exception("PyError".to_string()));
            self.insert_var(name, exc_type, handler.span)?;
        }

        self.except_handler_depth += 1;
        for stmt in &mut handler.body {
            self.check_stmt(stmt, expected_return)?;
        }
        self.except_handler_depth -= 1;

        Ok(())
    }

    fn validate_exception_type(&self, ty: &Type, span: Span) -> Result<(), CompileError> {
        match ty {
            Type::Exception(_) => Ok(()),
            Type::Custom(name) if self.is_builtin_exception(name) => Ok(()),
            Type::Custom(name) if self.ctx.classes.contains_key(name) => Ok(()),
            _ => Err(self.error(span, "Invalid exception type")),
        }
    }

    fn validate_exception_name(&self, name: &str, span: Span) -> Result<(), CompileError> {
        if self.is_builtin_exception(name) || self.ctx.classes.contains_key(name) {
            Ok(())
        } else {
            Err(self.error(span, format!("Unknown exception type: {}", name)))
        }
    }

    fn is_builtin_exception(&self, name: &str) -> bool {
        matches!(
            name,
            "Exception"
                | "ValueError"
                | "TypeError"
                | "RuntimeError"
                | "KeyError"
                | "IndexError"
                | "AttributeError"
                | "ZeroDivisionError"
                | "NameError"
                | "AssertionError"
                | "StopIteration"
                | "NotImplementedError"
                | "IOError"
                | "OverflowError"
        )
    }
}
