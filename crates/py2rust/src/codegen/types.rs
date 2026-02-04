use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn rust_type(&mut self, ty: &Type) -> String {
        match ty {
            Type::Int => "i64".to_string(),
            Type::Float => "f64".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Str => "String".to_string(),
            Type::None => "()".to_string(),
            Type::List(inner) => format!("Vec<{}>", self.rust_type(inner)),
            Type::Dict(k, v) => {
                self.uses.hash_map = true;
                format!("HashMap<{}, {}>", self.rust_type(k), self.rust_type(v))
            }
            Type::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| self.rust_type(t)).collect();
                if items.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }
            Type::Set(inner) => {
                self.uses.hash_set = true;
                format!("HashSet<{}>", self.rust_type(inner))
            }
            Type::Option(inner) => format!("Option<{}>", self.rust_type(inner)),
            Type::Custom(name) => name.clone(),
            Type::Union(name) => name.clone(),
            Type::Iterator(inner) => format!("impl Iterator<Item = {}>", self.rust_type(inner)),
            Type::Lambda { params, ret } => {
                let args: Vec<String> = params
                    .iter()
                    .map(|t| {
                        if matches!(t, Type::Unknown) {
                            "()".to_string()
                        } else {
                            self.rust_type(t)
                        }
                    })
                    .collect();
                let ret_ty = if matches!(ret.as_ref(), Type::Unknown) {
                    "()".to_string()
                } else {
                    self.rust_type(ret)
                };
                format!("impl Fn({}) -> {} + 'static", args.join(", "), ret_ty)
            }
            Type::Ref(inner) => {
                // Special case: &str instead of &String
                if matches!(inner.as_ref(), Type::Str) {
                    "&str".to_string()
                } else {
                    format!("&{}", self.rust_type(inner))
                }
            }
            Type::MutRef(inner) => format!("&mut {}", self.rust_type(inner)),
            Type::Slice(inner) => format!("&[{}]", self.rust_type(inner)),
            Type::Result(ok, err) => {
                format!("Result<{}, {}>", self.rust_type(ok), self.rust_type(err))
            }
            Type::Exception(name) => name.clone(),
            Type::Unknown => "_".to_string(),
        }
    }

    pub(crate) fn resolve_type_ref(&self, ty: &TypeRef, span: Span) -> Result<Type, CompileError> {
        match ty {
            TypeRef::Name(name) => Ok(match name.as_str() {
                "int" => Type::Int,
                "float" => Type::Float,
                "bool" => Type::Bool,
                "str" => Type::Str,
                _ => {
                    if self.ctx.unions.contains_key(name) {
                        Type::Union(name.clone())
                    } else if self.ctx.classes.contains_key(name) {
                        Type::Custom(name.clone())
                    } else {
                        return Err(self.error(span, format!("Unknown type: {name}")));
                    }
                }
            }),
            TypeRef::None => Ok(Type::None),
            TypeRef::List(inner) => Ok(Type::List(Box::new(self.resolve_type_ref(inner, span)?))),
            TypeRef::Dict(k, v) => Ok(Type::Dict(
                Box::new(self.resolve_type_ref(k, span)?),
                Box::new(self.resolve_type_ref(v, span)?),
            )),
            TypeRef::Tuple(items) => {
                let mut out = Vec::new();
                for item in items {
                    out.push(self.resolve_type_ref(item, span)?);
                }
                Ok(Type::Tuple(out))
            }
            TypeRef::Set(inner) => Ok(Type::Set(Box::new(self.resolve_type_ref(inner, span)?))),
            TypeRef::Optional(inner) => {
                Ok(Type::Option(Box::new(self.resolve_type_ref(inner, span)?)))
            }
            TypeRef::Iterator(inner) => Ok(Type::Iterator(Box::new(
                self.resolve_type_ref(inner, span)?,
            ))),
            TypeRef::Lambda { params, ret } => {
                let mut out = Vec::new();
                for param in params {
                    out.push(self.resolve_type_ref(param, span)?);
                }
                let ret_ty = self.resolve_type_ref(ret, span)?;
                Ok(Type::Lambda {
                    params: out,
                    ret: Box::new(ret_ty),
                })
            }
            TypeRef::Result(ok, err) => Ok(Type::Result(
                Box::new(self.resolve_type_ref(ok, span)?),
                Box::new(self.resolve_type_ref(err, span)?),
            )),
            TypeRef::Exception(name) => Ok(Type::Exception(name.clone())),
            TypeRef::Unknown => Ok(Type::Unknown),
            TypeRef::Union(parts) => {
                let mut has_none = false;
                let mut other = Vec::new();
                for part in parts {
                    let t = self.resolve_type_ref(part, span)?;
                    if matches!(t, Type::None) {
                        has_none = true;
                    } else {
                        other.push(t);
                    }
                }
                if has_none && other.len() == 1 {
                    Ok(Type::Option(Box::new(other.remove(0))))
                } else {
                    Err(self.error(span, "Inline unions are only allowed for Optional[T]"))
                }
            }
        }
    }
}
