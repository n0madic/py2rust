use super::*;

/// Type checking context structures.
///
/// These structures hold all the type information we've gathered about the program:
/// - Functions and their signatures (params, return type, exception info)
/// - Classes and their fields/methods
/// - Union types (for pattern matching)
/// - Global variables and their types
///
/// Why separate structures?
/// - FunctionSig: Used for both top-level functions and methods
/// - ClassInfo: Comprehensive info about a class (fields, methods, iterator protocol)
/// - UnionInfo: Simple list of variant names for pattern matching
/// - TypeContext: Global registry of all type information

/// Function signature including exception information.
///
/// This represents both top-level functions and class methods.
/// The exception information (can_throw, thrown_exceptions) is used
/// to determine if callers need to return Result<T, PyError>.
#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub params: Vec<Type>,
    pub ret: Type,
    pub span: Span,
    pub can_throw: bool,
    pub thrown_exceptions: Vec<String>,
    pub defaults: usize,
}

/// Information about a class definition.
///
/// We support simple data classes (no inheritance, no complex methods).
/// Special support for iterator protocol:
/// - iter_return: If __iter__ is defined, what type does it return?
/// - iter_item: If __iter__ returns self, what's the item type?
/// - next_item: If __next__ is defined, what does it yield?
///
/// These are used to type-check for loops properly.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub base: Option<String>,
    pub fields: IndexMap<String, Type>,
    pub class_attrs: IndexMap<String, ClassAttrInfo>,
    pub methods: HashMap<String, FunctionSig>,
    pub method_kinds: HashMap<String, MethodKind>,
    pub properties: HashMap<String, PropertyInfo>,
    pub init: Option<FunctionSig>,
    pub iter_return: Option<String>,
    pub iter_item: Option<Type>,
    pub next_item: Option<Type>,
}

#[derive(Debug, Clone)]
pub struct ClassAttrInfo {
    pub ty: Type,
    pub global_name: String,
}

#[derive(Debug, Clone)]
pub struct PropertyInfo {
    pub getter: String,
    pub setter: Option<String>,
    pub ty: Type,
}

/// Union type information (for pattern matching).
///
/// Unions are detected during lowering (classes with Union base).
/// We only store variant names here; the actual variant structures
/// are stored as regular classes in TypeContext::classes.
///
/// Example:
/// class Result(Union):
///     class Ok: value: int
///     class Err: msg: str
///
/// Creates UnionInfo { name: "Result", variants: ["Ok", "Err"] }
#[derive(Debug, Clone)]
pub struct UnionInfo {
    pub name: String,
    pub variants: Vec<String>,
}

/// Global type context for the entire program.
///
/// This is built during the initial pass and then used during
/// expression/statement type checking. All lookups go through this.
#[derive(Debug, Clone)]
pub struct TypeContext {
    pub classes: HashMap<String, ClassInfo>,
    pub unions: HashMap<String, UnionInfo>,
    pub functions: HashMap<String, FunctionSig>,
    pub globals: HashMap<String, Type>,
}
