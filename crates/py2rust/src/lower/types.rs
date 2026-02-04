use super::*;

impl<'a> Lowerer<'a> {
    /// Lower a Python type annotation expression to a TypeRef.
    ///
    /// Python type annotations can be:
    /// - Simple names: `int`, `str`, `MyClass`
    /// - Subscripted generics: `list[int]`, `dict[str, int]`, `Optional[str]`
    /// - Union types: `int | str` (Python 3.10+ syntax)
    /// - None type: `None` (for return types)
    ///
    /// We map Python types to their Rust equivalents during type checking.
    /// Here we just build the TypeRef AST representation.
    ///
    /// Design note: We allow TypeRef::Unknown for lambda type annotations
    /// because lambdas' types are usually inferred from context.
    pub(super) fn lower_type_ref(&self, expr: &ast::Expr) -> Result<TypeRef, CompileError> {
        match expr {
            ast::Expr::Name(name) => Ok(match name.id.as_ref() {
                "None" => TypeRef::None,
                _ => TypeRef::Name(name.id.to_string()),
            }),
            // Lambda in type position -> infer the type
            ast::Expr::Lambda(_) => Ok(TypeRef::Unknown),
            ast::Expr::Constant(cons) => match &cons.value {
                ast::Constant::None => Ok(TypeRef::None),
                // String literals as types (for forward references)
                ast::Constant::Str(s) => Ok(TypeRef::Name(s.to_string())),
                _ => Err(self.error(expr.range(), "Unsupported type annotation literal")),
            },
            // Generic types: list[T], dict[K, V], Optional[T], etc.
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
            // Union type with | operator: int | str
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

    /// Recursively collect types from a union expression (A | B | C).
    ///
    /// The parser represents `A | B | C` as nested BinOp nodes:
    /// BinOp(BinOp(A, |, B), |, C)
    ///
    /// We flatten this into a Vec of TypeRefs.
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

    /// Extract type arguments from a subscript slice.
    ///
    /// Handles both single arguments (list[int]) and tuple arguments (dict[str, int]).
    /// Python represents dict[str, int] as a tuple in the slice position.
    pub(super) fn extract_type_args(&self, expr: &ast::Expr) -> Result<Vec<TypeRef>, CompileError> {
        match expr {
            ast::Expr::Tuple(tuple) => {
                let mut args = Vec::new();
                for elt in &tuple.elts {
                    args.push(self.lower_type_ref(elt)?);
                }
                Ok(args)
            }
            // Single argument: list[int]
            _ => Ok(vec![self.lower_type_ref(expr)?]),
        }
    }
}
