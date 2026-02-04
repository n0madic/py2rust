use super::*;
use std::collections::{HashMap, HashSet};

/// Analyzes which functions can throw exceptions through control flow analysis
pub struct ThrowAnalyzer {
    throwing_functions: HashSet<String>,
}

impl ThrowAnalyzer {
    pub fn new(_ctx: &TypeContext) -> Self {
        Self {
            throwing_functions: HashSet::new(),
        }
    }

    /// Analyze the entire program to determine which functions can throw
    pub fn analyze_program(&mut self, program: &Program) -> HashMap<String, bool> {
        // Phase 1: Find functions with explicit uncaught raise
        for item in &program.items {
            if let Item::Function(func) = item {
                if self.has_uncaught_raise(&func.body) {
                    self.throwing_functions.insert(func.name.clone());
                }
            }
        }

        // Phase 2: Propagate through call graph (fixed-point iteration)
        loop {
            let mut changed = false;
            for item in &program.items {
                if let Item::Function(func) = item {
                    if !self.throwing_functions.contains(&func.name)
                        && self.has_uncaught_throwing_call(&func.body)
                    {
                        self.throwing_functions.insert(func.name.clone());
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // Phase 3: Build result map
        let mut result = HashMap::new();
        for item in &program.items {
            if let Item::Function(func) = item {
                let can_throw = self.throwing_functions.contains(&func.name);
                result.insert(func.name.clone(), can_throw);
            }
        }
        result
    }

    /// Check if statements contain uncaught raise (not inside try/except that catches it)
    fn has_uncaught_raise(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Raise { .. } => return true,

                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    // Check if try body has raises that AREN'T caught
                    if self.has_uncaught_raise(body) {
                        // Check if handlers catch all exceptions
                        let catches_all = handlers.iter().any(|h| h.exc_type.is_none());
                        if !catches_all {
                            // Handlers don't catch all, so raises propagate
                            return true;
                        }
                    }

                    // Check handlers themselves for uncaught raises
                    for handler in handlers {
                        if self.has_uncaught_raise(&handler.body) {
                            return true;
                        }
                    }

                    // Check else and finally for uncaught raises
                    if self.has_uncaught_raise(orelse) || self.has_uncaught_raise(finalbody) {
                        return true;
                    }
                }

                StmtKind::If { body, orelse, .. } => {
                    if self.has_uncaught_raise(body) || self.has_uncaught_raise(orelse) {
                        return true;
                    }
                }

                StmtKind::While { body, .. } => {
                    if self.has_uncaught_raise(body) {
                        return true;
                    }
                }

                StmtKind::For { body, .. } => {
                    if self.has_uncaught_raise(body) {
                        return true;
                    }
                }

                StmtKind::Match { cases, .. } => {
                    for case in cases {
                        if self.has_uncaught_raise(&case.body) {
                            return true;
                        }
                    }
                }

                _ => {}
            }
        }
        false
    }

    /// Check if statements contain calls to throwing functions (not caught)
    fn has_uncaught_throwing_call(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Expr(expr)
                | StmtKind::Let { value: expr, .. }
                | StmtKind::Return { value: Some(expr) } => {
                    if self.expr_calls_throwing_function(expr) {
                        return true;
                    }
                }

                StmtKind::Assign { value: expr, .. } => {
                    if self.expr_calls_throwing_function(expr) {
                        return true;
                    }
                }

                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    // Check if try body has throwing calls that AREN'T caught
                    if self.has_uncaught_throwing_call(body) {
                        // Check if handlers catch all exceptions
                        let catches_all = handlers.iter().any(|h| h.exc_type.is_none());
                        if !catches_all {
                            // Handlers don't catch all, so throws propagate
                            return true;
                        }
                    }

                    // Check handlers themselves for uncaught throws
                    for handler in handlers {
                        if self.has_uncaught_throwing_call(&handler.body) {
                            return true;
                        }
                    }

                    // Check else and finally for uncaught throws
                    if self.has_uncaught_throwing_call(orelse)
                        || self.has_uncaught_throwing_call(finalbody)
                    {
                        return true;
                    }
                }

                StmtKind::If { test, body, orelse } => {
                    if self.expr_calls_throwing_function(test)
                        || self.has_uncaught_throwing_call(body)
                        || self.has_uncaught_throwing_call(orelse)
                    {
                        return true;
                    }
                }

                StmtKind::While { test, body } => {
                    if self.expr_calls_throwing_function(test)
                        || self.has_uncaught_throwing_call(body)
                    {
                        return true;
                    }
                }

                StmtKind::For { iter, body, .. } => {
                    if self.expr_calls_throwing_function(iter)
                        || self.has_uncaught_throwing_call(body)
                    {
                        return true;
                    }
                }

                StmtKind::Match { subject, cases } => {
                    if self.expr_calls_throwing_function(subject) {
                        return true;
                    }
                    for case in cases {
                        if self.has_uncaught_throwing_call(&case.body) {
                            return true;
                        }
                    }
                }

                StmtKind::Assert { test, msg } => {
                    if self.expr_calls_throwing_function(test) {
                        return true;
                    }
                    if let Some(msg_expr) = msg {
                        if self.expr_calls_throwing_function(msg_expr) {
                            return true;
                        }
                    }
                }

                _ => {}
            }
        }
        false
    }

    /// Check if an expression calls a throwing function
    fn expr_calls_throwing_function(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Call { func, args } => {
                // Check if the called function throws
                if let ExprKind::Name(name) = &func.kind {
                    if self.throwing_functions.contains(name) {
                        return true;
                    }
                }

                // Check arguments recursively
                for arg in args {
                    if self.expr_calls_throwing_function(arg) {
                        return true;
                    }
                }

                false
            }

            ExprKind::Binary { left, right, .. } => {
                self.expr_calls_throwing_function(left) || self.expr_calls_throwing_function(right)
            }

            ExprKind::Unary { expr, .. } => self.expr_calls_throwing_function(expr),

            ExprKind::Compare { left, right, .. } => {
                self.expr_calls_throwing_function(left) || self.expr_calls_throwing_function(right)
            }

            ExprKind::BoolOp { values, .. } => {
                values.iter().any(|v| self.expr_calls_throwing_function(v))
            }

            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                items.iter().any(|e| self.expr_calls_throwing_function(e))
            }

            ExprKind::Dict(pairs) => pairs.iter().any(|(k, v)| {
                self.expr_calls_throwing_function(k) || self.expr_calls_throwing_function(v)
            }),

            ExprKind::Index { value, index } => {
                self.expr_calls_throwing_function(value) || self.expr_calls_throwing_function(index)
            }

            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.expr_calls_throwing_function(value)
                    || start
                        .as_ref()
                        .is_some_and(|s| self.expr_calls_throwing_function(s))
                    || end
                        .as_ref()
                        .is_some_and(|e| self.expr_calls_throwing_function(e))
                    || step
                        .as_ref()
                        .is_some_and(|st| self.expr_calls_throwing_function(st))
            }

            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.expr_calls_throwing_function(elt)
                    || self.expr_calls_throwing_function(iter)
                    || ifs.iter().any(|i| self.expr_calls_throwing_function(i))
            }

            ExprKind::UnionCtor { inner, .. } => self.expr_calls_throwing_function(inner),

            ExprKind::Lambda { body, .. } => self.expr_calls_throwing_function(body),

            ExprKind::IfExpr { test, body, orelse } => {
                self.expr_calls_throwing_function(test)
                    || self.expr_calls_throwing_function(body)
                    || self.expr_calls_throwing_function(orelse)
            }

            ExprKind::Attr { value, .. } => self.expr_calls_throwing_function(value),

            ExprKind::Block { stmts } => self.has_uncaught_throwing_call(stmts),

            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_explicit_raise_marks_throwing() {
        // TODO: Add test when lowering is implemented
    }

    #[test]
    fn test_call_propagation() {
        // TODO: Add test when lowering is implemented
    }

    #[test]
    fn test_try_catch_blocks_propagation() {
        // TODO: Add test when lowering is implemented
    }
}
