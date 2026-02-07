//! Macro-generated visitor traits for HIR expression and statement dispatch.
//!
//! The variant lists below are the single source of truth for `ExprKind` and
//! `StmtKind` visitor coverage. Adding a new variant now requires updating one
//! macro declaration instead of manually syncing multiple `match` statements.

use crate::hir::*;
use crate::types::TypeRef;

/// Define immutable/mutable expression visitors and `Expr::accept*` dispatch.
macro_rules! define_expr_visitors {
    (
        $(
            {
                imm_pat: $imm_pat:pat => $imm_method:ident ( $( $imm_arg:ident : $imm_ty:ty ),* $(,)? ) => ( $( $imm_forward:expr ),* $(,)? ),
                mut_pat: $mut_pat:pat => $mut_method:ident ( $( $mut_arg:ident : $mut_ty:ty ),* $(,)? ) => ( $( $mut_forward:expr ),* $(,)? )
            }
        ),+ $(,)?
    ) => {
        /// Immutable visitor for expression variants.
        pub trait ExprVisitor<R> {
            $(fn $imm_method(&mut self, $( $imm_arg : $imm_ty ),*) -> R;)+
        }

        /// Mutable visitor for expression variants.
        pub trait ExprVisitorMut<R> {
            $(fn $mut_method(&mut self, $( $mut_arg : $mut_ty ),*) -> R;)+
        }

        impl Expr {
            /// Dispatch an immutable expression to a visitor.
            pub fn accept<R, V: ExprVisitor<R>>(&self, visitor: &mut V) -> R {
                match &self.kind {
                    $(
                        $imm_pat => visitor.$imm_method($( $imm_forward ),*),
                    )+
                }
            }

            /// Dispatch a mutable expression to a visitor.
            pub fn accept_mut<R, V: ExprVisitorMut<R>>(&mut self, visitor: &mut V) -> R {
                match &mut self.kind {
                    $(
                        $mut_pat => visitor.$mut_method($( $mut_forward ),*),
                    )+
                }
            }
        }
    };
}

/// Define immutable/mutable statement visitors and `Stmt::accept*` dispatch.
macro_rules! define_stmt_visitors {
    (
        $(
            {
                imm_pat: $imm_pat:pat => $imm_method:ident ( $( $imm_arg:ident : $imm_ty:ty ),* $(,)? ) => ( $( $imm_forward:expr ),* $(,)? ),
                mut_pat: $mut_pat:pat => $mut_method:ident ( $( $mut_arg:ident : $mut_ty:ty ),* $(,)? ) => ( $( $mut_forward:expr ),* $(,)? )
            }
        ),+ $(,)?
    ) => {
        /// Immutable visitor for statement variants.
        pub trait StmtVisitor<R> {
            $(fn $imm_method(&mut self, $( $imm_arg : $imm_ty ),*) -> R;)+
        }

        /// Mutable visitor for statement variants.
        pub trait StmtVisitorMut<R> {
            $(fn $mut_method(&mut self, $( $mut_arg : $mut_ty ),*) -> R;)+
        }

        impl Stmt {
            /// Dispatch an immutable statement to a visitor.
            pub fn accept<R, V: StmtVisitor<R>>(&self, visitor: &mut V) -> R {
                match &self.kind {
                    $(
                        $imm_pat => visitor.$imm_method($( $imm_forward ),*),
                    )+
                }
            }

            /// Dispatch a mutable statement to a visitor.
            pub fn accept_mut<R, V: StmtVisitorMut<R>>(&mut self, visitor: &mut V) -> R {
                match &mut self.kind {
                    $(
                        $mut_pat => visitor.$mut_method($( $mut_forward ),*),
                    )+
                }
            }
        }
    };
}

define_expr_visitors!(
    {
        imm_pat: ExprKind::Literal(lit) => visit_literal(lit: &Literal) => (lit),
        mut_pat: ExprKind::Literal(lit) => visit_literal_mut(lit: &mut Literal) => (lit)
    },
    {
        imm_pat: ExprKind::Name(name) => visit_name(name: &str) => (name),
        mut_pat: ExprKind::Name(name) => visit_name_mut(name: &mut String) => (name)
    },
    {
        imm_pat: ExprKind::Yield { value } => visit_yield(value: &Option<Box<Expr>>) => (value),
        mut_pat: ExprKind::Yield { value } => visit_yield_mut(value: &mut Option<Box<Expr>>) => (value)
    },
    {
        imm_pat: ExprKind::Call { func, args, keywords } => visit_call(func: &Expr, args: &[Expr], keywords: &[KeywordArg]) => (func, args, keywords),
        mut_pat: ExprKind::Call { func, args, keywords } => visit_call_mut(func: &mut Expr, args: &mut [Expr], keywords: &mut [KeywordArg]) => (func, args, keywords)
    },
    {
        imm_pat: ExprKind::Starred { value } => visit_starred(value: &Expr) => (value),
        mut_pat: ExprKind::Starred { value } => visit_starred_mut(value: &mut Expr) => (value)
    },
    {
        imm_pat: ExprKind::Attr { value, attr } => visit_attr(value: &Expr, attr: &str) => (value, attr),
        mut_pat: ExprKind::Attr { value, attr } => visit_attr_mut(value: &mut Expr, attr: &mut String) => (value, attr)
    },
    {
        imm_pat: ExprKind::Binary { op, left, right } => visit_binary(op: &BinOp, left: &Expr, right: &Expr) => (op, left, right),
        mut_pat: ExprKind::Binary { op, left, right } => visit_binary_mut(op: &mut BinOp, left: &mut Expr, right: &mut Expr) => (op, left, right)
    },
    {
        imm_pat: ExprKind::Unary { op, expr } => visit_unary(op: &UnaryOp, inner: &Expr) => (op, expr),
        mut_pat: ExprKind::Unary { op, expr } => visit_unary_mut(op: &mut UnaryOp, inner: &mut Expr) => (op, expr)
    },
    {
        imm_pat: ExprKind::Compare { op, left, right } => visit_compare(op: &CmpOp, left: &Expr, right: &Expr) => (op, left, right),
        mut_pat: ExprKind::Compare { op, left, right } => visit_compare_mut(op: &mut CmpOp, left: &mut Expr, right: &mut Expr) => (op, left, right)
    },
    {
        imm_pat: ExprKind::CompareChain { left, ops, comparators } => visit_compare_chain(left: &Expr, ops: &[CmpOp], comparators: &[Expr]) => (left, ops, comparators),
        mut_pat: ExprKind::CompareChain { left, ops, comparators } => visit_compare_chain_mut(left: &mut Expr, ops: &mut [CmpOp], comparators: &mut [Expr]) => (left, ops, comparators)
    },
    {
        imm_pat: ExprKind::BoolOp { op, values } => visit_bool_op(op: &BoolOp, values: &[Expr]) => (op, values),
        mut_pat: ExprKind::BoolOp { op, values } => visit_bool_op_mut(op: &mut BoolOp, values: &mut [Expr]) => (op, values)
    },
    {
        imm_pat: ExprKind::List(items) => visit_list(items: &[Expr]) => (items),
        mut_pat: ExprKind::List(items) => visit_list_mut(items: &mut [Expr]) => (items)
    },
    {
        imm_pat: ExprKind::Tuple(items) => visit_tuple(items: &[Expr]) => (items),
        mut_pat: ExprKind::Tuple(items) => visit_tuple_mut(items: &mut [Expr]) => (items)
    },
    {
        imm_pat: ExprKind::Dict(items) => visit_dict(items: &[(Expr, Expr)]) => (items),
        mut_pat: ExprKind::Dict(items) => visit_dict_mut(items: &mut [(Expr, Expr)]) => (items)
    },
    {
        imm_pat: ExprKind::Set(items) => visit_set(items: &[Expr]) => (items),
        mut_pat: ExprKind::Set(items) => visit_set_mut(items: &mut [Expr]) => (items)
    },
    {
        imm_pat: ExprKind::Index { value, index } => visit_index(value: &Expr, index: &Expr) => (value, index),
        mut_pat: ExprKind::Index { value, index } => visit_index_mut(value: &mut Expr, index: &mut Expr) => (value, index)
    },
    {
        imm_pat: ExprKind::Slice { value, start, end, step } => visit_slice(value: &Expr, start: Option<&Expr>, end: Option<&Expr>, step: Option<&Expr>) => (value, start.as_deref(), end.as_deref(), step.as_deref()),
        mut_pat: ExprKind::Slice { value, start, end, step } => visit_slice_mut(value: &mut Expr, start: &mut Option<Box<Expr>>, end: &mut Option<Box<Expr>>, step: &mut Option<Box<Expr>>) => (value, start, end, step)
    },
    {
        imm_pat: ExprKind::ListComp { elt, target, iter, ifs, generators } => visit_list_comp(elt: &Expr, target: &str, iter: &Expr, ifs: &[Expr], generators: &[CompClause]) => (elt, target, iter, ifs, generators),
        mut_pat: ExprKind::ListComp { elt, target, iter, ifs, generators } => visit_list_comp_mut(elt: &mut Expr, target: &mut String, iter: &mut Expr, ifs: &mut [Expr], generators: &mut [CompClause]) => (elt, target, iter, ifs, generators)
    },
    {
        imm_pat: ExprKind::SetComp { elt, target, iter, ifs, generators } => visit_set_comp(elt: &Expr, target: &str, iter: &Expr, ifs: &[Expr], generators: &[CompClause]) => (elt, target, iter, ifs, generators),
        mut_pat: ExprKind::SetComp { elt, target, iter, ifs, generators } => visit_set_comp_mut(elt: &mut Expr, target: &mut String, iter: &mut Expr, ifs: &mut [Expr], generators: &mut [CompClause]) => (elt, target, iter, ifs, generators)
    },
    {
        imm_pat: ExprKind::UnionCtor { union, variant, inner } => visit_union_ctor(union: &str, variant: &str, inner: &Expr) => (union, variant, inner),
        mut_pat: ExprKind::UnionCtor { union, variant, inner } => visit_union_ctor_mut(union: &mut String, variant: &mut String, inner: &mut Expr) => (union, variant, inner)
    },
    {
        imm_pat: ExprKind::Lambda { params, body } => visit_lambda(params: &[String], body: &Expr) => (params, body),
        mut_pat: ExprKind::Lambda { params, body } => visit_lambda_mut(params: &mut [String], body: &mut Expr) => (params, body)
    },
    {
        imm_pat: ExprKind::IfExpr { test, body, orelse } => visit_if_expr(test: &Expr, body: &Expr, orelse: &Expr) => (test, body, orelse),
        mut_pat: ExprKind::IfExpr { test, body, orelse } => visit_if_expr_mut(test: &mut Expr, body: &mut Expr, orelse: &mut Expr) => (test, body, orelse)
    },
    {
        imm_pat: ExprKind::Block { stmts } => visit_block(stmts: &[Stmt]) => (stmts),
        mut_pat: ExprKind::Block { stmts } => visit_block_mut(stmts: &mut [Stmt]) => (stmts)
    },
);

define_stmt_visitors!(
    {
        imm_pat: StmtKind::Let { name, ann, value } => visit_let(name: &str, ann: Option<&TypeRef>, value: &Expr) => (name, ann.as_ref(), value),
        mut_pat: StmtKind::Let { name, ann, value } => visit_let_mut(name: &mut String, ann: &mut Option<TypeRef>, value: &mut Expr) => (name, ann, value)
    },
    {
        imm_pat: StmtKind::Assign { target, value } => visit_assign(target: &AssignTarget, value: &Expr) => (target, value),
        mut_pat: StmtKind::Assign { target, value } => visit_assign_mut(target: &mut AssignTarget, value: &mut Expr) => (target, value)
    },
    {
        imm_pat: StmtKind::Return { value } => visit_return(value: Option<&Expr>) => (value.as_ref()),
        mut_pat: StmtKind::Return { value } => visit_return_mut(value: &mut Option<Expr>) => (value)
    },
    {
        imm_pat: StmtKind::If { test, body, orelse } => visit_if(test: &Expr, body: &[Stmt], orelse: &[Stmt]) => (test, body, orelse),
        mut_pat: StmtKind::If { test, body, orelse } => visit_if_mut(test: &mut Expr, body: &mut [Stmt], orelse: &mut [Stmt]) => (test, body, orelse)
    },
    {
        imm_pat: StmtKind::While { test, body } => visit_while(test: &Expr, body: &[Stmt]) => (test, body),
        mut_pat: StmtKind::While { test, body } => visit_while_mut(test: &mut Expr, body: &mut [Stmt]) => (test, body)
    },
    {
        imm_pat: StmtKind::For { target, iter, body } => visit_for(target: &ForTarget, iter: &Expr, body: &[Stmt]) => (target, iter, body),
        mut_pat: StmtKind::For { target, iter, body } => visit_for_mut(target: &mut ForTarget, iter: &mut Expr, body: &mut [Stmt]) => (target, iter, body)
    },
    {
        imm_pat: StmtKind::Import { names } => visit_import(names: &[ImportBinding]) => (names),
        mut_pat: StmtKind::Import { names } => visit_import_mut(names: &mut [ImportBinding]) => (names)
    },
    {
        imm_pat: StmtKind::ImportFrom { module, names } => visit_import_from(module: &str, names: &[ImportFromBinding]) => (module, names),
        mut_pat: StmtKind::ImportFrom { module, names } => visit_import_from_mut(module: &mut String, names: &mut [ImportFromBinding]) => (module, names)
    },
    {
        imm_pat: StmtKind::Global { names } => visit_global(names: &[String]) => (names),
        mut_pat: StmtKind::Global { names } => visit_global_mut(names: &mut [String]) => (names)
    },
    {
        imm_pat: StmtKind::Nonlocal { names } => visit_nonlocal(names: &[String]) => (names),
        mut_pat: StmtKind::Nonlocal { names } => visit_nonlocal_mut(names: &mut [String]) => (names)
    },
    {
        imm_pat: StmtKind::Break => visit_break() => (),
        mut_pat: StmtKind::Break => visit_break_mut() => ()
    },
    {
        imm_pat: StmtKind::Continue => visit_continue() => (),
        mut_pat: StmtKind::Continue => visit_continue_mut() => ()
    },
    {
        imm_pat: StmtKind::Expr(expr) => visit_expr_stmt(expr: &Expr) => (expr),
        mut_pat: StmtKind::Expr(expr) => visit_expr_stmt_mut(expr: &mut Expr) => (expr)
    },
    {
        imm_pat: StmtKind::Assert { test, msg } => visit_assert(test: &Expr, msg: Option<&Expr>) => (test, msg.as_ref()),
        mut_pat: StmtKind::Assert { test, msg } => visit_assert_mut(test: &mut Expr, msg: &mut Option<Expr>) => (test, msg)
    },
    {
        imm_pat: StmtKind::Match { subject, cases } => visit_match(subject: &Expr, cases: &[MatchCase]) => (subject, cases),
        mut_pat: StmtKind::Match { subject, cases } => visit_match_mut(subject: &mut Expr, cases: &mut [MatchCase]) => (subject, cases)
    },
    {
        imm_pat: StmtKind::Try { body, handlers, orelse, finalbody } => visit_try(body: &[Stmt], handlers: &[ExceptHandler], orelse: &[Stmt], finalbody: &[Stmt]) => (body, handlers, orelse, finalbody),
        mut_pat: StmtKind::Try { body, handlers, orelse, finalbody } => visit_try_mut(body: &mut [Stmt], handlers: &mut [ExceptHandler], orelse: &mut [Stmt], finalbody: &mut [Stmt]) => (body, handlers, orelse, finalbody)
    },
    {
        imm_pat: StmtKind::Raise { exc, cause } => visit_raise(exc: Option<&Expr>, cause: Option<&Expr>) => (exc.as_ref(), cause.as_ref()),
        mut_pat: StmtKind::Raise { exc, cause } => visit_raise_mut(exc: &mut Option<Expr>, cause: &mut Option<Expr>) => (exc, cause)
    },
);
