// Try/except/finally emission helpers.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit a try/except/else/finally block.
    pub(super) fn emit_try_stmt(
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

        // Check if try body contains return with a value.
        let try_return_type = self.find_try_return_type(body);
        let has_value_return = try_return_type.is_some();

        // Collect variables declared in try body that might be used in else.
        let try_vars = self.collect_try_block_vars(body);

        if has_finally {
            // Use Drop trait for guaranteed cleanup.
            self.push_line("{"); // Open finally scope.
            self.indent += 1;

            // Define Finally struct.
            self.push_line("struct Finally<F: FnOnce()>(Option<F>);");
            self.push_line("impl<F: FnOnce()> Drop for Finally<F> {");
            self.indent += 1;
            self.push_line("fn drop(&mut self) {");
            self.indent += 1;
            self.push_line("if let Some(f) = self.0.take() { f(); }");
            self.indent -= 1;
            self.push_line("}"); // Close drop fn.
            self.indent -= 1;
            self.push_line("}"); // Close impl block.

            // Create Finally instance with closure.
            self.push_line("let _finally = Finally(Some(|| {");
            self.indent += 1;
            for stmt in finalbody {
                self.emit_stmt(stmt, mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}));"); // Close closure and Finally().
        }

        // If we have else block, declare variables outside the closure.
        if has_orelse && !try_vars.is_empty() {
            for (name, ty) in &try_vars {
                let ty_str = self.rust_type(ty);
                self.push_line(&format!(
                    "let mut _try_{}: Option<{}> = None;",
                    name, ty_str
                ));
            }
        }

        // Generate try body as closure returning Result.
        let result_type = if let Some(ref ty) = try_return_type {
            let ty_str = self.rust_type(ty);
            format!("Result<{}, PyError>", ty_str)
        } else {
            "Result<(), PyError>".to_string()
        };

        self.push_line(&format!("let _try_result = (|| -> {} {{", result_type));
        self.indent += 1;

        // Track that we're inside a try block with value return.
        let prev_try_return_type = self.try_block_return_type.take();
        self.try_block_return_type = try_return_type.clone();

        // Emit try body, but if we have else block, wrap Let statements.
        if has_orelse && !try_vars.is_empty() {
            for stmt in body {
                self.emit_try_body_stmt(stmt, mut_counts, &try_vars)?;
            }
        } else {
            for stmt in body {
                self.emit_stmt(stmt, mut_counts)?;
            }
        }

        // Restore previous try return type.
        self.try_block_return_type = prev_try_return_type;

        if has_value_return {
            // If try has value returns, we need unreachable at the end
            // (since all paths should return).
            self.push_line("unreachable!()");
        } else {
            self.push_line("Ok(())");
        }
        self.indent -= 1;
        self.push_line("})();");

        // Generate exception handlers.
        if !handlers.is_empty() {
            self.push_line("match _try_result {");
            self.indent += 1;

            if has_value_return {
                // If the enclosing function throws, wrap in Ok; otherwise return directly.
                if in_throwing_fn {
                    self.push_line("Ok(_v) => return Ok(_v),");
                } else {
                    self.push_line("Ok(_v) => return _v,");
                }
            } else {
                self.push_line("Ok(_) => {");
                self.indent += 1;

                // Unwrap variables from try block for use in else.
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

            // Add catch-all to re-propagate unhandled exceptions.
            let has_catch_all = handlers.iter().any(|h| h.exc_type.is_none());
            if !has_catch_all {
                if in_throwing_fn {
                    self.push_line("Err(e) => return Err(e),");
                } else {
                    // Function doesn't throw, panic on unhandled exception.
                    self.push_line("Err(e) => panic!(\"Unhandled exception: {}\", e),");
                }
            }

            self.indent -= 1;
            self.push_line("}");
        } else {
            // No exception handlers.
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
                    // Function doesn't throw, so unwrap (should never fail).
                    self.push_line("_try_result.unwrap();");
                }

                // Unwrap variables from try block for use in else.
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

        // Close the finally scope if we opened it.
        if has_finally {
            self.indent -= 1;
            self.push_line("}"); // Close the scope that contains Finally.
        }

        Ok(())
    }

    /// Find the return type from return statements in try block.
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

    /// Collect variables declared in try block (Let statements).
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

    /// Emit a statement from try body, wrapping Let statements to expose variables.
    fn emit_try_body_stmt(
        &mut self,
        stmt: &Stmt,
        mut_counts: &HashMap<String, usize>,
        try_vars: &[(String, Type)],
    ) -> Result<(), CompileError> {
        if let StmtKind::Let { name, ann, value } = &stmt.kind {
            // Check if this variable is in try_vars.
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
        // Default: emit normally.
        self.emit_stmt(stmt, mut_counts)
    }

    fn emit_except_handler(
        &mut self,
        handler: &ExceptHandler,
        mut_counts: &HashMap<String, usize>,
    ) -> Result<(), CompileError> {
        // Check if handler body contains a bare raise.
        let needs_current_exception = self.handler_has_bare_raise(&handler.body);

        if let Some(exc_type) = &handler.exc_type {
            // Handle "Exception" as catch-all.
            if exc_type == "Exception" {
                let pattern = if let Some(name) = &handler.name {
                    format!("Err({})", name)
                } else {
                    "Err(_e)".to_string()
                };

                self.push_line(&format!("{} => {{", pattern));
                self.indent += 1;

                // Save exception for bare raise if needed.
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

                // Save exception for bare raise if needed.
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
            // Catch all (no type specified).
            let pattern = if let Some(name) = &handler.name {
                format!("Err({})", name)
            } else {
                "Err(_e)".to_string()
            };

            self.push_line(&format!("{} => {{", pattern));
            self.indent += 1;

            // Save exception for bare raise if needed.
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

    /// Check if handler body contains a bare raise statement.
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

    /// Determine whether the current function is marked as throwing.
    pub(super) fn current_function_throws(&self) -> bool {
        self.current_function
            .as_ref()
            .and_then(|name| self.ctx.functions.get(name))
            .is_some_and(|sig| sig.can_throw)
    }
}
