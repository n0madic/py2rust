use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_stmt(&self, stmt: &ast::Stmt) -> Result<Stmt, CompileError> {
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
            ast::Stmt::Try(try_stmt) => {
                let body_stmts = try_stmt
                    .body
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<_, _>>()?;

                let handlers = try_stmt
                    .handlers
                    .iter()
                    .map(|h| self.lower_except_handler(h))
                    .collect::<Result<_, _>>()?;

                let orelse_stmts = try_stmt
                    .orelse
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<_, _>>()?;

                let final_stmts = try_stmt
                    .finalbody
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<_, _>>()?;

                StmtKind::Try {
                    body: body_stmts,
                    handlers,
                    orelse: orelse_stmts,
                    finalbody: final_stmts,
                }
            }
            ast::Stmt::Raise(raise_stmt) => {
                let exc = raise_stmt
                    .exc
                    .as_ref()
                    .map(|e| self.lower_expr(e))
                    .transpose()?;
                let cause = raise_stmt
                    .cause
                    .as_ref()
                    .map(|e| self.lower_expr(e))
                    .transpose()?;
                StmtKind::Raise { exc, cause }
            }
            _ => return Err(self.error(stmt.range(), "Unsupported statement")),
        };
        Ok(Stmt { kind, span })
    }

    pub(super) fn lower_match_case(
        &self,
        case: &ast::MatchCase,
    ) -> Result<MatchCase, CompileError> {
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

    pub(super) fn lower_except_handler(
        &self,
        handler: &ast::ExceptHandler,
    ) -> Result<ExceptHandler, CompileError> {
        match handler {
            ast::ExceptHandler::ExceptHandler(eh) => {
                let exc_type = match eh.type_.as_deref() {
                    Some(ast::Expr::Name(name)) => Some(name.id.to_string()),
                    Some(_) => {
                        return Err(
                            self.error(handler.range(), "Exception type must be a simple name")
                        )
                    }
                    None => None,
                };

                let name = eh.name.as_ref().map(|id| id.to_string());
                let body = eh
                    .body
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<_, _>>()?;

                Ok(ExceptHandler {
                    exc_type,
                    name,
                    body,
                    span: Span::from(handler.range()),
                })
            }
        }
    }

    pub(super) fn lower_pattern(
        &self,
        pattern: &ast::Pattern,
    ) -> Result<(String, Vec<String>), CompileError> {
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

    pub(super) fn lower_assign_target(
        &self,
        expr: &ast::Expr,
    ) -> Result<AssignTarget, CompileError> {
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
}
