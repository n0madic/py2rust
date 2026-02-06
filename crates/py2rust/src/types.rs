use std::fmt;

/// The type system used during type checking and code generation.
///
/// This is a fully-resolved type (as opposed to TypeRef which is an AST-level type annotation).
/// During type checking, TypeRef annotations are resolved into Type values.
///
/// Key design decisions:
/// - `Str` maps to Rust `String`, not `&str`, to avoid lifetime complexity. This means
///   string literals are allocated, but it simplifies the type system dramatically.
/// - `Int` and `Float` map directly to `i64` and `f64` respectively, with explicit
///   suffix literals in codegen to avoid ambiguity.
/// - `Ref` and `MutRef` are used internally for method receivers and borrowing but are
///   not expressible in Python source annotations.
/// - `Lambda` types are emitted as `impl Fn(...) -> ... + 'static` in Rust to avoid
///   boxing overhead while supporting closure captures.
/// - `Unknown` is used during type inference for variables that haven't been resolved yet.
///   It's allowed locally but should be resolved before codegen.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Bytes,
    None,
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Set(Box<Type>),
    Option(Box<Type>),
    /// Imported stdlib module binding (for example: `import os`).
    Module(String),
    /// Imported stdlib callable binding (for example: `from os import remove`).
    StdlibFunction {
        module: String,
        method: String,
    },
    Custom(String),
    /// Union types are only allowed for enum-like classes (tagged unions).
    /// Inline union types like `int | str` are not supported except via Optional[T].
    Union(String),
    Iterator(Box<Type>),
    Lambda {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Ref(Box<Type>),    // &T (immutable reference)
    MutRef(Box<Type>), // &mut T (mutable reference)
    Slice(Box<Type>),  // &[T] (slice reference)
    /// Result types are used for functions that can raise exceptions.
    /// The Ok type is the normal return value, Err is always PyError.
    Result(Box<Type>, Box<Type>), // Result<T, E>
    /// Exception types represent Python exception classes.
    /// Used in except handlers and raise statements.
    Exception(String), // PyError or custom exception
    Unknown,
}

impl Type {
    /// Check if this type is numeric (int, float, or bool).
    /// Used for determining valid arithmetic operations.
    pub fn is_numeric(&self) -> bool {
        // Python treats bool as a subtype of int for arithmetic purposes.
        matches!(self, Type::Int | Type::Float | Type::Bool)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Type::Option(_))
    }

    /// Extract the inner type from Optional[T], if this is an optional type.
    pub fn unwrap_option(&self) -> Option<&Type> {
        match self {
            Type::Option(inner) => Some(inner.as_ref()),
            _ => None,
        }
    }

    pub fn is_exception(&self) -> bool {
        matches!(self, Type::Exception(_))
    }

    /// Extract Ok and Err types from Result<T, E>.
    /// Used in exception handling to determine function signatures.
    pub fn unwrap_result(&self) -> Option<(&Type, &Type)> {
        match self {
            Type::Result(ok, err) => Some((ok.as_ref(), err.as_ref())),
            _ => None,
        }
    }

    /// Wrap this type in Result<T, error_type>.
    /// Used when a function can raise exceptions - the normal return type
    /// becomes the Ok variant, and error_type (typically PyError) is the Err variant.
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
            Type::Bytes => write!(f, "bytes"),
            Type::None => write!(f, "None"),
            Type::List(inner) => write!(f, "list[{inner}]"),
            Type::Dict(k, v) => write!(f, "dict[{k}, {v}]"),
            Type::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| t.to_string()).collect();
                write!(f, "tuple[{}]", parts.join(", "))
            }
            Type::Set(inner) => write!(f, "set[{inner}]"),
            Type::Option(inner) => write!(f, "Optional[{inner}]"),
            Type::Module(name) => write!(f, "module[{name}]"),
            Type::StdlibFunction { module, method } => {
                write!(f, "stdlib_function[{module}.{method}]")
            }
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

/// TypeRef represents a type annotation as it appears in Python source code.
///
/// This is the AST-level representation before type resolution. During lowering,
/// Python type annotations are converted into TypeRef nodes. During type checking,
/// TypeRef nodes are resolved into actual Type values.
///
/// Key differences from Type:
/// - TypeRef can have Union of arbitrary types, while Type restricts Union to named enum classes
/// - TypeRef::Name is unresolved (could refer to a class, builtin, or type parameter)
/// - TypeRef includes None as a separate variant (used in function signatures)
/// - TypeRef::Unknown represents missing or inferred type annotations
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
    /// Unknown represents inferred types or missing annotations.
    /// The type checker will attempt to infer the actual type.
    Unknown,
    /// None as a type annotation (e.g., in return type `-> None`)
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
