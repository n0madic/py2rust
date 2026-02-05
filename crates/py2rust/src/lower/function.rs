use super::*;
use std::collections::HashMap;

impl<'a> Lowerer<'a> {
    /// Lower a decorated function into multiple HIR items.
    ///
    /// Python decorators are syntactic sugar: `@dec def f(x): ...` means `f = dec(f)`.
    /// We support simple name decorators on top-level functions, including stacking.
    ///
    /// Lowering strategy:
    /// 1. Create `f_impl(x)` with the original function body
    /// 2. Create a wrapper `f(x)` that calls `dec(f_impl)(x)`
    ///
    /// This approach avoids needing to understand what decorators do - we just
    /// expand the syntactic sugar and let type checking handle the rest.
    ///
    /// Limitations:
    /// - Decorator must be a simple name (not @module.decorator or @decorator())
    /// - Only supported on top-level functions (not methods or nested functions)
    pub(super) fn lower_decorated_function(
        &self,
        func: &ast::StmtFunctionDef,
    ) -> Result<Vec<Item>, CompileError> {
        if func.decorator_list.is_empty() {
            return Err(self.error(func.range(), "Decorator list is empty"));
        }
        if !func.type_params.is_empty() {
            return Err(self.error(func.range(), "Type parameters are not supported"));
        }

        // Extract decorator names (top to bottom)
        let mut decorators = Vec::new();
        for dec in &func.decorator_list {
            match dec {
                ast::Expr::Name(name) => decorators.push(name.id.to_string()),
                _ => {
                    return Err(
                        self.error(func.range(), "Only simple name decorators are supported")
                    )
                }
            }
        }

        // Create implementation function with renamed name
        let mut impl_func = self.lower_function(func)?;
        let orig_name = impl_func.name.clone();
        let impl_name = format!("{orig_name}_impl");
        impl_func.name = impl_name.clone();

        // Build wrapper function that calls decorator(s)
        let tmp_name = format!("_decorated_{orig_name}");

        // let _decorated_f = dec1(dec2(...(f_impl)));
        let mut call_decorator = Expr {
            kind: ExprKind::Name(impl_name),
            span: Span::from(func.range()),
            ty: None,
        };
        for decorator in decorators.iter().rev() {
            call_decorator = Expr {
                kind: ExprKind::Call {
                    func: Box::new(Expr {
                        kind: ExprKind::Name(decorator.clone()),
                        span: Span::from(func.range()),
                        ty: None,
                    }),
                    args: vec![call_decorator],
                },
                span: Span::from(func.range()),
                ty: None,
            };
        }
        let let_stmt = Stmt {
            kind: StmtKind::Let {
                name: tmp_name.clone(),
                ann: Some(TypeRef::Unknown),
                value: call_decorator,
            },
            span: Span::from(func.range()),
        };

        // Build argument list for calling the decorated function
        let mut args = Vec::new();
        for param in &impl_func.params {
            args.push(Expr {
                kind: ExprKind::Name(param.name.clone()),
                span: Span::from(func.range()),
                ty: None,
            });
        }

        // return _decorated_f(args...);
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

        // Build the wrapper function
        let wrapper = Function {
            name: orig_name,
            params: impl_func.params.clone(),
            ret: impl_func.ret.clone(),
            body: vec![let_stmt, return_stmt],
            span: Span::from(func.range()),
        };

        Ok(vec![Item::Function(impl_func), Item::Function(wrapper)])
    }

    /// Detect and lower union type aliases.
    ///
    /// Python 3.10+ allows: `type Status = Success | Failure`
    /// We support a simpler form: `Status = Success | Failure`
    ///
    /// This is detected by looking for assignments where the RHS is a chain
    /// of bitwise-or operations on simple names. If detected, we create a
    /// UnionDef HIR item instead of a regular assignment.
    ///
    /// The union variants must be previously-defined classes. Type checking
    /// will verify this and generate a Rust enum.
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

    /// Recursively collect union variant names from a chain of | operators.
    ///
    /// Example: `A | B | C` is parsed as BinOp(BinOp(A, |, B), |, C)
    /// We recursively collect [A, B, C] from this structure.
    ///
    /// Returns true if the expression is a valid union definition, false otherwise.
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

    /// Lower a function or method definition to HIR.
    ///
    /// If self_type is Some, this is a method and we handle the `self` parameter specially:
    /// - If the first parameter is named "self", we infer its type as the class type
    /// - This allows `def method(self, x: int):` without needing `self: ClassName` annotation
    ///
    /// Unsupported features that will error:
    /// - Type parameters (generics)
    /// - Positional-only parameters (/)
    /// - Keyword-only parameters (*)
    /// - *args and **kwargs
    ///
    /// Missing type annotations default to TypeRef::Unknown and will be inferred during
    /// type checking if possible.
    pub(super) fn lower_function_with_self(
        &self,
        func: &ast::StmtFunctionDef,
        self_type: Option<&str>,
    ) -> Result<Function, CompileError> {
        // Ignore decorators for now (no-op).
        if !func.type_params.is_empty() {
            return Err(self.error(func.range(), "Type parameters are not supported"));
        }

        // Lower parameters
        let mut params = Vec::new();
        for (idx, arg) in func.args.args.iter().enumerate() {
            let def = &arg.def;
            let name_str = def.arg.to_string();
            let default = match &arg.default {
                Some(expr) => Some(self.lower_expr(expr)?),
                None => None,
            };

            // Special handling for `self` parameter in methods
            if idx == 0 && name_str == "self" {
                if let Some(self_name) = self_type {
                    let ann = if let Some(ann_expr) = &def.annotation {
                        self.lower_type_ref(ann_expr)?
                    } else {
                        // Infer self type as the class name
                        TypeRef::Name(self_name.to_string())
                    };
                    params.push(Param {
                        name: name_str,
                        ann,
                        default: None,
                        span: Span::from(def.range),
                    });
                    continue;
                }
            }

            // Regular parameters
            let ann = match &def.annotation {
                Some(expr) => self.lower_type_ref(expr)?,
                None => TypeRef::Unknown,
            };
            params.push(Param {
                name: name_str,
                ann,
                default,
                span: Span::from(def.range),
            });
        }

        // Validate we don't have unsupported parameter forms
        if !func.args.posonlyargs.is_empty() || !func.args.kwonlyargs.is_empty() {
            return Err(self.error(
                func.range(),
                "positional-only and keyword-only args are not supported",
            ));
        }
        if func.args.vararg.is_some() || func.args.kwarg.is_some() {
            return Err(self.error(func.range(), "*args/**kwargs are not supported"));
        }

        // Lower return type annotation
        let ret = if let Some(ret_expr) = &func.returns {
            self.lower_type_ref(ret_expr)?
        } else {
            TypeRef::Unknown
        };

        // Lower function body
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
        if !class.keywords.is_empty() {
            return Err(self.error(
                class.range(),
                "Class inheritance keywords are not supported",
            ));
        }
        if !class.decorator_list.is_empty() {
            return Err(self.error(class.range(), "Class decorators are not supported"));
        }
        if !class.type_params.is_empty() {
            return Err(self.error(class.range(), "Class type parameters are not supported"));
        }
        let base = if class.bases.is_empty() {
            None
        } else if class.bases.len() == 1 {
            match &class.bases[0] {
                ast::Expr::Name(name) => Some(name.id.to_string()),
                _ => {
                    return Err(
                        self.error(class.range(), "Only simple base class names are supported")
                    )
                }
            }
        } else {
            return Err(self.error(class.range(), "Multiple inheritance is not supported"));
        };
        let mut fields: Vec<FieldDef> = Vec::new();
        let mut class_attrs: Vec<ClassAttrDef> = Vec::new();
        let mut methods = Vec::new();
        let mut method_kinds: HashMap<String, MethodKind> = HashMap::new();
        let mut properties: Vec<PropertyDef> = Vec::new();
        for item in &class.body {
            match item {
                ast::Stmt::FunctionDef(def) => {
                    let mut kind = MethodKind::Instance;
                    let mut is_property = false;
                    let mut property_name: Option<String> = None;
                    let mut is_property_setter = false;
                    if !def.decorator_list.is_empty() {
                        if def.decorator_list.len() != 1 {
                            return Err(self.error(
                                def.range(),
                                "Only a single decorator is supported on methods",
                            ));
                        }
                        match &def.decorator_list[0] {
                            ast::Expr::Name(name) => match name.id.as_str() {
                                "property" => {
                                    is_property = true;
                                    property_name = Some(def.name.to_string());
                                }
                                "staticmethod" => {
                                    kind = MethodKind::Static;
                                }
                                "classmethod" => {
                                    kind = MethodKind::Class;
                                }
                                _ => {
                                    return Err(
                                        self.error(def.range(), "Unsupported method decorator")
                                    )
                                }
                            },
                            ast::Expr::Attribute(attr) => {
                                if let ast::Expr::Name(base_name) = &*attr.value {
                                    if attr.attr.as_str() == "setter" {
                                        is_property_setter = true;
                                        property_name = Some(base_name.id.to_string());
                                    } else {
                                        return Err(
                                            self.error(def.range(), "Unsupported method decorator")
                                        );
                                    }
                                } else {
                                    return Err(
                                        self.error(def.range(), "Unsupported method decorator")
                                    );
                                }
                            }
                            _ => {
                                return Err(self.error(def.range(), "Unsupported method decorator"))
                            }
                        }
                    }
                    let func = self.lower_method(def, class.name.as_ref())?;
                    if is_property_setter {
                        // Use as_ref().map() to avoid cloning until needed.
                        let prop_name = property_name
                            .as_ref()
                            .expect("setter must include property name")
                            .clone();
                        let setter_name = format!("__set_{}", prop_name);
                        let mut setter_func = func;
                        setter_func.name = setter_name.clone();
                        method_kinds.insert(setter_name.clone(), MethodKind::Instance);
                        properties.push(PropertyDef {
                            name: prop_name,
                            getter: String::new(),
                            setter: Some(setter_name),
                            span: Span::from(def.range()),
                        });
                        methods.push(setter_func);
                        continue;
                    }
                    if is_property {
                        let prop_name = property_name
                            .as_ref()
                            .expect("property must have a name")
                            .clone();
                        properties.push(PropertyDef {
                            name: prop_name,
                            getter: func.name.clone(),
                            setter: None,
                            span: Span::from(def.range()),
                        });
                    }
                    method_kinds.insert(func.name.clone(), kind);
                    if func.name == "__init__" {
                        let known_fields: std::collections::HashSet<String> =
                            fields.iter().map(|f| f.name.clone()).collect();
                        let init_fields =
                            self.extract_init_fields_from_ast(&def.body, &known_fields)?;
                        for field in init_fields {
                            if !fields.iter().any(|f| f.name == field.name) {
                                fields.push(field);
                            }
                        }
                    }
                    methods.push(func);
                }
                ast::Stmt::AnnAssign(def) => {
                    if let ast::Expr::Name(name) = &*def.target {
                        let ann = self.lower_type_ref(&def.annotation)?;
                        if let Some(value) = &def.value {
                            class_attrs.push(ClassAttrDef {
                                name: name.id.to_string(),
                                ann: Some(ann),
                                value: self.lower_expr(value)?,
                                span: Span::from(def.range()),
                            });
                        } else if !fields.iter().any(|f| f.name == name.id.as_str()) {
                            fields.push(FieldDef {
                                name: name.id.to_string(),
                                ty: ann,
                                span: Span::from(def.range()),
                            });
                        }
                    } else {
                        return Err(self.error(
                            def.range(),
                            "Only simple name annotations are supported in class bodies",
                        ));
                    }
                }
                ast::Stmt::Assign(def) => {
                    if def.targets.len() != 1 {
                        return Err(self.error(
                            def.range(),
                            "Only single-target assignments are supported in class bodies",
                        ));
                    }
                    if let ast::Expr::Name(name) = &def.targets[0] {
                        class_attrs.push(ClassAttrDef {
                            name: name.id.to_string(),
                            ann: None,
                            value: self.lower_expr(&def.value)?,
                            span: Span::from(def.range()),
                        });
                    } else {
                        return Err(self.error(
                            def.range(),
                            "Only simple name assignments are supported in class bodies",
                        ));
                    }
                }
                ast::Stmt::Pass(_) => {}
                ast::Stmt::Expr(expr_stmt) => {
                    // Allow docstrings (string literal expressions) in class bodies.
                    let is_docstring = matches!(&*expr_stmt.value, ast::Expr::Constant(cons) if matches!(cons.value, ast::Constant::Str(_)));
                    if !is_docstring {
                        return Err(self.error(
                            item.range(),
                            "Only method definitions are allowed inside classes",
                        ));
                    }
                    // Ignore docstrings.
                }
                _ => {
                    return Err(self.error(
                        item.range(),
                        "Only method definitions are allowed inside classes",
                    ))
                }
            }
        }

        // Extract __match_args__ if present from AST
        let match_args = self.extract_match_args_from_ast(&class.body)?;

        Ok(ClassDef {
            name: class.name.to_string(),
            base,
            fields,
            class_attrs,
            methods,
            method_kinds,
            properties,
            match_args,
            span: Span::from(class.range()),
        })
    }

    /// Extract __match_args__ from class body AST if present.
    ///
    /// __match_args__ should be a tuple of strings defining field order for pattern matching.
    /// Example: __match_args__ = ('x', 'y')
    fn extract_match_args_from_ast(
        &self,
        body: &[ast::Stmt],
    ) -> Result<Option<Vec<String>>, CompileError> {
        for item in body {
            if let ast::Stmt::Assign(def) = item {
                if def.targets.len() == 1 {
                    if let ast::Expr::Name(name) = &def.targets[0] {
                        if name.id.as_str() == "__match_args__" {
                            // __match_args__ must be a tuple of string literals
                            if let ast::Expr::Tuple(tuple) = &*def.value {
                                let mut field_names = Vec::new();
                                for elem in &tuple.elts {
                                    if let ast::Expr::Constant(cons) = elem {
                                        if let ast::Constant::Str(s) = &cons.value {
                                            field_names.push(s.to_string());
                                        } else {
                                            return Err(self.error(
                                                elem.range(),
                                                "__match_args__ must contain only string literals",
                                            ));
                                        }
                                    } else {
                                        return Err(self.error(
                                            elem.range(),
                                            "__match_args__ must contain only string literals",
                                        ));
                                    }
                                }
                                return Ok(Some(field_names));
                            } else {
                                return Err(self.error(
                                    def.value.range(),
                                    "__match_args__ must be a tuple of strings",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    pub(super) fn extract_init_fields_from_ast(
        &self,
        body: &[ast::Stmt],
        known_fields: &std::collections::HashSet<String>,
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
                    if def.targets.iter().any(|t| {
                        matches!(
                            t,
                            ast::Expr::Attribute(attr)
                                if matches!(&*attr.value, ast::Expr::Name(name) if name.id.as_str() == "self")
                        )
                    }) {
                        for target in &def.targets {
                            if let ast::Expr::Attribute(attr) = target {
                                if matches!(&*attr.value, ast::Expr::Name(name) if name.id.as_str() == "self")
                                    && !known_fields.contains(attr.attr.as_str()) {
                                        return Err(self.error(
                                            def.range(),
                                            "Field assignments in __init__ must use type annotations",
                                        ));
                                    }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(fields)
    }
}
