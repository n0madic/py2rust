// Main statement emission logic.

use super::super::util::collect_assign_counts;
use super::super::*;

impl<'a> Codegen<'a> {
    /// Wrap global values that need special ownership semantics.
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

    /// Emit a non-destructuring assignment target, optionally allowing new bindings.
    fn emit_simple_assign(
        &mut self,
        target: &AssignTarget,
        value: &Expr,
        mut_counts: &HashMap<String, usize>,
        allow_let: bool,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Name(name) => {
                // Global assignment uses OnceLock + Mutex for initialization and mutation.
                if self.is_global(name) {
                    let expected = self.ctx.globals.get(name).cloned();
                    let expr = self.gen_expr_with_expected(value, expected.as_ref())?;
                    let expr = self.wrap_global_value(expr, value, expected.as_ref());
                    if allow_let
                        && self.current_function.is_none()
                        && !self.initialized_globals.contains(name)
                    {
                        let tmp = self.new_tmp();
                        let gname = self.global_name(name);
                        self.push_line(&format!("let {} = {};", tmp, expr));
                        self.push_line(&format!(
                            "let _ = {}.get_or_init(|| Mutex::new({}));",
                            gname, tmp
                        ));
                        self.initialized_globals.insert(name.clone());
                        return Ok(());
                    }
                    self.push_line(&format!("*{} = {};", self.global_lock_expr(name), expr));
                    return Ok(());
                }

                if allow_let && self.local_var_type(name).is_none() {
                    let expr = self.gen_expr(value)?;
                    let mut_kw = if mut_counts.get(name).copied().unwrap_or(0) > 1 {
                        "mut "
                    } else {
                        ""
                    };
                    self.push_line(&format!("let {}{} = {};", mut_kw, name, expr));
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                    }
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
            AssignTarget::Tuple(_) | AssignTarget::List(_) => {
                self.emit_unpack_assign(target, value, mut_counts)?;
            }
        }
        Ok(())
    }

    /// Emit tuple/list unpacking assignments, evaluating the RHS once.
    fn emit_unpack_assign(
        &mut self,
        target: &AssignTarget,
        value: &Expr,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        let value_expr = self.gen_expr(value)?;
        let tmp = self.new_tmp();
        self.push_line(&format!("let {} = {};", tmp, value_expr));
        let tmp_expr = Expr {
            kind: ExprKind::Name(tmp),
            span: value.span,
            ty: value.ty.clone(),
        };
        self.emit_unpack_from(&tmp_expr, target, mut_counts)
    }

    /// Recursively unpack tuple/list targets from a source expression.
    fn emit_unpack_from(
        &mut self,
        source: &Expr,
        target: &AssignTarget,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                let element_types =
                    self.unpack_element_types(source.ty.as_ref(), items.len(), source.span)?;
                for (idx, item) in items.iter().enumerate() {
                    let elem_ty = element_types.get(idx).cloned().unwrap_or(Type::Unknown);
                    let idx_expr = Expr {
                        kind: ExprKind::Literal(Literal::Int(idx as i64)),
                        span: source.span,
                        ty: Some(Type::Int),
                    };
                    let elem_expr = Expr {
                        kind: ExprKind::Index {
                            value: Box::new(source.clone()),
                            index: Box::new(idx_expr),
                        },
                        span: source.span,
                        ty: Some(elem_ty.clone()),
                    };
                    if matches!(item, AssignTarget::Tuple(_) | AssignTarget::List(_)) {
                        let nested_tmp = self.new_tmp();
                        let elem_str = self.gen_expr(&elem_expr)?;
                        self.push_line(&format!("let {} = {};", nested_tmp, elem_str));
                        let nested_expr = Expr {
                            kind: ExprKind::Name(nested_tmp),
                            span: source.span,
                            ty: Some(elem_ty),
                        };
                        self.emit_unpack_from(&nested_expr, item, mut_counts)?;
                    } else {
                        self.emit_simple_assign(item, &elem_expr, mut_counts, true)?;
                    }
                }
                Ok(())
            }
            _ => self.emit_simple_assign(target, source, mut_counts, true),
        }
    }

    /// Determine element types when unpacking tuples/lists during codegen.
    fn unpack_element_types(
        &self,
        value_ty: Option<&Type>,
        count: usize,
        span: Span,
    ) -> Result<Vec<Type>, CompileError> {
        match value_ty {
            Some(Type::Tuple(items)) => {
                if items.len() != count {
                    return Err(self.error(
                        span,
                        format!("Unpacking expected {count} values, got {}", items.len()),
                    ));
                }
                Ok(items.clone())
            }
            Some(Type::List(inner)) => Ok(vec![inner.as_ref().clone(); count]),
            Some(Type::Unknown) | None => Ok(vec![Type::Unknown; count]),
            _ => Err(self.error(span, "Unpacking assignment requires a tuple or list value")),
        }
    }

    /// Emit a statement into the output buffer.
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
                    self.initialized_globals.insert(name.clone());
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

                // Inside try block with value returns, always wrap in Ok.
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
}
