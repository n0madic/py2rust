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
        // Drop-based finally captures mutable references across the whole try scope.
        // For plain statement try-blocks in non-throwing functions, emit finally inline
        // after try handling to avoid borrow conflicts.
        let use_drop_finally = has_finally && (has_value_return || in_throwing_fn);

        // Collect variables declared in try body that might be used in else.
        let try_vars = self.collect_try_block_vars(body);

        if use_drop_finally {
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
        // Value-returning try blocks are emitted in one of two modes:
        // - `Result<T, PyError>` when every non-exceptional path returns a value.
        // - `Result<Option<T>, PyError>` when fall-through is possible.
        let try_returns_option = has_value_return && !self.try_body_always_returns_value(body);
        let result_type = if let Some(ref ty) = try_return_type {
            let ty_str = self.rust_type(ty);
            if try_returns_option {
                format!("Result<Option<{}>, PyError>", ty_str)
            } else {
                format!("Result<{}, PyError>", ty_str)
            }
        } else {
            "Result<(), PyError>".to_string()
        };

        self.push_line(&format!("let _try_result = (|| -> {} {{", result_type));
        self.indent += 1;

        // Track that we're inside a try block with value return.
        let prev_try_return_type = self.try_block_return_type.take();
        let prev_try_returns_option = self.try_block_returns_option;
        self.try_block_return_type = if has_value_return {
            try_return_type.clone()
        } else {
            Some(Type::None)
        };
        self.try_block_returns_option = try_returns_option;

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
        self.try_block_returns_option = prev_try_returns_option;

        if has_value_return {
            if try_returns_option {
                self.push_line("Ok(None)");
            } else {
                self.push_line("unreachable!()");
            }
        } else {
            self.push_line("Ok(())");
        }
        self.indent -= 1;
        self.push_line("})();");

        // Generate exception handlers.
        if !handlers.is_empty() {
            self.push_line("match _try_result {");
            self.indent += 1;

            if has_value_return && !try_returns_option {
                self.push_line("Ok(_v) => {");
                self.indent += 1;
                // If the enclosing function throws, wrap in Ok; otherwise return directly.
                if in_throwing_fn {
                    self.push_line("return Ok(_v);");
                } else {
                    self.push_line("return _v;");
                }
                self.indent -= 1;
                self.push_line("}");
            } else if has_value_return {
                self.push_line("Ok(Some(_v)) => {");
                self.indent += 1;
                // If the enclosing function throws, wrap in Ok; otherwise return directly.
                if in_throwing_fn {
                    self.push_line("return Ok(_v);");
                } else {
                    self.push_line("return _v;");
                }
                self.indent -= 1;
                self.push_line("}");
                self.push_line("Ok(None) => {");
                self.indent += 1;

                // Unwrap variables from try block for use in else.
                if has_orelse && !try_vars.is_empty() {
                    for (name, ty) in &try_vars {
                        let ty_str = self.rust_type(ty);
                        self.push_line(&format!(
                            "let {}: {} = _try_{}.expect(\"try block did not initialize {}\");",
                            name, ty_str, name, name
                        ));
                    }
                }

                for stmt in orelse {
                    self.emit_stmt(stmt, mut_counts)?;
                }
                self.indent -= 1;
                self.push_line("}");
            } else {
                self.push_line("Ok(_) => {");
                self.indent += 1;

                // Unwrap variables from try block for use in else.
                if has_orelse && !try_vars.is_empty() {
                    for (name, ty) in &try_vars {
                        let ty_str = self.rust_type(ty);
                        self.push_line(&format!(
                            "let {}: {} = _try_{}.expect(\"try block did not initialize {}\");",
                            name, ty_str, name, name
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
                if try_returns_option {
                    self.push_line("match _try_result {");
                    self.indent += 1;
                    if in_throwing_fn {
                        self.push_line("Ok(Some(_v)) => return Ok(_v),");
                        self.push_line("Err(e) => return Err(e),");
                    } else {
                        self.push_line("Ok(Some(_v)) => return _v,");
                        self.push_line("Err(e) => panic!(\"Unhandled exception: {}\", e),");
                    }
                    self.push_line("Ok(None) => {}");
                    self.indent -= 1;
                    self.push_line("}");

                    // Unwrap variables from try block for use in else.
                    if has_orelse && !try_vars.is_empty() {
                        for (name, ty) in &try_vars {
                            let ty_str = self.rust_type(ty);
                            self.push_line(&format!(
                                "let {}: {} = _try_{}.expect(\"try block did not initialize {}\");",
                                name, ty_str, name, name
                            ));
                        }
                    }

                    for stmt in orelse {
                        self.emit_stmt(stmt, mut_counts)?;
                    }
                } else if in_throwing_fn {
                    self.push_line("return _try_result;");
                } else {
                    self.push_line(
                        "return _try_result.unwrap_or_else(|e| panic!(\"Unhandled exception: {}\", e));",
                    );
                }
            } else {
                if in_throwing_fn {
                    self.push_line("_try_result?;");
                } else {
                    // Function doesn't throw, so crash with context on unhandled exceptions.
                    self.push_line(
                        "_try_result.unwrap_or_else(|e| panic!(\"Unhandled exception: {}\", e));",
                    );
                }

                // Unwrap variables from try block for use in else.
                if has_orelse && !try_vars.is_empty() {
                    for (name, ty) in &try_vars {
                        let ty_str = self.rust_type(ty);
                        self.push_line(&format!(
                            "let {}: {} = _try_{}.expect(\"try block did not initialize {}\");",
                            name, ty_str, name, name
                        ));
                    }
                }

                for stmt in orelse {
                    self.emit_stmt(stmt, mut_counts)?;
                }
            }
        }

        if has_finally && !use_drop_finally {
            for stmt in finalbody {
                self.emit_stmt(stmt, mut_counts)?;
            }
        }

        // Close the finally scope if we opened it.
        if use_drop_finally {
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

    /// Return true when control flow cannot leave this block without returning a value.
    fn try_body_always_returns_value(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            if self.stmt_always_returns_value(stmt) {
                return true;
            }
        }
        false
    }

    /// Conservative all-paths-return analysis for value-returning statements.
    fn stmt_always_returns_value(&self, stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Return { value: Some(_) } => true,
            StmtKind::If { body, orelse, .. } => {
                !orelse.is_empty()
                    && self.try_body_always_returns_value(body)
                    && self.try_body_always_returns_value(orelse)
            }
            StmtKind::Match { cases, .. } => {
                !cases.is_empty()
                    && cases
                        .iter()
                        .all(|case| self.try_body_always_returns_value(&case.body))
            }
            _ => false,
        }
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
                // Copy values can be snapshotted directly; other values need clone
                // so the local binding remains available in the rest of the try body.
                let snapshot_expr = if let Some((_, ty)) = try_vars.iter().find(|(n, _)| n == name)
                {
                    if self.is_copy_type(ty) {
                        name.clone()
                    } else {
                        format!("{name}.clone()")
                    }
                } else {
                    format!("{name}.clone()")
                };
                self.push_line(&format!("_try_{} = Some({});", name, snapshot_expr));
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
                let variant = self
                    .resolve_exception_variant_name(exc_type)
                    .ok_or_else(|| {
                        self.error(
                            handler.span,
                            format!("Unknown exception type for handler: {exc_type}"),
                        )
                    })?;
                let pattern = if let Some(name) = &handler.name {
                    format!("Err(PyError::{}({}))", variant.as_str(), name)
                } else {
                    format!("Err(PyError::{}(_e))", variant.as_str())
                };

                self.push_line(&format!("{} => {{", pattern));
                self.indent += 1;

                // Save exception for bare raise if needed.
                if needs_current_exception {
                    let exc_var = handler.name.as_deref().unwrap_or("_e");
                    self.push_line(&format!(
                        "let _current_exception = PyError::{}({}.clone());",
                        variant.as_str(),
                        exc_var
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
