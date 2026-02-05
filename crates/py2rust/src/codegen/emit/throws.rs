// Throw analysis for top-level and expression/statement forms.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Determine whether top-level statements can throw.
    pub(super) fn analyze_top_level_throws(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            if self.stmt_can_throw(stmt) {
                return true;
            }
        }
        false
    }

    fn stmt_can_throw(&self, stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Raise { .. } => true,
            StmtKind::Try { .. } => true, // Has exception handling.
            StmtKind::Expr(expr) => self.expr_can_throw(expr),
            StmtKind::Let { value, .. } => self.expr_can_throw(value),
            StmtKind::Assign { target, value } => {
                if self.expr_can_throw(value) {
                    return true;
                }
                self.assign_target_can_throw(target, value.ty.as_ref())
            }
            StmtKind::Return { value } => value.as_ref().is_some_and(|e| self.expr_can_throw(e)),
            StmtKind::If { test, body, orelse } => {
                self.expr_can_throw(test)
                    || body.iter().any(|s| self.stmt_can_throw(s))
                    || orelse.iter().any(|s| self.stmt_can_throw(s))
            }
            StmtKind::While { test, body } => {
                self.expr_can_throw(test) || body.iter().any(|s| self.stmt_can_throw(s))
            }
            StmtKind::For { iter, body, .. } => {
                self.expr_can_throw(iter) || body.iter().any(|s| self.stmt_can_throw(s))
            }
            StmtKind::Match { subject, cases } => {
                self.expr_can_throw(subject)
                    || cases
                        .iter()
                        .any(|c| c.body.iter().any(|s| self.stmt_can_throw(s)))
            }
            _ => false,
        }
    }

    fn expr_can_throw(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Name(name) = &func.kind {
                    if self.builtin_call_can_throw(name, args) {
                        return true;
                    }
                    if self
                        .ctx
                        .functions
                        .get(name)
                        .is_some_and(|sig| sig.can_throw)
                    {
                        return true;
                    }
                }
                if let ExprKind::Attr { value, attr } = &func.kind {
                    if matches!(value.ty.as_ref(), Some(Type::List(_)))
                        && matches!(attr.as_str(), "pop" | "index")
                    {
                        return true;
                    }
                }
                if self.expr_can_throw(func) {
                    return true;
                }
                args.iter().any(|arg| self.expr_can_throw(arg))
                    || keywords.iter().any(|kw| self.expr_can_throw(&kw.value))
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr_can_throw(left) || self.expr_can_throw(right)
            }
            ExprKind::Unary { expr, .. } => self.expr_can_throw(expr),
            ExprKind::Compare { left, right, .. } => {
                self.expr_can_throw(left) || self.expr_can_throw(right)
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                if self.expr_can_throw(left) {
                    return true;
                }
                comparators.iter().any(|cmp| self.expr_can_throw(cmp))
            }
            ExprKind::BoolOp { values, .. } => values.iter().any(|v| self.expr_can_throw(v)),
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                items.iter().any(|e| self.expr_can_throw(e))
            }
            ExprKind::Dict(pairs) => pairs
                .iter()
                .any(|(k, v)| self.expr_can_throw(k) || self.expr_can_throw(v)),
            ExprKind::Index { value, index } => {
                if self.expr_can_throw(value) || self.expr_can_throw(index) {
                    return true;
                }
                matches!(
                    value.ty.as_ref(),
                    Some(Type::List(_)) | Some(Type::Dict(_, _))
                )
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                if self.expr_can_throw(value)
                    || start.as_ref().is_some_and(|s| self.expr_can_throw(s))
                    || end.as_ref().is_some_and(|e| self.expr_can_throw(e))
                {
                    return true;
                }
                step.as_ref()
                    .is_some_and(|st| self.expr_can_throw(st) || self.step_value_can_throw(st))
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.expr_can_throw(elt)
                    || self.expr_can_throw(iter)
                    || ifs.iter().any(|i| self.expr_can_throw(i))
            }
            ExprKind::UnionCtor { inner, .. } => self.expr_can_throw(inner),
            ExprKind::Lambda { body, .. } => self.expr_can_throw(body),
            ExprKind::IfExpr { test, body, orelse } => {
                self.expr_can_throw(test)
                    || self.expr_can_throw(body)
                    || self.expr_can_throw(orelse)
            }
            ExprKind::Attr { value, .. } => self.expr_can_throw(value),
            _ => false,
        }
    }

    fn builtin_call_can_throw(&self, name: &str, args: &[Expr]) -> bool {
        match name {
            "chr" | "ord" | "next" => true,
            "max" | "min" => args.len() == 1,
            "int" | "float" => {
                if args.is_empty() {
                    return false;
                }
                !matches!(
                    args[0].ty.as_ref(),
                    Some(Type::Int | Type::Float | Type::Bool)
                )
            }
            "range" => {
                if args.len() == 3 {
                    match &args[2].kind {
                        ExprKind::Literal(Literal::Int(n)) => *n == 0,
                        _ => true,
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn step_value_can_throw(&self, step: &Expr) -> bool {
        match &step.kind {
            ExprKind::Literal(Literal::Int(n)) => *n == 0,
            _ => true,
        }
    }

    /// Check whether assignment targets can throw (e.g., list indexing).
    fn assign_target_can_throw(&self, target: &AssignTarget, value_ty: Option<&Type>) -> bool {
        match target {
            AssignTarget::Name(_) => false,
            AssignTarget::Attr { value, .. } => self.expr_can_throw(value),
            AssignTarget::Index { value, index } => {
                if self.expr_can_throw(value) || self.expr_can_throw(index) {
                    return true;
                }
                matches!(value.ty.as_ref(), Some(Type::List(_)))
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                if matches!(value_ty, Some(Type::List(_))) {
                    return true;
                }
                for (idx, item) in items.iter().enumerate() {
                    let elem_ty = match value_ty {
                        Some(Type::Tuple(types)) => types.get(idx),
                        Some(Type::List(inner)) => Some(inner.as_ref()),
                        _ => None,
                    };
                    if self.assign_target_can_throw(item, elem_ty) {
                        return true;
                    }
                }
                false
            }
        }
    }
}
