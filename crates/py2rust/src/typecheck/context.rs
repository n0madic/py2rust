use super::*;

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub params: Vec<Type>,
    pub ret: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub fields: IndexMap<String, Type>,
    pub methods: HashMap<String, FunctionSig>,
    pub init: Option<FunctionSig>,
    pub iter_return: Option<String>,
    pub iter_item: Option<Type>,
    pub next_item: Option<Type>,
}

#[derive(Debug, Clone)]
pub struct UnionInfo {
    pub name: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TypeContext {
    pub classes: HashMap<String, ClassInfo>,
    pub unions: HashMap<String, UnionInfo>,
    pub functions: HashMap<String, FunctionSig>,
    pub globals: HashMap<String, Type>,
}
