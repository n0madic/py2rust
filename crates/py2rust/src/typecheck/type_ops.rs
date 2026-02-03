use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn iter_item_type(&self, ty: &Type, span: Span) -> Result<Type, CompileError> {
        match ty {
            Type::List(inner) => Ok(*inner.clone()),
            Type::Dict(key, _) => Ok(*key.clone()),
            Type::Tuple(items) => {
                if items.is_empty() {
                    Err(self.error(span, "Cannot iterate over empty tuple"))
                } else if items.iter().all(|t| t == &items[0]) {
                    Ok(items[0].clone())
                } else {
                    Err(self.error(span, "Tuple iteration requires uniform types"))
                }
            }
            Type::Set(inner) => Ok(*inner.clone()),
            Type::Str => Ok(Type::Str),
            Type::Iterator(inner) => Ok(*inner.clone()),
            Type::Unknown => Ok(Type::Unknown),
            Type::Custom(class_name) => {
                let class_info = self
                    .ctx
                    .classes
                    .get(class_name)
                    .ok_or_else(|| self.error(span, format!("Unknown class: {class_name}")))?;
                if let Some(item_ty) = &class_info.iter_item {
                    Ok(item_ty.clone())
                } else if let Some(iter_return) = &class_info.iter_return {
                    let iter_info = self.ctx.classes.get(iter_return).ok_or_else(|| {
                        self.error(span, format!("Unknown iterator class: {iter_return}"))
                    })?;
                    if let Some(next_ty) = &iter_info.next_item {
                        Ok(next_ty.clone())
                    } else {
                        Err(self.error(span, "Iterator class must define next() -> Optional[T]"))
                    }
                } else {
                    Err(self.error(span, "Type is not iterable"))
                }
            }
            _ => Err(self.error(span, "Type is not iterable")),
        }
    }
    pub(super) fn merge_types(left: Type, right: Type) -> Type {
        if matches!(left, Type::Unknown) {
            return right;
        }
        if matches!(right, Type::Unknown) {
            return left;
        }
        if left.is_numeric() && right.is_numeric() {
            if matches!(left, Type::Float) || matches!(right, Type::Float) {
                return Type::Float;
            }
            return Type::Int;
        }
        left
    }

    pub(super) fn type_to_ref(ty: &Type) -> TypeRef {
        match ty {
            Type::Int => TypeRef::Name("int".to_string()),
            Type::Float => TypeRef::Name("float".to_string()),
            Type::Bool => TypeRef::Name("bool".to_string()),
            Type::Str => TypeRef::Name("str".to_string()),
            Type::None => TypeRef::None,
            Type::List(inner) => TypeRef::List(Box::new(Self::type_to_ref(inner))),
            Type::Dict(k, v) => TypeRef::Dict(
                Box::new(Self::type_to_ref(k)),
                Box::new(Self::type_to_ref(v)),
            ),
            Type::Tuple(items) => TypeRef::Tuple(items.iter().map(Self::type_to_ref).collect()),
            Type::Set(inner) => TypeRef::Set(Box::new(Self::type_to_ref(inner))),
            Type::Option(inner) => TypeRef::Optional(Box::new(Self::type_to_ref(inner))),
            Type::Custom(name) => TypeRef::Name(name.clone()),
            Type::Union(name) => TypeRef::Name(name.clone()),
            Type::Iterator(inner) => TypeRef::Iterator(Box::new(Self::type_to_ref(inner))),
            Type::Lambda { params, ret } => TypeRef::Lambda {
                params: params.iter().map(Self::type_to_ref).collect(),
                ret: Box::new(Self::type_to_ref(ret)),
            },
            // Reference types are internal to codegen, convert to underlying type
            Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => Self::type_to_ref(inner),
            Type::Unknown => TypeRef::Unknown,
        }
    }

    pub(super) fn ensure_assignable(
        &self,
        actual: &Type,
        expected: &Type,
        span: Span,
    ) -> Result<(), CompileError> {
        if matches!(expected, Type::Unknown) || matches!(actual, Type::Unknown) {
            return Ok(());
        }
        if expected == actual {
            return Ok(());
        }
        match (expected, actual) {
            (Type::Float, Type::Int) => Ok(()),
            (Type::Option(_inner), Type::None) => Ok(()),
            (Type::Option(expected_inner), Type::Option(actual_inner)) => {
                self.ensure_assignable(actual_inner, expected_inner, span)
            }
            (Type::Option(inner), actual) => self.ensure_assignable(actual, inner, span),
            (
                Type::Lambda {
                    params: e_params,
                    ret: e_ret,
                },
                Type::Lambda {
                    params: a_params,
                    ret: a_ret,
                },
            ) => {
                if e_params.len() != a_params.len() {
                    return Err(self.error(span, "Callable arity mismatch"));
                }
                for (e, a) in e_params.iter().zip(a_params.iter()) {
                    if matches!(e, Type::Unknown) || matches!(a, Type::Unknown) {
                        continue;
                    }
                    if e != a {
                        return Err(self.error(span, "Callable parameter type mismatch"));
                    }
                }
                if !matches!(e_ret.as_ref(), Type::Unknown)
                    && !matches!(a_ret.as_ref(), Type::Unknown)
                    && e_ret.as_ref() != a_ret.as_ref()
                {
                    return Err(self.error(span, "Callable return type mismatch"));
                }
                Ok(())
            }
            _ => Err(self.error(
                span,
                format!("Type mismatch: expected {expected}, got {actual}"),
            )),
        }
    }
}
