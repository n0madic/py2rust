use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    None,
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Set(Box<Type>),
    Option(Box<Type>),
    Custom(String),
    Union(String),
    Iterator(Box<Type>),
    Lambda { params: Vec<Type>, ret: Box<Type> },
    Ref(Box<Type>),               // &T (immutable reference)
    MutRef(Box<Type>),            // &mut T (mutable reference)
    Slice(Box<Type>),             // &[T] (slice reference)
    Result(Box<Type>, Box<Type>), // Result<T, E>
    Exception(String),            // PyError or custom exception
    Unknown,
}

impl Type {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Type::Option(_))
    }

    pub fn unwrap_option(&self) -> Option<&Type> {
        match self {
            Type::Option(inner) => Some(inner.as_ref()),
            _ => None,
        }
    }

    pub fn is_exception(&self) -> bool {
        matches!(self, Type::Exception(_))
    }

    pub fn unwrap_result(&self) -> Option<(&Type, &Type)> {
        match self {
            Type::Result(ok, err) => Some((ok.as_ref(), err.as_ref())),
            _ => None,
        }
    }

    pub fn wrap_result(self, error_type: Type) -> Type {
        Type::Result(Box::new(self), Box::new(error_type))
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::None => write!(f, "None"),
            Type::List(inner) => write!(f, "list[{inner}]"),
            Type::Dict(k, v) => write!(f, "dict[{k}, {v}]"),
            Type::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| t.to_string()).collect();
                write!(f, "tuple[{}]", parts.join(", "))
            }
            Type::Set(inner) => write!(f, "set[{inner}]"),
            Type::Option(inner) => write!(f, "Optional[{inner}]"),
            Type::Custom(name) => write!(f, "{name}"),
            Type::Union(name) => write!(f, "{name}"),
            Type::Iterator(inner) => write!(f, "Iterator[{inner}]"),
            Type::Lambda { .. } => write!(f, "lambda"),
            Type::Ref(inner) => write!(f, "&{inner}"),
            Type::MutRef(inner) => write!(f, "&mut {inner}"),
            Type::Slice(inner) => write!(f, "&[{inner}]"),
            Type::Result(ok, err) => write!(f, "Result[{ok}, {err}]"),
            Type::Exception(name) => write!(f, "{name}"),
            Type::Unknown => write!(f, "<unknown>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Name(String),
    List(Box<TypeRef>),
    Dict(Box<TypeRef>, Box<TypeRef>),
    Tuple(Vec<TypeRef>),
    Set(Box<TypeRef>),
    Optional(Box<TypeRef>),
    Union(Vec<TypeRef>),
    Iterator(Box<TypeRef>),
    Lambda {
        params: Vec<TypeRef>,
        ret: Box<TypeRef>,
    },
    Result(Box<TypeRef>, Box<TypeRef>),
    Exception(String),
    Unknown,
    None,
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeRef::Name(name) => write!(f, "{name}"),
            TypeRef::List(inner) => write!(f, "list[{inner}]"),
            TypeRef::Dict(k, v) => write!(f, "dict[{k}, {v}]"),
            TypeRef::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| t.to_string()).collect();
                write!(f, "tuple[{}]", parts.join(", "))
            }
            TypeRef::Set(inner) => write!(f, "set[{inner}]"),
            TypeRef::Optional(inner) => write!(f, "Optional[{inner}]"),
            TypeRef::Union(items) => {
                let parts: Vec<String> = items.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
            TypeRef::Iterator(inner) => write!(f, "Iterator[{inner}]"),
            TypeRef::Lambda { .. } => write!(f, "callable"),
            TypeRef::Result(ok, err) => write!(f, "Result[{ok}, {err}]"),
            TypeRef::Exception(name) => write!(f, "{name}"),
            TypeRef::Unknown => write!(f, "_"),
            TypeRef::None => write!(f, "None"),
        }
    }
}
