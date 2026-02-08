use super::*;
use std::collections::{HashMap, HashSet};

/// Exception propagation analysis.
///
/// This module determines which functions can throw exceptions that aren't caught.
/// This information is critical for Rust code generation because:
/// 1. Functions that can throw must return Result<T, PyError>
/// 2. Callers of throwing functions must handle the Result
/// 3. We need to know this before generating code
///
/// Analysis strategy:
/// Phase 1: Find functions with explicit uncaught `raise` statements
/// Phase 2: Propagate through call graph (fixed-point iteration)
///          If function A calls throwing function B without catching, A throws
/// Phase 3: Build final map of function_name -> can_throw
///
/// Why multi-phase?
/// - Explicit raises are easy to detect directly
/// - Call propagation requires knowing what other functions throw
/// - Fixed-point iteration handles mutual recursion (A calls B, B calls A)
///
/// Try/except handling:
/// - `except:` or `except Exception:` catches all exceptions
/// - Specific exception types don't catch all (yet)
/// - Raises in except/else/finally handlers propagate normally
///
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

    /// Analyze the entire program to determine which functions can throw.
    ///
    /// Returns a map from function name to whether it can throw.
    ///
    /// The analysis is conservative - we may mark functions as throwing
    /// even if they never actually throw in practice. But we never miss
    /// a function that could throw (which would cause Rust compile errors).
    ///
    /// Algorithm:
    /// 1. Find all functions with direct `raise` statements not in try/except
    /// 2. Repeatedly scan all functions to find those calling throwing functions
    /// 3. Continue until no new throwing functions are discovered (fixed-point)
    pub fn analyze_program(&mut self, program: &Program) -> HashMap<String, bool> {
        // Phase 1: Direct exception detection
        // Find functions that explicitly raise exceptions without catching them
        for item in &program.items {
            if let Item::Function(func) = item {
                if self.has_uncaught_raise(&func.body) {
                    self.throwing_functions.insert(func.name.clone());
                }
            }
        }

        // Phase 2: Call graph propagation (fixed-point iteration)
        // If function calls a throwing function and doesn't catch, it throws too.
        // We repeat until no new throwing functions are discovered.
        //
        // Why loop? Consider:
        //   def a(): b()  # a throws if b throws
        //   def b(): c()  # b throws if c throws
        //   def c(): raise ValueError()
        //
        // Iteration 1: c is marked throwing (phase 1)
        // Iteration 2: b is marked throwing (calls c)
        // Iteration 3: a is marked throwing (calls b)
        // Iteration 4: no changes, done
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
        // Convert our internal set into a map for all functions
        let mut result = HashMap::new();
        for item in &program.items {
            if let Item::Function(func) = item {
                let can_throw = self.throwing_functions.contains(&func.name);
                result.insert(func.name.clone(), can_throw);
            }
        }
        result
    }

    /// Check if statements contain uncaught raise statements.
    ///
    /// A raise is "uncaught" if:
    /// 1. It's not inside a try/except block, OR
    /// 2. It's inside a try but the except doesn't catch all exceptions
    ///
    /// We're conservative: we only consider `except:` or `except Exception:`
    /// as catching all. Specific exception types might not catch everything.
    ///
    /// Raises in except/else/finally blocks always propagate (they're not
    /// caught by the same try/except they're in).
    fn has_uncaught_raise(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match &stmt.kind {
                // Bare raise statement - always propagates
                StmtKind::Raise { .. } => return true,

                StmtKind::Expr(expr)
                | StmtKind::Let { value: expr, .. }
                | StmtKind::Return { value: Some(expr) } => {
                    if self.expr_contains_uncaught_raise(expr) {
                        return true;
                    }
                }

                StmtKind::Assign { value, .. } => {
                    if self.expr_contains_uncaught_raise(value) {
                        return true;
                    }
                }

                // Try/except block - need to check if exceptions are caught
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    // Check if try body has raises
                    if self.has_uncaught_raise(body) {
                        // Check if any handler catches all exceptions.
                        // `except:` and `except (..., Exception, ...)` both catch all.
                        let catches_all = handlers.iter().any(|h| {
                            h.exc_types.is_none()
                                || h.exc_types.as_ref().is_some_and(|types| {
                                    types.iter().any(|ty| ty.as_str() == "Exception")
                                })
                        });
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

                // Control flow - check all branches for uncaught raises
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

    /// Recursively inspect expression trees for embedded block raises.
    fn expr_contains_uncaught_raise(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Block { stmts } => self.has_uncaught_raise(stmts),
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                self.expr_contains_uncaught_raise(func)
                    || args
                        .iter()
                        .any(|arg| self.expr_contains_uncaught_raise(arg))
                    || keywords
                        .iter()
                        .any(|kw| self.expr_contains_uncaught_raise(&kw.value))
            }
            ExprKind::Starred { value } => self.expr_contains_uncaught_raise(value),
            ExprKind::Yield { value } => value
                .as_ref()
                .is_some_and(|inner| self.expr_contains_uncaught_raise(inner)),
            ExprKind::Attr { value, .. } => self.expr_contains_uncaught_raise(value),
            ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                self.expr_contains_uncaught_raise(left) || self.expr_contains_uncaught_raise(right)
            }
            ExprKind::Unary { expr, .. } => self.expr_contains_uncaught_raise(expr),
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.expr_contains_uncaught_raise(left)
                    || comparators
                        .iter()
                        .any(|cmp| self.expr_contains_uncaught_raise(cmp))
            }
            ExprKind::BoolOp { values, .. }
            | ExprKind::List(values)
            | ExprKind::Tuple(values)
            | ExprKind::Set(values) => values
                .iter()
                .any(|value| self.expr_contains_uncaught_raise(value)),
            ExprKind::Dict(items) => items.iter().any(|(k, v)| {
                self.expr_contains_uncaught_raise(k) || self.expr_contains_uncaught_raise(v)
            }),
            ExprKind::Index { value, index } => {
                self.expr_contains_uncaught_raise(value) || self.expr_contains_uncaught_raise(index)
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.expr_contains_uncaught_raise(value)
                    || start
                        .as_ref()
                        .is_some_and(|inner| self.expr_contains_uncaught_raise(inner))
                    || end
                        .as_ref()
                        .is_some_and(|inner| self.expr_contains_uncaught_raise(inner))
                    || step
                        .as_ref()
                        .is_some_and(|inner| self.expr_contains_uncaught_raise(inner))
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.expr_contains_uncaught_raise(elt)
                    || self.expr_contains_uncaught_raise(iter)
                    || ifs
                        .iter()
                        .any(|cond| self.expr_contains_uncaught_raise(cond))
            }
            ExprKind::UnionCtor { inner, .. } => self.expr_contains_uncaught_raise(inner),
            ExprKind::Lambda { body, .. } => self.expr_contains_uncaught_raise(body),
            ExprKind::IfExpr { test, body, orelse } => {
                self.expr_contains_uncaught_raise(test)
                    || self.expr_contains_uncaught_raise(body)
                    || self.expr_contains_uncaught_raise(orelse)
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => false,
        }
    }

    /// Check if statements contain calls to throwing functions (not caught).
    ///
    /// Similar to has_uncaught_raise but looks for function calls instead.
    ///
    /// A throwing call is "uncaught" if:
    /// 1. The called function can throw (in our throwing_functions set)
    /// 2. It's not inside a try/except that catches all
    ///
    /// We recursively check expressions to find calls at any depth.
    fn has_uncaught_throwing_call(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match &stmt.kind {
                // Check expressions in statements that can contain calls
                StmtKind::Expr(expr)
                | StmtKind::Let { value: expr, .. }
                | StmtKind::Return { value: Some(expr) } => {
                    if self.expr_calls_throwing_function(expr) {
                        return true;
                    }
                }

                StmtKind::Assign { target, value } => {
                    if self.expr_calls_throwing_function(value) {
                        return true;
                    }
                    if self.assign_target_can_throw(target, value.ty.as_ref()) {
                        return true;
                    }
                }

                // Try/except - same logic as has_uncaught_raise
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    // Check if try body has throwing calls
                    if self.has_uncaught_throwing_call(body) {
                        // `except:` and `except (..., Exception, ...)` both catch all.
                        let catches_all = handlers.iter().any(|h| {
                            h.exc_types.is_none()
                                || h.exc_types.as_ref().is_some_and(|types| {
                                    types.iter().any(|ty| ty.as_str() == "Exception")
                                })
                        });
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

    /// Recursively check if an expression contains a call to a throwing function.
    ///
    /// We need to check:
    /// 1. Direct calls: foo() where foo throws
    /// 2. Calls in subexpressions: (foo() + 1) where foo throws
    /// 3. Calls in collections: [foo(), bar()] where either throws
    ///
    /// Returns true if any throwing call is found at any depth.
    fn expr_calls_throwing_function(&self, expr: &Expr) -> bool {
        match &expr.kind {
            // Function call - check if the function throws
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Name(name) = &func.kind {
                    if self.builtin_call_can_throw(name, args) {
                        return true;
                    }
                    if self.throwing_functions.contains(name) {
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

                if self.expr_calls_throwing_function(func) {
                    return true;
                }

                // Check arguments recursively
                for arg in args {
                    if self.expr_calls_throwing_function(arg) {
                        return true;
                    }
                }
                for kw in keywords {
                    if self.expr_calls_throwing_function(&kw.value) {
                        return true;
                    }
                }

                false
            }

            // Binary/unary/comparison operators - check operands
            ExprKind::Binary { left, right, .. } => {
                self.expr_calls_throwing_function(left) || self.expr_calls_throwing_function(right)
            }

            ExprKind::Unary { expr, .. } => self.expr_calls_throwing_function(expr),

            ExprKind::Compare { left, right, .. } => {
                self.expr_calls_throwing_function(left) || self.expr_calls_throwing_function(right)
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                if self.expr_calls_throwing_function(left) {
                    return true;
                }
                comparators
                    .iter()
                    .any(|cmp| self.expr_calls_throwing_function(cmp))
            }

            // Boolean operations - check all values
            ExprKind::BoolOp { values, .. } => {
                values.iter().any(|v| self.expr_calls_throwing_function(v))
            }

            // Collection literals - check all elements
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                items.iter().any(|e| self.expr_calls_throwing_function(e))
            }

            // Dict literal - check keys and values
            ExprKind::Dict(pairs) => pairs.iter().any(|(k, v)| {
                self.expr_calls_throwing_function(k) || self.expr_calls_throwing_function(v)
            }),

            // Indexing/slicing - check all parts
            ExprKind::Index { value, index } => {
                if self.expr_calls_throwing_function(value)
                    || self.expr_calls_throwing_function(index)
                {
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
                if self.expr_calls_throwing_function(value)
                    || start
                        .as_ref()
                        .is_some_and(|s| self.expr_calls_throwing_function(s))
                    || end
                        .as_ref()
                        .is_some_and(|e| self.expr_calls_throwing_function(e))
                {
                    return true;
                }
                step.as_ref().is_some_and(|st| {
                    self.expr_calls_throwing_function(st) || self.step_value_can_throw(st)
                })
            }

            // List/set comprehension - check element expr, iterator, and filters
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.expr_calls_throwing_function(elt)
                    || self.expr_calls_throwing_function(iter)
                    || ifs.iter().any(|i| self.expr_calls_throwing_function(i))
            }

            // Union constructor and lambda - check inner expressions
            ExprKind::UnionCtor { inner, .. } => self.expr_calls_throwing_function(inner),

            ExprKind::Lambda { body, .. } => self.expr_calls_throwing_function(body),

            // Conditional expression - check all branches
            ExprKind::IfExpr { test, body, orelse } => {
                self.expr_calls_throwing_function(test)
                    || self.expr_calls_throwing_function(body)
                    || self.expr_calls_throwing_function(orelse)
            }

            // Attribute access - check the object
            ExprKind::Attr { value, .. } => self.expr_calls_throwing_function(value),

            // Block expression - check statements
            ExprKind::Block { stmts } => self.has_uncaught_throwing_call(stmts),

            // Literals and names can't throw
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

    /// Determine whether assignment targets can throw (e.g., list indexing).
    fn assign_target_can_throw(&self, target: &AssignTarget, value_ty: Option<&Type>) -> bool {
        match target {
            AssignTarget::Name(_) => false,
            AssignTarget::Attr { value, .. } => self.expr_calls_throwing_function(value),
            AssignTarget::Index { value, index } => {
                if self.expr_calls_throwing_function(value)
                    || self.expr_calls_throwing_function(index)
                {
                    return true;
                }
                matches!(value.ty.as_ref(), Some(Type::List(_)))
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                // Unpacking from a list uses indexing and can throw on length mismatch.
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
            AssignTarget::Starred(inner) => self.assign_target_can_throw(inner, value_ty),
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
