use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_expr(&self, expr: &ast::Expr) -> Result<Expr, CompileError> {
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
                let is_dict_ctor =
                    matches!(&*call.func, ast::Expr::Name(name) if name.id.as_str() == "dict");
                if !call.keywords.is_empty() {
                    if !is_dict_ctor {
                        return Err(self.error(expr.range(), "Keyword arguments are not supported"));
                    }
                    if !call.args.is_empty() {
                        return Err(self.error(
                            expr.range(),
                            "dict() with positional and keyword args is not supported",
                        ));
                    }
                    let mut items = Vec::new();
                    for kw in &call.keywords {
                        let key = match &kw.arg {
                            Some(arg) => Expr {
                                kind: ExprKind::Literal(Literal::Str(arg.to_string())),
                                span,
                                ty: None,
                            },
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
                    ast::CmpOp::In => CmpOp::In,
                    ast::CmpOp::NotIn => CmpOp::NotIn,
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
