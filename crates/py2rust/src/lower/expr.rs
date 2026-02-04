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
/// - List/dict/set comprehensions are simplified to single-loop form only

impl<'a> Lowerer<'a> {
    /// Lower a Python expression to HIR.
    ///
    /// This is the main entry point for expression lowering. It pattern matches
    /// on the RustPython AST node type and delegates to appropriate handling.
    pub(super) fn lower_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
        let span = Span::from(expr.range());
        let kind = match expr {
            // Simple name reference (variable, function name, etc.)
            ast::Expr::Name(name) => ExprKind::Name(name.id.to_string()),

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
                // Special case: dict(a=1, b=2) constructor syntax
                // This is more ergonomic in Python than {"a": 1, "b": 2}
                // We convert it to literal dict syntax in HIR
                let is_dict_ctor =
                    matches!(&*call.func, ast::Expr::Name(name) if name.id.as_str() == "dict");
                if !call.keywords.is_empty() {
                    // Only dict() supports keyword arguments (all other functions error)
                    if !is_dict_ctor {
                        return Err(self.error(expr.range(), "Keyword arguments are not supported"));
                    }
                    if !call.args.is_empty() {
                        return Err(self.error(
                            expr.range(),
                            "dict() with positional and keyword args is not supported",
                        ));
                    }
                    // Convert dict(a=1, b=2) to {"a": 1, "b": 2}
                    let mut items = Vec::new();
                    for kw in &call.keywords {
                        let key = match &kw.arg {
                            Some(arg) => Expr {
                                kind: ExprKind::Literal(Literal::Str(arg.to_string())),
                                span,
                                ty: None,
                            },
                            // **kwargs syntax not supported
                            None => {
                                return Err(
                                    self.error(expr.range(), "dict() does not support **kwargs")
                                )
                            }
                        };
                        let value = self.lower_expr(&kw.value)?;
                        items.push((key, value));
                    }
                    ExprKind::Dict(items)
                } else if is_dict_ctor && call.args.is_empty() {
                    // Empty dict() -> {}
                    ExprKind::Dict(Vec::new())
                } else {
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
            }
            // Attribute access: obj.field
            // Simple delegation to HIR - type checker will verify the attribute exists
            ast::Expr::Attribute(attr) => ExprKind::Attr {
                value: Box::new(self.lower_expr(&attr.value)?),
                attr: attr.attr.to_string(),
            },
            // Binary operations: +, -, *, /, %, //, **, |, &, ^
            // We map each Python operator to our HIR operator.
            // Note: We reject MatMult (@), LShift (<<), RShift (>>) because
            // they're rarely used and complicate codegen.
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
            // Unary operations: -x, not x
            // We only support negation and logical not.
            // Python's unary + and ~ are rejected.
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
            // Comparison operations
            ast::Expr::Compare(comp) => {
                // Python allows chained comparisons: a < b < c
                // We reject these because they complicate type checking and codegen.
                // Users must write: (a < b) and (b < c)
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
                    ast::CmpOp::In => CmpOp::In,
                    ast::CmpOp::NotIn => CmpOp::NotIn,
                };
                ExprKind::Compare {
                    op,
                    left: Box::new(self.lower_expr(&comp.left)?),
                    right: Box::new(self.lower_expr(&comp.comparators[0])?),
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
            // We only support single-loop comprehensions (no nested for)
            // Multiple conditions (if x > 0 if x < 10) are also rejected
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
            // Set comprehension: {x * 2 for x in items if x > 0}
            // We only support single-loop comprehensions (no nested for)
            ast::Expr::SetComp(setcomp) => {
                if setcomp.generators.len() != 1 {
                    return Err(self.error(
                        expr.range(),
                        "Only single-generator comprehensions are supported",
                    ));
                }
                let gen = &setcomp.generators[0];
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
                ExprKind::SetComp {
                    elt: Box::new(self.lower_expr(&setcomp.elt)?),
                    target,
                    iter,
                    ifs,
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
                            // Python allows !s (str) and !r (repr) conversions
                            // We only support !s and implicit (None)
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

    pub(super) fn assign_target_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
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
}
