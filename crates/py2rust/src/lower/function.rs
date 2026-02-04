use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_decorated_function(
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

    pub(super) fn lower_union_alias(
        &self,
        stmt: &ast::StmtAssign,
    ) -> Result<Option<UnionDef>, CompileError> {
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

    pub(super) fn collect_union_variants(expr: &ast::Expr, out: &mut Vec<String>) -> bool {
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

    pub(super) fn lower_function(
        &self,
        func: &ast::StmtFunctionDef,
    ) -> Result<Function, CompileError> {
        self.lower_function_with_self(func, None)
    }

    pub(super) fn lower_method(
        &self,
        func: &ast::StmtFunctionDef,
        class_name: &str,
    ) -> Result<Function, CompileError> {
        self.lower_function_with_self(func, Some(class_name))
    }

    pub(super) fn lower_function_with_self(
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
            if idx == 0 && name_str == "self" {
                if let Some(self_name) = self_type {
                    let ann = if let Some(ann_expr) = &def.annotation {
                        self.lower_type_ref(ann_expr)?
                    } else {
                        TypeRef::Name(self_name.to_string())
                    };
                    params.push(Param {
                        name: name_str,
                        ann,
                        span: Span::from(def.range),
                    });
                    continue;
                }
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

    pub(super) fn lower_class(&self, class: &ast::StmtClassDef) -> Result<ClassDef, CompileError> {
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

    pub(super) fn extract_init_fields_from_ast(
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
}
