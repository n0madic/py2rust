use super::util::collect_assign_counts;
use super::*;

/// Statement code generation.
///
/// Statements are the imperative building blocks of programs. This module handles:
/// - Control flow (if, while, for, match)
/// - Variable binding (let, assign)
/// - Exception handling (try/except/finally/raise)
/// - Returns and breaks
///
/// Key challenges:
/// 1. Variable mutability: Rust requires `mut` annotations, Python doesn't
/// 2. Exception handling: Python's try/except maps to Rust's Result + closures
/// 3. Scope management: Python's function-level scope vs Rust's block scope
/// 4. Lambda captures: Determining when lambdas need `move` keyword

impl<'a> Codegen<'a> {
    /// Determine if a lambda captures variables from outer scope.
    ///
    /// Why this matters:
    /// - Lambdas that capture must use `move` keyword in Rust
    /// - This transfers ownership of captured variables into the closure
    /// - Without `move`, we'd get borrow checker errors when the lambda outlives
    ///   the scope that created it
    ///
    /// This function analyzes the lambda body to detect references to variables
    /// defined in outer scopes (but not in the lambda's own parameters).
    fn lambda_captures_outer(&self, name: &str, params: &[String], body: &Expr) -> bool {
        let outer_locals: HashSet<String> = match &self.local_vars {
            Some(vars) => vars.keys().cloned().collect(),
            None => return false,
        };
        if outer_locals.is_empty() {
            return false;
        }

        let mut local_defs: HashSet<String> = params.iter().cloned().collect();
        local_defs.insert(name.to_string());
        let mut global_names = HashSet::new();

        let stmts = match &body.kind {
            ExprKind::Block { stmts } => stmts,
            _ => return false,
        };

        fn collect_local_defs(
            stmts: &[Stmt],
            locals: &mut HashSet<String>,
            globals: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Let { name, .. } => {
                        locals.insert(name.clone());
                    }
                    StmtKind::Assign { target, .. } => {
                        if let AssignTarget::Name(name) = target {
                            locals.insert(name.clone());
                        }
                    }
                    StmtKind::For { target, body, .. } => {
                        locals.insert(target.clone());
                        collect_local_defs(body, locals, globals);
                    }
                    StmtKind::If { body, orelse, .. } => {
                        collect_local_defs(body, locals, globals);
                        collect_local_defs(orelse, locals, globals);
                    }
                    StmtKind::While { body, .. } => {
                        collect_local_defs(body, locals, globals);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            for binding in &case.bindings {
                                locals.insert(binding.clone());
                            }
                            collect_local_defs(&case.body, locals, globals);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        collect_local_defs(body, locals, globals);
                        for handler in handlers {
                            if let Some(name) = &handler.name {
                                locals.insert(name.clone());
                            }
                            collect_local_defs(&handler.body, locals, globals);
                        }
                        collect_local_defs(orelse, locals, globals);
                        collect_local_defs(finalbody, locals, globals);
                    }
                    StmtKind::Global { names } => {
                        for name in names {
                            globals.insert(name.clone());
                        }
                    }
                    StmtKind::Expr(_)
                    | StmtKind::Assert { .. }
                    | StmtKind::Raise { .. }
                    | StmtKind::Return { .. } => {}
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
        }

        fn expr_uses_outer(
            expr: &Expr,
            locals: &HashSet<String>,
            globals: &HashSet<String>,
            outers: &HashSet<String>,
        ) -> bool {
            match &expr.kind {
                ExprKind::Name(name) => {
                    outers.contains(name) && !locals.contains(name) && !globals.contains(name)
                }
                ExprKind::Literal(_) => false,
                ExprKind::Lambda { .. } => false,
                ExprKind::Block { .. } => false,
                ExprKind::Call { func, args } => {
                    expr_uses_outer(func, locals, globals, outers)
                        || args
                            .iter()
                            .any(|arg| expr_uses_outer(arg, locals, globals, outers))
                }
                ExprKind::Attr { value, .. } => expr_uses_outer(value, locals, globals, outers),
                ExprKind::Binary { left, right, .. } => {
                    expr_uses_outer(left, locals, globals, outers)
                        || expr_uses_outer(right, locals, globals, outers)
                }
                ExprKind::Unary { expr, .. } => expr_uses_outer(expr, locals, globals, outers),
                ExprKind::Compare { left, right, .. } => {
                    expr_uses_outer(left, locals, globals, outers)
                        || expr_uses_outer(right, locals, globals, outers)
                }
                ExprKind::BoolOp { values, .. } => values
                    .iter()
                    .any(|v| expr_uses_outer(v, locals, globals, outers)),
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => items
                    .iter()
                    .any(|e| expr_uses_outer(e, locals, globals, outers)),
                ExprKind::Dict(items) => items.iter().any(|(k, v)| {
                    expr_uses_outer(k, locals, globals, outers)
                        || expr_uses_outer(v, locals, globals, outers)
                }),
                ExprKind::Index { value, index } => {
                    expr_uses_outer(value, locals, globals, outers)
                        || expr_uses_outer(index, locals, globals, outers)
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    expr_uses_outer(value, locals, globals, outers)
                        || start
                            .as_deref()
                            .is_some_and(|s| expr_uses_outer(s, locals, globals, outers))
                        || end
                            .as_deref()
                            .is_some_and(|e| expr_uses_outer(e, locals, globals, outers))
                        || step
                            .as_deref()
                            .is_some_and(|st| expr_uses_outer(st, locals, globals, outers))
                }
                ExprKind::ListComp { elt, iter, ifs, .. } => {
                    expr_uses_outer(elt, locals, globals, outers)
                        || expr_uses_outer(iter, locals, globals, outers)
                        || ifs
                            .iter()
                            .any(|i| expr_uses_outer(i, locals, globals, outers))
                }
                ExprKind::UnionCtor { inner, .. } => {
                    expr_uses_outer(inner, locals, globals, outers)
                }
                ExprKind::IfExpr { test, body, orelse } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || expr_uses_outer(body, locals, globals, outers)
                        || expr_uses_outer(orelse, locals, globals, outers)
                }
            }
        }

        fn stmt_uses_outer(
            stmt: &Stmt,
            locals: &HashSet<String>,
            globals: &HashSet<String>,
            outers: &HashSet<String>,
        ) -> bool {
            match &stmt.kind {
                StmtKind::Let { value, .. } => expr_uses_outer(value, locals, globals, outers),
                StmtKind::Assign { value, .. } => expr_uses_outer(value, locals, globals, outers),
                StmtKind::Return { value } => value
                    .as_ref()
                    .is_some_and(|expr| expr_uses_outer(expr, locals, globals, outers)),
                StmtKind::If { test, body, orelse } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || body
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        || orelse
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::While { test, body } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || body
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::For { iter, body, .. } => {
                    expr_uses_outer(iter, locals, globals, outers)
                        || body
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::Expr(expr) => expr_uses_outer(expr, locals, globals, outers),
                StmtKind::Assert { test, msg } => {
                    expr_uses_outer(test, locals, globals, outers)
                        || msg
                            .as_ref()
                            .is_some_and(|m| expr_uses_outer(m, locals, globals, outers))
                }
                StmtKind::Match { subject, cases } => {
                    expr_uses_outer(subject, locals, globals, outers)
                        || cases.iter().any(|c| {
                            c.body
                                .iter()
                                .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        })
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    body.iter()
                        .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        || handlers.iter().any(|h| {
                            h.body
                                .iter()
                                .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        })
                        || orelse
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                        || finalbody
                            .iter()
                            .any(|s| stmt_uses_outer(s, locals, globals, outers))
                }
                StmtKind::Raise { exc, cause } => {
                    exc.as_ref()
                        .is_some_and(|e| expr_uses_outer(e, locals, globals, outers))
                        || cause
                            .as_ref()
                            .is_some_and(|c| expr_uses_outer(c, locals, globals, outers))
                }
                StmtKind::Global { .. } | StmtKind::Break | StmtKind::Continue => false,
            }
        }

        collect_local_defs(stmts, &mut local_defs, &mut global_names);
        stmts
            .iter()
            .any(|stmt| stmt_uses_outer(stmt, &local_defs, &global_names, &outer_locals))
    }

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
                    return Ok(());
                }
                if let ExprKind::Lambda { params, body } = &value.kind {
                    if let ExprKind::Block { stmts } = &body.kind {
                        // Nested def: inside a function, emit a closure to allow captures.
                        if self.current_function.is_some()
                            && self.lambda_captures_outer(name, params, body)
                        {
                            let expected = if let Some(ann) = ann {
                                Some(self.resolve_type_ref(ann, stmt.span)?)
                            } else {
                                None
                            };
                            let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                            let mut_kw = if mut_counts.get(name).copied().unwrap_or(0) > 1 {
                                "mut "
                            } else {
                                ""
                            };
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
                let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                let mut_kw = if mut_counts.get(name).copied().unwrap_or(0) > 1 {
                    "mut "
                } else {
                    ""
                };
                if ann.is_some() {
                    let ty = expected.expect("resolved above");
                    let ty_str = self.rust_type(&ty);
                    self.push_line(&format!("let {}{}: {} = {};", mut_kw, name, ty_str, expr));
                    self.set_local_var_type(name, ty);
                } else {
                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                    }
                }
            }
            StmtKind::Assign { target, value } => match target {
                AssignTarget::Name(name) => {
                    if self.is_global(name) {
                        let expected = self.ctx.globals.get(name).cloned();
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        let expr = self.wrap_global_value(expr, value, expected.as_ref());
                        let gname = self.global_name(name);
                        self.push_line(&format!(
                            "*{}.get().unwrap().lock().unwrap() = {};",
                            gname, expr
                        ));
                    } else {
                        let expected = self.local_var_type(name).cloned();
                        let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                        self.push_line(&format!("{} = {};", name, expr));
                    }
                }
                AssignTarget::Attr { value: obj, attr } => {
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
                            self.push_line("{");
                            self.indent += 1;
                            self.push_line(&format!(
                                "let mut {} = {};",
                                guard,
                                self.global_lock_expr(name)
                            ));
                            if let Some(Type::Dict(_, _)) = container.ty.as_ref() {
                                let idx_expr = self.gen_expr(index)?;
                                self.push_line(&format!(
                                    "{}.insert({}, {});",
                                    guard, idx_expr, val_expr
                                ));
                            } else if matches!(
                                container.ty.as_ref(),
                                Some(Type::List(_)) | Some(Type::Tuple(_))
                            ) {
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
                        self.push_line(&format!(
                            "{}.insert({}, {});",
                            cont_expr, idx_expr, val_expr
                        ));
                    } else if matches!(
                        container.ty.as_ref(),
                        Some(Type::List(_)) | Some(Type::Tuple(_))
                    ) {
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
            },
            StmtKind::Return { value } => {
                // Check if we're in a throwing function or inside a try block with value return
                let in_throwing_fn = self.current_function_throws();
                let in_try_with_value = self.try_block_return_type.is_some();

                // Inside try block with value returns, always wrap in Ok
                let wrap_in_ok = in_throwing_fn || in_try_with_value;

                if let Some(expr) = value {
                    let expected = self.current_function_ret.as_ref().map(|ty| {
                        if let Some((ok, _)) = ty.unwrap_result() {
                            ok.clone()
                        } else {
                            ty.clone()
                        }
                    });
                    let expr_str = self.gen_expr_with_expected(expr, expected.as_ref())?;
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
                            let mut_kw = if mut_counts.get(&name_left).copied().unwrap_or(0) > 1 {
                                "mut "
                            } else {
                                ""
                            };
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
                let iter_expr = self.gen_expr(iter)?;
                let iter_src = if let Some(Type::Dict(_, _)) = iter.ty.as_ref() {
                    format!("{}.into_iter().map(|(k, _)| k)", iter_expr)
                } else {
                    format!("{}.into_iter()", iter_expr)
                };
                self.push_line(&format!("for {} in {} {{", target, iter_src));
                self.indent += 1;
                let saved_locals = self.local_vars.clone();
                let mut scoped_locals = saved_locals.clone().unwrap_or_default();
                let item_ty = iter
                    .ty
                    .as_ref()
                    .and_then(|ty| self.iter_item_type_hint(ty))
                    .unwrap_or(Type::Unknown);
                scoped_locals.insert(target.clone(), item_ty);
                self.local_vars = Some(scoped_locals);
                for stmt in body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.local_vars = saved_locals;
                self.indent -= 1;
                self.push_line("}");
            }
            StmtKind::Global { .. } => {}
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

    fn emit_match_case(&mut self, case: &MatchCase) -> Result<(), CompileError> {
        let class_info = self.ctx.classes.get(&case.variant).ok_or_else(|| {
            self.error(
                case.span,
                format!("Unknown variant class: {}", case.variant),
            )
        })?;
        let mut bindings = Vec::new();
        for ((field, _), binding) in class_info.fields.iter().zip(case.bindings.iter()) {
            if field == binding {
                bindings.push(field.clone());
            } else {
                bindings.push(format!("{}: {}", field, binding));
            }
        }
        let union = self
            .find_union_for_variant(&case.variant)
            .ok_or_else(|| self.error(case.span, "Unable to locate union for variant"))?;
        let fields = if bindings.is_empty() {
            String::new()
        } else {
            bindings.join(", ")
        };
        self.push_line(&format!(
            "{}::{}({} {{ {} }}) => {{",
            union, case.variant, case.variant, fields
        ));
        self.indent += 1;
        let mut_counts = collect_assign_counts(&case.body);
        for stmt in &case.body {
            self.emit_stmt(stmt, &mut_counts)?;
        }
        self.indent -= 1;
        self.push_line("}");
        Ok(())
    }

    fn find_union_for_variant(&self, variant: &str) -> Option<String> {
        for (name, info) in &self.ctx.unions {
            if info.variants.contains(&variant.to_string()) {
                return Some(name.clone());
            }
        }
        None
    }

    fn emit_try_stmt(
        &mut self,
        body: &[Stmt],
        handlers: &[ExceptHandler],
        orelse: &[Stmt],
        finalbody: &[Stmt],
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        let has_finally = !finalbody.is_empty();
        let has_orelse = !orelse.is_empty();
        let in_throwing_fn = self.current_function_throws();

        // Check if try body contains return with a value
        let try_return_type = self.find_try_return_type(body);
        let has_value_return = try_return_type.is_some();

        // Collect variables declared in try body that might be used in else
        let try_vars = self.collect_try_block_vars(body);

        if has_finally {
            // Use Drop trait for guaranteed cleanup
            self.push_line("{"); // Open finally scope
            self.indent += 1;

            // Define Finally struct
            self.push_line("struct Finally<F: FnOnce()>(Option<F>);");
            self.push_line("impl<F: FnOnce()> Drop for Finally<F> {");
            self.indent += 1;
            self.push_line("fn drop(&mut self) {");
            self.indent += 1;
            self.push_line("if let Some(f) = self.0.take() { f(); }");
            self.indent -= 1;
            self.push_line("}"); // Close drop fn
            self.indent -= 1;
            self.push_line("}"); // Close impl block

            // Create Finally instance with closure
            self.push_line("let _finally = Finally(Some(|| {");
            self.indent += 1;
            for stmt in finalbody {
                self.emit_stmt(stmt, mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}));"); // Close closure and Finally()
        }

        // If we have else block, declare variables outside the closure
        if has_orelse && !try_vars.is_empty() {
            for (name, ty) in &try_vars {
                let ty_str = self.rust_type(ty);
                self.push_line(&format!(
                    "let mut _try_{}: Option<{}> = None;",
                    name, ty_str
                ));
            }
        }

        // Generate try body as closure returning Result
        let result_type = if let Some(ref ty) = try_return_type {
            let ty_str = self.rust_type(ty);
            format!("Result<{}, PyError>", ty_str)
        } else {
            "Result<(), PyError>".to_string()
        };

        self.push_line(&format!("let _try_result = (|| -> {} {{", result_type));
        self.indent += 1;

        // Track that we're inside a try block with value return
        let prev_try_return_type = self.try_block_return_type.take();
        self.try_block_return_type = try_return_type.clone();

        // Emit try body, but if we have else block, wrap Let statements
        if has_orelse && !try_vars.is_empty() {
            for stmt in body {
                self.emit_try_body_stmt(stmt, mut_counts, &try_vars)?;
            }
        } else {
            for stmt in body {
                self.emit_stmt(stmt, mut_counts)?;
            }
        }

        // Restore previous try return type
        self.try_block_return_type = prev_try_return_type;

        if has_value_return {
            // If try has value returns, we need unreachable at the end
            // (since all paths should return)
            self.push_line("unreachable!()");
        } else {
            self.push_line("Ok(())");
        }
        self.indent -= 1;
        self.push_line("})();");

        // Generate exception handlers
        if !handlers.is_empty() {
            self.push_line("match _try_result {");
            self.indent += 1;

            if has_value_return {
                // If the enclosing function throws, wrap in Ok; otherwise return directly
                if in_throwing_fn {
                    self.push_line("Ok(_v) => return Ok(_v),");
                } else {
                    self.push_line("Ok(_v) => return _v,");
                }
            } else {
                self.push_line("Ok(_) => {");
                self.indent += 1;

                // Unwrap variables from try block for use in else
                if has_orelse && !try_vars.is_empty() {
                    for (name, ty) in &try_vars {
                        let ty_str = self.rust_type(ty);
                        self.push_line(&format!(
                            "let {}: {} = _try_{}.unwrap();",
                            name, ty_str, name
                        ));
                    }
                }

                for stmt in orelse {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.indent -= 1;
                self.push_line("}");
            }

            for handler in handlers {
                self.emit_except_handler(handler, mut_counts)?;
            }

            // Add catch-all to re-propagate unhandled exceptions
            let has_catch_all = handlers.iter().any(|h| h.exc_type.is_none());
            if !has_catch_all {
                if in_throwing_fn {
                    self.push_line("Err(e) => return Err(e),");
                } else {
                    // Function doesn't throw, panic on unhandled exception
                    self.push_line("Err(e) => panic!(\"Unhandled exception: {}\", e),");
                }
            }

            self.indent -= 1;
            self.push_line("}");
        } else {
            // No exception handlers
            if has_value_return {
                if in_throwing_fn {
                    self.push_line("return _try_result;");
                } else {
                    self.push_line("return _try_result.unwrap();");
                }
            } else {
                if in_throwing_fn {
                    self.push_line("_try_result?;");
                } else {
                    // Function doesn't throw, so unwrap (should never fail)
                    self.push_line("_try_result.unwrap();");
                }

                // Unwrap variables from try block for use in else
                if has_orelse && !try_vars.is_empty() {
                    for (name, ty) in &try_vars {
                        let ty_str = self.rust_type(ty);
                        self.push_line(&format!(
                            "let {}: {} = _try_{}.unwrap();",
                            name, ty_str, name
                        ));
                    }
                }

                for stmt in orelse {
                    self.emit_stmt(stmt, mut_counts)?;
                }
            }
        }

        // Close the finally scope if we opened it
        if has_finally {
            self.indent -= 1;
            self.push_line("}"); // Close the scope that contains Finally
        }

        Ok(())
    }

    /// Find the return type from return statements in try block
    fn find_try_return_type(&self, stmts: &[Stmt]) -> Option<Type> {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Return { value: Some(expr) } => {
                    if let Some(ty) = &expr.ty {
                        if !matches!(ty, Type::None) {
                            return Some(ty.clone());
                        }
                    }
                }
                StmtKind::If { body, orelse, .. } => {
                    if let Some(ty) = self.find_try_return_type(body) {
                        return Some(ty);
                    }
                    if let Some(ty) = self.find_try_return_type(orelse) {
                        return Some(ty);
                    }
                }
                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                    if let Some(ty) = self.find_try_return_type(body) {
                        return Some(ty);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Collect variables declared in try block (Let statements)
    fn collect_try_block_vars(&self, stmts: &[Stmt]) -> Vec<(String, Type)> {
        let mut vars = Vec::new();
        for stmt in stmts {
            if let StmtKind::Let { name, value, .. } = &stmt.kind {
                if let Some(ty) = &value.ty {
                    vars.push((name.clone(), ty.clone()));
                }
            }
        }
        vars
    }

    /// Emit a statement from try body, wrapping Let statements to expose variables
    fn emit_try_body_stmt(
        &mut self,
        stmt: &Stmt,
        mut_counts: &HashMap<String, usize>,
        try_vars: &[(String, Type)],
    ) -> Result<(), CompileError> {
        if let StmtKind::Let { name, ann, value } = &stmt.kind {
            // Check if this variable is in try_vars
            if try_vars.iter().any(|(n, _)| n == name) {
                // Generate: let name = expr; _try_name = Some(name);
                let expr = self.gen_expr(value)?;
                let mut_kw = if mut_counts.get(name).copied().unwrap_or(0) > 1 {
                    "mut "
                } else {
                    ""
                };
                if let Some(ann) = ann {
                    let ty = self.resolve_type_ref(ann, stmt.span)?;
                    let ty_str = self.rust_type(&ty);
                    self.push_line(&format!("let {}{}: {} = {};", mut_kw, name, ty_str, expr));
                } else {
                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                }
                self.push_line(&format!("_try_{} = Some({}.clone());", name, name));
                return Ok(());
            }
        }
        // Default: emit normally
        self.emit_stmt(stmt, mut_counts)
    }

    fn emit_except_handler(
        &mut self,
        handler: &ExceptHandler,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        // Check if handler body contains a bare raise
        let needs_current_exception = self.handler_has_bare_raise(&handler.body);

        if let Some(exc_type) = &handler.exc_type {
            // Handle "Exception" as catch-all
            if exc_type == "Exception" {
                let pattern = if let Some(name) = &handler.name {
                    format!("Err({})", name)
                } else {
                    "Err(_e)".to_string()
                };

                self.push_line(&format!("{} => {{", pattern));
                self.indent += 1;

                // Save exception for bare raise if needed
                if needs_current_exception {
                    let exc_var = handler.name.as_deref().unwrap_or("_e");
                    self.push_line(&format!("let _current_exception = {}.clone();", exc_var));
                }

                for stmt in &handler.body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.indent -= 1;
                self.push_line("}");
            } else {
                let pattern = if let Some(name) = &handler.name {
                    format!("Err(PyError::{}({}))", exc_type, name)
                } else {
                    format!("Err(PyError::{}(_e))", exc_type)
                };

                self.push_line(&format!("{} => {{", pattern));
                self.indent += 1;

                // Save exception for bare raise if needed
                if needs_current_exception {
                    let exc_var = handler.name.as_deref().unwrap_or("_e");
                    self.push_line(&format!(
                        "let _current_exception = PyError::{}({}.clone());",
                        exc_type, exc_var
                    ));
                }

                for stmt in &handler.body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.indent -= 1;
                self.push_line("}");
            }
        } else {
            // Catch all (no type specified)
            let pattern = if let Some(name) = &handler.name {
                format!("Err({})", name)
            } else {
                "Err(_e)".to_string()
            };

            self.push_line(&format!("{} => {{", pattern));
            self.indent += 1;

            // Save exception for bare raise if needed
            if needs_current_exception {
                let exc_var = handler.name.as_deref().unwrap_or("_e");
                self.push_line(&format!("let _current_exception = {}.clone();", exc_var));
            }

            for stmt in &handler.body {
                self.emit_stmt(stmt, mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
        }

        Ok(())
    }

    /// Check if handler body contains a bare raise statement
    fn handler_has_bare_raise(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Raise { exc: None, .. } => return true,
                StmtKind::If { body, orelse, .. } => {
                    if self.handler_has_bare_raise(body) || self.handler_has_bare_raise(orelse) {
                        return true;
                    }
                }
                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                    if self.handler_has_bare_raise(body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn emit_raise_stmt(
        &mut self,
        exc: Option<&Expr>,
        cause: Option<&Expr>,
        span: Span,
    ) -> Result<(), CompileError> {
        // Check for unsupported exception chaining
        if cause.is_some() {
            return Err(self.error(
                span,
                "Exception chaining (raise ... from ...) is not supported",
            ));
        }

        if let Some(exc_expr) = exc {
            // Check if it's exception constructor call
            if let ExprKind::Call { func, args } = &exc_expr.kind {
                if let ExprKind::Name(exc_name) = &func.kind {
                    let msg = if !args.is_empty() {
                        self.gen_expr(&args[0])?
                    } else {
                        "String::new()".to_string()
                    };

                    self.push_line(&format!("return Err(PyError::{}({}));", exc_name, msg));
                    return Ok(());
                }
            }

            let exc_code = self.gen_expr(exc_expr)?;
            self.push_line(&format!("return Err({});", exc_code));
        } else {
            // Re-raise - use captured exception
            self.push_line("return Err(_current_exception);");
        }

        Ok(())
    }

    fn current_function_throws(&self) -> bool {
        self.current_function
            .as_ref()
            .and_then(|name| self.ctx.functions.get(name))
            .is_some_and(|sig| sig.can_throw)
    }
}
