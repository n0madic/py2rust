use super::util::collect_assign_counts;
use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_stmt(
        &mut self,
        stmt: &Stmt,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Let { name, ann, value } => {
                if self.is_global(name) {
                    let expr = self.gen_expr(value)?;
                    let gname = self.global_name(name);
                    self.push_line(&format!(
                        "let _ = {}.get_or_init(|| Mutex::new({}));",
                        gname, expr
                    ));
                    return Ok(());
                }
                if let ExprKind::Lambda { params, body } = &value.kind {
                    if let ExprKind::Block { stmts } = &body.kind {
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
            }
            StmtKind::Assign { target, value } => match target {
                AssignTarget::Name(name) => {
                    let expr = self.gen_expr(value)?;
                    if self.is_global(name) {
                        let gname = self.global_name(name);
                        self.push_line(&format!(
                            "*{}.get().unwrap().lock().unwrap() = {};",
                            gname, expr
                        ));
                    } else {
                        self.push_line(&format!("{} = {};", name, expr));
                    }
                }
                AssignTarget::Attr { value: obj, attr } => {
                    let obj_expr = self.gen_expr(obj)?;
                    let val_expr = self.gen_expr(value)?;
                    self.push_line(&format!("{}.{} = {};", obj_expr, attr, val_expr));
                }
                AssignTarget::Index {
                    value: container,
                    index,
                } => {
                    let cont_expr = self.gen_expr(container)?;
                    let val_expr = self.gen_expr(value)?;
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
                        if self.may_be_negative(index) {
                            self.uses.py_index = true;
                            self.push_line(&format!(
                                "{}[py_index({}, {}.len())] = {};",
                                cont_expr, idx_raw, cont_expr, val_expr
                            ));
                        } else {
                            self.push_line(&format!(
                                "{}[{} as usize] = {};",
                                cont_expr, idx_raw, val_expr
                            ));
                        }
                    } else {
                        let idx_expr = self.gen_expr(index)?;
                        self.push_line(&format!("{}[{}] = {};", cont_expr, idx_expr, val_expr));
                    }
                }
            },
            StmtKind::Return { value } => {
                // Check if we're in a throwing function
                let in_throwing_fn = self.current_function_throws();

                if let Some(expr) = value {
                    if matches!(&expr.kind, ExprKind::Literal(Literal::None)) {
                        if matches!(expr.ty.as_ref(), Some(Type::Option(_))) {
                            if in_throwing_fn {
                                self.push_line("return Ok(None);");
                            } else {
                                self.push_line("return None;");
                            }
                        } else {
                            if in_throwing_fn {
                                self.push_line("return Ok(());");
                            } else {
                                self.push_line("return;");
                            }
                        }
                    } else {
                        let expr_str = self.gen_expr(expr)?;
                        if in_throwing_fn {
                            self.push_line(&format!("return Ok({});", expr_str));
                        } else {
                            self.push_line(&format!("return {};", expr_str));
                        }
                    }
                } else {
                    if in_throwing_fn {
                        self.push_line("return Ok(());");
                    } else {
                        self.push_line("return;");
                    }
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
                for stmt in body {
                    self.emit_stmt(stmt, mut_counts)?;
                }
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
                self.emit_raise_stmt(exc.as_ref(), cause.as_ref())?;
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

        // Generate try body as closure returning Result
        self.push_line("let _try_result = (|| -> Result<(), PyError> {");
        self.indent += 1;
        for stmt in body {
            self.emit_stmt(stmt, mut_counts)?;
        }
        self.push_line("Ok(())");
        self.indent -= 1;
        self.push_line("})();");

        // Generate exception handlers
        if !handlers.is_empty() {
            self.push_line("match _try_result {");
            self.indent += 1;

            self.push_line("Ok(_) => {");
            self.indent += 1;
            for stmt in orelse {
                self.emit_stmt(stmt, mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");

            for handler in handlers {
                self.emit_except_handler(handler, mut_counts)?;
            }

            // Add catch-all to re-propagate unhandled exceptions
            let has_catch_all = handlers.iter().any(|h| h.exc_type.is_none());
            if !has_catch_all {
                self.push_line("Err(e) => return Err(e),");
            }

            self.indent -= 1;
            self.push_line("}");
        } else {
            // No exception handlers, so propagate if function can throw
            if self.current_function_throws() {
                self.push_line("_try_result?;");
            } else {
                // Function doesn't throw, so unwrap (should never fail)
                self.push_line("_try_result.unwrap();");
            }
            for stmt in orelse {
                self.emit_stmt(stmt, mut_counts)?;
            }
        }

        // Close the finally scope if we opened it
        if has_finally {
            self.indent -= 1;
            self.push_line("}"); // Close the scope that contains Finally
        }

        Ok(())
    }

    fn emit_except_handler(
        &mut self,
        handler: &ExceptHandler,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        if let Some(exc_type) = &handler.exc_type {
            let pattern = if let Some(name) = &handler.name {
                format!("Err(PyError::{}({}))", exc_type, name)
            } else {
                format!("Err(PyError::{}(_))", exc_type)
            };

            self.push_line(&format!("{} => {{", pattern));
            self.indent += 1;
            for stmt in &handler.body {
                self.emit_stmt(stmt, mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
        } else {
            // Catch all
            let pattern = if let Some(name) = &handler.name {
                format!("Err({})", name)
            } else {
                "Err(_)".to_string()
            };

            self.push_line(&format!("{} => {{", pattern));
            self.indent += 1;
            for stmt in &handler.body {
                self.emit_stmt(stmt, mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
        } // Close the else block

        Ok(())
    }

    fn emit_raise_stmt(
        &mut self,
        exc: Option<&Expr>,
        _cause: Option<&Expr>,
    ) -> Result<(), CompileError> {
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
            .map_or(false, |sig| sig.can_throw)
    }
}
