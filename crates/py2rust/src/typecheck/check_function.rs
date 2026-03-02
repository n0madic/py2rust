use super::*;

/// Function definition type checking.
///
/// Functions are type-checked in their own scope with:
/// 1. Parameters pre-populated with their declared types
/// 2. Return type validated against all return statements
/// 3. Automatic return type inference if not annotated
///
/// Special handling:
/// - Methods receive `self` parameter (checked against class type)
/// - Return type inference scans all return statements
/// - Unknown parameter types can be inferred from usage
/// - Global variable handling (must be declared before use)
impl<'a> TypeChecker<'a> {
    /// Type check a function definition.
    ///
    /// If class_name is provided, this is a method and we validate `self` parameter.
    pub(super) fn check_function(
        &mut self,
        func: &mut Function,
        class_name: Option<&str>,
        require_self: bool,
    ) -> Result<(), CompileError> {
        let prev_class = self.current_class.clone();
        self.current_class = class_name.map(|name| name.to_string());
        // Create a new scope for this function
        self.scopes.push(HashMap::new());
        self.global_scopes.push(GlobalScope::default());
        self.nonlocal_scopes.push(NonlocalScope::default());
        self.function_scopes.push(self.scopes.len() - 1);
        // For methods, validate and insert `self` parameter
        if require_self {
            if let Some(class_name) = class_name {
                if let Some(first) = func.params.first() {
                    let self_ty = self.resolve_type_ref(&first.ann, first.span)?;
                    self.insert_var(&first.name, self_ty, first.span)?;
                    if first.name != "self" {
                        return Err(
                            self.error(first.span, "First parameter in methods must be self")
                        );
                    }
                } else {
                    return Err(self.error(func.span, "Methods must take self parameter"));
                }
                if class_name != func.params[0].ann.to_string() && !class_name.is_empty() {
                    // Ignore mismatch for now if annotated differently.
                }
            }
        }
        for param in func.params.iter().skip(if require_self { 1 } else { 0 }) {
            let mut ty = self.resolve_param_type(param)?;
            // For dunder operator methods, force `other` to class type when Unknown.
            // These methods are invoked through trait impls, not called directly,
            // so call-site inference cannot determine the parameter type.
            if matches!(ty, Type::Unknown) && param.name == "other" {
                if let Some(cn) = class_name {
                    const DUNDER_OPS: &[&str] = &[
                        "__add__",
                        "__radd__",
                        "__sub__",
                        "__rsub__",
                        "__mul__",
                        "__rmul__",
                        "__truediv__",
                        "__rtruediv__",
                    ];
                    if DUNDER_OPS.contains(&func.name.as_str()) {
                        ty = Type::Custom(cn.to_string());
                    }
                    // __pow__ always takes a numeric exponent, not the class type.
                    // Force to Float since ** can be called with both int and float.
                    if func.name == "__pow__" {
                        ty = Type::Float;
                    }
                }
            }
            self.insert_var(&param.name, ty, param.span)?;
        }

        // Type check default argument expressions.
        for idx in 0..func.params.len() {
            let expected = if require_self && idx == 0 {
                None
            } else {
                Some(self.resolve_param_type(&func.params[idx])?)
            };
            if let Some(default) = &mut func.params[idx].default {
                let ty = self.check_expr(default, expected.as_ref())?;
                if let Some(expected) = expected {
                    if !matches!(ty, Type::Unknown) && !matches!(expected, Type::Unknown) {
                        // Allow empty tuple () as default for list params (Python idiom).
                        let is_empty_tuple_to_list = matches!(&ty, Type::Tuple(items) if items.is_empty())
                            && matches!(expected, Type::List(_));
                        if !is_empty_tuple_to_list {
                            self.ensure_assignable(&ty, &expected, default.span)?;
                        }
                    }
                }
            }
        }

        // Track whether this function uses `yield` and what item type it produces.
        self.generator_yield_stack.push(None);
        for stmt in &mut func.body {
            self.check_stmt(stmt, Some(&func.ret))?;
        }
        // Back-propagate refined container types from scope to Let expressions.
        // When `visited = set()` is later refined to `Set(Value)` via `.add(v)`,
        // the Let's expr.ty still says `Set(Unknown)`. This pass fixes that so
        // codegen emits `HashSet<Value>` instead of `HashSet<PyRepr>`.
        if let Some(scope) = self.scopes.last() {
            Self::propagate_refined_types_to_lets(&mut func.body, scope);
        }
        let inferred_yield = self
            .generator_yield_stack
            .pop()
            .expect("generator_yield_stack push/pop must be balanced");
        let is_generator = inferred_yield.is_some();
        if let Some(yield_ty) = inferred_yield {
            let inferred_iter = Type::Iterator(Box::new(yield_ty.clone()));
            if matches!(func.ret, TypeRef::Unknown) {
                func.ret = TypeRef::Iterator(Box::new(Self::type_to_ref(&yield_ty)));
            } else {
                let declared = self.resolve_type_ref(&func.ret, func.span)?;
                if !matches!(declared, Type::Iterator(_)) {
                    return Err(self.error(func.span, "Generator function must return Iterator[T]"));
                }
                self.ensure_assignable(&inferred_iter, &declared, func.span)?;
            }
        }

        // Infer return type if not annotated or still partially Unknown.
        // We scan all return statements and find a common type.
        // Also re-infer when the current return annotation contains Unknown (from
        // a previous pass that couldn't fully resolve types).
        let ret_needs_inference = matches!(func.ret, TypeRef::Unknown)
            || self
                .resolve_type_ref(&func.ret, func.span)
                .ok()
                .is_some_and(|ty| ty.contains_unknown());
        if ret_needs_inference {
            let mut inferred: Option<Type> = None;
            // Recursively visit statements to find return statements
            fn visit(stmt: &Stmt, inferred: &mut Option<Type>) {
                match &stmt.kind {
                    StmtKind::Return { value } => {
                        let ty = match value {
                            Some(expr) => expr.ty.clone().unwrap_or(Type::Unknown),
                            None => Type::None,
                        };
                        if let Some(existing) = inferred {
                            if existing != &ty {
                                *inferred = Some(Type::Unknown);
                            }
                        } else {
                            *inferred = Some(ty);
                        }
                    }
                    StmtKind::If { body, orelse, .. } => {
                        for stmt in body {
                            visit(stmt, inferred);
                        }
                        for stmt in orelse {
                            visit(stmt, inferred);
                        }
                    }
                    StmtKind::While { body, .. } => {
                        for stmt in body {
                            visit(stmt, inferred);
                        }
                    }
                    StmtKind::For { body, .. } => {
                        for stmt in body {
                            visit(stmt, inferred);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            for stmt in &case.body {
                                visit(stmt, inferred);
                            }
                        }
                    }
                    _ => {}
                }
            }
            for stmt in &func.body {
                visit(stmt, &mut inferred);
            }
            if let Some(ty) = inferred {
                if !matches!(ty, Type::Unknown) {
                    func.ret = Self::type_to_ref(&ty);
                }
            } else {
                func.ret = TypeRef::None;
            }
        }

        // Update inferred parameter types in the function signature.
        if let Some(scope) = self.scopes.last() {
            for param in &mut func.params {
                if let Some(ty) = scope.get(&param.name) {
                    if matches!(ty, Type::Unknown) {
                        continue;
                    }
                    if matches!(param.ann, TypeRef::Unknown) {
                        // Fully unknown annotation → adopt scope type.
                        param.ann = Self::type_to_ref(ty);
                    } else if let Ok(current) = self.resolve_type_ref(&param.ann, param.span) {
                        // Annotation has nested Unknown (e.g. List(List(Unknown))) →
                        // refine from scope if the scope type is fully resolved.
                        if current.contains_unknown() && !ty.contains_unknown() {
                            param.ann = Self::type_to_ref(ty);
                        }
                    }
                }
            }
        }
        if func
            .params
            .iter()
            .any(|p| matches!(p.ann, TypeRef::Unknown))
        {
            use std::collections::HashSet;
            let mut string_params = HashSet::new();
            fn is_str_expr(expr: &Expr) -> bool {
                matches!(&expr.kind, ExprKind::Literal(Literal::Str(_)))
                    || matches!(expr.ty.as_ref(), Some(Type::Str))
            }
            fn collect_names(expr: &Expr, out: &mut HashSet<String>) {
                match &expr.kind {
                    ExprKind::Name(name) => {
                        out.insert(name.clone());
                    }
                    ExprKind::Binary { left, right, .. } => {
                        collect_names(left, out);
                        collect_names(right, out);
                    }
                    ExprKind::Call {
                        func,
                        args,
                        keywords,
                    } => {
                        collect_names(func, out);
                        for arg in args {
                            collect_names(arg, out);
                        }
                        for kw in keywords {
                            collect_names(&kw.value, out);
                        }
                    }
                    ExprKind::Starred { value } => collect_names(value, out),
                    ExprKind::Yield { value } => {
                        if let Some(value) = value {
                            collect_names(value, out);
                        }
                    }
                    ExprKind::Attr { value, .. } => collect_names(value, out),
                    ExprKind::Compare { left, right, .. } => {
                        collect_names(left, out);
                        collect_names(right, out);
                    }
                    ExprKind::CompareChain {
                        left, comparators, ..
                    } => {
                        collect_names(left, out);
                        for cmp in comparators {
                            collect_names(cmp, out);
                        }
                    }
                    ExprKind::Unary { expr: inner, .. } => collect_names(inner, out),
                    ExprKind::BoolOp { values, .. } => {
                        for v in values {
                            collect_names(v, out);
                        }
                    }
                    ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                        for item in items {
                            collect_names(item, out);
                        }
                    }
                    ExprKind::Dict(items) => {
                        for item in items {
                            match item {
                                DictEntry::Item { key, value } => {
                                    collect_names(key, out);
                                    collect_names(value, out);
                                }
                                DictEntry::Unpack { value } => collect_names(value, out),
                            }
                        }
                    }
                    ExprKind::Index { value, index } => {
                        collect_names(value, out);
                        collect_names(index, out);
                    }
                    ExprKind::Slice {
                        value,
                        start,
                        end,
                        step,
                    } => {
                        collect_names(value, out);
                        if let Some(s) = start {
                            collect_names(s, out);
                        }
                        if let Some(e) = end {
                            collect_names(e, out);
                        }
                        if let Some(st) = step.as_deref() {
                            collect_names(st, out);
                        }
                    }
                    ExprKind::ListComp { elt, iter, ifs, .. }
                    | ExprKind::SetComp { elt, iter, ifs, .. } => {
                        collect_names(elt, out);
                        collect_names(iter, out);
                        for cond in ifs {
                            collect_names(cond, out);
                        }
                    }
                    ExprKind::UnionCtor { inner, .. } => collect_names(inner, out),
                    ExprKind::Lambda { body, .. } => collect_names(body, out),
                    ExprKind::IfExpr { test, body, orelse } => {
                        collect_names(test, out);
                        collect_names(body, out);
                        collect_names(orelse, out);
                    }
                    ExprKind::Block { stmts } => {
                        for stmt in stmts {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    ExprKind::Literal(_) => {}
                }
            }
            fn visit_expr(expr: &Expr, out: &mut HashSet<String>) {
                match &expr.kind {
                    ExprKind::Binary {
                        op: BinOp::Add,
                        left,
                        right,
                    } => {
                        if is_str_expr(left) {
                            collect_names(right, out);
                        }
                        if is_str_expr(right) {
                            collect_names(left, out);
                        }
                        visit_expr(left, out);
                        visit_expr(right, out);
                    }
                    ExprKind::Binary { left, right, .. } => {
                        visit_expr(left, out);
                        visit_expr(right, out);
                    }
                    ExprKind::Call {
                        func,
                        args,
                        keywords,
                    } => {
                        visit_expr(func, out);
                        for arg in args {
                            visit_expr(arg, out);
                        }
                        for kw in keywords {
                            visit_expr(&kw.value, out);
                        }
                    }
                    ExprKind::Starred { value } => visit_expr(value, out),
                    ExprKind::Yield { value } => {
                        if let Some(value) = value {
                            visit_expr(value, out);
                        }
                    }
                    ExprKind::Attr { value, .. } => visit_expr(value, out),
                    ExprKind::Compare { left, right, .. } => {
                        visit_expr(left, out);
                        visit_expr(right, out);
                    }
                    ExprKind::CompareChain {
                        left, comparators, ..
                    } => {
                        visit_expr(left, out);
                        for cmp in comparators {
                            visit_expr(cmp, out);
                        }
                    }
                    ExprKind::Unary { expr: inner, .. } => visit_expr(inner, out),
                    ExprKind::BoolOp { values, .. } => {
                        for v in values {
                            visit_expr(v, out);
                        }
                    }
                    ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                        for item in items {
                            visit_expr(item, out);
                        }
                    }
                    ExprKind::Dict(items) => {
                        for item in items {
                            match item {
                                DictEntry::Item { key, value } => {
                                    visit_expr(key, out);
                                    visit_expr(value, out);
                                }
                                DictEntry::Unpack { value } => visit_expr(value, out),
                            }
                        }
                    }
                    ExprKind::Index { value, index } => {
                        visit_expr(value, out);
                        visit_expr(index, out);
                    }
                    ExprKind::Slice {
                        value,
                        start,
                        end,
                        step,
                    } => {
                        visit_expr(value, out);
                        if let Some(s) = start {
                            visit_expr(s, out);
                        }
                        if let Some(e) = end {
                            visit_expr(e, out);
                        }
                        if let Some(st) = step.as_deref() {
                            visit_expr(st, out);
                        }
                    }
                    ExprKind::ListComp { elt, iter, ifs, .. }
                    | ExprKind::SetComp { elt, iter, ifs, .. } => {
                        visit_expr(elt, out);
                        visit_expr(iter, out);
                        for cond in ifs {
                            visit_expr(cond, out);
                        }
                    }
                    ExprKind::UnionCtor { inner, .. } => visit_expr(inner, out),
                    ExprKind::Lambda { body, .. } => visit_expr(body, out),
                    ExprKind::IfExpr { test, body, orelse } => {
                        visit_expr(test, out);
                        visit_expr(body, out);
                        visit_expr(orelse, out);
                    }
                    ExprKind::Block { stmts } => {
                        for stmt in stmts {
                            visit_stmt(stmt, out);
                        }
                    }
                    ExprKind::Literal(_) | ExprKind::Name(_) => {}
                }
            }
            fn collect_names_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
                fn collect_names_in_target(target: &AssignTarget, out: &mut HashSet<String>) {
                    match target {
                        AssignTarget::Attr { value, .. } => collect_names(value, out),
                        AssignTarget::Index { value, index } => {
                            collect_names(value, out);
                            collect_names(index, out);
                        }
                        AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                            for item in items {
                                collect_names_in_target(item, out);
                            }
                        }
                        AssignTarget::Starred(inner) => collect_names_in_target(inner, out),
                        AssignTarget::Name(_) => {}
                    }
                }

                match &stmt.kind {
                    StmtKind::Let { value, .. } => collect_names(value, out),
                    StmtKind::Assign { value, .. } => collect_names(value, out),
                    StmtKind::Delete { target } => collect_names_in_target(target, out),
                    StmtKind::Return { value } => {
                        if let Some(expr) = value {
                            collect_names(expr, out);
                        }
                    }
                    StmtKind::If { test, body, orelse } => {
                        collect_names(test, out);
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                        for stmt in orelse {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::While { test, body } => {
                        collect_names(test, out);
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::For { iter, body, .. } => {
                        collect_names(iter, out);
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::Expr(expr) => collect_names(expr, out),
                    StmtKind::Assert { test, msg } => {
                        collect_names(test, out);
                        if let Some(msg) = msg {
                            collect_names(msg, out);
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        collect_names(subject, out);
                        for case in cases {
                            for stmt in &case.body {
                                collect_names_in_stmt(stmt, out);
                            }
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                        for handler in handlers {
                            for stmt in &handler.body {
                                collect_names_in_stmt(stmt, out);
                            }
                        }
                        for stmt in orelse {
                            collect_names_in_stmt(stmt, out);
                        }
                        for stmt in finalbody {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::Raise { exc, cause } => {
                        if let Some(expr) = exc {
                            collect_names(expr, out);
                        }
                        if let Some(expr) = cause {
                            collect_names(expr, out);
                        }
                    }
                    StmtKind::Import { .. }
                    | StmtKind::ImportFrom { .. }
                    | StmtKind::Global { .. }
                    | StmtKind::Nonlocal { .. } => {}
                    StmtKind::Class { def } => {
                        for attr in &def.class_attrs {
                            collect_names(&attr.value, out);
                        }
                        for method in &def.methods {
                            for stmt in &method.body {
                                collect_names_in_stmt(stmt, out);
                            }
                        }
                    }
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
            fn visit_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
                fn visit_target_exprs(target: &AssignTarget, out: &mut HashSet<String>) {
                    match target {
                        AssignTarget::Attr { value, .. } => visit_expr(value, out),
                        AssignTarget::Index { value, index } => {
                            visit_expr(value, out);
                            visit_expr(index, out);
                        }
                        AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                            for item in items {
                                visit_target_exprs(item, out);
                            }
                        }
                        AssignTarget::Starred(inner) => visit_target_exprs(inner, out),
                        AssignTarget::Name(_) => {}
                    }
                }

                match &stmt.kind {
                    StmtKind::Let { value, .. } => visit_expr(value, out),
                    StmtKind::Assign { value, .. } => visit_expr(value, out),
                    StmtKind::Delete { target } => visit_target_exprs(target, out),
                    StmtKind::Return { value } => {
                        if let Some(expr) = value {
                            visit_expr(expr, out);
                        }
                    }
                    StmtKind::If { test, body, orelse } => {
                        visit_expr(test, out);
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                        for stmt in orelse {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::While { test, body } => {
                        visit_expr(test, out);
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::For { iter, body, .. } => {
                        visit_expr(iter, out);
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::Expr(expr) => visit_expr(expr, out),
                    StmtKind::Assert { test, msg } => {
                        visit_expr(test, out);
                        if let Some(msg) = msg {
                            visit_expr(msg, out);
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        visit_expr(subject, out);
                        for case in cases {
                            for stmt in &case.body {
                                visit_stmt(stmt, out);
                            }
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                        for handler in handlers {
                            for stmt in &handler.body {
                                visit_stmt(stmt, out);
                            }
                        }
                        for stmt in orelse {
                            visit_stmt(stmt, out);
                        }
                        for stmt in finalbody {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::Raise { exc, cause } => {
                        if let Some(expr) = exc {
                            visit_expr(expr, out);
                        }
                        if let Some(expr) = cause {
                            visit_expr(expr, out);
                        }
                    }
                    StmtKind::Import { .. }
                    | StmtKind::ImportFrom { .. }
                    | StmtKind::Global { .. }
                    | StmtKind::Nonlocal { .. } => {}
                    StmtKind::Class { def } => {
                        for attr in &def.class_attrs {
                            visit_expr(&attr.value, out);
                        }
                        for method in &def.methods {
                            for stmt in &method.body {
                                visit_stmt(stmt, out);
                            }
                        }
                    }
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
            for stmt in &func.body {
                visit_stmt(stmt, &mut string_params);
            }
            for param in &mut func.params {
                if matches!(param.ann, TypeRef::Unknown) && string_params.contains(&param.name) {
                    param.ann = TypeRef::Name("str".to_string());
                }
            }
        }
        let mut params = self.resolve_params(&func.params)?;
        if let Some(scope) = self.scopes.last() {
            for (idx, param) in func.params.iter().enumerate() {
                let inferred = scope.get(&param.name).cloned();
                if let Some(inferred) = inferred {
                    if matches!(params.get(idx), Some(Type::Unknown))
                        && !inferred.contains_unknown()
                    {
                        // Refine unannotated parameter types from in-body usage.
                        // This keeps generated Rust signatures concrete while preserving
                        // CPython-like flexibility for untyped source.
                        if idx < params.len() {
                            params[idx] = inferred;
                        }
                    } else if let Some(Type::Option(inner)) = params.get(idx) {
                        if matches!(inner.as_ref(), Type::Unknown) && !inferred.contains_unknown() {
                            // Refine Optional[Unknown] (from `param=None`) using in-body usage.
                            // If inference already yields Optional[T], keep T; otherwise wrap T.
                            let refined_inner = if let Type::Option(inferred_inner) = inferred {
                                *inferred_inner
                            } else {
                                inferred
                            };
                            if idx < params.len() {
                                params[idx] = Type::Option(Box::new(refined_inner));
                            }
                        }
                    } else if let Some(current) = params.get(idx) {
                        // Refine params with nested Unknown (e.g. List(List(Unknown)))
                        // using body-inferred types.
                        if current.contains_unknown()
                            && !inferred.contains_unknown()
                            && idx < params.len()
                        {
                            params[idx] = inferred;
                        }
                    }
                }
            }
        }
        // CPython-compat compromise:
        // unannotated parameters with a `None` default are treated as Optional[T]
        // after local inference of T. This keeps default binding semantics correct
        // in generated Rust while preserving `is None` control-flow narrowing.
        for (idx, param) in func.params.iter().enumerate() {
            let has_none_default = param
                .default
                .as_ref()
                .is_some_and(|d| matches!(d.kind, ExprKind::Literal(Literal::None)));
            if !has_none_default {
                continue;
            }
            let Some(current) = params.get(idx).cloned() else {
                continue;
            };
            if matches!(current, Type::Option(_)) {
                continue;
            }
            let inner = if matches!(current, Type::None) {
                Type::Unknown
            } else {
                current
            };
            if idx < params.len() {
                params[idx] = Type::Option(Box::new(inner));
            }
        }
        let ret = self.resolve_type_ref(&func.ret, func.span)?;
        let defaults = func.params.iter().filter(|p| p.default.is_some()).count();
        for (param, ty) in func.params.iter().zip(params.iter()) {
            if param.default.is_some() {
                let gname = if let Some(class_name) = class_name {
                    format!("__default_{}_{}_{}", class_name, func.name, param.name)
                } else {
                    format!("__default_{}_{}", func.name, param.name)
                };
                // Always update default globals with the latest (most refined) type.
                // Re-check passes may produce more accurate types than initial passes.
                let entry = self.ctx.globals.entry(gname);
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        let existing = e.get();
                        // Update if current type is less specific.
                        let is_trivial = matches!(existing, Type::Unknown | Type::None)
                            || matches!(existing, Type::Tuple(items) if items.is_empty());
                        if is_trivial && !matches!(ty, Type::Unknown | Type::None) {
                            e.insert(ty.clone());
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(ty.clone());
                    }
                }
            }
        }
        let sig = FunctionSig {
            param_names: func.params.iter().map(|p| p.name.clone()).collect(),
            param_kinds: func.params.iter().map(|p| p.kind).collect(),
            has_defaults: func.params.iter().map(|p| p.default.is_some()).collect(),
            params: params.clone(),
            ret: ret.clone(),
            span: func.span,
            is_generator,
            can_throw: false,
            thrown_exceptions: Vec::new(),
            defaults,
        };
        if let Some(class_name) = class_name {
            if let Some(class_info) = self.ctx.classes.get_mut(class_name) {
                class_info.methods.insert(func.name.clone(), sig.clone());
                if func.name == "__init__" {
                    class_info.init = Some(sig);
                }
            }
        } else {
            self.ctx.functions.insert(func.name.clone(), sig);
        }

        self.global_scopes.pop();
        self.nonlocal_scopes.pop();
        self.scopes.pop();
        self.function_scopes.pop();
        self.current_class = prev_class;
        Ok(())
    }

    /// Walk `Let` statements and update stale `Unknown`-containing expression types
    /// from the scope's refined variable types.
    ///
    /// This handles the case where `visited = set()` initially gets `Set(Unknown)`,
    /// but later `.add(v)` refines the scope entry to `Set(Value)`. Without this
    /// pass, codegen reads the stale `Set(Unknown)` from the `Let` expression and
    /// emits `HashSet<PyRepr>` instead of `HashSet<Value>`.
    fn propagate_refined_types_to_lets(stmts: &mut [Stmt], scope: &HashMap<String, Type>) {
        for stmt in stmts.iter_mut() {
            match &mut stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    if let Some(scope_ty) = scope.get(name) {
                        if let Some(expr_ty) = &value.ty {
                            if expr_ty.contains_unknown() && !scope_ty.contains_unknown() {
                                value.ty = Some(scope_ty.clone());
                            }
                        }
                    }
                }
                StmtKind::If { body, orelse, .. } => {
                    Self::propagate_refined_types_to_lets(body, scope);
                    Self::propagate_refined_types_to_lets(orelse, scope);
                }
                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                    Self::propagate_refined_types_to_lets(body, scope);
                }
                StmtKind::Match { cases, .. } => {
                    for case in cases {
                        Self::propagate_refined_types_to_lets(&mut case.body, scope);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    Self::propagate_refined_types_to_lets(body, scope);
                    for handler in handlers {
                        Self::propagate_refined_types_to_lets(&mut handler.body, scope);
                    }
                    Self::propagate_refined_types_to_lets(orelse, scope);
                    Self::propagate_refined_types_to_lets(finalbody, scope);
                }
                _ => {}
            }
        }
    }
}
