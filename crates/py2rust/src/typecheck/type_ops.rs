use super::*;

/// Type operations and utilities.
///
/// This module provides:
/// 1. iter_item_type: Determine what type a for-loop yields
/// 2. merge_types: Combine type information from multiple sources
/// 3. type_to_ref: Convert concrete Type to TypeRef for annotations
/// 4. ensure_assignable: Check type compatibility
///
/// These are fundamental operations used throughout type checking.
impl<'a> TypeChecker<'a> {
    /// Determine the item type for iteration.
    ///
    /// For `for x in collection:`, what is the type of `x`?
    ///
    /// Handles:
    /// - Built-in collections (list, dict, tuple, set, str)
    /// - Iterator types
    /// - Custom iterator protocol (__iter__ and next)
    ///
    /// Why tuple iteration requires uniform types?
    /// Rust's for-loop requires a single item type. Python allows
    /// heterogeneous tuples but we can't express that in Rust's type system.
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
            Type::Bytes => Ok(Type::Int),
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

    /// Merge type information from two sources.
    ///
    /// Used for type inference when we have partial information:
    /// - Unknown + T = T (we learn T)
    /// - int + float = float (numeric promotion)
    /// - Lambda types merge parameter-wise
    ///
    /// Why this design?
    /// Type inference often gives us Unknown initially, then we learn
    /// more. We need to combine this information.
    pub(super) fn merge_types(left: Type, right: Type) -> Type {
        // Unknown absorbs any concrete type
        if matches!(left, Type::Unknown) {
            return right;
        }
        if matches!(right, Type::Unknown) {
            return left;
        }
        match (left, right) {
            // Lambda types: merge parameter and return types recursively
            (
                Type::Lambda {
                    params: left_params,
                    ret: left_ret,
                },
                Type::Lambda {
                    params: right_params,
                    ret: right_ret,
                },
            ) => {
                if left_params.len() != right_params.len() {
                    return Type::Lambda {
                        params: left_params,
                        ret: left_ret,
                    };
                }
                let params = left_params
                    .into_iter()
                    .zip(right_params)
                    .map(|(l, r)| Self::merge_types(l, r))
                    .collect();
                let ret = Box::new(Self::merge_types(*left_ret, *right_ret));
                Type::Lambda { params, ret }
            }
            (left, right) => {
                // Numeric types: int + float = float
                if left.is_numeric() && right.is_numeric() {
                    if matches!(left, Type::Float) || matches!(right, Type::Float) {
                        return Type::Float;
                    }
                    return Type::Int;
                }
                // Otherwise keep left side (no better option)
                left
            }
        }
    }

    /// Convert Type to TypeRef.
    ///
    /// Used when we infer a function's return type and need to
    /// update the function signature's TypeRef annotation.
    ///
    /// This is the inverse of resolve_type_ref.
    pub(super) fn type_to_ref(ty: &Type) -> TypeRef {
        match ty {
            Type::Int => TypeRef::Name("int".to_string()),
            Type::Float => TypeRef::Name("float".to_string()),
            Type::Bool => TypeRef::Name("bool".to_string()),
            Type::Str => TypeRef::Name("str".to_string()),
            Type::Bytes => TypeRef::Name("bytes".to_string()),
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
            Type::Result(ok, err) => TypeRef::Result(
                Box::new(Self::type_to_ref(ok)),
                Box::new(Self::type_to_ref(err)),
            ),
            Type::Exception(name) => TypeRef::Exception(name.clone()),
            Type::Unknown => TypeRef::Unknown,
        }
    }

    /// Check if actual type can be assigned to expected type.
    ///
    /// This is our type compatibility check. Allows:
    /// - Exact matches (int == int)
    /// - int -> float (numeric promotion)
    /// - T -> Optional[T] (auto-wrap in Some)
    /// - None -> Optional[T] (None works for any Optional)
    /// - Callable with matching signatures
    /// - Unknown matches anything (for inference)
    ///
    /// Why these rules?
    /// - Python allows int in float contexts (1 + 1.5 works)
    /// - Python's None is compatible with Optional types
    /// - We need to support gradual typing (Unknown)
    pub(super) fn ensure_assignable(
        &self,
        actual: &Type,
        expected: &Type,
        span: Span,
    ) -> Result<(), CompileError> {
        // Unknown types pass all checks (for gradual typing)
        if matches!(expected, Type::Unknown) || matches!(actual, Type::Unknown) {
            return Ok(());
        }
        // Exact match always works
        if expected == actual {
            return Ok(());
        }
        match (expected, actual) {
            // Numeric promotion: int can be used as float
            (Type::Float, Type::Int) => Ok(()),
            // None is assignable to any Optional type
            (Type::Option(_inner), Type::None) => Ok(()),
            // Optional to Optional: check inner types
            (Type::Option(expected_inner), Type::Option(actual_inner)) => {
                self.ensure_assignable(actual_inner, expected_inner, span)
            }
            // Auto-wrap in Optional: T is assignable to Optional[T]
            (Type::Option(inner), actual) => self.ensure_assignable(actual, inner, span),
            // Tuple types: allow length mismatch if expected is homogeneous.
            (Type::Tuple(expected_items), Type::Tuple(actual_items)) => {
                if expected_items.len() == actual_items.len() {
                    for (e, a) in expected_items.iter().zip(actual_items.iter()) {
                        self.ensure_assignable(a, e, span)?;
                    }
                    Ok(())
                } else if expected_items
                    .first()
                    .is_some_and(|first| expected_items.iter().all(|t| t == first))
                {
                    let elem = expected_items.first().expect("checked above").clone();
                    for a in actual_items.iter() {
                        self.ensure_assignable(a, &elem, span)?;
                    }
                    Ok(())
                } else {
                    Err(self.error(span, "Tuple length mismatch"))
                }
            }
            // Callable types: check parameter and return type compatibility
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
