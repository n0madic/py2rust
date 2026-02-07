use super::*;

/// Expression lowering from RustPython AST to HIR.
///
/// This module handles the conversion of Python expressions to our intermediate representation.
/// Key responsibilities:
/// 1. Simplify Python's complex expression AST into our focused HIR
/// 2. Reject unsupported Python features with clear error messages
/// 3. Normalize similar constructs (e.g., all comparisons are binary)
/// 4. Preserve source spans for error reporting
///
/// Design decisions:
/// - We reject chained comparisons (a < b < c) because they complicate type checking
///   Users must write them as (a < b) and (b < c)
/// - Dict constructor dict(a=1, b=2) is special-cased to become literal syntax
/// - F-strings are lowered to format!() macro calls
/// - Comprehensions support multiple generator clauses
impl<'a> Lowerer<'a> {
    /// Lower a Python expression to HIR.
    ///
    /// This is the main entry point for expression lowering. It pattern matches
    /// on the RustPython AST node type and delegates to appropriate handling.
    pub(super) fn lower_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
        let span = Span::from(expr.range());
        let kind = match expr {
            // Simple name reference (variable, function name, etc.)
            ast::Expr::Name(name) => ExprKind::Name(self.ident(name.id.as_str())),

            // Constant literals (int, float, bool, str, None)
            ast::Expr::Constant(cons) => match &cons.value {
                ast::Constant::Int(value) => {
                    // We target i64 for integers. Python's unbounded integers would
                    // require BigInt support, which we don't provide.
                    // If the literal is too large, we reject it with a clear error.
                    let parsed = value.to_string().parse::<i64>().map_err(|_| {
                        self.error(expr.range(), "Integer literal out of range for i64")
                    })?;
                    ExprKind::Literal(Literal::Int(parsed))
                }
                ast::Constant::Float(value) => ExprKind::Literal(Literal::Float(*value)),
                ast::Constant::Bool(value) => ExprKind::Literal(Literal::Bool(*value)),
                ast::Constant::Str(value) => ExprKind::Literal(Literal::Str(value.to_string())),
                ast::Constant::Bytes(value) => ExprKind::Literal(Literal::Bytes(value.clone())),
                ast::Constant::None => ExprKind::Literal(Literal::None),
                // Reject unsupported literals (bytes, complex, etc.)
                _ => return Err(self.error(expr.range(), "Unsupported literal")),
            },
            // Function call expression
            ast::Expr::Call(call) => {
                let lower_call_arg = |arg: &ast::Expr| -> Result<Expr, CompileError> {
                    if let ast::Expr::Starred(star) = arg {
                        Ok(Expr {
                            kind: ExprKind::Starred {
                                value: Box::new(self.lower_expr(&star.value)?),
                            },
                            span: Span::from(arg.range()),
                            ty: None,
                        })
                    } else {
                        self.lower_expr(arg)
                    }
                };
                // Special case: dict(a=1, b=2) constructor syntax
                // This is more ergonomic in Python than {"a": 1, "b": 2}
                // We convert it to literal dict syntax in HIR
                let is_dict_ctor =
                    matches!(&*call.func, ast::Expr::Name(name) if name.id.as_str() == "dict");
                if !call.keywords.is_empty() {
                    // Convert dict(a=1, b=2) to {"a": 1, "b": 2} for parity with literal dicts.
                    if is_dict_ctor && call.args.is_empty() {
                        let mut items = Vec::new();
                        for kw in &call.keywords {
                            let key = match &kw.arg {
                                Some(arg) => Expr {
                                    kind: ExprKind::Literal(Literal::Str(arg.to_string())),
                                    span,
                                    ty: None,
                                },
                                // Keep dict() constructor lowering strict: only named pairs.
                                None => {
                                    return Err(self
                                        .error(expr.range(), "dict() does not support **kwargs"))
                                }
                            };
                            let value = self.lower_expr(&kw.value)?;
                            items.push((key, value));
                        }
                        ExprKind::Dict(items)
                    } else {
                        let func = Box::new(self.lower_expr(&call.func)?);
                        let mut lowered_args = Vec::new();
                        for arg in &call.args {
                            lowered_args.push(lower_call_arg(arg)?);
                        }
                        let mut lowered_keywords = Vec::new();
                        for kw in &call.keywords {
                            lowered_keywords.push(KeywordArg {
                                name: kw.arg.as_ref().map(|name| self.ident(name.as_str())),
                                value: self.lower_expr(&kw.value)?,
                            });
                        }
                        ExprKind::Call {
                            func,
                            args: lowered_args,
                            keywords: lowered_keywords,
                        }
                    }
                } else if is_dict_ctor && call.args.is_empty() {
                    // Empty dict() -> {}
                    ExprKind::Dict(Vec::new())
                } else {
                    let func = Box::new(self.lower_expr(&call.func)?);
                    let mut lowered_args = Vec::new();
                    for arg in &call.args {
                        lowered_args.push(lower_call_arg(arg)?);
                    }
                    ExprKind::Call {
                        func,
                        args: lowered_args,
                        keywords: Vec::new(),
                    }
                }
            }
            // Attribute access: obj.field
            // Simple delegation to HIR - type checker will verify the attribute exists
            ast::Expr::Attribute(attr) => ExprKind::Attr {
                value: Box::new(self.lower_expr(&attr.value)?),
                attr: attr.attr.to_string(),
            },
            // Binary operations: +, -, *, /, %, //, **, |, &, ^, <<, >>
            // We map each Python operator to our HIR operator.
            // Note: We reject MatMult (@) because we don't support matrix ops.
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
                    ast::Operator::LShift => BinOp::ShiftLeft,
                    ast::Operator::RShift => BinOp::ShiftRight,
                    _ => return Err(self.error(expr.range(), "Unsupported binary operator")),
                };
                ExprKind::Binary {
                    op,
                    left: Box::new(self.lower_expr(&bin.left)?),
                    right: Box::new(self.lower_expr(&bin.right)?),
                }
            }
            // Unary operations: -x, not x, ~x
            // We support negation, logical not, and bitwise not.
            // Python's unary + is rejected.
            ast::Expr::UnaryOp(unary) => {
                let op = match unary.op {
                    ast::UnaryOp::USub => UnaryOp::Neg,
                    ast::UnaryOp::Not => UnaryOp::Not,
                    ast::UnaryOp::Invert => UnaryOp::BitNot,
                    _ => return Err(self.error(expr.range(), "Unsupported unary operator")),
                };
                ExprKind::Unary {
                    op,
                    expr: Box::new(self.lower_expr(&unary.operand)?),
                }
            }
            // Boolean operations: and, or
            // Python allows chains: a and b and c
            // We preserve this in HIR - codegen will short-circuit properly
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
            // Comparison operations (including chained comparisons)
            ast::Expr::Compare(comp) => {
                let map_op = |op: &ast::CmpOp| -> CmpOp {
                    match op {
                        ast::CmpOp::Eq => CmpOp::Eq,
                        ast::CmpOp::NotEq => CmpOp::NotEq,
                        ast::CmpOp::Lt => CmpOp::Lt,
                        ast::CmpOp::LtE => CmpOp::LtEq,
                        ast::CmpOp::Gt => CmpOp::Gt,
                        ast::CmpOp::GtE => CmpOp::GtEq,
                        ast::CmpOp::Is => CmpOp::Is,
                        ast::CmpOp::IsNot => CmpOp::IsNot,
                        ast::CmpOp::In => CmpOp::In,
                        ast::CmpOp::NotIn => CmpOp::NotIn,
                    }
                };
                if comp.ops.len() != comp.comparators.len() {
                    return Err(self.error(expr.range(), "Invalid comparison expression"));
                }
                if comp.ops.len() == 1 {
                    let op = map_op(&comp.ops[0]);
                    ExprKind::Compare {
                        op,
                        left: Box::new(self.lower_expr(&comp.left)?),
                        right: Box::new(self.lower_expr(&comp.comparators[0])?),
                    }
                } else {
                    let mut ops = Vec::new();
                    for op in &comp.ops {
                        ops.push(map_op(op));
                    }
                    let mut comparators = Vec::new();
                    for cmp in &comp.comparators {
                        comparators.push(self.lower_expr(cmp)?);
                    }
                    ExprKind::CompareChain {
                        left: Box::new(self.lower_expr(&comp.left)?),
                        ops,
                        comparators,
                    }
                }
            }
            // List literal: [1, 2, 3]
            ast::Expr::List(list) => {
                let mut items = Vec::new();
                for elt in &list.elts {
                    items.push(self.lower_expr(elt)?);
                }
                ExprKind::List(items)
            }
            // Tuple literal: (1, 2, 3) or (1,) for single-element tuple
            ast::Expr::Tuple(tuple) => {
                let mut items = Vec::new();
                for elt in &tuple.elts {
                    items.push(self.lower_expr(elt)?);
                }
                ExprKind::Tuple(items)
            }
            // Set literal: {1, 2, 3}
            // Note: {} alone is an empty dict, not an empty set
            ast::Expr::Set(set_expr) => {
                let mut items = Vec::new();
                for elt in &set_expr.elts {
                    items.push(self.lower_expr(elt)?);
                }
                ExprKind::Set(items)
            }
            // Dict literal: {"a": 1, "b": 2}
            // Python allows dict unpacking: {**other_dict}
            // We reject this because it's runtime-dependent
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
            // Subscript: obj[index] or obj[start:end:step]
            // We distinguish between slicing (returns a new collection) and
            // indexing (returns a single element)
            ast::Expr::Subscript(sub) => match &*sub.slice {
                // Slice syntax: obj[start:end:step]
                // All parts are optional: obj[:], obj[1:], obj[:5], obj[::2]
                ast::Expr::Slice(slice) => {
                    let start = match &slice.lower {
                        Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                        None => None,
                    };
                    let end = match &slice.upper {
                        Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                        None => None,
                    };
                    let step = match &slice.step {
                        Some(expr) => Some(Box::new(self.lower_expr(expr)?)),
                        None => None,
                    };
                    ExprKind::Slice {
                        value: Box::new(self.lower_expr(&sub.value)?),
                        start,
                        end,
                        step,
                    }
                }
                _ => ExprKind::Index {
                    value: Box::new(self.lower_expr(&sub.value)?),
                    index: Box::new(self.lower_expr(&sub.slice)?),
                },
            },
            // List comprehension: [x * 2 for x in items if x > 0]
            ast::Expr::ListComp(listcomp) => {
                if listcomp.generators.is_empty() {
                    return Err(self.error(expr.range(), "Comprehension has no generators"));
                }
                let mut generators = Vec::with_capacity(listcomp.generators.len());
                for gen in &listcomp.generators {
                    if gen.is_async {
                        return Err(
                            self.error(expr.range(), "Async comprehensions are not supported")
                        );
                    }
                    let target = match &gen.target {
                        ast::Expr::Name(name) => self.ident(name.id.as_str()),
                        _ => {
                            return Err(self.error(
                                gen.target.range(),
                                "Only simple targets are supported in comprehensions",
                            ))
                        }
                    };
                    let iter = Box::new(self.lower_expr(&gen.iter)?);
                    let mut ifs = Vec::with_capacity(gen.ifs.len());
                    for cond in &gen.ifs {
                        ifs.push(self.lower_expr(cond)?);
                    }
                    generators.push(CompClause { target, iter, ifs });
                }
                let first = generators[0].clone();
                ExprKind::ListComp {
                    elt: Box::new(self.lower_expr(&listcomp.elt)?),
                    target: first.target,
                    iter: first.iter,
                    ifs: first.ifs,
                    generators,
                }
            }
            // Set comprehension: {x * 2 for x in items if x > 0}
            ast::Expr::SetComp(setcomp) => {
                if setcomp.generators.is_empty() {
                    return Err(self.error(expr.range(), "Comprehension has no generators"));
                }
                let mut generators = Vec::with_capacity(setcomp.generators.len());
                for gen in &setcomp.generators {
                    if gen.is_async {
                        return Err(
                            self.error(expr.range(), "Async comprehensions are not supported")
                        );
                    }
                    let target = match &gen.target {
                        ast::Expr::Name(name) => self.ident(name.id.as_str()),
                        _ => {
                            return Err(self.error(
                                gen.target.range(),
                                "Only simple targets are supported in comprehensions",
                            ))
                        }
                    };
                    let iter = Box::new(self.lower_expr(&gen.iter)?);
                    let mut ifs = Vec::with_capacity(gen.ifs.len());
                    for cond in &gen.ifs {
                        ifs.push(self.lower_expr(cond)?);
                    }
                    generators.push(CompClause { target, iter, ifs });
                }
                let first = generators[0].clone();
                ExprKind::SetComp {
                    elt: Box::new(self.lower_expr(&setcomp.elt)?),
                    target: first.target,
                    iter: first.iter,
                    ifs: first.ifs,
                    generators,
                }
            }
            // Dict comprehension: {key: value for ... in ... if ...}
            // Lower to dict([(key, value) for ...]) so we can reuse list-comp
            // and dict(iterable-of-pairs) machinery.
            ast::Expr::DictComp(dictcomp) => {
                if dictcomp.generators.is_empty() {
                    return Err(self.error(expr.range(), "Comprehension has no generators"));
                }
                let mut generators = Vec::with_capacity(dictcomp.generators.len());
                for gen in &dictcomp.generators {
                    if gen.is_async {
                        return Err(
                            self.error(expr.range(), "Async comprehensions are not supported")
                        );
                    }
                    let target = match &gen.target {
                        ast::Expr::Name(name) => self.ident(name.id.as_str()),
                        _ => {
                            return Err(self.error(
                                gen.target.range(),
                                "Only simple targets are supported in comprehensions",
                            ))
                        }
                    };
                    let iter = Box::new(self.lower_expr(&gen.iter)?);
                    let mut ifs = Vec::with_capacity(gen.ifs.len());
                    for cond in &gen.ifs {
                        ifs.push(self.lower_expr(cond)?);
                    }
                    generators.push(CompClause { target, iter, ifs });
                }
                let first = generators[0].clone();
                let pair_expr = Expr {
                    kind: ExprKind::Tuple(vec![
                        self.lower_expr(&dictcomp.key)?,
                        self.lower_expr(&dictcomp.value)?,
                    ]),
                    span,
                    ty: None,
                };
                let list_comp_expr = Expr {
                    kind: ExprKind::ListComp {
                        elt: Box::new(pair_expr),
                        target: first.target,
                        iter: first.iter,
                        ifs: first.ifs,
                        generators,
                    },
                    span,
                    ty: None,
                };
                let dict_name = Expr {
                    kind: ExprKind::Name("dict".to_string()),
                    span,
                    ty: None,
                };
                ExprKind::Call {
                    func: Box::new(dict_name),
                    args: vec![list_comp_expr],
                    keywords: Vec::new(),
                }
            }
            // Conditional expression (ternary): value_if_true if condition else value_if_false
            // Python's syntax is opposite of Rust's: Python puts test in middle, Rust at start
            ast::Expr::IfExp(ifexp) => ExprKind::IfExpr {
                test: Box::new(self.lower_expr(&ifexp.test)?),
                body: Box::new(self.lower_expr(&ifexp.body)?),
                orelse: Box::new(self.lower_expr(&ifexp.orelse)?),
            },
            // F-string: f"Hello {name}!"
            // We convert this to Rust's format! macro call.
            // Process involves:
            // 1. Build format string with {} placeholders
            // 2. Collect expressions to be formatted
            // 3. Map Python format specs to Rust format specs
            ast::Expr::JoinedStr(joined) => {
                let mut fmt = String::new();
                let mut args = Vec::new();
                // F-strings consist of literal parts and formatted value parts
                for value in &joined.values {
                    match value {
                        // Literal string part: "Hello " in f"Hello {name}!"
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
                        // Formatted value: {name} or {value:.2f} in f-string
                        ast::Expr::FormattedValue(fv) => {
                            // Python supports conversion flags:
                            // - !s -> str(...)
                            // - !r -> repr(...)
                            // - !a -> ascii(...)
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
                            let lowered = self.lower_expr(&fv.value)?;
                            let converted = match fv.conversion {
                                ast::ConversionFlag::None => lowered,
                                ast::ConversionFlag::Str => Expr {
                                    kind: ExprKind::Call {
                                        func: Box::new(Expr {
                                            kind: ExprKind::Name("str".to_string()),
                                            span: Span::from(value.range()),
                                            ty: Some(Type::Str),
                                        }),
                                        args: vec![lowered],
                                        keywords: Vec::new(),
                                    },
                                    span: Span::from(value.range()),
                                    ty: Some(Type::Str),
                                },
                                ast::ConversionFlag::Repr => Expr {
                                    kind: ExprKind::Call {
                                        func: Box::new(Expr {
                                            kind: ExprKind::Name("repr".to_string()),
                                            span: Span::from(value.range()),
                                            ty: Some(Type::Str),
                                        }),
                                        args: vec![lowered],
                                        keywords: Vec::new(),
                                    },
                                    span: Span::from(value.range()),
                                    ty: Some(Type::Str),
                                },
                                ast::ConversionFlag::Ascii => Expr {
                                    kind: ExprKind::Call {
                                        func: Box::new(Expr {
                                            kind: ExprKind::Name("ascii".to_string()),
                                            span: Span::from(value.range()),
                                            ty: Some(Type::Str),
                                        }),
                                        args: vec![lowered],
                                        keywords: Vec::new(),
                                    },
                                    span: Span::from(value.range()),
                                    ty: Some(Type::Str),
                                },
                            };
                            args.push(converted);
                        }
                        _ => return Err(self.error(value.range(), "Unsupported f-string element")),
                    }
                }
                // Build format!() call from collected parts
                // The format string becomes the first argument, expressions follow
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
                    keywords: Vec::new(),
                }
            }
            // Lambda expression: lambda x, y: x + y
            // Lambdas are anonymous functions with restricted syntax:
            // - No statements, only a single expression
            // - No type annotations on parameters
            // We reject advanced parameter features (defaults, *args, **kwargs)
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
                    params.push(self.ident(arg.def.arg.as_str()));
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

    pub(super) fn assign_target_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
        let span = Span::from(expr.range());
        let kind = match expr {
            ast::Expr::Name(name) => ExprKind::Name(self.ident(name.id.as_str())),
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
}
