use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn is_global(&self, name: &str) -> bool {
        self.ctx.globals.contains_key(name)
    }

    pub(crate) fn global_name(&self, name: &str) -> String {
        format!("__GLOBAL_{}", name.to_uppercase())
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

pub(crate) fn collect_assign_counts(stmts: &[Stmt]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    fn visit(stmt: &Stmt, counts: &mut HashMap<String, usize>) {
        match &stmt.kind {
            StmtKind::Let { name, .. } => {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            StmtKind::Assign { target, .. } => {
                if let AssignTarget::Name(name) = target {
                    *counts.entry(name.clone()).or_insert(0) += 1;
                }
                if let AssignTarget::Attr { value, .. } = target {
                    if let ExprKind::Name(name) = &value.kind {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
                if let AssignTarget::Index { value, .. } = target {
                    if let ExprKind::Name(name) = &value.kind {
                        *counts.entry(name.clone()).or_insert(0) += 1;
                    }
                }
            }
            StmtKind::If { body, orelse, .. } => {
                for stmt in body {
                    visit(stmt, counts);
                }
                for stmt in orelse {
                    visit(stmt, counts);
                }
            }
            StmtKind::While { body, .. } => {
                for stmt in body {
                    visit(stmt, counts);
                }
            }
            StmtKind::For { body, .. } => {
                for stmt in body {
                    visit(stmt, counts);
                }
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    for stmt in &case.body {
                        visit(stmt, counts);
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
                    visit(stmt, counts);
                }
                for handler in handlers {
                    for stmt in &handler.body {
                        visit(stmt, counts);
                    }
                }
                for stmt in orelse {
                    visit(stmt, counts);
                }
                for stmt in finalbody {
                    visit(stmt, counts);
                }
            }
            StmtKind::Expr(expr) => {
                if let ExprKind::Call { func, .. } = &expr.kind {
                    if let ExprKind::Attr { value, attr } = &func.kind {
                        if matches!(attr.as_str(), "append" | "add" | "remove") {
                            if let ExprKind::Name(name) = &value.kind {
                                *counts.entry(name.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for stmt in stmts {
        visit(stmt, &mut counts);
    }
    counts
}
