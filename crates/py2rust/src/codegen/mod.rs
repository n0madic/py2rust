mod analysis;
mod emit;
mod expr;
mod scan;
mod stmt;
mod types;
mod util;

use crate::diagnostic::CompileError;
use crate::hir::*;
use crate::span::Span;
use crate::typecheck::{ClassInfo, PropertyInfo, TypeContext};
use crate::types::{Type, TypeRef};
use std::collections::{HashMap, HashSet};
use std::mem;

/// Tracks which helper functions and imports are needed in the generated code.
///
/// Rather than always emitting all possible helper functions, we scan the HIR
/// to determine which helpers are actually used and only emit those. This keeps
/// the generated Rust code minimal and clean.
///
/// Why this matters:
/// - Smaller generated code is easier to read and debug
/// - Unused code doesn't affect compilation but adds noise
/// - Each helper is injected inline rather than being in a separate crate,
///   so we want to minimize what we inject
#[derive(Default)]
pub(crate) struct Uses {
    pub(crate) print: bool,
    pub(crate) len: bool,
    pub(crate) range: bool,
    pub(crate) range2: bool,
    pub(crate) range3: bool,
    pub(crate) round: bool,
    pub(crate) hash_map: bool,
    pub(crate) hash_set: bool,
    pub(crate) type_name: bool,
    pub(crate) py_max: bool,
    pub(crate) py_min: bool,
    pub(crate) py_parse_int: bool,
    pub(crate) py_parse_float: bool,
    pub(crate) py_index: bool,
    pub(crate) py_list_get: bool,
    pub(crate) py_str_get: bool,
    pub(crate) py_list_index: bool,
    pub(crate) py_list_count: bool,
    pub(crate) py_dict_get: bool,
    pub(crate) py_chr: bool,
    pub(crate) py_ord: bool,
    pub(crate) py_next: bool,
    pub(crate) py_insert_index: bool,
    pub(crate) py_list_str: bool,
    pub(crate) py_float_str: bool,
    pub(crate) py_str_repr: bool,
    pub(crate) py_ascii: bool,
    pub(crate) py_string_methods: bool,
    pub(crate) py_str_slice: bool,
    pub(crate) py_str_slice_step: bool,
    pub(crate) py_list_slice_step: bool,
    pub(crate) py_file: bool,
    pub(crate) py_os_remove: bool,
    pub(crate) py_iter: bool,
    pub(crate) py_repr: bool,
    pub(crate) py_int: bool,
    pub(crate) py_bytes_from_len: bool,
    pub(crate) py_bytes_from_str: bool,
    /// Force-emits `PyError` support for generated control-flow that references it directly.
    pub(crate) py_error: bool,
}

/// Storage strategy for list values in generated Rust.
///
/// Local lists are represented as `Vec<T>` for zero-cost mutation,
/// while shared lists use `Arc<Mutex<Vec<T>>>` to preserve Python aliasing.
///
/// # Decision Strategy
///
/// A list is stored locally (`Vec<T>`) when:
/// - It's declared and used only within a single function scope
/// - It's not passed to functions that might store a reference
/// - It's not returned from functions
/// - It's not assigned to another variable that could alias it
///
/// A list uses shared storage (`Arc<Mutex<Vec<T>>>`) when:
/// - It's a global variable (accessed from multiple scopes)
/// - It escapes via return, function argument, or aliased assignment
/// - The escape analysis cannot prove it's safe to use local storage
///
/// See `ListStorageAnalyzer` in `analysis.rs` for the escape analysis implementation.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum ListStorage {
    /// Non-escaping list stored as `Vec<T>`. Zero mutex overhead.
    Local,
    /// Potentially shared list stored as `Arc<Mutex<Vec<T>>>`.
    Shared,
}

/// Storage strategy for dict values in generated Rust.
///
/// Local dicts are represented as `HashMap<K, V>` for zero-cost mutation,
/// while shared dicts use `Arc<Mutex<HashMap<K, V>>>` to preserve Python aliasing.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum DictStorage {
    /// Non-escaping dict stored as `HashMap<K, V>`.
    Local,
    /// Potentially shared dict stored as `Arc<Mutex<HashMap<K, V>>>`.
    Shared,
}

/// Iterator consumption context for optimization decisions.
///
/// Determines whether an iterator from Arc<Mutex<Vec<T>>> should acquire
/// the lock once for the entire iteration (ImmediateConsumption) or per-item
/// (DeferredCapture) to enable returning/storing the iterator.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum IterContext {
    /// Iterator is immediately consumed (for loops, enumerate, zip, all/any).
    /// Can hold the lock for the entire iteration.
    ImmediateConsumption,
    /// Iterator may be returned or stored (map/filter result, general expressions).
    /// Must lock per-iteration to avoid lifetime issues.
    DeferredCapture,
}

/// Iterator source plus any setup lines required to keep borrows/locks alive.
pub(crate) struct IterSource {
    /// Setup statements that must run before the iterator is consumed.
    pub(crate) setup: Vec<String>,
    /// Iterator expression to consume within the setup's scope.
    pub(crate) expr: String,
}

impl IterSource {
    /// Wrap a consumer expression so any required setup (e.g., list locks) stays in scope.
    pub(crate) fn wrap(self, body: String) -> String {
        if self.setup.is_empty() {
            return body;
        }
        // Keep iterator consumption within the same scope as any lock guard.
        let mut out = String::new();
        out.push('{');
        for line in self.setup {
            out.push(' ');
            out.push_str(&line);
            out.push(';');
        }
        out.push(' ');
        out.push_str(&body);
        out.push_str(" }");
        out
    }
}

/// The code generator transforms typed HIR into Rust source code.
///
/// Codegen is the final phase of compilation. At this point:
/// - All types have been inferred and filled into Expr.ty fields
/// - We know which functions can throw exceptions
/// - Helper injection points have been identified
///
/// Key design decisions:
///
/// 1. **No runtime crate**: All helpers are injected directly into generated code.
///    This makes the generated program self-contained and easier to distribute.
///
/// 2. **__name__ handling**: We detect if __name__ is only compared to literals
///    (common in `if __name__ == "__main__":`). If so, we emit const comparisons
///    without allocation. Otherwise we emit `.to_string()` on each access.
///
/// 3. **Numeric literals**: We suffix all numeric literals (42i64, 3.14f64) to
///    avoid type ambiguity in Rust. This is more verbose but prevents inference
///    errors.
///
/// 4. **Mixed arithmetic**: When mixing int and float in arithmetic, we cast
///    ints to f64 to match Python's behavior.
///
/// 5. **Exception handling**: Functions that can throw return Result<T, PyError>.
///    Return statements in such functions are wrapped in Ok(). Try blocks are
///    emitted as closures that return Result.
///
/// 6. **Borrowing**: We track which parameters should be borrowed (&str, &[T])
///    vs owned (String, Vec<T>) to generate idiomatic Rust.
pub struct Codegen<'a> {
    pub(crate) ctx: &'a TypeContext,
    pub(crate) source: &'a str,
    pub(crate) filename: &'a str,
    /// Output buffer where generated Rust code is accumulated
    pub(crate) out: String,
    /// Current indentation level (number of spaces)
    pub(crate) indent: usize,
    /// Counter for generating unique temporary variable names
    pub(crate) tmp_counter: usize,
    /// Tracks which helper functions need to be injected
    pub(crate) uses: Uses,
    /// True if __name__ is only compared to string literals (optimization)
    pub(crate) name_compare_only: bool,
    /// Parameters that have been converted to borrowed types (e.g., &[T], &str, &HashSet)
    pub(crate) borrowed_params: HashSet<String>,
    /// Current function being emitted (for tracking if returns should be wrapped in Ok)
    pub(crate) current_function: Option<String>,
    /// Return type of current function (resolved), if any
    pub(crate) current_function_ret: Option<Type>,
    /// Return type when inside a try block with value returns
    pub(crate) try_block_return_type: Option<Type>,
    /// True when try-return lowering uses `Result<Option<T>, PyError>` fallback semantics.
    pub(crate) try_block_returns_option: bool,
    /// Local variable types for current function (function scope)
    pub(crate) local_vars: Option<HashMap<String, Type>>,
    /// Names declared nonlocal in the current scope.
    pub(crate) nonlocal_decls: Option<HashSet<String>>,
    /// Locals that must be stored in Rc<RefCell<_>> due to nonlocal writes.
    pub(crate) cell_locals: Option<HashSet<String>>,
    /// Whether top-level main has exception handling
    pub(crate) top_level_can_throw: bool,
    /// Track which globals have been initialized in `main`.
    pub(crate) initialized_globals: HashSet<String>,
    /// Globals that must be emitted because they're shared with functions or helpers.
    pub(crate) shared_globals: HashSet<String>,
    /// Stack of temporary global name overrides for expression generation.
    pub(crate) global_overrides: Vec<(String, String)>,
    /// Track nested lambda emission to disable Result propagation inside closures.
    pub(crate) lambda_depth: usize,
    /// Expected return types for lambdas currently being emitted (innermost last).
    pub(crate) lambda_return_types: Vec<Option<Type>>,
    /// Map of class definitions for codegen lookups (defaults, super).
    pub(crate) class_defs: HashMap<String, ClassDef>,
    /// Map of top-level function definitions for default arguments.
    pub(crate) function_defs: HashMap<String, Function>,
    /// Stack of temporary name overrides for expression generation.
    pub(crate) name_overrides: Vec<(String, String)>,
    /// Inferred list element types for the current function scope, if any.
    pub(crate) inferred_list_elems: Option<HashMap<String, Type>>,
    /// Inferred list element types for top-level statements.
    pub(crate) main_list_elems: HashMap<String, Type>,
    /// Storage strategy for list locals in the current function.
    pub(crate) local_list_storage: Option<HashMap<String, ListStorage>>,
    /// Storage strategy for list locals at top level (inside main).
    pub(crate) main_list_storage: HashMap<String, ListStorage>,
    /// Storage strategy for dict locals in the current function.
    pub(crate) local_dict_storage: Option<HashMap<String, DictStorage>>,
    /// Storage strategy for dict locals at top level (inside main).
    pub(crate) main_dict_storage: HashMap<String, DictStorage>,
}

impl<'a> Codegen<'a> {
    pub fn new(ctx: &'a TypeContext, source: &'a str, filename: &'a str) -> Self {
        Self {
            ctx,
            source,
            filename,
            out: String::new(),
            indent: 0,
            tmp_counter: 0,
            uses: Uses::default(),
            name_compare_only: false,
            borrowed_params: HashSet::new(),
            current_function: None,
            current_function_ret: None,
            try_block_return_type: None,
            try_block_returns_option: false,
            local_vars: None,
            nonlocal_decls: None,
            cell_locals: None,
            top_level_can_throw: false,
            initialized_globals: HashSet::new(),
            shared_globals: HashSet::new(),
            global_overrides: Vec::new(),
            lambda_depth: 0,
            lambda_return_types: Vec::new(),
            class_defs: HashMap::new(),
            function_defs: HashMap::new(),
            name_overrides: Vec::new(),
            inferred_list_elems: None,
            main_list_elems: HashMap::new(),
            local_list_storage: None,
            main_list_storage: HashMap::new(),
            local_dict_storage: None,
            main_dict_storage: HashMap::new(),
        }
    }

    pub(crate) fn set_local_var_type(&mut self, name: &str, ty: Type) {
        if let Some(vars) = self.local_vars.as_mut() {
            vars.insert(name.to_string(), ty);
        }
    }

    pub(crate) fn local_var_type(&self, name: &str) -> Option<&Type> {
        self.local_vars.as_ref().and_then(|vars| vars.get(name))
    }

    /// Return true when a name refers to a built-in Python exception variant.
    pub(crate) fn is_builtin_exception_name(name: &str) -> bool {
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

    /// Resolve a user exception class to the built-in PyError variant it maps to.
    pub(crate) fn resolve_exception_variant_name(&self, name: &str) -> Option<String> {
        if Self::is_builtin_exception_name(name) {
            return Some(name.to_string());
        }

        let mut current = name;
        let mut seen: HashSet<&str> = HashSet::new();
        loop {
            if !seen.insert(current) {
                // Defensive cycle guard for malformed class inheritance.
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

    /// Main entry point for code generation.
    ///
    /// Code generation happens in multiple phases:
    ///
    /// 1. **Scan phase**: Traverse the HIR to determine which helpers are needed
    ///    (ranges, print, collections, etc.)
    ///
    /// 2. **__name__ analysis**: Determine if __name__ needs allocation or can
    ///    be optimized to const comparisons
    ///
    /// 3. **Emit phase**: Generate Rust code for unions, classes, functions, and
    ///    top-level statements (in that order)
    ///
    /// 4. **Header injection**: After generating all code, we prepend:
    ///    - `#![allow(dead_code, unused_variables, clippy::all)]`
    ///    - Necessary imports (HashMap, HashSet, etc.)
    ///    - Global constant declarations (__NAME__)
    ///    - Helper function definitions
    ///
    /// Why generate code first, then inject headers?
    /// - We don't know which imports/helpers are needed until we scan the code
    /// - String building is easier when we can append, then prepend headers
    pub fn emit_program(mut self, program: &Program) -> Result<String, CompileError> {
        // Cache class and function definitions for codegen lookups.
        for item in &program.items {
            if let Item::Class(def) = item {
                self.class_defs.insert(def.name.clone(), def.clone());
            }
            if let Item::Function(func) = item {
                self.function_defs.insert(func.name.clone(), func.clone());
            }
        }
        // Phase 1: Scan to determine which helpers are needed
        self.collect_uses(program)?;

        // Determine which globals must be emitted for cross-function access.
        self.shared_globals = self.collect_shared_globals(program);

        // Phase 2: Analyze __name__ usage
        self.name_compare_only = self.analyze_name_compare_only(program);

        // Phase 3: Generate code for all items
        // Generate unions first (they're enum definitions)
        for item in &program.items {
            if let Item::Union(def) = item {
                self.emit_union(def)?;
            }
        }

        // Generate classes (struct definitions + impl blocks)
        for item in &program.items {
            if let Item::Class(class_def) = item {
                self.emit_class(class_def)?;
            }
        }

        // Generate top-level functions
        for item in &program.items {
            if let Item::Function(func) = item {
                self.emit_function(func, None)?;
            }
        }

        // Collect top-level statements and generate main()
        let mut top_level = Vec::new();
        for item in &program.items {
            if let Item::Stmt(stmt) = item {
                top_level.push(stmt.as_ref().clone());
            }
        }
        // Capture list element type hints for top-level statements before codegen.
        self.main_list_elems = self.collect_list_elem_types_for_stmts(&top_level);
        // Compute list storage strategy for top-level locals.
        self.main_list_storage =
            self.collect_list_storage_for_stmts(&top_level, &self.shared_globals);
        // Compute dict storage strategy for top-level locals.
        self.main_dict_storage =
            self.collect_dict_storage_for_stmts(&top_level, &self.shared_globals);
        self.emit_main(program, &top_level)?;

        // Phase 4: Inject header and helpers before the generated code
        let generated_code = mem::take(&mut self.out);
        self.emit_header();
        self.emit_globals();
        self.emit_helpers();
        self.out.push_str(&generated_code);

        Ok(self.out)
    }
}
