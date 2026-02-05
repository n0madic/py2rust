// Expression codegen entry point and module wiring.

mod atoms;
mod calls;
mod collections;
mod helpers;
mod lambda;
mod ops;

use super::*;

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
        match &expr.kind {
            ExprKind::Literal(lit) => self.gen_literal_expr(expr, lit),
            ExprKind::Name(name) => self.gen_name_expr(name),
            ExprKind::Attr { value, attr } => self.gen_attr_expr(value, attr),
            ExprKind::Call { func, args } => self.gen_call_expr(expr, func, args),
            ExprKind::Binary { op, left, right } => self.gen_binary_expr(expr, op, left, right),
            ExprKind::Unary { op, expr: inner } => self.gen_unary_expr(op, inner),
            ExprKind::Compare { op, left, right } => self.gen_compare_expr(expr, op, left, right),
            ExprKind::BoolOp { op, values } => self.gen_boolop_expr(op, values),
            ExprKind::List(items) => self.gen_list_expr(expr, items),
            ExprKind::Tuple(items) => self.gen_tuple_expr(expr, items),
            ExprKind::Dict(items) => self.gen_dict_expr(expr, items),
            ExprKind::Set(items) => self.gen_set_expr(expr, items),
            ExprKind::Index { value, index } => self.gen_index_expr(expr, value, index),
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => self.gen_slice_expr(
                expr,
                value,
                start.as_deref(),
                end.as_deref(),
                step.as_deref(),
            ),
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
            } => self.gen_list_comp_expr(elt, target, iter, ifs),
            ExprKind::SetComp {
                elt,
                target,
                iter,
                ifs,
            } => self.gen_set_comp_expr(elt, target, iter, ifs),
            ExprKind::Lambda { params, body } => self.gen_lambda_expr(expr, params, body),
            ExprKind::IfExpr { test, body, orelse } => self.gen_if_expr(test, body, orelse),
            ExprKind::Block { stmts } => self.gen_block_expr(stmts),
            ExprKind::UnionCtor {
                union,
                variant,
                inner,
            } => self.gen_union_ctor_expr(union, variant, inner),
        }
    }
}
