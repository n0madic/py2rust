use crate::diagnostic::CompileError;
use crate::hir::*;
use crate::span::Span;
use crate::types::{Type, TypeRef};
use rustpython_parser::ast;
use rustpython_parser::ast::Ranged;

pub struct Lowerer<'a> {
    source: &'a str,
    filename: &'a str,
}

impl<'a> Lowerer<'a> {
    pub fn new(source: &'a str, filename: &'a str) -> Self {
        Self { source, filename }
    }

    pub fn lower(&self, suite: &ast::Suite) -> Result<Program, CompileError> {
        let mut items = Vec::new();
        for stmt in suite {
            match stmt {
                ast::Stmt::FunctionDef(def) => {
                    if def.decorator_list.is_empty() {
                        items.push(Item::Function(self.lower_function(def)?));
                    } else {
                        let mut decorated = self.lower_decorated_function(def)?;
                        items.append(&mut decorated);
                    }
                }
                ast::Stmt::ClassDef(def) => {
                    items.push(Item::Class(self.lower_class(def)?));
                }
                ast::Stmt::Assign(def) => {
                    if let Some(union_item) = self.lower_union_alias(def)? {
                        items.push(Item::Union(union_item));
                    } else {
                        items.push(Item::Stmt(Box::new(self.lower_stmt(stmt)?)));
                    }
                }
                _ => items.push(Item::Stmt(Box::new(self.lower_stmt(stmt)?))),
            }
        }
        Ok(Program { items })
    }

    fn lower_decorated_function(
        &self,
        func: &ast::StmtFunctionDef,
    ) -> Result<Vec<Item>, CompileError> {
        if func.decorator_list.len() != 1 {
            return Err(self.error(func.range(), "Only a single decorator is supported"));
        }
        if !func.type_params.is_empty() {
            return Err(self.error(func.range(), "Type parameters are not supported"));
        }
        let decorator = match &func.decorator_list[0] {
            ast::Expr::Name(name) => name.id.to_string(),
            _ => return Err(self.error(func.range(), "Only simple name decorators are supported")),
        };
        let mut impl_func = self.lower_function(func)?;
        let orig_name = impl_func.name.clone();
        let impl_name = format!("{orig_name}_impl");
        impl_func.name = impl_name.clone();

        let tmp_name = format!("_decorated_{orig_name}");
        let call_decorator = Expr {
            kind: ExprKind::Call {
                func: Box::new(Expr {
                    kind: ExprKind::Name(decorator),
                    span: Span::from(func.range()),
                    ty: None,
                }),
                args: vec![Expr {
                    kind: ExprKind::Name(impl_name),
                    span: Span::from(func.range()),
                    ty: None,
                }],
            },
            span: Span::from(func.range()),
            ty: None,
        };
        let let_stmt = Stmt {
            kind: StmtKind::Let {
                name: tmp_name.clone(),
                ann: Some(TypeRef::Unknown),
                value: call_decorator,
            },
            span: Span::from(func.range()),
        };
        let mut args = Vec::new();
        for param in &impl_func.params {
            args.push(Expr {
                kind: ExprKind::Name(param.name.clone()),
                span: Span::from(func.range()),
                ty: None,
            });
        }
        let call_wrapped = Expr {
            kind: ExprKind::Call {
                func: Box::new(Expr {
                    kind: ExprKind::Name(tmp_name),
                    span: Span::from(func.range()),
                    ty: None,
                }),
                args,
            },
            span: Span::from(func.range()),
            ty: None,
        };
        let return_stmt = Stmt {
            kind: StmtKind::Return {
                value: Some(call_wrapped),
            },
            span: Span::from(func.range()),
        };

        let wrapper = Function {
            name: orig_name,
            params: impl_func.params.clone(),
            ret: impl_func.ret.clone(),
            body: vec![let_stmt, return_stmt],
            span: Span::from(func.range()),
        };
        Ok(vec![Item::Function(impl_func), Item::Function(wrapper)])
    }

    fn lower_union_alias(&self, stmt: &ast::StmtAssign) -> Result<Option<UnionDef>, CompileError> {
        if stmt.targets.len() != 1 {
            return Ok(None);
        }
        let target = &stmt.targets[0];
        let target_name = match target {
            ast::Expr::Name(name) => name.id.to_string(),
            _ => return Ok(None),
        };
        let mut variants = Vec::new();
        if Self::collect_union_variants(&stmt.value, &mut variants) {
            if variants.is_empty() {
                return Ok(None);
            }
            return Ok(Some(UnionDef {
                name: target_name,
                variants,
                span: Span::from(stmt.range()),
            }));
        }
        Ok(None)
    }

    fn collect_union_variants(expr: &ast::Expr, out: &mut Vec<String>) -> bool {
        match expr {
            ast::Expr::BinOp(bin) => {
                if !matches!(bin.op, ast::Operator::BitOr) {
                    return false;
                }
                let left_ok = Self::collect_union_variants(&bin.left, out);
                let right_ok = Self::collect_union_variants(&bin.right, out);
                left_ok && right_ok
            }
            ast::Expr::Name(name) => {
                out.push(name.id.to_string());
                true
            }
            _ => false,
        }
    }

    fn lower_function(&self, func: &ast::StmtFunctionDef) -> Result<Function, CompileError> {
        self.lower_function_with_self(func, None)
    }

    fn lower_method(
        &self,
        func: &ast::StmtFunctionDef,
        class_name: &str,
    ) -> Result<Function, CompileError> {
        self.lower_function_with_self(func, Some(class_name))
    }

    fn lower_function_with_self(
        &self,
        func: &ast::StmtFunctionDef,
        self_type: Option<&str>,
    ) -> Result<Function, CompileError> {
        // Ignore decorators for now (no-op).
        if !func.type_params.is_empty() {
            return Err(self.error(func.range(), "Type parameters are not supported"));
        }
        let mut params = Vec::new();
        for (idx, arg) in func.args.args.iter().enumerate() {
            let def = &arg.def;
            if arg.default.is_some() {
                return Err(self.error(def.range, "Default arguments are not supported"));
            }
            let name_str = def.arg.to_string();
            if idx == 0 && name_str == "self" && self_type.is_some() {
                let ann = if let Some(ann_expr) = &def.annotation {
                    self.lower_type_ref(ann_expr)?
                } else {
                    TypeRef::Name(self_type.unwrap().to_string())
                };
                params.push(Param {
                    name: name_str,
                    ann,
                    span: Span::from(def.range),
                });
                continue;
            }
            let ann = match &def.annotation {
                Some(expr) => self.lower_type_ref(expr)?,
                None => TypeRef::Unknown,
            };
            params.push(Param {
                name: name_str,
                ann,
                span: Span::from(def.range),
            });
        }
        if !func.args.posonlyargs.is_empty() || !func.args.kwonlyargs.is_empty() {
            return Err(self.error(
                func.range(),
                "positional-only and keyword-only args are not supported",
            ));
        }
        if func.args.vararg.is_some() || func.args.kwarg.is_some() {
            return Err(self.error(func.range(), "*args/**kwargs are not supported"));
        }
        let ret = if let Some(ret_expr) = &func.returns {
            self.lower_type_ref(ret_expr)?
        } else {
            TypeRef::Unknown
        };
        let mut body_stmts = Vec::new();
        for stmt in &func.body {
            body_stmts.push(self.lower_stmt(stmt)?);
        }
        Ok(Function {
            name: func.name.to_string(),
            params,
            ret,
            body: body_stmts,
            span: Span::from(func.range()),
        })
    }

    fn lower_class(&self, class: &ast::StmtClassDef) -> Result<ClassDef, CompileError> {
        if !class.bases.is_empty() || !class.keywords.is_empty() {
            return Err(self.error(class.range(), "Class inheritance is not supported"));
        }
        if !class.decorator_list.is_empty() {
            return Err(self.error(class.range(), "Class decorators are not supported"));
        }
        if !class.type_params.is_empty() {
            return Err(self.error(class.range(), "Class type parameters are not supported"));
        }
        let mut fields: Vec<FieldDef> = Vec::new();
        let mut methods = Vec::new();
        for item in &class.body {
            match item {
                ast::Stmt::FunctionDef(def) => {
                    let func = self.lower_method(def, class.name.as_ref())?;
                    if func.name == "__init__" {
                        let init_fields = self.extract_init_fields_from_ast(&def.body)?;
                        for field in init_fields {
                            if !fields.iter().any(|f| f.name == field.name) {
                                fields.push(field);
                            }
                        }
                    }
                    methods.push(func);
                }
                ast::Stmt::Pass(_) => {}
                _ => {
                    return Err(self.error(
                        item.range(),
                        "Only method definitions are allowed inside classes",
                    ))
                }
            }
        }
        Ok(ClassDef {
            name: class.name.to_string(),
            fields,
            methods,
            span: Span::from(class.range()),
        })
    }

    fn extract_init_fields_from_ast(
        &self,
        body: &[ast::Stmt],
    ) -> Result<Vec<FieldDef>, CompileError> {
        let mut fields = Vec::new();
        for stmt in body {
            match stmt {
                ast::Stmt::AnnAssign(def) => {
                    if let ast::Expr::Attribute(attr) = &*def.target {
                        if matches!(&*attr.value, ast::Expr::Name(name) if name.id.as_str() == "self") {
                            let ty = self.lower_type_ref(&def.annotation)?;
                            fields.push(FieldDef {
                                name: attr.attr.to_string(),
                                ty,
                                span: Span::from(def.range()),
                            });
                        }
                    }
                }
                ast::Stmt::Assign(def) => {
                    if def.targets.iter().any(|t| matches!(t, ast::Expr::Attribute(attr) if matches!(&*attr.value, ast::Expr::Name(name) if name.id.as_str() == "self"))) {
                        return Err(self.error(
                            def.range(),
                            "Field assignments in __init__ must use type annotations",
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(fields)
    }

    fn lower_stmt(&self, stmt: &ast::Stmt) -> Result<Stmt, CompileError> {
        let span = Span::from(stmt.range());
        let kind = match stmt {
            ast::Stmt::FunctionDef(def) => {
                if !def.decorator_list.is_empty() {
                    return Err(
                        self.error(def.range(), "Decorators are not supported inside functions")
                    );
                }
                if !def.type_params.is_empty() {
                    return Err(self.error(def.range(), "Type parameters are not supported"));
                }
                if !def.args.posonlyargs.is_empty() || !def.args.kwonlyargs.is_empty() {
                    return Err(self.error(
                        def.range(),
                        "positional-only and keyword-only args are not supported",
                    ));
                }
                if def.args.vararg.is_some() || def.args.kwarg.is_some() {
                    return Err(self.error(def.range(), "*args/**kwargs are not supported"));
                }
                let mut params = Vec::new();
                let mut param_types = Vec::new();
                for arg in &def.args.args {
                    if arg.default.is_some() {
                        return Err(self.error(def.range(), "Default arguments are not supported"));
                    }
                    params.push(arg.def.arg.to_string());
                    let ann = match &arg.def.annotation {
                        Some(expr) => self.lower_type_ref(expr)?,
                        None => TypeRef::Unknown,
                    };
                    param_types.push(ann);
                }
                let ret = if let Some(ret_expr) = &def.returns {
                    self.lower_type_ref(ret_expr)?
                } else {
                    TypeRef::Unknown
                };
                let mut body_stmts = Vec::new();
                for stmt in &def.body {
                    body_stmts.push(self.lower_stmt(stmt)?);
                }
                let block = Expr {
                    kind: ExprKind::Block { stmts: body_stmts },
                    span: Span::from(def.range()),
                    ty: None,
                };
                let value = Expr {
                    kind: ExprKind::Lambda {
                        params,
                        body: Box::new(block),
                    },
                    span: Span::from(def.range()),
                    ty: None,
                };
                StmtKind::Let {
                    name: def.name.to_string(),
                    ann: Some(TypeRef::Lambda {
                        params: param_types,
                        ret: Box::new(ret),
                    }),
                    value,
                }
            }
            ast::Stmt::AnnAssign(def) => {
                let ann = self.lower_type_ref(&def.annotation)?;
                let value = def.value.as_ref().ok_or_else(|| {
                    self.error(def.range(), "Annotated assignment must have a value")
                })?;
                let value = self.lower_expr(value)?;
                match &*def.target {
                    ast::Expr::Name(name) => StmtKind::Let {
                        name: name.id.to_string(),
                        ann: Some(ann),
                        value,
                    },
                    ast::Expr::Attribute(attr) => {
                        let value_expr = self.lower_expr(&attr.value)?;
                        StmtKind::Assign {
                            target: AssignTarget::Attr {
                                value: value_expr,
                                attr: attr.attr.to_string(),
                            },
                            value,
                        }
                    }
                    _ => {
                        return Err(self.error(
                            def.range(),
                            "Only simple names or attributes can be annotated",
                        ))
                    }
                }
            }
            ast::Stmt::Assign(def) => {
                if def.targets.len() != 1 {
                    return Err(
                        self.error(def.range(), "Only single-target assignments are supported")
                    );
                }
                let target = self.lower_assign_target(&def.targets[0])?;
                let value = self.lower_expr(&def.value)?;
                StmtKind::Assign { target, value }
            }
            ast::Stmt::Return(def) => {
                let value = match &def.value {
                    Some(expr) => Some(self.lower_expr(expr)?),
                    None => None,
                };
                StmtKind::Return { value }
            }
            ast::Stmt::Global(def) => StmtKind::Global {
                names: def.names.iter().map(|n| n.to_string()).collect(),
            },
            ast::Stmt::If(def) => {
                let test = self.lower_expr(&def.test)?;
                let mut body_stmts = Vec::new();
                for stmt in &def.body {
                    body_stmts.push(self.lower_stmt(stmt)?);
                }
                let mut else_stmts = Vec::new();
                for stmt in &def.orelse {
                    else_stmts.push(self.lower_stmt(stmt)?);
                }
                StmtKind::If {
                    test,
                    body: body_stmts,
                    orelse: else_stmts,
                }
            }
            ast::Stmt::While(def) => {
                let test = self.lower_expr(&def.test)?;
                let mut body_stmts = Vec::new();
                for stmt in &def.body {
                    body_stmts.push(self.lower_stmt(stmt)?);
                }
                StmtKind::While {
                    test,
                    body: body_stmts,
                }
            }
            ast::Stmt::For(def) => {
                let iter = self.lower_expr(&def.iter)?;
                let mut body_stmts = Vec::new();
                let target_name = match &*def.target {
                    ast::Expr::Name(name) => name.id.to_string(),
                    ast::Expr::Tuple(tuple) => {
                        let tmp_name = format!("_iter{}_tmp", usize::from(def.range().start()));
                        for (idx, elt) in tuple.elts.iter().enumerate() {
                            if let ast::Expr::Name(name) = elt {
                                let idx_expr = Expr {
                                    kind: ExprKind::Literal(Literal::Int(idx as i64)),
                                    span: Span::from(elt.range()),
                                    ty: None,
                                };
                                let value = Expr {
                                    kind: ExprKind::Index {
                                        value: Box::new(Expr {
                                            kind: ExprKind::Name(tmp_name.clone()),
                                            span: Span::from(elt.range()),
                                            ty: None,
                                        }),
                                        index: Box::new(idx_expr),
                                    },
                                    span: Span::from(elt.range()),
                                    ty: None,
                                };
                                body_stmts.push(Stmt {
                                    kind: StmtKind::Let {
                                        name: name.id.to_string(),
                                        ann: None,
                                        value,
                                    },
                                    span: Span::from(elt.range()),
                                });
                            } else {
                                return Err(self.error(
                                    elt.range(),
                                    "Only simple tuple targets are supported",
                                ));
                            }
                        }
                        tmp_name
                    }
                    _ => {
                        return Err(self.error(
                            def.range(),
                            "Only simple names or tuples are supported in for targets",
                        ))
                    }
                };
                for stmt in &def.body {
                    body_stmts.push(self.lower_stmt(stmt)?);
                }
                StmtKind::For {
                    target: target_name,
                    iter,
                    body: body_stmts,
                }
            }
            ast::Stmt::Break(_) => StmtKind::Break,
            ast::Stmt::Continue(_) => StmtKind::Continue,
            ast::Stmt::Assert(def) => {
                let test = self.lower_expr(&def.test)?;
                let msg = match &def.msg {
                    Some(expr) => Some(self.lower_expr(expr)?),
                    None => None,
                };
                StmtKind::Assert { test, msg }
            }
            ast::Stmt::Expr(def) => {
                let expr = self.lower_expr(&def.value)?;
                StmtKind::Expr(expr)
            }
            ast::Stmt::AugAssign(def) => {
                let target = self.lower_assign_target(&def.target)?;
                let target_expr = self.assign_target_expr(&def.target)?;
                let op = match def.op {
                    ast::Operator::Add => BinOp::Add,
                    ast::Operator::Sub => BinOp::Sub,
                    ast::Operator::Mult => BinOp::Mul,
                    ast::Operator::Div => BinOp::Div,
                    ast::Operator::Pow => BinOp::Pow,
                    ast::Operator::Mod => BinOp::Mod,
                    ast::Operator::FloorDiv => BinOp::FloorDiv,
                    _ => return Err(self.error(def.range(), "Unsupported augmented operator")),
                };
                let value = Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(target_expr),
                        right: Box::new(self.lower_expr(&def.value)?),
                    },
                    span,
                    ty: None,
                };
                StmtKind::Assign { target, value }
            }
            ast::Stmt::Match(def) => {
                let subject = self.lower_expr(&def.subject)?;
                let mut lowered_cases = Vec::new();
                for case in &def.cases {
                    lowered_cases.push(self.lower_match_case(case)?);
                }
                StmtKind::Match {
                    subject,
                    cases: lowered_cases,
                }
            }
            ast::Stmt::Pass(_) => StmtKind::Expr(Expr {
                kind: ExprKind::Literal(Literal::None),
                span,
                ty: None,
            }),
            _ => return Err(self.error(stmt.range(), "Unsupported statement")),
        };
        Ok(Stmt { kind, span })
    }

    fn lower_match_case(&self, case: &ast::MatchCase) -> Result<MatchCase, CompileError> {
        let span = Span::from(case.pattern.range());
        let (variant, bindings) = self.lower_pattern(&case.pattern)?;
        if case.guard.is_some() {
            return Err(self.error(case.pattern.range(), "Match guards are not supported"));
        }
        let mut body = Vec::new();
        for stmt in &case.body {
            body.push(self.lower_stmt(stmt)?);
        }
        Ok(MatchCase {
            variant,
            bindings,
            body,
            span,
        })
    }

    fn lower_pattern(&self, pattern: &ast::Pattern) -> Result<(String, Vec<String>), CompileError> {
        match pattern {
            ast::Pattern::MatchClass(cls_pat) => {
                if !cls_pat.kwd_attrs.is_empty() || !cls_pat.kwd_patterns.is_empty() {
                    return Err(self.error(pattern.range(), "Keyword patterns are not supported"));
                }
                let variant = match &*cls_pat.cls {
                    ast::Expr::Name(name) => name.id.to_string(),
                    _ => {
                        return Err(self.error(
                            pattern.range(),
                            "Only class constructor patterns are supported",
                        ))
                    }
                };
                let mut bindings = Vec::new();
                for pat in &cls_pat.patterns {
                    match pat {
                        ast::Pattern::MatchAs(as_pat) => {
                            if as_pat.pattern.is_some() {
                                return Err(
                                    self.error(pat.range(), "Nested patterns are not supported")
                                );
                            }
                            let name = as_pat.name.as_ref().ok_or_else(|| {
                                self.error(pat.range(), "Unnamed bindings are not supported")
                            })?;
                            bindings.push(name.to_string());
                        }
                        _ => {
                            return Err(
                                self.error(pat.range(), "Only simple bindings are supported")
                            )
                        }
                    }
                }
                Ok((variant, bindings))
            }
            _ => Err(self.error(pattern.range(), "Unsupported match pattern")),
        }
    }

    fn lower_assign_target(&self, expr: &ast::Expr) -> Result<AssignTarget, CompileError> {
        match expr {
            ast::Expr::Name(name) => Ok(AssignTarget::Name(name.id.to_string())),
            ast::Expr::Attribute(attr) => {
                let value_expr = self.lower_expr(&attr.value)?;
                Ok(AssignTarget::Attr {
                    value: value_expr,
                    attr: attr.attr.to_string(),
                })
            }
            ast::Expr::Subscript(sub) => {
                let value_expr = self.lower_expr(&sub.value)?;
                let index_expr = self.lower_expr(&sub.slice)?;
                Ok(AssignTarget::Index {
                    value: value_expr,
                    index: index_expr,
                })
            }
            _ => Err(self.error(expr.range(), "Unsupported assignment target")),
        }
    }

    fn lower_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
        let span = Span::from(expr.range());
        let kind = match expr {
            ast::Expr::Name(name) => ExprKind::Name(name.id.to_string()),
            ast::Expr::Constant(cons) => match &cons.value {
                ast::Constant::Int(value) => {
                    let parsed = value.to_string().parse::<i64>().map_err(|_| {
                        self.error(expr.range(), "Integer literal out of range for i64")
                    })?;
                    ExprKind::Literal(Literal::Int(parsed))
                }
                ast::Constant::Float(value) => ExprKind::Literal(Literal::Float(*value)),
                ast::Constant::Bool(value) => ExprKind::Literal(Literal::Bool(*value)),
                ast::Constant::Str(value) => ExprKind::Literal(Literal::Str(value.to_string())),
                ast::Constant::None => ExprKind::Literal(Literal::None),
                _ => return Err(self.error(expr.range(), "Unsupported literal")),
            },
            ast::Expr::Call(call) => {
                if !call.keywords.is_empty() {
                    return Err(self.error(expr.range(), "Keyword arguments are not supported"));
                }
                let func = Box::new(self.lower_expr(&call.func)?);
                let mut lowered_args = Vec::new();
                for arg in &call.args {
                    lowered_args.push(self.lower_expr(arg)?);
                }
                ExprKind::Call {
                    func,
                    args: lowered_args,
                }
            }
            ast::Expr::Attribute(attr) => ExprKind::Attr {
                value: Box::new(self.lower_expr(&attr.value)?),
                attr: attr.attr.to_string(),
            },
            ast::Expr::BinOp(bin) => {
                let op = match bin.op {
                    ast::Operator::Add => BinOp::Add,
                    ast::Operator::Sub => BinOp::Sub,
                    ast::Operator::Mult => BinOp::Mul,
                    ast::Operator::Div => BinOp::Div,
                    ast::Operator::Pow => BinOp::Pow,
                    ast::Operator::FloorDiv => BinOp::FloorDiv,
                    ast::Operator::Mod => BinOp::Mod,
                    ast::Operator::BitOr => BinOp::BitOr,
                    ast::Operator::BitAnd => BinOp::BitAnd,
                    ast::Operator::BitXor => BinOp::BitXor,
                    _ => return Err(self.error(expr.range(), "Unsupported binary operator")),
                };
                ExprKind::Binary {
                    op,
                    left: Box::new(self.lower_expr(&bin.left)?),
                    right: Box::new(self.lower_expr(&bin.right)?),
                }
            }
            ast::Expr::UnaryOp(unary) => {
                let op = match unary.op {
                    ast::UnaryOp::USub => UnaryOp::Neg,
                    ast::UnaryOp::Not => UnaryOp::Not,
                    _ => return Err(self.error(expr.range(), "Unsupported unary operator")),
                };
                ExprKind::Unary {
                    op,
                    expr: Box::new(self.lower_expr(&unary.operand)?),
                }
            }
            ast::Expr::BoolOp(boolop) => {
                let op = match boolop.op {
                    ast::BoolOp::And => BoolOp::And,
                    ast::BoolOp::Or => BoolOp::Or,
                };
                let mut lowered = Vec::new();
                for v in &boolop.values {
                    lowered.push(self.lower_expr(v)?);
                }
                ExprKind::BoolOp {
                    op,
                    values: lowered,
                }
            }
            ast::Expr::Compare(comp) => {
                if comp.ops.len() != 1 || comp.comparators.len() != 1 {
                    return Err(self.error(expr.range(), "Only single comparisons are supported"));
                }
                let op = match comp.ops[0] {
                    ast::CmpOp::Eq => CmpOp::Eq,
                    ast::CmpOp::NotEq => CmpOp::NotEq,
                    ast::CmpOp::Lt => CmpOp::Lt,
                    ast::CmpOp::LtE => CmpOp::LtEq,
                    ast::CmpOp::Gt => CmpOp::Gt,
                    ast::CmpOp::GtE => CmpOp::GtEq,
                    ast::CmpOp::Is => CmpOp::Is,
                    ast::CmpOp::IsNot => CmpOp::IsNot,
                    _ => return Err(self.error(expr.range(), "Unsupported comparison")),
                };
                ExprKind::Compare {
                    op,
                    left: Box::new(self.lower_expr(&comp.left)?),
                    right: Box::new(self.lower_expr(&comp.comparators[0])?),
                }
            }
            ast::Expr::List(list) => {
                let mut items = Vec::new();
                for elt in &list.elts {
                    items.push(self.lower_expr(elt)?);
                }
                ExprKind::List(items)
            }
            ast::Expr::Tuple(tuple) => {
                let mut items = Vec::new();
                for elt in &tuple.elts {
                    items.push(self.lower_expr(elt)?);
                }
                ExprKind::Tuple(items)
            }
            ast::Expr::Set(set_expr) => {
                let mut items = Vec::new();
                for elt in &set_expr.elts {
                    items.push(self.lower_expr(elt)?);
                }
                ExprKind::Set(items)
            }
            ast::Expr::Dict(dict) => {
                let mut items = Vec::new();
                for (k, v) in dict.keys.iter().zip(dict.values.iter()) {
                    let key = k.as_ref().ok_or_else(|| {
                        self.error(expr.range(), "Dict unpacking is not supported")
                    })?;
                    items.push((self.lower_expr(key)?, self.lower_expr(v)?));
                }
                ExprKind::Dict(items)
            }
            ast::Expr::Subscript(sub) => match &*sub.slice {
                ast::Expr::Slice(slice) => {
                    if slice.step.is_some() {
                        return Err(self.error(sub.range(), "Slice steps are not supported"));
                    }
                    let start = match &slice.lower {
                        Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                        None => None,
                    };
                    let end = match &slice.upper {
                        Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                        None => None,
                    };
                    ExprKind::Slice {
                        value: Box::new(self.lower_expr(&sub.value)?),
                        start,
                        end,
                    }
                }
                _ => ExprKind::Index {
                    value: Box::new(self.lower_expr(&sub.value)?),
                    index: Box::new(self.lower_expr(&sub.slice)?),
                },
            },
            ast::Expr::ListComp(listcomp) => {
                if listcomp.generators.len() != 1 {
                    return Err(self.error(
                        expr.range(),
                        "Only single-generator comprehensions are supported",
                    ));
                }
                let gen = &listcomp.generators[0];
                if gen.is_async {
                    return Err(self.error(expr.range(), "Async comprehensions are not supported"));
                }
                let target = match &gen.target {
                    ast::Expr::Name(name) => name.id.to_string(),
                    _ => {
                        return Err(self.error(
                            gen.target.range(),
                            "Only simple targets are supported in comprehensions",
                        ))
                    }
                };
                let iter = Box::new(self.lower_expr(&gen.iter)?);
                let mut ifs = Vec::new();
                for cond in &gen.ifs {
                    ifs.push(self.lower_expr(cond)?);
                }
                ExprKind::ListComp {
                    elt: Box::new(self.lower_expr(&listcomp.elt)?),
                    target,
                    iter,
                    ifs,
                }
            }
            ast::Expr::IfExp(ifexp) => ExprKind::IfExpr {
                test: Box::new(self.lower_expr(&ifexp.test)?),
                body: Box::new(self.lower_expr(&ifexp.body)?),
                orelse: Box::new(self.lower_expr(&ifexp.orelse)?),
            },
            ast::Expr::JoinedStr(joined) => {
                let mut fmt = String::new();
                let mut args = Vec::new();
                for value in &joined.values {
                    match value {
                        ast::Expr::Constant(cons) => match &cons.value {
                            ast::Constant::Str(s) => {
                                fmt.push_str(&self.escape_format_literal(s));
                            }
                            _ => {
                                return Err(
                                    self.error(value.range(), "Unsupported f-string literal")
                                )
                            }
                        },
                        ast::Expr::FormattedValue(fv) => {
                            if !matches!(
                                fv.conversion,
                                ast::ConversionFlag::None | ast::ConversionFlag::Str
                            ) {
                                return Err(self.error(
                                    value.range(),
                                    "f-string conversions are not supported",
                                ));
                            }
                            let spec = if let Some(spec_expr) = &fv.format_spec {
                                let raw = self.format_spec_literal(spec_expr)?;
                                let mapped = self.map_format_spec(&raw, spec_expr.range())?;
                                if mapped.is_empty() {
                                    None
                                } else {
                                    Some(mapped)
                                }
                            } else {
                                None
                            };
                            fmt.push('{');
                            if let Some(spec) = spec {
                                fmt.push(':');
                                fmt.push_str(&spec);
                            }
                            fmt.push('}');
                            args.push(self.lower_expr(&fv.value)?);
                        }
                        _ => return Err(self.error(value.range(), "Unsupported f-string element")),
                    }
                }
                let fmt_expr = Expr {
                    kind: ExprKind::Literal(Literal::Str(fmt)),
                    span: Span::from(expr.range()),
                    ty: Some(Type::Str),
                };
                let func = Expr {
                    kind: ExprKind::Attr {
                        value: Box::new(fmt_expr),
                        attr: "format".to_string(),
                    },
                    span: Span::from(expr.range()),
                    ty: None,
                };
                ExprKind::Call {
                    func: Box::new(func),
                    args,
                }
            }
            ast::Expr::Lambda(lam) => {
                if !lam.args.posonlyargs.is_empty() || !lam.args.kwonlyargs.is_empty() {
                    return Err(self.error(
                        expr.range(),
                        "Lambda with posonly/kwonly args not supported",
                    ));
                }
                if lam.args.vararg.is_some() || lam.args.kwarg.is_some() {
                    return Err(self.error(expr.range(), "Lambda *args/**kwargs not supported"));
                }
                let mut params = Vec::new();
                for arg in &lam.args.args {
                    if arg.default.is_some() {
                        return Err(self.error(expr.range(), "Lambda defaults not supported"));
                    }
                    params.push(arg.def.arg.to_string());
                }
                ExprKind::Lambda {
                    params,
                    body: Box::new(self.lower_expr(&lam.body)?),
                }
            }
            _ => return Err(self.error(expr.range(), "Unsupported expression")),
        };
        Ok(Expr {
            kind,
            span,
            ty: None,
        })
    }

    fn lower_type_ref(&self, expr: &ast::Expr) -> Result<TypeRef, CompileError> {
        match expr {
            ast::Expr::Name(name) => Ok(match name.id.as_ref() {
                "None" => TypeRef::None,
                _ => TypeRef::Name(name.id.to_string()),
            }),
            ast::Expr::Lambda(_) => Ok(TypeRef::Unknown),
            ast::Expr::Constant(cons) => match &cons.value {
                ast::Constant::None => Ok(TypeRef::None),
                ast::Constant::Str(s) => Ok(TypeRef::Name(s.to_string())),
                _ => Err(self.error(expr.range(), "Unsupported type annotation literal")),
            },
            ast::Expr::Subscript(sub) => {
                let base = match &*sub.value {
                    ast::Expr::Name(name) => name.id.as_ref(),
                    _ => {
                        return Err(self.error(sub.value.range(), "Unsupported type constructor"));
                    }
                };
                match base {
                    "list" => Ok(TypeRef::List(Box::new(self.lower_type_ref(&sub.slice)?))),
                    "dict" => {
                        let args = self.extract_type_args(&sub.slice)?;
                        if args.len() != 2 {
                            return Err(
                                self.error(sub.slice.range(), "dict expects two type arguments")
                            );
                        }
                        Ok(TypeRef::Dict(
                            Box::new(args[0].clone()),
                            Box::new(args[1].clone()),
                        ))
                    }
                    "tuple" => {
                        let args = self.extract_type_args(&sub.slice)?;
                        Ok(TypeRef::Tuple(args))
                    }
                    "set" => Ok(TypeRef::Set(Box::new(self.lower_type_ref(&sub.slice)?))),
                    "Optional" => Ok(TypeRef::Optional(Box::new(
                        self.lower_type_ref(&sub.slice)?,
                    ))),
                    "Union" => {
                        let args = self.extract_type_args(&sub.slice)?;
                        Ok(TypeRef::Union(args))
                    }
                    "Iterator" => Ok(TypeRef::Iterator(Box::new(
                        self.lower_type_ref(&sub.slice)?,
                    ))),
                    _ => Err(self.error(sub.value.range(), "Unsupported type constructor")),
                }
            }
            ast::Expr::BinOp(bin) => {
                if !matches!(bin.op, ast::Operator::BitOr) {
                    return Err(self.error(expr.range(), "Unsupported type expression"));
                }
                let mut parts = Vec::new();
                self.collect_union_type_refs(expr, &mut parts)?;
                Ok(TypeRef::Union(parts))
            }
            _ => Err(self.error(expr.range(), "Unsupported type annotation")),
        }
    }

    fn collect_union_type_refs(
        &self,
        expr: &ast::Expr,
        out: &mut Vec<TypeRef>,
    ) -> Result<(), CompileError> {
        match expr {
            ast::Expr::BinOp(bin) => {
                if !matches!(bin.op, ast::Operator::BitOr) {
                    return Err(self.error(expr.range(), "Unsupported union type"));
                }
                self.collect_union_type_refs(&bin.left, out)?;
                self.collect_union_type_refs(&bin.right, out)?;
            }
            _ => out.push(self.lower_type_ref(expr)?),
        }
        Ok(())
    }

    fn extract_type_args(&self, expr: &ast::Expr) -> Result<Vec<TypeRef>, CompileError> {
        match expr {
            ast::Expr::Tuple(tuple) => {
                let mut args = Vec::new();
                for elt in &tuple.elts {
                    args.push(self.lower_type_ref(elt)?);
                }
                Ok(args)
            }
            _ => Ok(vec![self.lower_type_ref(expr)?]),
        }
    }

    fn assign_target_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
        let span = Span::from(expr.range());
        let kind = match expr {
            ast::Expr::Name(name) => ExprKind::Name(name.id.to_string()),
            ast::Expr::Attribute(attr) => ExprKind::Attr {
                value: Box::new(self.lower_expr(&attr.value)?),
                attr: attr.attr.to_string(),
            },
            ast::Expr::Subscript(sub) => ExprKind::Index {
                value: Box::new(self.lower_expr(&sub.value)?),
                index: Box::new(self.lower_expr(&sub.slice)?),
            },
            _ => return Err(self.error(expr.range(), "Unsupported target in augmented assignment")),
        };
        Ok(Expr {
            kind,
            span,
            ty: None,
        })
    }

    fn escape_format_literal(&self, s: &str) -> String {
        let mut out = String::new();
        for ch in s.chars() {
            match ch {
                '{' => out.push_str("{{"),
                '}' => out.push_str("}}"),
                _ => out.push(ch),
            }
        }
        out
    }

    fn format_spec_literal(&self, expr: &ast::Expr) -> Result<String, CompileError> {
        match expr {
            ast::Expr::Constant(cons) => match &cons.value {
                ast::Constant::Str(s) => Ok(s.clone()),
                _ => Err(self.error(expr.range(), "f-string format spec must be a literal")),
            },
            ast::Expr::JoinedStr(joined) => {
                let mut out = String::new();
                for value in &joined.values {
                    match value {
                        ast::Expr::Constant(cons) => match &cons.value {
                            ast::Constant::Str(s) => out.push_str(s),
                            _ => {
                                return Err(self.error(
                                    value.range(),
                                    "f-string format spec must be a literal",
                                ))
                            }
                        },
                        _ => {
                            return Err(
                                self.error(value.range(), "f-string format spec must be a literal")
                            )
                        }
                    }
                }
                Ok(out)
            }
            _ => Err(self.error(expr.range(), "f-string format spec must be a literal")),
        }
    }

    fn map_format_spec(
        &self,
        spec: &str,
        range: rustpython_parser::text_size::TextRange,
    ) -> Result<String, CompileError> {
        if spec.is_empty() {
            return Ok(String::new());
        }
        if spec.contains('{') || spec.contains('}') {
            return Err(self.error(range, "f-string format spec may not contain braces"));
        }
        let last = spec.chars().last();
        let (body, ty) = if let Some(ch) = last {
            if matches!(ch, 'f' | 'd' | 'x' | 'X' | 'o' | 'b') {
                let cut = spec.len() - ch.len_utf8();
                (&spec[..cut], Some(ch))
            } else {
                (spec, None)
            }
        } else {
            ("", None)
        };
        let mut out = String::new();
        if !body.is_empty() {
            let mut parts = body.splitn(2, '.');
            let width = parts.next().unwrap_or("");
            if !width.is_empty() && !width.chars().all(|c| c.is_ascii_digit()) {
                return Err(self.error(range, "Unsupported f-string format specifier"));
            }
            out.push_str(width);
            if let Some(prec) = parts.next() {
                if !prec.chars().all(|c| c.is_ascii_digit()) {
                    return Err(self.error(range, "Unsupported f-string format specifier"));
                }
                out.push('.');
                out.push_str(prec);
            }
        }
        if let Some(ty) = ty {
            match ty {
                'f' => {
                    if out.is_empty() {
                        out.push_str(".6");
                    }
                }
                'd' => {}
                'x' | 'X' | 'o' | 'b' => out.push(ty),
                _ => {}
            }
        }
        Ok(out)
    }

    fn error(&self, range: rustpython_parser::text_size::TextRange, msg: &str) -> CompileError {
        CompileError::new(msg, Span::from(range), self.source, self.filename)
    }
}
