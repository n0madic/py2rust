use super::*;
use crate::hir_visit::ExprVisitorMut;

mod access;
mod compound;
mod ops;

/// Mutable visitor that maps expression variants to type-checking handlers.
struct CheckExprVisitor<'tc, 'a, 'e> {
    tc: &'tc mut TypeChecker<'a>,
    expected: Option<&'e Type>,
    span: Span,
}

impl<'tc, 'a, 'e> ExprVisitorMut<Result<Type, CompileError>> for CheckExprVisitor<'tc, 'a, 'e> {
    fn visit_literal_mut(&mut self, lit: &mut Literal) -> Result<Type, CompileError> {
        Ok(TypeChecker::literal_type(lit))
    }

    fn visit_name_mut(&mut self, name: &mut String) -> Result<Type, CompileError> {
        self.tc.check_name_expr(name, self.expected, self.span)
    }

    fn visit_yield_mut(&mut self, value: &mut Option<Box<Expr>>) -> Result<Type, CompileError> {
        self.tc.check_yield_expr(value.as_deref_mut(), self.span)
    }

    fn visit_call_mut(
        &mut self,
        func: &mut Expr,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
    ) -> Result<Type, CompileError> {
        self.tc
            .check_call(func, args, keywords, self.expected, self.span)
    }

    fn visit_starred_mut(&mut self, value: &mut Expr) -> Result<Type, CompileError> {
        self.tc.check_starred_expr(value, self.span)
    }

    fn visit_attr_mut(
        &mut self,
        value: &mut Expr,
        attr: &mut String,
    ) -> Result<Type, CompileError> {
        self.tc.check_attr_expr(value, attr, self.span)
    }

    fn visit_binary_mut(
        &mut self,
        op: &mut BinOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc.check_binary_expr(op, left, right, self.span)
    }

    fn visit_unary_mut(
        &mut self,
        op: &mut UnaryOp,
        inner: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc.check_unary_expr(op, inner, self.span)
    }

    fn visit_compare_mut(
        &mut self,
        op: &mut CmpOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc.check_compare_expr(self.span, op, left, right)
    }

    fn visit_compare_chain_mut(
        &mut self,
        left: &mut Expr,
        ops: &mut [CmpOp],
        comparators: &mut [Expr],
    ) -> Result<Type, CompileError> {
        self.tc
            .check_compare_chain_expr(self.span, left, ops, comparators)
    }

    fn visit_bool_op_mut(
        &mut self,
        _op: &mut BoolOp,
        values: &mut [Expr],
    ) -> Result<Type, CompileError> {
        self.tc.check_bool_op_expr(values, self.span)
    }

    fn visit_list_mut(&mut self, items: &mut [Expr]) -> Result<Type, CompileError> {
        self.tc.check_list_expr(items, self.expected, self.span)
    }

    fn visit_tuple_mut(&mut self, items: &mut [Expr]) -> Result<Type, CompileError> {
        self.tc.check_tuple_expr(items)
    }

    fn visit_dict_mut(&mut self, items: &mut [DictEntry]) -> Result<Type, CompileError> {
        self.tc.check_dict_expr(items, self.expected, self.span)
    }

    fn visit_set_mut(&mut self, items: &mut [Expr]) -> Result<Type, CompileError> {
        self.tc.check_set_expr(items, self.expected, self.span)
    }

    fn visit_index_mut(
        &mut self,
        value: &mut Expr,
        index: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc.check_index_expr(value, index, self.span)
    }

    fn visit_slice_mut(
        &mut self,
        value: &mut Expr,
        start: &mut Option<Box<Expr>>,
        end: &mut Option<Box<Expr>>,
        step: &mut Option<Box<Expr>>,
    ) -> Result<Type, CompileError> {
        self.tc.check_slice_expr(value, start, end, step, self.span)
    }

    fn visit_list_comp_mut(
        &mut self,
        elt: &mut Expr,
        target: &mut String,
        iter: &mut Expr,
        ifs: &mut [Expr],
        generators: &mut [CompClause],
    ) -> Result<Type, CompileError> {
        self.tc
            .check_list_comp_expr(elt, target, iter, ifs, generators, self.span)
    }

    fn visit_set_comp_mut(
        &mut self,
        elt: &mut Expr,
        target: &mut String,
        iter: &mut Expr,
        ifs: &mut [Expr],
        generators: &mut [CompClause],
    ) -> Result<Type, CompileError> {
        self.tc
            .check_set_comp_expr(elt, target, iter, ifs, generators, self.span)
    }

    fn visit_union_ctor_mut(
        &mut self,
        union: &mut String,
        variant: &mut String,
        inner: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc
            .check_union_ctor_expr(union, variant, inner, self.span)
    }

    fn visit_lambda_mut(
        &mut self,
        params: &mut [String],
        param_kinds: &mut [ParamKind],
        has_defaults: &mut [bool],
        _defaults: &mut [Option<Expr>],
        body: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc.check_lambda_expr(
            params,
            param_kinds,
            has_defaults,
            body,
            self.expected,
            self.span,
        )
    }

    fn visit_if_expr_mut(
        &mut self,
        test: &mut Expr,
        body: &mut Expr,
        orelse: &mut Expr,
    ) -> Result<Type, CompileError> {
        self.tc.check_if_expr_expr(test, body, orelse, self.span)
    }

    fn visit_block_mut(&mut self, stmts: &mut [Stmt]) -> Result<Type, CompileError> {
        self.tc.check_block_expr(stmts)
    }
}

/// Expression type checking.
///
/// This module coordinates expression type inference and validation and
/// delegates concrete expression families to focused submodules.
impl<'a> TypeChecker<'a> {
    /// Check an expression and determine its type.
    ///
    /// The expected parameter guides type inference when we have a hint
    /// about what type we want (e.g., from an annotation or context).
    ///
    /// Returns the inferred type and updates expr.ty.
    pub(super) fn check_expr(
        &mut self,
        expr: &mut Expr,
        expected: Option<&Type>,
    ) -> Result<Type, CompileError> {
        let mut visitor = CheckExprVisitor {
            tc: self,
            expected,
            span: expr.span,
        };
        let ty = expr.accept_mut(&mut visitor)?;
        expr.ty = Some(ty.clone());
        Ok(ty)
    }

    /// Validate a generator `yield` expression and track inferred yield type.
    fn check_yield_expr(
        &mut self,
        value: Option<&mut Expr>,
        span: Span,
    ) -> Result<Type, CompileError> {
        if self.generator_yield_stack.is_empty() {
            return Err(self.error(span, "yield is only valid inside functions"));
        }

        let yielded = match value {
            Some(expr) => self.check_expr(expr, None)?,
            None => Type::None,
        };

        // Merge all yield sites to infer the function's iterator item type.
        if let Some(slot) = self.generator_yield_stack.last_mut() {
            let merged = match slot.take() {
                Some(prev) => Self::merge_types(prev, yielded.clone()),
                None => yielded.clone(),
            };
            *slot = Some(merged);
        }

        // In expression position, `yield` evaluates to the value sent back in.
        // We conservatively treat it as the yielded type to keep local inference usable.
        Ok(yielded)
    }
}
