use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn lower_type_ref(&self, expr: &ast::Expr) -> Result<TypeRef, CompileError> {
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

    pub(super) fn collect_union_type_refs(
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

    pub(super) fn extract_type_args(&self, expr: &ast::Expr) -> Result<Vec<TypeRef>, CompileError> {
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
}
