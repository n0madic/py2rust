use super::*;

/// Lowered `match` pattern payload:
/// - variant class name
/// - bound variable names
/// - optional field names for keyword patterns
type LoweredMatchPattern = (String, Vec<String>, Option<Vec<String>>);

/// Lowered runtime match payload for non-union `match` statements.
struct LoweredRuntimePattern {
    /// Boolean condition that determines if the pattern matches.
    test: Expr,
    /// Variable bindings introduced by the pattern.
    bindings: Vec<(String, Expr)>,
}

/// Lowered runtime case payload used when desugaring `match` to `if/elif`.
struct LoweredRuntimeMatchCase {
    /// Pattern match condition.
    test: Expr,
    /// Optional guard condition (`case pat if guard:`).
    guard: Option<Expr>,
    /// Binding statements emitted before body/guard checks.
    bindings: Vec<Stmt>,
    /// Lowered case body statements.
    body: Vec<Stmt>,
    /// Case span for diagnostics and generated HIR nodes.
    span: Span,
}

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
            // Context manager: with expr as name: body
            //
            // We lower this to a block with explicit close() calls:
            // with open("x") as f:
            //     body
            // becomes:
            // {
            //     __tmp = open("x")
            //     f = __tmp
            //     body
            //     __tmp.close()
            // }
            //
            // For multiple with-items, close() calls are appended in reverse order.
            ast::Stmt::With(with_stmt) => {
                let mut block_stmts = Vec::new();
                let mut close_stmts = Vec::new();
                let mut lowered_body = with_stmt
                    .body
                    .iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>, _>>()?;

                let with_id = stmt.range().start().to_u32();
                for (item_index, item) in with_stmt.items.iter().enumerate() {
                    let item_span = Span::from(item.context_expr.range());
                    let temp_name =
                        self.ident(&format!("__py2rust_with_{}_{}", with_id, item_index));
                    let manager_name = if let Some(optional_vars) = &item.optional_vars {
                        match &**optional_vars {
                            ast::Expr::Name(name) => self.ident(name.id.as_str()),
                            _ => {
                                return Err(self.error(
                                    optional_vars.range(),
                                    "with target must be a simple name",
                                ));
                            }
                        }
                    } else {
                        temp_name
                    };

                    block_stmts.push(Stmt {
                        kind: StmtKind::Let {
                            name: manager_name.clone(),
                            ann: None,
                            value: self.lower_expr(&item.context_expr)?,
                        },
                        span: item_span,
                    });

                    close_stmts.push(Stmt {
                        kind: StmtKind::Expr(Expr {
                            kind: ExprKind::Call {
                                func: Box::new(Expr {
                                    kind: ExprKind::Attr {
                                        value: Box::new(Expr {
                                            kind: ExprKind::Name(manager_name),
                                            span: item_span,
                                            ty: None,
                                        }),
                                        attr: "close".to_string(),
                                    },
                                    span: item_span,
                                    ty: None,
                                }),
                                args: vec![],
                                keywords: vec![],
                            },
                            span: item_span,
                            ty: None,
                        }),
                        span: item_span,
                    });
                }
                block_stmts.append(&mut lowered_body);
                for close_stmt in close_stmts.into_iter().rev() {
                    block_stmts.push(close_stmt);
                }
                StmtKind::Expr(Expr {
                    kind: ExprKind::Block { stmts: block_stmts },
                    span,
                    ty: None,
                })
            }
            // For loop: for target in iter: body
            // We support:
            // - Simple name target: for x in items:
            // - General unpacking/assignment targets via per-iteration desugaring.
            ast::Stmt::For(def) => {
                let iter = self.lower_expr(&def.iter)?;
                let target_span = Span::from(def.target.range());
                let mut body_stmts = Vec::new();
                let target = match &*def.target {
                    ast::Expr::Name(name) => ForTarget::Name(self.ident(name.id.as_str())),
                    ast::Expr::Tuple(tuple) => {
                        // Keep simple tuple patterns as first-class loop patterns.
                        let mut names = Vec::with_capacity(tuple.elts.len());
                        let mut all_simple = true;
                        for elt in &tuple.elts {
                            if let ast::Expr::Name(name) = elt {
                                names.push(self.ident(name.id.as_str()));
                            } else {
                                all_simple = false;
                                break;
                            }
                        }
                        if all_simple {
                            ForTarget::Tuple(names)
                        } else {
                            // Desugar complex tuple targets (including starred patterns).
                            let iter_item_name = self.ident(&format!(
                                "__py2rust_for_item_{}",
                                def.target.range().start().to_u32()
                            ));
                            let unpack_target = self.lower_assign_target(&def.target)?;
                            let unpack_value = Expr {
                                kind: ExprKind::Name(iter_item_name.clone()),
                                span: target_span,
                                ty: None,
                            };
                            body_stmts.push(Stmt {
                                kind: StmtKind::Assign {
                                    target: unpack_target,
                                    value: unpack_value,
                                },
                                span: target_span,
                            });
                            ForTarget::Name(iter_item_name)
                        }
                    }
                    _ => {
                        // Desugar complex targets:
                        //   for a, *rest in items: ...
                        // into:
                        //   for __py2rust_for_item_N in items:
                        //       a, *rest = __py2rust_for_item_N
                        let iter_item_name = self.ident(&format!(
                            "__py2rust_for_item_{}",
                            def.target.range().start().to_u32()
                        ));
                        let unpack_target = self.lower_assign_target(&def.target)?;
                        let unpack_value = Expr {
                            kind: ExprKind::Name(iter_item_name.clone()),
                            span: target_span,
                            ty: None,
                        };
                        body_stmts.push(Stmt {
                            kind: StmtKind::Assign {
                                target: unpack_target,
                                value: unpack_value,
                            },
                            span: target_span,
                        });
                        ForTarget::Name(iter_item_name)
                    }
                };
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
            // 1. Class-constructor-only matches stay as HIR Match (union dispatch path).
            // 2. Literal/sequence/capture/or/guard patterns are desugared to if/elif.
            ast::Stmt::Match(def) => {
                // Keep the existing class-pattern behavior and diagnostics so
                // union pattern matching and its negative tests remain intact.
                let has_class_guard = def.cases.iter().any(|case| {
                    matches!(&case.pattern, ast::Pattern::MatchClass(_)) && case.guard.is_some()
                });
                if has_class_guard {
                    return Err(self.error(def.range(), "Match guards are not supported"));
                }

                let all_class_patterns = def
                    .cases
                    .iter()
                    .all(|case| matches!(&case.pattern, ast::Pattern::MatchClass(_)));
                if all_class_patterns {
                    let subject = self.lower_expr(&def.subject)?;
                    let mut lowered_cases = Vec::new();
                    for case in &def.cases {
                        lowered_cases.push(self.lower_match_case(case)?);
                    }
                    StmtKind::Match {
                        subject,
                        cases: lowered_cases,
                    }
                } else {
                    self.lower_runtime_match_stmt(&def.subject, &def.cases, def.range())?
                }
            }
            // Import statement: import os, import os as o
            // We allow:
            // - typing (for annotations)
            // - os/sys (for supported stdlib calls)
            // All other imports are rejected.
            ast::Stmt::Import(import) => {
                let mut names = Vec::new();
                for alias in &import.names {
                    let module = self.ident(alias.name.as_str());
                    if !matches!(module.as_str(), "typing" | "os" | "sys") {
                        return Err(self.error(stmt.range(), "Unsupported import"));
                    }
                    let alias = alias.asname.as_ref().map(|name| self.ident(name.as_str()));
                    names.push(ImportBinding { module, alias });
                }
                StmtKind::Import { names }
            }
            // From-import: from os import remove, from os import remove as rm
            // and from typing import ...
            ast::Stmt::ImportFrom(import) => {
                let module_name = import
                    .module
                    .as_ref()
                    .map(|m| self.ident(m.as_str()))
                    .ok_or_else(|| self.error(stmt.range(), "Unsupported import"))?;
                let level_ok = import.level.map(|lvl| lvl.to_u32()).unwrap_or(0) == 0;
                if !level_ok {
                    return Err(self.error(stmt.range(), "Unsupported import"));
                }
                if !matches!(module_name.as_str(), "typing" | "os" | "sys") {
                    return Err(self.error(stmt.range(), "Unsupported import"));
                }
                let mut names = Vec::new();
                for alias in &import.names {
                    let name = self.ident(alias.name.as_str());
                    if name == "*" {
                        if module_name == "os" {
                            return Err(
                                self.error(stmt.range(), "from os import * is not supported")
                            );
                        }
                        return Err(self.error(stmt.range(), "from import * is not supported"));
                    }
                    let alias = alias.asname.as_ref().map(|id| self.ident(id.as_str()));
                    names.push(ImportFromBinding { name, alias });
                }
                StmtKind::ImportFrom {
                    module: module_name,
                    names,
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
    /// We support matching on Union variant constructors:
    /// - positional patterns: case Constructor(x, y):
    /// - keyword patterns: case Constructor(x=a, y=b):
    ///
    /// Guards (case X if condition:) are not supported.
    pub(super) fn lower_match_case(
        &self,
        case: &ast::MatchCase,
    ) -> Result<MatchCase, CompileError> {
        let span = Span::from(case.pattern.range());
        let (variant, bindings, binding_fields) = self.lower_pattern(&case.pattern)?;
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
            binding_fields,
            body,
            span,
        })
    }

    /// Lower non-union match syntax by desugaring to an if/elif chain.
    ///
    /// This path supports literal, singleton, capture/wildcard, sequence, OR, and guard
    /// patterns while preserving "evaluate subject once" semantics.
    fn lower_runtime_match_stmt(
        &self,
        subject: &ast::Expr,
        cases: &[ast::MatchCase],
        range: rustpython_parser::text_size::TextRange,
    ) -> Result<StmtKind, CompileError> {
        let span = Span::from(range);
        let subject_span = Span::from(subject.range());
        let subject_name = self.ident(&format!(
            "__py2rust_match_subject_{}",
            range.start().to_u32()
        ));

        let subject_expr = Expr {
            kind: ExprKind::Name(subject_name.clone()),
            span: subject_span,
            ty: None,
        };

        let mut block_stmts = Vec::new();
        block_stmts.push(Stmt {
            kind: StmtKind::Let {
                name: subject_name,
                ann: None,
                value: self.lower_expr(subject)?,
            },
            span: subject_span,
        });

        let mut next_branch: Vec<Stmt> = Vec::new();
        for case in cases.iter().rev() {
            let lowered_case = self.lower_runtime_match_case(case, &subject_expr)?;
            let mut matched_body = lowered_case.bindings;
            if let Some(guard) = lowered_case.guard {
                // Guard failure continues with the next case, just like Python's `match`.
                matched_body.push(Stmt {
                    kind: StmtKind::If {
                        test: guard,
                        body: lowered_case.body,
                        orelse: next_branch.clone(),
                    },
                    span: lowered_case.span,
                });
            } else {
                matched_body.extend(lowered_case.body);
            }

            let outer_if = Stmt {
                kind: StmtKind::If {
                    test: lowered_case.test,
                    body: matched_body,
                    orelse: next_branch,
                },
                span: lowered_case.span,
            };
            next_branch = vec![outer_if];
        }

        block_stmts.extend(next_branch);
        Ok(StmtKind::Expr(Expr {
            kind: ExprKind::Block { stmts: block_stmts },
            span,
            ty: None,
        }))
    }

    /// Lower one runtime case to a condition + bindings + body representation.
    fn lower_runtime_match_case(
        &self,
        case: &ast::MatchCase,
        subject: &Expr,
    ) -> Result<LoweredRuntimeMatchCase, CompileError> {
        let span = Span::from(case.pattern.range());
        let lowered_pattern = self.lower_runtime_pattern(&case.pattern, subject)?;
        let guard = case
            .guard
            .as_ref()
            .map(|guard| self.lower_expr(guard))
            .transpose()?;
        let bindings = lowered_pattern
            .bindings
            .into_iter()
            .map(|(name, value)| Stmt {
                kind: StmtKind::Let {
                    name,
                    ann: None,
                    value,
                },
                span,
            })
            .collect::<Vec<_>>();
        let body = case
            .body
            .iter()
            .map(|stmt| self.lower_stmt(stmt))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(LoweredRuntimeMatchCase {
            test: lowered_pattern.test,
            guard,
            bindings,
            body,
            span,
        })
    }

    /// Lower a runtime pattern into a boolean condition plus bindings.
    fn lower_runtime_pattern(
        &self,
        pattern: &ast::Pattern,
        subject: &Expr,
    ) -> Result<LoweredRuntimePattern, CompileError> {
        let span = Span::from(pattern.range());
        match pattern {
            ast::Pattern::MatchValue(value_pat) => {
                let value = self.lower_expr(&value_pat.value)?;
                Ok(LoweredRuntimePattern {
                    test: self.make_compare_expr(subject.clone(), CmpOp::Eq, value, span),
                    bindings: Vec::new(),
                })
            }
            ast::Pattern::MatchSingleton(singleton_pat) => {
                let (op, literal) = match &singleton_pat.value {
                    ast::Constant::Bool(value) => (CmpOp::Eq, Literal::Bool(*value)),
                    ast::Constant::None => (CmpOp::Is, Literal::None),
                    _ => {
                        return Err(self.error(
                            pattern.range(),
                            "Only True/False/None singleton patterns are supported",
                        ))
                    }
                };
                let rhs = Expr {
                    kind: ExprKind::Literal(literal),
                    span,
                    ty: None,
                };
                Ok(LoweredRuntimePattern {
                    test: self.make_compare_expr(subject.clone(), op, rhs, span),
                    bindings: Vec::new(),
                })
            }
            ast::Pattern::MatchSequence(seq_pat) => {
                self.lower_runtime_sequence_pattern(seq_pat, subject)
            }
            ast::Pattern::MatchStar(_) => Err(self.error(
                pattern.range(),
                "Starred patterns are only valid inside sequence patterns",
            )),
            ast::Pattern::MatchAs(as_pat) => {
                // `case _:` => MatchAs(name=None, pattern=None)
                // `case name:` => MatchAs(name=Some, pattern=None)
                // `case pat as name:` => MatchAs(name=Some, pattern=Some)
                if let Some(inner) = &as_pat.pattern {
                    let mut lowered = self.lower_runtime_pattern(inner, subject)?;
                    if let Some(name) = &as_pat.name {
                        lowered.bindings.push((
                            self.ident(name.as_str()),
                            self.make_clone_expr(subject, span),
                        ));
                    }
                    return Ok(lowered);
                }
                let test = self.make_bool_expr(true, span);
                let mut bindings = Vec::new();
                if let Some(name) = &as_pat.name {
                    bindings.push((
                        self.ident(name.as_str()),
                        self.make_clone_expr(subject, span),
                    ));
                }
                Ok(LoweredRuntimePattern { test, bindings })
            }
            ast::Pattern::MatchOr(or_pat) => {
                let mut tests = Vec::new();
                for subpattern in &or_pat.patterns {
                    let lowered = self.lower_runtime_pattern(subpattern, subject)?;
                    if !lowered.bindings.is_empty() {
                        return Err(self.error(
                            subpattern.range(),
                            "OR patterns with bindings are not supported",
                        ));
                    }
                    tests.push(lowered.test);
                }
                Ok(LoweredRuntimePattern {
                    test: self.combine_bool_exprs(BoolOp::Or, tests, span),
                    bindings: Vec::new(),
                })
            }
            ast::Pattern::MatchClass(_) => Err(self.error(
                pattern.range(),
                "Class constructor patterns are only supported for union matches",
            )),
            ast::Pattern::MatchMapping(_) => {
                Err(self.error(pattern.range(), "Mapping match patterns are not supported"))
            }
        }
    }

    /// Lower sequence patterns like `[a, b]`, `[1, x, 3]`, `[head, *rest]`.
    fn lower_runtime_sequence_pattern(
        &self,
        seq_pat: &ast::PatternMatchSequence,
        subject: &Expr,
    ) -> Result<LoweredRuntimePattern, CompileError> {
        let span = Span::from(seq_pat.range);
        let mut checks = Vec::new();
        let mut bindings = Vec::new();

        let star_positions = seq_pat
            .patterns
            .iter()
            .enumerate()
            .filter_map(|(idx, pattern)| {
                if matches!(pattern, ast::Pattern::MatchStar(_)) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if star_positions.len() > 1 {
            return Err(self.error(
                seq_pat.range,
                "Only one starred pattern is allowed in a sequence pattern",
            ));
        }

        let star_idx = star_positions.first().copied();
        let min_len = if star_idx.is_some() {
            seq_pat.patterns.len().saturating_sub(1)
        } else {
            seq_pat.patterns.len()
        };
        let min_len_expr = self.make_int_expr(min_len as i64, span);
        let len_cmp = if star_idx.is_some() {
            CmpOp::GtEq
        } else {
            CmpOp::Eq
        };
        checks.push(self.make_compare_expr(
            self.make_len_expr(subject, span),
            len_cmp,
            min_len_expr,
            span,
        ));

        let trailing_count = star_idx
            .map(|star_pos| seq_pat.patterns.len() - star_pos - 1)
            .unwrap_or(0);

        for (idx, pattern) in seq_pat.patterns.iter().enumerate() {
            if let ast::Pattern::MatchStar(star_pat) = pattern {
                if let Some(name) = &star_pat.name {
                    let start = if idx == 0 {
                        None
                    } else {
                        Some(Box::new(self.make_int_expr(idx as i64, span)))
                    };
                    let end = if trailing_count == 0 {
                        None
                    } else {
                        // Use a negative end index to avoid nested lock attempts like
                        // py_list_slice_step(&lock, ..., py_len(subject) - n, ...) during codegen.
                        Some(Box::new(self.make_int_expr(-(trailing_count as i64), span)))
                    };
                    let slice_expr = Expr {
                        kind: ExprKind::Slice {
                            value: Box::new(subject.clone()),
                            start,
                            end,
                            step: None,
                        },
                        span,
                        ty: None,
                    };
                    bindings.push((self.ident(name.as_str()), slice_expr));
                }
                continue;
            }

            let index_expr = if let Some(star_pos) = star_idx {
                if idx < star_pos {
                    self.make_int_expr(idx as i64, span)
                } else {
                    let idx_from_tail = idx - star_pos - 1;
                    let distance_from_end = trailing_count - idx_from_tail;
                    // Prefer negative indexing from the tail to avoid `len(subject)` inside
                    // list indexing code paths that already hold the list mutex.
                    self.make_int_expr(-(distance_from_end as i64), span)
                }
            } else {
                self.make_int_expr(idx as i64, span)
            };

            let element_subject = Expr {
                kind: ExprKind::Index {
                    value: Box::new(subject.clone()),
                    index: Box::new(index_expr),
                },
                span,
                ty: None,
            };
            let lowered = self.lower_runtime_pattern(pattern, &element_subject)?;
            checks.push(lowered.test);
            bindings.extend(lowered.bindings);
        }

        Ok(LoweredRuntimePattern {
            test: self.combine_bool_exprs(BoolOp::And, checks, span),
            bindings,
        })
    }

    /// Build `left <op> right`.
    fn make_compare_expr(&self, left: Expr, op: CmpOp, right: Expr, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Compare {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
            ty: None,
        }
    }

    /// Build a boolean literal expression.
    fn make_bool_expr(&self, value: bool, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Literal(Literal::Bool(value)),
            span,
            ty: None,
        }
    }

    /// Build an integer literal expression.
    fn make_int_expr(&self, value: i64, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Literal(Literal::Int(value)),
            span,
            ty: None,
        }
    }

    /// Build `len(subject)`.
    fn make_len_expr(&self, subject: &Expr, span: Span) -> Expr {
        Expr {
            kind: ExprKind::Call {
                func: Box::new(Expr {
                    kind: ExprKind::Name("len".to_string()),
                    span,
                    ty: None,
                }),
                args: vec![subject.clone()],
                keywords: vec![],
            },
            span,
            ty: None,
        }
    }

    /// Build a binding expression from the current subject value.
    fn make_clone_expr(&self, subject: &Expr, _span: Span) -> Expr {
        subject.clone()
    }

    /// Combine multiple boolean expressions with a single boolean operator.
    fn combine_bool_exprs(&self, op: BoolOp, mut values: Vec<Expr>, span: Span) -> Expr {
        if values.is_empty() {
            return self.make_bool_expr(true, span);
        }
        if values.len() == 1 {
            return values.remove(0);
        }
        Expr {
            kind: ExprKind::BoolOp { op, values },
            span,
            ty: None,
        }
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
    /// We support class constructor patterns:
    /// - positional: Constructor(x, y)
    /// - keyword: Constructor(x=a, y=b)
    ///
    /// Returns:
    /// - variant name
    /// - binding names
    /// - optional field-name mapping for keyword patterns
    ///
    /// Not supported:
    /// - Literal patterns (case 1:, case "hello":)
    /// - Wildcard patterns (case _:) - use bare except instead
    /// - OR patterns (case X | Y:)
    /// - Nested patterns
    pub(super) fn lower_pattern(
        &self,
        pattern: &ast::Pattern,
    ) -> Result<LoweredMatchPattern, CompileError> {
        match pattern {
            ast::Pattern::MatchClass(cls_pat) => {
                let variant = match &*cls_pat.cls {
                    ast::Expr::Name(name) => self.ident(name.id.as_str()),
                    _ => {
                        return Err(self.error(
                            pattern.range(),
                            "Only class constructor patterns are supported",
                        ))
                    }
                };

                if !cls_pat.patterns.is_empty() && !cls_pat.kwd_patterns.is_empty() {
                    return Err(self.error(
                        pattern.range(),
                        "Mixed positional and keyword patterns are not supported",
                    ));
                }

                let mut positional_bindings = Vec::new();
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
                            positional_bindings.push(self.ident(name.as_str()));
                        }
                        _ => {
                            return Err(
                                self.error(pat.range(), "Only simple bindings are supported")
                            )
                        }
                    }
                }

                if cls_pat.kwd_patterns.is_empty() {
                    return Ok((variant, positional_bindings, None));
                }

                if cls_pat.kwd_attrs.len() != cls_pat.kwd_patterns.len() {
                    return Err(self.error(
                        pattern.range(),
                        "Keyword pattern attribute and binding counts do not match",
                    ));
                }

                let mut keyword_fields = Vec::new();
                let mut keyword_bindings = Vec::new();
                for (attr, pat) in cls_pat.kwd_attrs.iter().zip(cls_pat.kwd_patterns.iter()) {
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
                            keyword_fields.push(self.ident(attr.as_str()));
                            keyword_bindings.push(self.ident(name.as_str()));
                        }
                        _ => {
                            return Err(
                                self.error(pat.range(), "Only simple bindings are supported")
                            )
                        }
                    }
                }

                Ok((variant, keyword_bindings, Some(keyword_fields)))
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
                let mut saw_starred = false;
                for elt in &tuple.elts {
                    if let ast::Expr::Starred(star) = elt {
                        if saw_starred {
                            return Err(self.error(
                                elt.range(),
                                "Only one starred assignment target is allowed",
                            ));
                        }
                        saw_starred = true;
                        let inner = self.lower_assign_target(&star.value)?;
                        if !matches!(inner, AssignTarget::Name(_)) {
                            return Err(self.error(
                                elt.range(),
                                "Starred assignment target must be a simple name",
                            ));
                        }
                        targets.push(AssignTarget::Starred(Box::new(inner)));
                        continue;
                    }
                    targets.push(self.lower_assign_target(elt)?);
                }
                Ok(AssignTarget::Tuple(targets))
            }
            ast::Expr::List(list) => {
                let mut targets = Vec::new();
                let mut saw_starred = false;
                for elt in &list.elts {
                    if let ast::Expr::Starred(star) = elt {
                        if saw_starred {
                            return Err(self.error(
                                elt.range(),
                                "Only one starred assignment target is allowed",
                            ));
                        }
                        saw_starred = true;
                        let inner = self.lower_assign_target(&star.value)?;
                        if !matches!(inner, AssignTarget::Name(_)) {
                            return Err(self.error(
                                elt.range(),
                                "Starred assignment target must be a simple name",
                            ));
                        }
                        targets.push(AssignTarget::Starred(Box::new(inner)));
                        continue;
                    }
                    targets.push(self.lower_assign_target(elt)?);
                }
                Ok(AssignTarget::List(targets))
            }
            ast::Expr::Starred(star) => Err(self.error(
                star.range(),
                "Starred assignment target is only valid inside tuple/list unpacking",
            )),
            _ => Err(self.error(expr.range(), "Unsupported assignment target")),
        }
    }
}
