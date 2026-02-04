use super::*;

/// Utility functions for code generation.
///
/// These are small helper functions that are used throughout codegen:
/// - Scope queries (is_global, local_var_type)
/// - Name generation (global_name, new_tmp)
/// - Output formatting (push_line)
/// - Error creation
///
/// Also includes mutation analysis for determining when variables need `mut`.

impl<'a> Codegen<'a> {
    /// Check if a variable name refers to a global variable.
    ///
    /// A name is global if:
    /// 1. It's NOT in the current function's local variable map, AND
    /// 2. It IS in the program's global variables map
    ///
    /// This is used to determine if we need to emit the mutex wrapper
    /// access pattern for globals.
    pub(crate) fn is_global(&self, name: &str) -> bool {
        if let Some(vars) = self.local_vars.as_ref() {
            if vars.contains_key(name) {
                return false;
            }
        }
        self.ctx.globals.contains_key(name)
    }

    pub(crate) fn global_name(&self, name: &str) -> String {
        format!("__GLOBAL_{}", name.to_uppercase())
    }

    pub(crate) fn global_lock_expr(&self, name: &str) -> String {
        // Use expect to surface clear panic messages if globals are misused.
        format!(
            "{}.get().expect(\"global not initialized\").lock().expect(\"global mutex poisoned\")",
            self.global_name(name)
        )
    }

    pub(crate) fn new_tmp(&mut self) -> String {
        let name = format!("_tmp{}", self.tmp_counter);
        self.tmp_counter += 1;
        name
    }

    pub(crate) fn push_line(&mut self, line: &str) {
        if !line.is_empty() {
            self.out.push_str(&"    ".repeat(self.indent));
            self.out.push_str(line);
        }
        self.out.push('\n');
    }

    pub(crate) fn error(&self, span: Span, msg: impl Into<String>) -> CompileError {
        CompileError::new(msg, span, self.source, self.filename)
    }
}

/// Count how many times each variable is assigned/mutated in a statement block.
///
/// Why this matters:
/// Rust requires variables to be declared `mut` if they'll be reassigned.
/// Python doesn't have this distinction - all variables are implicitly mutable.
///
/// We analyze the code to determine which variables are assigned more than once
/// (or mutated by operations like `next(iter)`), and emit `let mut` for those.
///
/// This function returns a map of variable name -> assignment count.
/// If count > 1 (or there's a mutation), we emit `mut`.
///
/// Special cases tracked:
/// - Regular assignments: `x = value`
/// - next() calls: `next(iterator)` mutates the iterator
/// - Index assignments: `list[i] = value` mutates the list
/// - Method calls that mutate: Some methods (if any) mutate their receiver
pub(crate) fn collect_assign_counts(stmts: &[Stmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    fn visit_expr(expr: &Expr, counts: &mut HashMap<String, usize>) {
        match &expr.kind {
            ExprKind::Call { func, args } => {
                if let ExprKind::Attr { value, attr } = &func.kind {
                    // Mutating collection methods require the receiver to be `mut`.
                    if matches!(
                        attr.as_str(),
                        "append"
                            | "extend"
                            | "pop"
                            | "insert"
                            | "clear"
                            | "reverse"
                            | "sort"
                            | "add"
                            | "remove"
                    ) {
                        if let ExprKind::Name(name) = &value.kind {
                            *counts.entry(name.clone()).or_insert(0) += 1;
                        }
                    }
                }
                if let ExprKind::Name(name) = &func.kind {
                    if name == "next" {
                        if let Some(ExprKind::Name(arg_name)) = args.first().map(|arg| &arg.kind) {
                            *counts.entry(arg_name.clone()).or_insert(0) += 1;
                        }
                    }
                }
                visit_expr(func, counts);
                for arg in args {
                    visit_expr(arg, counts);
                }
            }
            ExprKind::Attr { value, .. } => visit_expr(value, counts),
            ExprKind::Binary { left, right, .. } => {
                visit_expr(left, counts);
                visit_expr(right, counts);
            }
            ExprKind::Unary { expr, .. } => visit_expr(expr, counts),
            ExprKind::Compare { left, right, .. } => {
                visit_expr(left, counts);
                visit_expr(right, counts);
            }
            ExprKind::BoolOp { values, .. } => {
                for v in values {
                    visit_expr(v, counts);
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    visit_expr(item, counts);
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    visit_expr(k, counts);
                    visit_expr(v, counts);
                }
            }
            ExprKind::Index { value, index } => {
                visit_expr(value, counts);
                visit_expr(index, counts);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                visit_expr(value, counts);
                if let Some(s) = start {
                    visit_expr(s, counts);
                }
                if let Some(e) = end {
                    visit_expr(e, counts);
                }
                if let Some(st) = step.as_deref() {
                    visit_expr(st, counts);
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                visit_expr(elt, counts);
                visit_expr(iter, counts);
                for cond in ifs {
                    visit_expr(cond, counts);
                }
            }
            ExprKind::UnionCtor { inner, .. } => visit_expr(inner, counts),
            ExprKind::Lambda { body, .. } => visit_expr(body, counts),
            ExprKind::IfExpr { test, body, orelse } => {
                visit_expr(test, counts);
                visit_expr(body, counts);
                visit_expr(orelse, counts);
            }
            ExprKind::Block { stmts } => {
                for stmt in stmts {
                    visit_stmt(stmt, counts);
                }
            }
            ExprKind::Name(_) | ExprKind::Literal(_) => {}
        }
    }

    fn visit_stmt(stmt: &Stmt, counts: &mut HashMap<String, usize>) {
        fn record_target(target: &AssignTarget, counts: &mut HashMap<String, usize>) {
            match target {
                AssignTarget::Name(name) => {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
                AssignTarget::Attr { value, .. } => {
                    if let ExprKind::Name(name) = &value.kind {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                AssignTarget::Index { value, .. } => {
                    if let ExprKind::Name(name) = &value.kind {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    // Unpacking assigns to each leaf target.
                    for item in items {
                        record_target(item, counts);
                    }
                }
            }
        }

        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                *counts.entry(name.clone()).or_insert(0) += 1;
                visit_expr(value, counts);
            }
            StmtKind::Assign { target, value } => {
                record_target(target, counts);
                visit_expr(value, counts);
            }
            StmtKind::If { test, body, orelse } => {
                visit_expr(test, counts);
                for stmt in body {
                    visit_stmt(stmt, counts);
                }
                for stmt in orelse {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::While { test, body } => {
                visit_expr(test, counts);
                for stmt in body {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::For { iter, body, .. } => {
                visit_expr(iter, counts);
                for stmt in body {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::Match { subject, cases } => {
                visit_expr(subject, counts);
                for case in cases {
                    for stmt in &case.body {
                        visit_stmt(stmt, counts);
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
                    visit_stmt(stmt, counts);
                }
                for handler in handlers {
                    for stmt in &handler.body {
                        visit_stmt(stmt, counts);
                    }
                }
                for stmt in orelse {
                    visit_stmt(stmt, counts);
                }
                for stmt in finalbody {
                    visit_stmt(stmt, counts);
                }
            }
            StmtKind::Expr(expr) => {
                visit_expr(expr, counts);
            }
            StmtKind::Assert { test, msg } => {
                visit_expr(test, counts);
                if let Some(msg) = msg {
                    visit_expr(msg, counts);
                }
            }
            StmtKind::Return { value: Some(expr) } => {
                visit_expr(expr, counts);
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(expr) = exc {
                    visit_expr(expr, counts);
                }
                if let Some(expr) = cause {
                    visit_expr(expr, counts);
                }
            }
            _ => {}
        }
    }
    for stmt in stmts {
        visit_stmt(stmt, &mut counts);
    }
    counts
}
