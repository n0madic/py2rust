// Expression codegen entry point and module wiring.

mod atoms;
mod calls;
mod collections;
mod helpers;
mod lambda;
mod ops;

use super::*;
use crate::hir_visit::ExprVisitor;

/// Immutable visitor that maps expression variants to codegen handlers.
struct GenExprVisitor<'cg, 'a> {
    cg: &'cg mut Codegen<'a>,
    expr: &'cg Expr,
}

impl<'cg, 'a> ExprVisitor<Result<String, CompileError>> for GenExprVisitor<'cg, 'a> {
    fn visit_literal(&mut self, lit: &Literal) -> Result<String, CompileError> {
        self.cg.gen_literal_expr(self.expr, lit)
    }

    fn visit_name(&mut self, name: &str) -> Result<String, CompileError> {
        self.cg.gen_name_expr(name)
    }

    fn visit_yield(&mut self, _value: &Option<Box<Expr>>) -> Result<String, CompileError> {
        Err(self.cg.error(
            self.expr.span,
            "yield expressions are only emitted through generator wrappers",
        ))
    }

    fn visit_call(
        &mut self,
        func: &Expr,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        self.cg.gen_call_expr(self.expr, func, args, keywords)
    }

    fn visit_starred(&mut self, _value: &Expr) -> Result<String, CompileError> {
        Err(self.cg.error(
            self.expr.span,
            "Starred arguments are only valid directly inside call expressions",
        ))
    }

    fn visit_attr(&mut self, value: &Expr, attr: &str) -> Result<String, CompileError> {
        self.cg.gen_attr_expr(value, attr)
    }

    fn visit_binary(
        &mut self,
        op: &BinOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<String, CompileError> {
        self.cg.gen_binary_expr(self.expr, op, left, right)
    }

    fn visit_unary(&mut self, op: &UnaryOp, inner: &Expr) -> Result<String, CompileError> {
        self.cg.gen_unary_expr(op, inner)
    }

    fn visit_compare(
        &mut self,
        op: &CmpOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<String, CompileError> {
        self.cg.gen_compare_expr(self.expr, op, left, right)
    }

    fn visit_compare_chain(
        &mut self,
        left: &Expr,
        ops: &[CmpOp],
        comparators: &[Expr],
    ) -> Result<String, CompileError> {
        self.cg
            .gen_compare_chain_expr(self.expr, left, ops, comparators)
    }

    fn visit_bool_op(&mut self, op: &BoolOp, values: &[Expr]) -> Result<String, CompileError> {
        self.cg.gen_boolop_expr(op, values)
    }

    fn visit_list(&mut self, items: &[Expr]) -> Result<String, CompileError> {
        self.cg.gen_list_expr(self.expr, items)
    }

    fn visit_tuple(&mut self, items: &[Expr]) -> Result<String, CompileError> {
        self.cg.gen_tuple_expr(self.expr, items)
    }

    fn visit_dict(&mut self, items: &[(Expr, Expr)]) -> Result<String, CompileError> {
        self.cg.gen_dict_expr(self.expr, items)
    }

    fn visit_set(&mut self, items: &[Expr]) -> Result<String, CompileError> {
        self.cg.gen_set_expr(self.expr, items)
    }

    fn visit_index(&mut self, value: &Expr, index: &Expr) -> Result<String, CompileError> {
        self.cg.gen_index_expr(self.expr, value, index)
    }

    fn visit_slice(
        &mut self,
        value: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
        step: Option<&Expr>,
    ) -> Result<String, CompileError> {
        self.cg.gen_slice_expr(self.expr, value, start, end, step)
    }

    fn visit_list_comp(
        &mut self,
        elt: &Expr,
        target: &str,
        iter: &Expr,
        ifs: &[Expr],
        generators: &[CompClause],
    ) -> Result<String, CompileError> {
        self.cg
            .gen_list_comp_expr(elt, target, iter, ifs, generators)
    }

    fn visit_set_comp(
        &mut self,
        elt: &Expr,
        target: &str,
        iter: &Expr,
        ifs: &[Expr],
        generators: &[CompClause],
    ) -> Result<String, CompileError> {
        self.cg
            .gen_set_comp_expr(elt, target, iter, ifs, generators)
    }

    fn visit_union_ctor(
        &mut self,
        union: &str,
        variant: &str,
        inner: &Expr,
    ) -> Result<String, CompileError> {
        self.cg.gen_union_ctor_expr(union, variant, inner)
    }

    fn visit_lambda(&mut self, params: &[String], body: &Expr) -> Result<String, CompileError> {
        self.cg.gen_lambda_expr(self.expr, params, body)
    }

    fn visit_if_expr(
        &mut self,
        test: &Expr,
        body: &Expr,
        orelse: &Expr,
    ) -> Result<String, CompileError> {
        self.cg.gen_if_expr(test, body, orelse)
    }

    fn visit_block(&mut self, stmts: &[Stmt]) -> Result<String, CompileError> {
        self.cg.gen_block_expr(stmts)
    }
}

impl<'a> Codegen<'a> {
    /// Generate Rust code for an expression.
    ///
    /// This is one of the most complex parts of codegen because expressions:
    /// 1. Need type-specific handling (numeric suffixes, collection constructors)
    /// 2. May require helper function injection (print, len, range, etc.)
    /// 3. Must handle mixed int/float arithmetic with casts
    /// 4. Need to bridge Python's dynamic semantics to Rust's static types
    ///
    /// Key design decisions:
    /// - Literals: Always suffix numeric literals (42i64, 3.14f64) to avoid ambiguity
    /// - Strings: Prefer literal .to_string() for string constants
    /// - None: Maps to () or None depending on whether it's in Optional context
    /// - __name__: Special variable backed by const __NAME__, calls .to_string() on access
    /// - Globals: Access via OnceLock mutex wrapper for thread-safe mutation
    /// - Builtins: Many Python builtins (print, len, range) are emitted as helper calls
    pub(crate) fn gen_expr(&mut self, expr: &Expr) -> Result<String, CompileError> {
        let mut visitor = GenExprVisitor { cg: self, expr };
        expr.accept(&mut visitor)
    }
}
