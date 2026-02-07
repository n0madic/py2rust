use crate::diagnostic::{CompileError, Warning};
use crate::hir::*;
use crate::span::Span;
use crate::types::{Type, TypeRef};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

mod call;
mod check_class;
mod check_function;
mod context;
mod diag;
mod expr;
mod resolve;
mod scope;
mod signatures;
mod stmt;
mod throws;
mod type_ops;

pub use context::{ClassAttrInfo, ClassInfo, FunctionSig, PropertyInfo, TypeContext, UnionInfo};

/// Global scope tracking for top-level statements.
///
/// Python's `global` statement affects variable resolution. We track which
/// variables have been declared as global and detect uses before declaration.
#[derive(Debug, Default, Clone)]
struct GlobalScope {
    /// Variables explicitly declared with `global` statement
    declared: HashSet<String>,
    /// Variables used before being declared global (for error reporting)
    used_before_decl: HashMap<String, Span>,
}

/// Nonlocal scope tracking for nested functions.
///
/// This mirrors global tracking but resolves names to enclosing function scopes.
#[derive(Debug, Default, Clone)]
struct NonlocalScope {
    /// Variables explicitly declared with `nonlocal` statement
    declared: HashSet<String>,
    /// Variables used before being declared nonlocal (for error reporting)
    used_before_decl: HashMap<String, Span>,
}

/// The type checker performs type inference and validation on the HIR.
///
/// Type checking is a critical phase that:
/// 1. Resolves TypeRef annotations to concrete Type values
/// 2. Infers types for variables and expressions without annotations
/// 3. Validates type compatibility in assignments, function calls, operations
/// 4. Detects which functions can throw exceptions (for Result return types)
/// 5. Fills in the `ty` fields in HIR Expr nodes for codegen to use
///
/// The type checker maintains:
/// - TypeContext: Global registry of classes, unions, functions, and types
/// - Scope stack: For lexical scoping of local variables
/// - Global scope stack: For tracking Python's `global` declarations
/// - Lambda definitions: Inline lambda bodies that need type inference
///
/// Design notes:
/// - We use a simple scope stack rather than a symbol table because Python
///   has relatively simple scoping rules (no block scoping, just function scoping)
/// - Exception analysis is done in a separate pass AFTER type checking because
///   it needs complete type information to determine which operations can throw
/// - We track exception handler depth to properly handle bare `raise` statements
pub struct TypeChecker<'a> {
    source: &'a str,
    filename: &'a str,
    /// Type context containing all type information (output of type checking)
    ctx: TypeContext,
    /// Stack of local variable scopes (for nested functions, comprehensions, etc.)
    scopes: Vec<HashMap<String, Type>>,
    /// Stack of global scopes (for tracking `global` declarations)
    global_scopes: Vec<GlobalScope>,
    /// Stack of nonlocal scopes (for tracking `nonlocal` declarations)
    nonlocal_scopes: Vec<NonlocalScope>,
    /// Accumulated warnings (e.g., unused variables, potential issues)
    warnings: Vec<Warning>,
    /// Depth of nested exception handlers (for bare `raise` validation)
    except_handler_depth: usize,
    /// Lambda expression bodies (stored for deferred type inference)
    lambda_defs: HashMap<String, Expr>,
    /// Current class name when type checking methods, used for inference.
    current_class: Option<String>,
    /// Stack of scope indices marking the start of each function scope.
    function_scopes: Vec<usize>,
    /// Stack of inferred generator yield types for the current function nesting.
    generator_yield_stack: Vec<Option<Type>>,
}

impl<'a> TypeChecker<'a> {
    /// Create a new type checker and collect top-level signatures.
    ///
    /// Before we can type check function bodies, we need to know the signatures
    /// of all functions and the structure of all classes/unions. This first pass
    /// collects that information.
    ///
    /// We do this in two passes because:
    /// 1. Functions can call each other recursively
    /// 2. Classes can reference each other (e.g., Node having a List[Node] field)
    /// 3. Union variants must be previously-defined classes
    pub fn new(
        program: &Program,
        source: &'a str,
        filename: &'a str,
    ) -> Result<Self, CompileError> {
        let mut classes = HashMap::new();
        let mut unions = HashMap::new();
        let functions = HashMap::new();
        let globals = HashMap::new();

        // First pass: collect union definitions
        for item in &program.items {
            if let Item::Union(def) = item {
                unions.insert(
                    def.name.clone(),
                    UnionInfo {
                        name: def.name.clone(),
                        variants: def.variants.clone(),
                    },
                );
            }
        }

        // Second pass: collect class definitions (need to know about unions first)
        for item in &program.items {
            if let Item::Class(class_def) = item {
                classes.insert(
                    class_def.name.clone(),
                    ClassInfo {
                        name: class_def.name.clone(),
                        base: class_def.base.clone(),
                        fields: IndexMap::new(),
                        class_attrs: IndexMap::new(),
                        methods: HashMap::new(),
                        method_kinds: HashMap::new(),
                        properties: HashMap::new(),
                        init: None,
                        iter_return: None,
                        iter_item: None,
                        next_item: None,
                        match_args: class_def.match_args.clone(),
                    },
                );
            }
        }

        let mut checker = Self {
            source,
            filename,
            ctx: TypeContext {
                classes,
                unions,
                functions,
                globals,
            },
            scopes: Vec::new(),
            global_scopes: Vec::new(),
            nonlocal_scopes: Vec::new(),
            warnings: Vec::new(),
            except_handler_depth: 0,
            lambda_defs: HashMap::new(),
            current_class: None,
            function_scopes: Vec::new(),
            generator_yield_stack: Vec::new(),
        };

        // Third pass: collect function and class signatures (methods, fields)
        checker.collect_signatures(program)?;

        Ok(checker)
    }

    /// Main entry point for type checking a program.
    ///
    /// Type checking happens in several phases:
    /// 1. Check top-level statements (initializers, assignments)
    /// 2. Check all function bodies
    /// 3. Check all class methods
    /// 4. Run exception analysis to determine which functions can throw
    /// 5. Update function signatures with Result types if they can throw
    ///
    /// The order matters: we need to type check all code before we can
    /// analyze exception propagation, because exception analysis needs
    /// to know the types of all function calls.
    pub fn check_program(&mut self, program: &mut Program) -> Result<TypeContext, CompileError> {
        // Set up top-level scope with __name__
        self.scopes.push(HashMap::new());
        self.insert_var("__name__", Type::Str, Span::new(0, 0))?;

        // Type check top-level statements
        for item in &mut program.items {
            if let Item::Stmt(stmt) = item {
                self.check_stmt(stmt.as_mut(), None)?;
            }
        }

        // Save global variable types (excluding __name__ which is a constant)
        if let Some(scope) = self.scopes.last() {
            for (name, ty) in scope.iter() {
                if name.as_str() == "__name__" {
                    continue;
                }
                if matches!(ty, Type::Module(_) | Type::StdlibFunction { .. }) {
                    // Import bindings are compile-time only and must not become runtime globals.
                    continue;
                }
                self.ctx.globals.insert(name.clone(), ty.clone());
            }
        }

        // Type check all functions and classes
        for item in &mut program.items {
            match item {
                Item::Function(func) => self.check_function(func, None, false)?,
                Item::Class(class) => self.check_class(class)?,
                Item::Stmt(_) => {}
                Item::Union(_) => {}
            }
        }

        // Run exception analysis AFTER type checking
        // This determines which functions can throw exceptions and need Result return types
        let mut throw_analyzer = throws::ThrowAnalyzer::new(&self.ctx);
        let throw_map = throw_analyzer.analyze_program(program);

        // Update function signatures with exception information
        // Functions that can throw get their return type wrapped in Result<T, PyError>
        for (func_name, can_throw) in throw_map {
            if let Some(sig) = self.ctx.functions.get_mut(&func_name) {
                sig.can_throw = can_throw;
                if can_throw {
                    // Wrap return type in Result
                    sig.ret = sig
                        .ret
                        .clone()
                        .wrap_result(Type::Exception("PyError".to_string()));
                }
            }
        }

        self.scopes.pop();
        Ok(self.ctx.clone())
    }

    /// Return true when a name refers to a built-in Python exception type we support.
    pub(super) fn is_builtin_exception_name(name: &str) -> bool {
        matches!(
            name,
            "Exception"
                | "ValueError"
                | "TypeError"
                | "RuntimeError"
                | "KeyError"
                | "IndexError"
                | "AttributeError"
                | "ZeroDivisionError"
                | "NameError"
                | "AssertionError"
                | "StopIteration"
                | "NotImplementedError"
                | "IOError"
                | "OverflowError"
                | "GeneratorExit"
                | "MemoryError"
        )
    }

    /// Resolve a custom exception class name to its built-in root variant.
    ///
    /// For built-ins this returns the same name. For user-defined exceptions this walks
    /// the class inheritance chain until it reaches a built-in exception base.
    pub(super) fn resolve_exception_variant_name(&self, name: &str) -> Option<String> {
        if Self::is_builtin_exception_name(name) {
            return Some(name.to_string());
        }

        let mut current = name;
        let mut seen: HashSet<&str> = HashSet::new();
        loop {
            if !seen.insert(current) {
                // Defensive cycle guard for malformed inheritance graphs.
                return None;
            }
            let class_info = self.ctx.classes.get(current)?;
            let base = class_info.base.as_deref()?;
            if Self::is_builtin_exception_name(base) {
                return Some(base.to_string());
            }
            current = base;
        }
    }
}
