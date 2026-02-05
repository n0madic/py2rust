use super::*;

/// Statement lowering from RustPython AST to HIR.
///
/// Statements are the imperative building blocks of Python programs.
/// This module handles:
/// 1. Control flow (if, while, for, match, try/except)
/// 2. Variable binding (assignments, annotated assignments)
/// 3. Function definitions (converted to lambda assignments)
/// 4. Class definitions (handled elsewhere)
/// 5. Global declarations
///
/// Key design decisions:
/// - Nested function definitions are lowered to lambda assignments
///   This allows them to be type-checked like any other value
///   Example: `def f(): ...` becomes `f = lambda: ...` in HIR
/// - We distinguish between Let (new variable) and Assign (mutation)
///   This makes it easier to determine Rust's `let` vs bare assignment
/// - We normalize assignment targets (name, attribute, index) into AssignTarget
/// - Exception handling is preserved in HIR for later analysis
impl<'a> Lowerer<'a> {
    pub(super) fn lower_stmt(&self, stmt: &ast::Stmt) -> Result<Stmt, CompileError> {
        let span = Span::from(stmt.range());
        let kind = match stmt {
            // Nested function definition (inside another function/class)
            // We lower this to a lambda assignment: `f = lambda params: body`
            //
            // Why treat nested functions differently from top-level?
            // - Top-level functions are Item::Function (proper function definitions)
            // - Nested functions are local variables that happen to contain lambdas
            // - This matches Python's semantics where nested functions are closures
            ast::Stmt::FunctionDef(def) => {
                // Decorators only allowed on top-level functions
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
                    params.push(self.ident(arg.def.arg.as_str()));
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
                    name: self.ident(def.name.as_str()),
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
                        name: self.ident(name.id.as_str()),
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
            ast::Stmt::Nonlocal(def) => StmtKind::Nonlocal {
                names: def.names.iter().map(|n| n.to_string()).collect(),
            },
            ast::Stmt::If(def) => {
                let test = self.lower_expr(&def.test)?;
                let mut body_stmts = Vec::new();
                for stmt in &def.body {
                    body_stmts.push(self.lower_stmt(stmt)?);
                }
                // else branch (can be empty)
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
            // While loop: while test: body
            // Python allows else clause (runs if loop completes without break)
            // We don't support else on while yet
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
            // For loop: for target in iter: body
            // We support:
            // - Simple name target: for x in items:
            // - Tuple unpacking: for (a, b) in pairs:
            ast::Stmt::For(def) => {
                let iter = self.lower_expr(&def.iter)?;
                let target = match &*def.target {
                    ast::Expr::Name(name) => ForTarget::Name(self.ident(name.id.as_str())),
                    ast::Expr::Tuple(tuple) => {
                        // Extract simple names from tuple elements for pattern matching.
                        let mut names = Vec::new();
                        for elt in &tuple.elts {
                            if let ast::Expr::Name(name) = elt {
                                names.push(self.ident(name.id.as_str()));
                            } else {
                                return Err(self.error(
                                    def.target.range(),
                                    "For loop tuple unpacking only supports simple names",
                                ));
                            }
                        }
                        ForTarget::Tuple(names)
                    }
                    _ => {
                        return Err(self.error(
                            def.target.range(),
                            "For loop target must be a name or tuple of names",
                        ));
                    }
                };
                let mut body_stmts = Vec::new();
                for stmt in &def.body {
                    body_stmts.push(self.lower_stmt(stmt)?);
                }
                StmtKind::For {
                    target,
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
            // Augmented assignment: x += 1, obj.field *= 2, list[0] //= 3, a <<= 1
            // These are syntactic sugar for x = x + 1, etc.
            // We lower to a regular assignment with binary operation
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
                    ast::Operator::BitOr => BinOp::BitOr,
                    ast::Operator::BitAnd => BinOp::BitAnd,
                    ast::Operator::BitXor => BinOp::BitXor,
                    ast::Operator::LShift => BinOp::ShiftLeft,
                    ast::Operator::RShift => BinOp::ShiftRight,
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
            // Match statement (pattern matching):
            // match value:
            //     case Constructor(x):
            //         ...
            // We only support matching on Union constructors
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
            // Import statement: import typing
            // We only allow importing typing module (used for type annotations)
            // All other imports are rejected
            ast::Stmt::Import(import) => {
                if import
                    .names
                    .iter()
                    .all(|alias| alias.name.as_str() == "typing")
                {
                    StmtKind::Expr(Expr {
                        kind: ExprKind::Literal(Literal::None),
                        span,
                        ty: None,
                    })
                } else {
                    return Err(self.error(stmt.range(), "Unsupported import"));
                }
            }
            // From-import: from typing import List, Dict, ...
            // Only allowed for typing module, all others rejected
            ast::Stmt::ImportFrom(import) => {
                let module_ok = import
                    .module
                    .as_ref()
                    .is_some_and(|m| m.as_str() == "typing");
                let level_ok = import.level.map(|lvl| lvl.to_u32()).unwrap_or(0) == 0;
                if module_ok && level_ok {
                    StmtKind::Expr(Expr {
                        kind: ExprKind::Literal(Literal::None),
                        span,
                        ty: None,
                    })
                } else {
                    return Err(self.error(stmt.range(), "Unsupported import"));
                }
            }
            // Pass statement (no-op)
            // Lowered to None expression for consistency
            ast::Stmt::Pass(_) => StmtKind::Expr(Expr {
                kind: ExprKind::Literal(Literal::None),
                span,
                ty: None,
            }),
            // Exception handling:
            // try:
            //     body
            // except ExceptionType as var:
            //     handler
            // else:
            //     orelse  (runs if no exception)
            // finally:
            //     finalbody  (always runs)
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
            // Raise statement: raise Exception("message") or bare raise (re-raise)
            // We support:
            // - raise ExceptionType(args): create and raise new exception
            // - raise: re-raise current exception (only valid in except handler)
            // - raise X from Y: exception chaining (not yet supported)
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

    /// Lower a match case pattern.
    ///
    /// We only support matching on Union variant constructors:
    /// case Constructor(x, y):
    ///
    /// Guards (case X if condition:) are not supported.
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

    /// Lower an except handler clause.
    ///
    /// Forms:
    /// - except: (catch all exceptions)
    /// - except ExceptionType: (catch specific type)
    /// - except ExceptionType as var: (catch and bind to variable)
    ///
    /// We only support simple exception type names, not complex expressions.
    pub(super) fn lower_except_handler(
        &self,
        handler: &ast::ExceptHandler,
    ) -> Result<ExceptHandler, CompileError> {
        match handler {
            ast::ExceptHandler::ExceptHandler(eh) => {
                let exc_type = match eh.type_.as_deref() {
                    Some(ast::Expr::Name(name)) => Some(self.ident(name.id.as_str())),
                    Some(_) => {
                        return Err(
                            self.error(handler.range(), "Exception type must be a simple name")
                        )
                    }
                    None => None,
                };

                let name = eh.name.as_ref().map(|id| self.ident(id.as_str()));
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

    /// Lower a match pattern.
    ///
    /// We only support class constructor patterns: Constructor(x, y)
    /// Returns: (variant_name, list_of_binding_names)
    ///
    /// Not supported:
    /// - Literal patterns (case 1:, case "hello":)
    /// - Wildcard patterns (case _:) - use bare except instead
    /// - OR patterns (case X | Y:)
    /// - Nested patterns
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
                    ast::Expr::Name(name) => self.ident(name.id.as_str()),
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
                            bindings.push(self.ident(name.as_str()));
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

    /// Lower an assignment target (left-hand side of assignment).
    ///
    /// Valid targets:
    /// - name: simple variable assignment
    /// - obj.attr: attribute assignment (mutation)
    /// - obj[index]: subscript assignment (mutation)
    /// - tuple/list unpacking: (a, b) = values, [a, (b, c)] = values
    ///
    /// Not supported:
    /// - Starred targets (a, *rest = values)
    pub(super) fn lower_assign_target(
        &self,
        expr: &ast::Expr,
    ) -> Result<AssignTarget, CompileError> {
        match expr {
            ast::Expr::Name(name) => Ok(AssignTarget::Name(self.ident(name.id.as_str()))),
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
            ast::Expr::Tuple(tuple) => {
                let mut targets = Vec::new();
                for elt in &tuple.elts {
                    if matches!(elt, ast::Expr::Starred(_)) {
                        return Err(
                            self.error(elt.range(), "Starred assignment targets are not supported")
                        );
                    }
                    targets.push(self.lower_assign_target(elt)?);
                }
                Ok(AssignTarget::Tuple(targets))
            }
            ast::Expr::List(list) => {
                let mut targets = Vec::new();
                for elt in &list.elts {
                    if matches!(elt, ast::Expr::Starred(_)) {
                        return Err(
                            self.error(elt.range(), "Starred assignment targets are not supported")
                        );
                    }
                    targets.push(self.lower_assign_target(elt)?);
                }
                Ok(AssignTarget::List(targets))
            }
            _ => Err(self.error(expr.range(), "Unsupported assignment target")),
        }
    }
}
