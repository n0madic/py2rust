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
            pub fn accept<R, V: ExprVisitor<R> + ?Sized>(&self, visitor: &mut V) -> R {
                match &self.kind {
                    $(
                        $imm_pat => visitor.$imm_method($( $imm_forward ),*),
                    )+
                }
            }

            /// Dispatch a mutable expression to a visitor.
            pub fn accept_mut<R, V: ExprVisitorMut<R> + ?Sized>(&mut self, visitor: &mut V) -> R {
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
            pub fn accept<R, V: StmtVisitor<R> + ?Sized>(&self, visitor: &mut V) -> R {
                match &self.kind {
                    $(
                        $imm_pat => visitor.$imm_method($( $imm_forward ),*),
                    )+
                }
            }

            /// Dispatch a mutable statement to a visitor.
            pub fn accept_mut<R, V: StmtVisitorMut<R> + ?Sized>(&mut self, visitor: &mut V) -> R {
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
        imm_pat: ExprKind::Dict(items) => visit_dict(items: &[DictEntry]) => (items),
        mut_pat: ExprKind::Dict(items) => visit_dict_mut(items: &mut [DictEntry]) => (items)
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
        imm_pat: ExprKind::Lambda { params, param_kinds, has_defaults, defaults, body } => visit_lambda(params: &[String], param_kinds: &[ParamKind], has_defaults: &[bool], defaults: &[Option<Expr>], body: &Expr) => (params, param_kinds, has_defaults, defaults, body),
        mut_pat: ExprKind::Lambda { params, param_kinds, has_defaults, defaults, body } => visit_lambda_mut(params: &mut [String], param_kinds: &mut [ParamKind], has_defaults: &mut [bool], defaults: &mut [Option<Expr>], body: &mut Expr) => (params, param_kinds, has_defaults, defaults, body)
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
        imm_pat: StmtKind::Delete { target } => visit_delete(target: &AssignTarget) => (target),
        mut_pat: StmtKind::Delete { target } => visit_delete_mut(target: &mut AssignTarget) => (target)
    },
    {
        imm_pat: StmtKind::Class { def } => visit_class(def: &ClassDef) => (def),
        mut_pat: StmtKind::Class { def } => visit_class_mut(def: &mut ClassDef) => (def)
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

/// Default mutable walker for expressions.
///
/// This trait provides a full recursive walk for `ExprKind` with no-op leaf defaults.
/// Implementers can override only the variants they care about.
pub trait ExprWalkerMut {
    fn visit_literal_mut(&mut self, _lit: &mut Literal) {}

    fn visit_name_mut(&mut self, _name: &mut String) {}

    fn visit_yield_mut(&mut self, value: &mut Option<Box<Expr>>) {
        if let Some(value) = value {
            walk_expr_mut(self, value);
        }
    }

    fn visit_call_mut(&mut self, func: &mut Expr, args: &mut [Expr], keywords: &mut [KeywordArg]) {
        walk_expr_mut(self, func);
        walk_expr_slice_mut(self, args);
        for kw in keywords {
            walk_expr_mut(self, &mut kw.value);
        }
    }

    fn visit_starred_mut(&mut self, value: &mut Expr) {
        walk_expr_mut(self, value);
    }

    fn visit_attr_mut(&mut self, value: &mut Expr, _attr: &mut String) {
        walk_expr_mut(self, value);
    }

    fn visit_binary_mut(&mut self, _op: &mut BinOp, left: &mut Expr, right: &mut Expr) {
        walk_expr_mut(self, left);
        walk_expr_mut(self, right);
    }

    fn visit_unary_mut(&mut self, _op: &mut UnaryOp, inner: &mut Expr) {
        walk_expr_mut(self, inner);
    }

    fn visit_compare_mut(&mut self, _op: &mut CmpOp, left: &mut Expr, right: &mut Expr) {
        walk_expr_mut(self, left);
        walk_expr_mut(self, right);
    }

    fn visit_compare_chain_mut(
        &mut self,
        left: &mut Expr,
        _ops: &mut [CmpOp],
        comparators: &mut [Expr],
    ) {
        walk_expr_mut(self, left);
        walk_expr_slice_mut(self, comparators);
    }

    fn visit_bool_op_mut(&mut self, _op: &mut BoolOp, values: &mut [Expr]) {
        walk_expr_slice_mut(self, values);
    }

    fn visit_list_mut(&mut self, items: &mut [Expr]) {
        walk_expr_slice_mut(self, items);
    }

    fn visit_tuple_mut(&mut self, items: &mut [Expr]) {
        walk_expr_slice_mut(self, items);
    }

    fn visit_dict_mut(&mut self, items: &mut [DictEntry]) {
        for entry in items {
            match entry {
                DictEntry::Item { key, value } => {
                    walk_expr_mut(self, key);
                    walk_expr_mut(self, value);
                }
                DictEntry::Unpack { value } => {
                    walk_expr_mut(self, value);
                }
            }
        }
    }

    fn visit_set_mut(&mut self, items: &mut [Expr]) {
        walk_expr_slice_mut(self, items);
    }

    fn visit_index_mut(&mut self, value: &mut Expr, index: &mut Expr) {
        walk_expr_mut(self, value);
        walk_expr_mut(self, index);
    }

    fn visit_slice_mut(
        &mut self,
        value: &mut Expr,
        start: &mut Option<Box<Expr>>,
        end: &mut Option<Box<Expr>>,
        step: &mut Option<Box<Expr>>,
    ) {
        walk_expr_mut(self, value);
        if let Some(start) = start {
            walk_expr_mut(self, start);
        }
        if let Some(end) = end {
            walk_expr_mut(self, end);
        }
        if let Some(step) = step {
            walk_expr_mut(self, step);
        }
    }

    fn visit_list_comp_mut(
        &mut self,
        elt: &mut Expr,
        _target: &mut String,
        iter: &mut Expr,
        ifs: &mut [Expr],
        generators: &mut [CompClause],
    ) {
        walk_expr_mut(self, elt);
        walk_expr_mut(self, iter);
        walk_expr_slice_mut(self, ifs);
        walk_comp_clause_slice_mut(self, generators);
    }

    fn visit_set_comp_mut(
        &mut self,
        elt: &mut Expr,
        _target: &mut String,
        iter: &mut Expr,
        ifs: &mut [Expr],
        generators: &mut [CompClause],
    ) {
        walk_expr_mut(self, elt);
        walk_expr_mut(self, iter);
        walk_expr_slice_mut(self, ifs);
        walk_comp_clause_slice_mut(self, generators);
    }

    fn visit_union_ctor_mut(
        &mut self,
        _union: &mut String,
        _variant: &mut String,
        inner: &mut Expr,
    ) {
        walk_expr_mut(self, inner);
    }

    fn visit_lambda_mut(
        &mut self,
        _params: &mut [String],
        _param_kinds: &mut [ParamKind],
        _has_defaults: &mut [bool],
        _defaults: &mut [Option<Expr>],
        body: &mut Expr,
    ) {
        walk_expr_mut(self, body);
    }

    fn visit_if_expr_mut(&mut self, test: &mut Expr, body: &mut Expr, orelse: &mut Expr) {
        walk_expr_mut(self, test);
        walk_expr_mut(self, body);
        walk_expr_mut(self, orelse);
    }

    fn visit_block_mut(&mut self, stmts: &mut [Stmt]) {
        walk_stmt_slice_expr_only_mut(self, stmts);
    }
}

/// Default mutable walker for statements.
///
/// This trait uses `ExprWalkerMut` for nested expression traversal.
pub trait StmtWalkerMut: ExprWalkerMut {
    fn visit_let_mut(&mut self, _name: &mut String, _ann: &mut Option<TypeRef>, value: &mut Expr) {
        walk_expr_mut(self, value);
    }

    fn visit_assign_mut(&mut self, target: &mut AssignTarget, value: &mut Expr) {
        walk_assign_target_mut(self, target);
        walk_expr_mut(self, value);
    }

    fn visit_delete_mut(&mut self, target: &mut AssignTarget) {
        walk_assign_target_mut(self, target);
    }

    fn visit_class_mut(&mut self, def: &mut ClassDef) {
        walk_class_def_mut(self, def);
    }

    fn visit_return_mut(&mut self, value: &mut Option<Expr>) {
        if let Some(value) = value {
            walk_expr_mut(self, value);
        }
    }

    fn visit_if_mut(&mut self, test: &mut Expr, body: &mut [Stmt], orelse: &mut [Stmt]) {
        walk_expr_mut(self, test);
        walk_stmt_slice_mut(self, body);
        walk_stmt_slice_mut(self, orelse);
    }

    fn visit_while_mut(&mut self, test: &mut Expr, body: &mut [Stmt]) {
        walk_expr_mut(self, test);
        walk_stmt_slice_mut(self, body);
    }

    fn visit_for_mut(&mut self, target: &mut ForTarget, iter: &mut Expr, body: &mut [Stmt]) {
        walk_for_target_mut(self, target);
        walk_expr_mut(self, iter);
        walk_stmt_slice_mut(self, body);
    }

    fn visit_import_mut(&mut self, _names: &mut [ImportBinding]) {}

    fn visit_import_from_mut(&mut self, _module: &mut String, _names: &mut [ImportFromBinding]) {}

    fn visit_global_mut(&mut self, _names: &mut [String]) {}

    fn visit_nonlocal_mut(&mut self, _names: &mut [String]) {}

    fn visit_break_mut(&mut self) {}

    fn visit_continue_mut(&mut self) {}

    fn visit_expr_stmt_mut(&mut self, expr: &mut Expr) {
        walk_expr_mut(self, expr);
    }

    fn visit_assert_mut(&mut self, test: &mut Expr, msg: &mut Option<Expr>) {
        walk_expr_mut(self, test);
        if let Some(msg) = msg {
            walk_expr_mut(self, msg);
        }
    }

    fn visit_match_mut(&mut self, subject: &mut Expr, cases: &mut [MatchCase]) {
        walk_expr_mut(self, subject);
        walk_match_case_slice_mut(self, cases);
    }

    fn visit_try_mut(
        &mut self,
        body: &mut [Stmt],
        handlers: &mut [ExceptHandler],
        orelse: &mut [Stmt],
        finalbody: &mut [Stmt],
    ) {
        walk_stmt_slice_mut(self, body);
        walk_except_handler_slice_mut(self, handlers);
        walk_stmt_slice_mut(self, orelse);
        walk_stmt_slice_mut(self, finalbody);
    }

    fn visit_raise_mut(&mut self, exc: &mut Option<Expr>, cause: &mut Option<Expr>) {
        if let Some(exc) = exc {
            walk_expr_mut(self, exc);
        }
        if let Some(cause) = cause {
            walk_expr_mut(self, cause);
        }
    }
}

impl<T: ExprWalkerMut + ?Sized> ExprVisitorMut<()> for T {
    fn visit_literal_mut(&mut self, lit: &mut Literal) {
        ExprWalkerMut::visit_literal_mut(self, lit);
    }

    fn visit_name_mut(&mut self, name: &mut String) {
        ExprWalkerMut::visit_name_mut(self, name);
    }

    fn visit_yield_mut(&mut self, value: &mut Option<Box<Expr>>) {
        ExprWalkerMut::visit_yield_mut(self, value);
    }

    fn visit_call_mut(&mut self, func: &mut Expr, args: &mut [Expr], keywords: &mut [KeywordArg]) {
        ExprWalkerMut::visit_call_mut(self, func, args, keywords);
    }

    fn visit_starred_mut(&mut self, value: &mut Expr) {
        ExprWalkerMut::visit_starred_mut(self, value);
    }

    fn visit_attr_mut(&mut self, value: &mut Expr, attr: &mut String) {
        ExprWalkerMut::visit_attr_mut(self, value, attr);
    }

    fn visit_binary_mut(&mut self, op: &mut BinOp, left: &mut Expr, right: &mut Expr) {
        ExprWalkerMut::visit_binary_mut(self, op, left, right);
    }

    fn visit_unary_mut(&mut self, op: &mut UnaryOp, inner: &mut Expr) {
        ExprWalkerMut::visit_unary_mut(self, op, inner);
    }

    fn visit_compare_mut(&mut self, op: &mut CmpOp, left: &mut Expr, right: &mut Expr) {
        ExprWalkerMut::visit_compare_mut(self, op, left, right);
    }

    fn visit_compare_chain_mut(
        &mut self,
        left: &mut Expr,
        ops: &mut [CmpOp],
        comparators: &mut [Expr],
    ) {
        ExprWalkerMut::visit_compare_chain_mut(self, left, ops, comparators);
    }

    fn visit_bool_op_mut(&mut self, op: &mut BoolOp, values: &mut [Expr]) {
        ExprWalkerMut::visit_bool_op_mut(self, op, values);
    }

    fn visit_list_mut(&mut self, items: &mut [Expr]) {
        ExprWalkerMut::visit_list_mut(self, items);
    }

    fn visit_tuple_mut(&mut self, items: &mut [Expr]) {
        ExprWalkerMut::visit_tuple_mut(self, items);
    }

    fn visit_dict_mut(&mut self, items: &mut [DictEntry]) {
        ExprWalkerMut::visit_dict_mut(self, items);
    }

    fn visit_set_mut(&mut self, items: &mut [Expr]) {
        ExprWalkerMut::visit_set_mut(self, items);
    }

    fn visit_index_mut(&mut self, value: &mut Expr, index: &mut Expr) {
        ExprWalkerMut::visit_index_mut(self, value, index);
    }

    fn visit_slice_mut(
        &mut self,
        value: &mut Expr,
        start: &mut Option<Box<Expr>>,
        end: &mut Option<Box<Expr>>,
        step: &mut Option<Box<Expr>>,
    ) {
        ExprWalkerMut::visit_slice_mut(self, value, start, end, step);
    }

    fn visit_list_comp_mut(
        &mut self,
        elt: &mut Expr,
        target: &mut String,
        iter: &mut Expr,
        ifs: &mut [Expr],
        generators: &mut [CompClause],
    ) {
        ExprWalkerMut::visit_list_comp_mut(self, elt, target, iter, ifs, generators);
    }

    fn visit_set_comp_mut(
        &mut self,
        elt: &mut Expr,
        target: &mut String,
        iter: &mut Expr,
        ifs: &mut [Expr],
        generators: &mut [CompClause],
    ) {
        ExprWalkerMut::visit_set_comp_mut(self, elt, target, iter, ifs, generators);
    }

    fn visit_union_ctor_mut(&mut self, union: &mut String, variant: &mut String, inner: &mut Expr) {
        ExprWalkerMut::visit_union_ctor_mut(self, union, variant, inner);
    }

    fn visit_lambda_mut(
        &mut self,
        params: &mut [String],
        param_kinds: &mut [ParamKind],
        has_defaults: &mut [bool],
        defaults: &mut [Option<Expr>],
        body: &mut Expr,
    ) {
        ExprWalkerMut::visit_lambda_mut(self, params, param_kinds, has_defaults, defaults, body);
    }

    fn visit_if_expr_mut(&mut self, test: &mut Expr, body: &mut Expr, orelse: &mut Expr) {
        ExprWalkerMut::visit_if_expr_mut(self, test, body, orelse);
    }

    fn visit_block_mut(&mut self, stmts: &mut [Stmt]) {
        ExprWalkerMut::visit_block_mut(self, stmts);
    }
}

impl<T: StmtWalkerMut + ?Sized> StmtVisitorMut<()> for T {
    fn visit_let_mut(&mut self, name: &mut String, ann: &mut Option<TypeRef>, value: &mut Expr) {
        StmtWalkerMut::visit_let_mut(self, name, ann, value);
    }

    fn visit_assign_mut(&mut self, target: &mut AssignTarget, value: &mut Expr) {
        StmtWalkerMut::visit_assign_mut(self, target, value);
    }

    fn visit_delete_mut(&mut self, target: &mut AssignTarget) {
        StmtWalkerMut::visit_delete_mut(self, target);
    }

    fn visit_class_mut(&mut self, def: &mut ClassDef) {
        StmtWalkerMut::visit_class_mut(self, def);
    }

    fn visit_return_mut(&mut self, value: &mut Option<Expr>) {
        StmtWalkerMut::visit_return_mut(self, value);
    }

    fn visit_if_mut(&mut self, test: &mut Expr, body: &mut [Stmt], orelse: &mut [Stmt]) {
        StmtWalkerMut::visit_if_mut(self, test, body, orelse);
    }

    fn visit_while_mut(&mut self, test: &mut Expr, body: &mut [Stmt]) {
        StmtWalkerMut::visit_while_mut(self, test, body);
    }

    fn visit_for_mut(&mut self, target: &mut ForTarget, iter: &mut Expr, body: &mut [Stmt]) {
        StmtWalkerMut::visit_for_mut(self, target, iter, body);
    }

    fn visit_import_mut(&mut self, names: &mut [ImportBinding]) {
        StmtWalkerMut::visit_import_mut(self, names);
    }

    fn visit_import_from_mut(&mut self, module: &mut String, names: &mut [ImportFromBinding]) {
        StmtWalkerMut::visit_import_from_mut(self, module, names);
    }

    fn visit_global_mut(&mut self, names: &mut [String]) {
        StmtWalkerMut::visit_global_mut(self, names);
    }

    fn visit_nonlocal_mut(&mut self, names: &mut [String]) {
        StmtWalkerMut::visit_nonlocal_mut(self, names);
    }

    fn visit_break_mut(&mut self) {
        StmtWalkerMut::visit_break_mut(self);
    }

    fn visit_continue_mut(&mut self) {
        StmtWalkerMut::visit_continue_mut(self);
    }

    fn visit_expr_stmt_mut(&mut self, expr: &mut Expr) {
        StmtWalkerMut::visit_expr_stmt_mut(self, expr);
    }

    fn visit_assert_mut(&mut self, test: &mut Expr, msg: &mut Option<Expr>) {
        StmtWalkerMut::visit_assert_mut(self, test, msg);
    }

    fn visit_match_mut(&mut self, subject: &mut Expr, cases: &mut [MatchCase]) {
        StmtWalkerMut::visit_match_mut(self, subject, cases);
    }

    fn visit_try_mut(
        &mut self,
        body: &mut [Stmt],
        handlers: &mut [ExceptHandler],
        orelse: &mut [Stmt],
        finalbody: &mut [Stmt],
    ) {
        StmtWalkerMut::visit_try_mut(self, body, handlers, orelse, finalbody);
    }

    fn visit_raise_mut(&mut self, exc: &mut Option<Expr>, cause: &mut Option<Expr>) {
        StmtWalkerMut::visit_raise_mut(self, exc, cause);
    }
}

fn walk_expr_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, expr: &mut Expr) {
    expr.accept_mut(walker);
}

fn walk_expr_slice_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, exprs: &mut [Expr]) {
    for expr in exprs {
        walk_expr_mut(walker, expr);
    }
}

fn walk_stmt_expr_only_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, stmt: &mut Stmt) {
    match &mut stmt.kind {
        StmtKind::Let { value, .. } => walk_expr_mut(walker, value),
        StmtKind::Assign { target, value } => {
            walk_assign_target_mut(walker, target);
            walk_expr_mut(walker, value);
        }
        StmtKind::Delete { target } => {
            walk_assign_target_mut(walker, target);
        }
        StmtKind::Class { def } => {
            walk_class_def_expr_only_mut(walker, def);
        }
        StmtKind::Return { value } => {
            if let Some(value) = value {
                walk_expr_mut(walker, value);
            }
        }
        StmtKind::If { test, body, orelse } => {
            walk_expr_mut(walker, test);
            walk_stmt_slice_expr_only_mut(walker, body);
            walk_stmt_slice_expr_only_mut(walker, orelse);
        }
        StmtKind::While { test, body } => {
            walk_expr_mut(walker, test);
            walk_stmt_slice_expr_only_mut(walker, body);
        }
        StmtKind::For { iter, body, .. } => {
            walk_expr_mut(walker, iter);
            walk_stmt_slice_expr_only_mut(walker, body);
        }
        StmtKind::Import { .. }
        | StmtKind::ImportFrom { .. }
        | StmtKind::Global { .. }
        | StmtKind::Nonlocal { .. }
        | StmtKind::Break
        | StmtKind::Continue => {}
        StmtKind::Expr(expr) => walk_expr_mut(walker, expr),
        StmtKind::Assert { test, msg } => {
            walk_expr_mut(walker, test);
            if let Some(msg) = msg {
                walk_expr_mut(walker, msg);
            }
        }
        StmtKind::Match { subject, cases } => {
            walk_expr_mut(walker, subject);
            for case in cases {
                walk_stmt_slice_expr_only_mut(walker, &mut case.body);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            walk_stmt_slice_expr_only_mut(walker, body);
            for handler in handlers {
                walk_stmt_slice_expr_only_mut(walker, &mut handler.body);
            }
            walk_stmt_slice_expr_only_mut(walker, orelse);
            walk_stmt_slice_expr_only_mut(walker, finalbody);
        }
        StmtKind::Raise { exc, cause } => {
            if let Some(exc) = exc {
                walk_expr_mut(walker, exc);
            }
            if let Some(cause) = cause {
                walk_expr_mut(walker, cause);
            }
        }
    }
}

fn walk_stmt_slice_expr_only_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, stmts: &mut [Stmt]) {
    for stmt in stmts {
        walk_stmt_expr_only_mut(walker, stmt);
    }
}

fn walk_stmt_mut<W: StmtWalkerMut + ?Sized>(walker: &mut W, stmt: &mut Stmt) {
    stmt.accept_mut(walker);
}

fn walk_stmt_slice_mut<W: StmtWalkerMut + ?Sized>(walker: &mut W, stmts: &mut [Stmt]) {
    for stmt in stmts {
        walk_stmt_mut(walker, stmt);
    }
}

fn walk_comp_clause_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, clause: &mut CompClause) {
    walk_expr_mut(walker, clause.iter.as_mut());
    walk_expr_slice_mut(walker, &mut clause.ifs);
}

fn walk_comp_clause_slice_mut<W: ExprWalkerMut + ?Sized>(
    walker: &mut W,
    clauses: &mut [CompClause],
) {
    for clause in clauses {
        walk_comp_clause_mut(walker, clause);
    }
}

fn walk_function_expr_only_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, func: &mut Function) {
    for param in &mut func.params {
        if let Some(default) = &mut param.default {
            walk_expr_mut(walker, default);
        }
    }
    walk_stmt_slice_expr_only_mut(walker, &mut func.body);
}

fn walk_class_def_expr_only_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, def: &mut ClassDef) {
    for attr in &mut def.class_attrs {
        walk_expr_mut(walker, &mut attr.value);
    }
    for method in &mut def.methods {
        walk_function_expr_only_mut(walker, method);
    }
}

fn walk_class_def_mut<W: StmtWalkerMut + ?Sized>(walker: &mut W, def: &mut ClassDef) {
    for attr in &mut def.class_attrs {
        walk_expr_mut(walker, &mut attr.value);
    }
    for method in &mut def.methods {
        for param in &mut method.params {
            if let Some(default) = &mut param.default {
                walk_expr_mut(walker, default);
            }
        }
        walk_stmt_slice_mut(walker, &mut method.body);
    }
}

fn walk_assign_target_mut<W: ExprWalkerMut + ?Sized>(walker: &mut W, target: &mut AssignTarget) {
    match target {
        AssignTarget::Name(_) => {}
        AssignTarget::Attr { value, .. } => walk_expr_mut(walker, value),
        AssignTarget::Index { value, index } => {
            walk_expr_mut(walker, value);
            walk_expr_mut(walker, index);
        }
        AssignTarget::Tuple(items) | AssignTarget::List(items) => {
            for item in items {
                walk_assign_target_mut(walker, item);
            }
        }
        AssignTarget::Starred(inner) => walk_assign_target_mut(walker, inner.as_mut()),
    }
}

fn walk_for_target_mut<W: ExprWalkerMut + ?Sized>(_walker: &mut W, _target: &mut ForTarget) {
    // Current ForTarget variants contain only names.
}

fn walk_match_case_mut<W: StmtWalkerMut + ?Sized>(walker: &mut W, case: &mut MatchCase) {
    walk_stmt_slice_mut(walker, &mut case.body);
}

fn walk_match_case_slice_mut<W: StmtWalkerMut + ?Sized>(walker: &mut W, cases: &mut [MatchCase]) {
    for case in cases {
        walk_match_case_mut(walker, case);
    }
}

fn walk_except_handler_mut<W: StmtWalkerMut + ?Sized>(walker: &mut W, handler: &mut ExceptHandler) {
    walk_stmt_slice_mut(walker, &mut handler.body);
}

fn walk_except_handler_slice_mut<W: StmtWalkerMut + ?Sized>(
    walker: &mut W,
    handlers: &mut [ExceptHandler],
) {
    for handler in handlers {
        walk_except_handler_mut(walker, handler);
    }
}

/// Compile-time verification that all HIR variants are covered by the visitor.
///
/// These functions are never called at runtime, but they MUST compile. If a new
/// variant is added to `ExprKind` or `StmtKind` without updating the visitor
/// macro and walker traits, the exhaustive match here will fail to compile.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verify all ExprKind variants are accounted for.
    /// If you add a new variant to ExprKind, you MUST also add it to
    /// `define_expr_visitors!` and provide a default in `ExprWalkerMut`.
    #[allow(dead_code, unreachable_code)]
    fn assert_all_expr_variants_covered(kind: &ExprKind) {
        match kind {
            ExprKind::Literal(_) => {}
            ExprKind::Name(_) => {}
            ExprKind::Yield { .. } => {}
            ExprKind::Call { .. } => {}
            ExprKind::Starred { .. } => {}
            ExprKind::Attr { .. } => {}
            ExprKind::Binary { .. } => {}
            ExprKind::Unary { .. } => {}
            ExprKind::Compare { .. } => {}
            ExprKind::CompareChain { .. } => {}
            ExprKind::BoolOp { .. } => {}
            ExprKind::List(_) => {}
            ExprKind::Tuple(_) => {}
            ExprKind::Dict(_) => {}
            ExprKind::Set(_) => {}
            ExprKind::Index { .. } => {}
            ExprKind::Slice { .. } => {}
            ExprKind::ListComp { .. } => {}
            ExprKind::SetComp { .. } => {}
            ExprKind::UnionCtor { .. } => {}
            ExprKind::Lambda { .. } => {}
            ExprKind::IfExpr { .. } => {}
            ExprKind::Block { .. } => {}
        }
    }

    /// Verify all StmtKind variants are accounted for.
    /// If you add a new variant to StmtKind, you MUST also add it to
    /// `define_stmt_visitors!` and provide a default in `StmtWalkerMut`.
    #[allow(dead_code, unreachable_code)]
    fn assert_all_stmt_variants_covered(kind: &StmtKind) {
        match kind {
            StmtKind::Let { .. } => {}
            StmtKind::Assign { .. } => {}
            StmtKind::Delete { .. } => {}
            StmtKind::Class { .. } => {}
            StmtKind::Return { .. } => {}
            StmtKind::If { .. } => {}
            StmtKind::While { .. } => {}
            StmtKind::For { .. } => {}
            StmtKind::Import { .. } => {}
            StmtKind::ImportFrom { .. } => {}
            StmtKind::Global { .. } => {}
            StmtKind::Nonlocal { .. } => {}
            StmtKind::Break => {}
            StmtKind::Continue => {}
            StmtKind::Expr(_) => {}
            StmtKind::Assert { .. } => {}
            StmtKind::Match { .. } => {}
            StmtKind::Try { .. } => {}
            StmtKind::Raise { .. } => {}
        }
    }
}
