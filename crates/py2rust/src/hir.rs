use crate::span::Span;
use crate::types::{Type, TypeRef};
use std::collections::HashMap;

/// The High-level Intermediate Representation (HIR) for the program.
///
/// HIR sits between the RustPython AST and Rust codegen. It's designed to:
/// 1. Strip away Python-specific AST details that aren't needed for transpilation
/// 2. Normalize similar constructs (e.g., all assignments use AssignTarget)
/// 3. Provide stable anchor points for type information via `ty` fields
/// 4. Make the structure easier to traverse during type checking and codegen
///
/// The HIR is built by the Lowerer and consumed by TypeChecker and Codegen.
#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

/// Top-level items in a Python module.
///
/// Design note: We separate top-level statements into Item::Stmt because they need
/// special handling in codegen (they go into the generated `fn main()`), whereas
/// functions and classes become top-level Rust items.
#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Class(ClassDef),
    /// Union types are Python's enum-like tagged unions (e.g., `type Result = Ok | Err`)
    Union(UnionDef),
    /// Top-level statement that will be executed in `fn main()`
    Stmt(Box<Stmt>),
}

/// A union type definition (tagged union/enum).
///
/// Python: `type Status = Success | Failure`
/// Rust:   `enum Status { Success(SuccessData), Failure(FailureData) }`
///
/// Each variant must be a previously-defined class. The union becomes a Rust enum
/// where each variant wraps the corresponding class type.
#[derive(Debug, Clone)]
pub struct UnionDef {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeRef,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    /// Parameter kind controls call-site binding semantics.
    pub kind: ParamKind,
    pub ann: TypeRef,
    pub default: Option<Expr>,
    pub span: Span,
}

/// Function parameter kinds supported by the transpiler.
///
/// - PositionalOnly: parameter before `/` (`def f(x, /): ...`)
/// - PositionalOrKeyword: regular parameter (`def f(x): ...`)
/// - VarArgs: captures extra positional args (`def f(*args): ...`)
/// - KeywordOnly: parameter after `*` or `*args` (`def f(*, x): ...`)
/// - VarKeywords: captures extra keyword args (`def f(**kwargs): ...`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParamKind {
    PositionalOnly,
    PositionalOrKeyword,
    VarArgs,
    KeywordOnly,
    VarKeywords,
}

/// Class definition for data classes only.
///
/// We only support simple data classes (plain structs), not full OOP with inheritance,
/// polymorphism, etc. Methods are just functions that take `self` as the first parameter.
///
/// Python: `class Point: x: int; y: int`
/// Rust:   `struct Point { x: i64, y: i64 }`
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub base: Option<String>,
    pub fields: Vec<FieldDef>,
    pub class_attrs: Vec<ClassAttrDef>,
    pub methods: Vec<Function>,
    pub method_kinds: HashMap<String, MethodKind>,
    pub properties: Vec<PropertyDef>,
    /// Pattern matching field order (from __match_args__).
    /// If None, use field declaration order.
    pub match_args: Option<Vec<String>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassAttrDef {
    pub name: String,
    pub ann: Option<TypeRef>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Instance,
    Static,
    Class,
}

#[derive(Debug, Clone)]
pub struct PropertyDef {
    pub name: String,
    pub getter: String,
    pub setter: Option<String>,
    pub deleter: Option<String>,
    pub span: Span,
}

/// Statement in the HIR.
///
/// Each statement has a Span for error reporting and source mapping.
/// Unlike expressions, statements don't directly carry type information
/// (though their sub-expressions do).
#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

/// Import binding for `import module [as alias]`.
#[derive(Debug, Clone)]
pub struct ImportBinding {
    pub module: String,
    pub alias: Option<String>,
}

/// Import binding for `from module import name [as alias]`.
#[derive(Debug, Clone)]
pub struct ImportFromBinding {
    pub name: String,
    pub alias: Option<String>,
}

/// Statement kinds supported in the transpiler.
///
/// Design notes:
/// - Let vs Assign: Let introduces a new variable, Assign mutates existing.
///   This distinction is important for generating `let` vs bare assignment in Rust.
/// - Global: Tracks which variables should be treated as module-level globals.
///   Currently limited in scope but needed for `__name__` and similar.
/// - Try/Except: Exception handling is complex - see EXCEPTIONS.md for details.
///   Variables declared in try blocks are wrapped in Option<T> to be accessible
///   in else blocks without violating Rust's borrow checker.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum StmtKind {
    /// Variable declaration with optional type annotation.
    /// In Rust: `let name: Type = value`
    Let {
        name: String,
        ann: Option<TypeRef>,
        value: Expr,
    },
    /// Assignment to an existing variable, field, or index.
    /// In Rust: bare assignment without `let`
    Assign {
        target: Box<AssignTarget>,
        value: Expr,
    },
    /// Delete from a mutable container/attribute.
    /// Supported forms are validated later (e.g., `del list[idx]`, `del dict[key]`, `del obj.attr`).
    Delete {
        target: Box<AssignTarget>,
    },
    /// Local class definition inside a function body.
    ///
    /// This keeps the original class body semantics available to later passes while
    /// allowing scope-aware restrictions (for example, no closure capture from methods).
    Class {
        def: ClassDef,
    },
    Return {
        value: Option<Expr>,
    },
    If {
        test: Expr,
        body: Vec<Stmt>,
        orelse: Vec<Stmt>,
    },
    While {
        test: Expr,
        body: Vec<Stmt>,
    },
    For {
        target: ForTarget,
        iter: Expr,
        body: Vec<Stmt>,
    },
    /// Import module bindings.
    ///
    /// Examples:
    /// - `import os`
    /// - `import os as o`
    Import {
        names: Vec<ImportBinding>,
    },
    /// Import members from a module.
    ///
    /// Examples:
    /// - `from os import remove`
    /// - `from os import remove as rm`
    ImportFrom {
        module: String,
        names: Vec<ImportFromBinding>,
    },
    /// Global declaration (limited support)
    Global {
        names: Vec<String>,
    },
    /// Nonlocal declaration for closure variables.
    ///
    /// Python: `nonlocal x, y`
    /// Marks names that must resolve to an enclosing function scope.
    Nonlocal {
        names: Vec<String>,
    },
    Break,
    Continue,
    /// Expression statement (e.g., a function call for side effects)
    Expr(Expr),
    Assert {
        test: Expr,
        msg: Option<Expr>,
    },
    /// Pattern matching on union variants.
    /// Python: `match value: case Success(x): ...`
    /// Rust:   `match value { Status::Success(x) => ... }`
    Match {
        subject: Expr,
        cases: Vec<MatchCase>,
    },
    /// Exception handling try/except/else/finally.
    /// This is one of the most complex constructs because we need to:
    /// 1. Transform try block into a closure returning Result
    /// 2. Handle variable scoping across try/except/else
    /// 3. Support bare `raise` (re-raising current exception)
    /// 4. Ensure finally always runs via Drop guard
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
        orelse: Vec<Stmt>,
        finalbody: Vec<Stmt>,
    },
    /// Raise an exception.
    /// - exc: The exception to raise (None for bare `raise`)
    /// - cause: Exception chaining is NOT supported, will error if present
    Raise {
        exc: Option<Expr>,
        cause: Option<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct MatchCase {
    pub variant: String,
    /// Positional binding names in source order or keyword binding names in keyword order.
    pub bindings: Vec<String>,
    /// Field names for each binding when the source pattern uses keyword arguments.
    /// For positional patterns this is None and field order comes from __match_args__/declaration.
    pub binding_fields: Option<Vec<String>>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Exception handler (except clause).
///
/// - exc_types: The exception class names to catch (None means catch-all)
/// - name: Variable binding for the exception object (if provided)
/// - body: Handler body statements
#[derive(Debug, Clone)]
pub struct ExceptHandler {
    pub exc_types: Option<Vec<String>>,
    pub name: Option<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// For loop iteration target.
///
/// Represents what variables are bound on each iteration:
/// - Name: Simple variable binding (`for x in items`)
/// - Tuple: Tuple unpacking (`for (a, b) in pairs`)
#[derive(Debug, Clone)]
pub enum ForTarget {
    /// Simple variable name.
    Name(String),
    /// Tuple unpacking pattern (supports nesting).
    Tuple(Vec<String>),
}

impl ForTarget {
    /// Get all variable names bound by this target.
    pub fn names(&self) -> Vec<&str> {
        match self {
            ForTarget::Name(name) => vec![name.as_str()],
            ForTarget::Tuple(names) => names.iter().map(|s| s.as_str()).collect(),
        }
    }

    /// Returns true if this target contains the given variable name.
    pub fn contains_name(&self, search: &str) -> bool {
        match self {
            ForTarget::Name(name) => name == search,
            ForTarget::Tuple(names) => names.iter().any(|n| n == search),
        }
    }
}

/// Targets for assignment operations.
///
/// AssignTarget unifies different assignment forms:
/// - Simple: `x = value`
/// - Attribute: `obj.field = value`
/// - Index: `list[i] = value`
/// - Tuple/List unpacking: `(a, b) = value`, `[a, (b, c)] = value`
///
/// This makes codegen cleaner since we handle all assignment targets uniformly.
#[derive(Debug, Clone)]
pub enum AssignTarget {
    Name(String),
    Attr {
        value: Expr,
        attr: String,
    },
    Index {
        value: Expr,
        index: Expr,
    },
    /// Tuple unpacking target, supports nesting.
    Tuple(Vec<AssignTarget>),
    /// List unpacking target, supports nesting.
    List(Vec<AssignTarget>),
    /// Starred unpacking target (`*rest`) inside tuple/list unpacking.
    /// The wrapped target is usually a simple name.
    Starred(Box<AssignTarget>),
}

/// Expression in the HIR.
///
/// Each expression has:
/// - kind: The actual expression structure
/// - span: Source location for error reporting
/// - ty: Type information filled in by the type checker
///
/// The `ty` field is None during lowering and gets populated during type checking.
/// This allows us to thread type information through the HIR without modifying
/// its structure after type checking completes.
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub ty: Option<Type>,
}

/// Keyword argument in a call expression.
///
/// Python: `f(x=1)` has one keyword argument with `name=Some("x")` and `value=1`.
/// Python: `f(**m)` has one keyword argument with `name=None` and `value=m`.
#[derive(Debug, Clone)]
pub struct KeywordArg {
    pub name: Option<String>,
    pub value: Expr,
}

/// One `for ... in ... if ...` clause inside a comprehension.
///
/// Python comprehensions may contain multiple generator clauses:
/// `[x + y for x in xs for y in ys if y > 0]`.
/// Clauses are stored in source order.
#[derive(Debug, Clone)]
pub struct CompClause {
    pub target: String,
    /// Optional tuple target names for unpacking `for a, b in ...`.
    /// When set, `target` is a synthesized temp name and `tuple_targets`
    /// holds the individual names to destructure into.
    pub tuple_targets: Option<Vec<String>>,
    pub iter: Box<Expr>,
    pub ifs: Vec<Expr>,
}

/// One entry in a dict literal expression.
///
/// `Item` is a normal `key: value` pair, while `Unpack` preserves source-order
/// `**mapping` entries so codegen can apply CPython override semantics.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum DictEntry {
    Item { key: Expr, value: Box<Expr> },
    Unpack { value: Expr },
}

/// Expression kinds supported by the transpiler.
///
/// Design notes:
/// - Call is generic: it handles builtins (print, len), user functions, methods, and lambdas.
///   The type checker determines what kind of call it is based on the func type.
/// - Binary/Unary/Compare: Separated for clarity, though they could be unified.
///   This makes pattern matching in type checking and codegen more ergonomic.
/// - UnionCtor: Special constructor form for creating union variants.
///   Python: `Success(value)` where Success is a union variant
///   Rust:   `Status::Success(value)` where Status is the enum
/// - Block: Used for try blocks and lambdas. Contains statements that are executed
///   sequentially, with the last expression (if any) being the block's value.
#[derive(Debug, Clone)]
pub enum ExprKind {
    Literal(Literal),
    Name(String),
    /// Generator yield expression.
    ///
    /// Python: `yield value`
    /// - As a statement, it produces an iterator item.
    /// - As an expression, it also evaluates to the value provided by `send(...)`.
    Yield {
        value: Option<Box<Expr>>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        keywords: Vec<KeywordArg>,
    },
    /// Starred positional call argument (`*args` in a call expression).
    ///
    /// This node is only valid inside `ExprKind::Call.args`.
    Starred {
        value: Box<Expr>,
    },
    Attr {
        value: Box<Expr>,
        attr: String,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Compare {
        op: CmpOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Chained comparisons (e.g., a < b < c).
    /// Evaluates left-to-right with short-circuiting.
    CompareChain {
        left: Box<Expr>,
        ops: Vec<CmpOp>,
        comparators: Vec<Expr>,
    },
    BoolOp {
        op: BoolOp,
        values: Vec<Expr>,
    },
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Dict(Vec<DictEntry>),
    Set(Vec<Expr>),
    Index {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    /// Slice expression: value[start:end:step]
    /// All slice components are optional (e.g., `lst[:]` copies the list)
    Slice {
        value: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
    /// List comprehension: [elt for ... in ... if ...]
    ///
    /// `generators` stores all clauses in source order.
    /// `target`/`iter`/`ifs` mirror the first clause for compatibility with
    /// existing passes that only need the leading generator.
    ListComp {
        elt: Box<Expr>,
        target: String,
        iter: Box<Expr>,
        ifs: Vec<Expr>,
        generators: Vec<CompClause>,
    },
    /// Set comprehension: {elt for ... in ... if ...}
    ///
    /// `generators` stores all clauses in source order.
    /// `target`/`iter`/`ifs` mirror the first clause for compatibility with
    /// existing passes that only need the leading generator.
    SetComp {
        elt: Box<Expr>,
        target: String,
        iter: Box<Expr>,
        ifs: Vec<Expr>,
        generators: Vec<CompClause>,
    },
    /// Constructor call for a union variant.
    /// This is lowered from a Call expression when the function is determined
    /// to be a union variant name during type checking.
    UnionCtor {
        union: String,
        variant: String,
        inner: Box<Expr>,
    },
    /// Lambda/anonymous function.
    /// Python: `lambda x, y: x + y`
    /// Rust:   `|x, y| x + y` or `move |x, y| x + y`
    Lambda {
        params: Vec<String>,
        param_kinds: Vec<ParamKind>,
        has_defaults: Vec<bool>,
        defaults: Vec<Option<Expr>>,
        body: Box<Expr>,
    },
    /// Conditional expression (ternary).
    /// Python: `value if condition else other`
    /// Rust:   `if condition { value } else { other }`
    IfExpr {
        test: Box<Expr>,
        body: Box<Expr>,
        orelse: Box<Expr>,
    },
    /// Block expression containing statements.
    /// Used for try blocks and complex lambda bodies.
    /// The block's value is the value of the last expression (if any).
    Block {
        stmts: Vec<Stmt>,
    },
}

/// Literal values in Python.
///
/// Design note: We use i64/f64 for int/float to match Rust's default numeric types.
/// Larger integers would require BigInt support, which we don't currently provide.
/// String literals are stored as String (not &str) to simplify ownership.
#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    None,
}

/// Binary operators.
///
/// Note: Pow (**) is emitted as `i64::pow()` or `f64::powf()` in Rust,
/// not as a binary operator (Rust doesn't have one).
#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    FloorDiv,
    Mod,
    BitOr,
    BitAnd,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

/// Unary operators.
///
/// Not is logical negation (!)
/// Neg is arithmetic negation (-)
/// BitNot is bitwise NOT (~) for integers
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

/// Comparison operators.
///
/// Design notes:
/// - Is/IsNot: For checking object identity (maps to `ptr::eq` for None checks,
///   otherwise generates compile error since we don't expose object identity)
/// - In/NotIn: For membership testing (lists, sets, dicts, strings)
#[derive(Debug, Clone, Copy)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Is,
    IsNot,
    In,
    NotIn,
}

/// Boolean operators.
///
/// These are short-circuiting in Python and Rust.
/// And/Or map directly to && and ||.
#[derive(Debug, Clone, Copy)]
pub enum BoolOp {
    And,
    Or,
}
