use super::*;

mod access;
mod compound;
mod ops;

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
        let mut replace_with_none = false;
        let ty = match &mut expr.kind {
            ExprKind::Literal(lit) => Self::literal_type(lit),
            ExprKind::Name(name) => {
                let (ty, should_replace) = self.check_name_expr(name, expected, expr.span)?;
                replace_with_none = should_replace;
                ty
            }
            ExprKind::Attr { value, attr } => self.check_attr_expr(value, attr, expr.span)?,
            ExprKind::Call {
                func,
                args,
                keywords,
            } => self.check_call(func, args, keywords, expected, expr.span)?,
            ExprKind::Starred { value } => self.check_starred_expr(value, expr.span)?,
            ExprKind::Binary { op, left, right } => {
                self.check_binary_expr(op, left, right, expr.span)?
            }
            ExprKind::Unary { op, expr: inner } => self.check_unary_expr(op, inner, expr.span)?,
            ExprKind::Compare { op, left, right } => {
                self.check_compare_expr(expr.span, op, left, right)?
            }
            ExprKind::CompareChain {
                left,
                ops,
                comparators,
            } => self.check_compare_chain_expr(expr.span, left, ops, comparators)?,
            ExprKind::BoolOp { op: _, values } => self.check_bool_op_expr(values, expr.span)?,
            ExprKind::List(items) => self.check_list_expr(items, expected, expr.span)?,
            ExprKind::Tuple(items) => self.check_tuple_expr(items)?,
            ExprKind::Set(items) => self.check_set_expr(items, expected, expr.span)?,
            ExprKind::Dict(items) => self.check_dict_expr(items, expected, expr.span)?,
            ExprKind::Index { value, index } => self.check_index_expr(value, index, expr.span)?,
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => self.check_slice_expr(value, start, end, step, expr.span)?,
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
            } => self.check_list_comp_expr(elt, target, iter, ifs, expr.span)?,
            ExprKind::SetComp {
                elt,
                target,
                iter,
                ifs,
            } => self.check_set_comp_expr(elt, target, iter, ifs, expr.span)?,
            ExprKind::UnionCtor {
                union,
                variant,
                inner,
            } => self.check_union_ctor_expr(union, variant, inner, expr.span)?,
            ExprKind::Lambda { params, body } => {
                self.check_lambda_expr(params, body, expected, expr.span)?
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.check_if_expr_expr(test, body, orelse, expr.span)?
            }
            ExprKind::Block { stmts } => self.check_block_expr(stmts)?,
        };

        if replace_with_none {
            // Replace unresolved names to reduce cascading diagnostics downstream.
            expr.kind = ExprKind::Literal(Literal::None);
        }
        expr.ty = Some(ty.clone());
        Ok(ty)
    }
}
