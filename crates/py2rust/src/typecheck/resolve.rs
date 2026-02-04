use super::*;

/// Type reference resolution.
///
/// During lowering, we create TypeRef objects from Python type annotations.
/// During type checking, we need to resolve these to actual Type objects.
///
/// Why separate TypeRef and Type?
/// - TypeRef: What we parse from source ("List[int]", "Optional[str]")
/// - Type: What we use during checking (Type::List(Box::new(Type::Int)))
///
/// Resolution involves:
/// 1. Looking up class/union names in the type context
/// 2. Validating that generic parameters make sense
/// 3. Handling special cases (Optional = Union[T, None])
/// 4. Rejecting invalid uses (Iterator[T] only valid as return type)

impl<'a> TypeChecker<'a> {
    /// Resolve parameter type annotations.
    ///
    /// Validates that Iterator[T] is not used as a parameter type
    /// (only allowed as return type for generator functions).
    pub(super) fn resolve_params(&self, params: &[Param]) -> Result<Vec<Type>, CompileError> {
        let mut out = Vec::new();
        for p in params {
            let ty = self.resolve_type_ref(&p.ann, p.span)?;
            if matches!(ty, Type::Iterator(_)) {
                return Err(self.error(p.span, "Iterator[T] is only allowed as a return type"));
            }
            out.push(ty);
        }
        Ok(out)
    }

    /// Resolve a type reference to a concrete type.
    ///
    /// Handles:
    /// - Built-in types (int, float, bool, str)
    /// - User-defined classes and unions
    /// - Generic types (List, Dict, Tuple, Set, Optional)
    /// - Function types (Lambda)
    /// - Iterator types (only valid as return type)
    /// - Inline unions (only Optional[T] = T | None allowed)
    ///
    /// Why reject most inline unions?
    /// Rust doesn't have union types, only enums. We only support
    /// Union classes (which become Rust enums) and Optional (which becomes Option).
    pub(super) fn resolve_type_ref(&self, ty: &TypeRef, span: Span) -> Result<Type, CompileError> {
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
            TypeRef::Union(parts) => {
                // Inline union type: T | U | None
                // We only support T | None (which becomes Option<T>)
                // Other union combinations must use Union class syntax
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
                // Accept T | None (becomes Option<T>)
                if has_none && other.len() == 1 {
                    Ok(Type::Option(Box::new(other.remove(0))))
                } else {
                    // Reject A | B, A | B | C, etc.
                    Err(self.error(span, "Inline unions are only allowed for Optional[T]"))
                }
            }
            TypeRef::Result(ok, err) => Ok(Type::Result(
                Box::new(self.resolve_type_ref(ok, span)?),
                Box::new(self.resolve_type_ref(err, span)?),
            )),
            TypeRef::Exception(name) => Ok(Type::Exception(name.clone())),
            TypeRef::Unknown => Ok(Type::Unknown),
        }
    }
}
