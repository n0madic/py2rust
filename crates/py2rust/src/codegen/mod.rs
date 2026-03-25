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
    pub(crate) index_map: bool,
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
    pub(crate) py_input: bool,
    pub(crate) py_os_remove: bool,
    pub(crate) py_os_getcwd: bool,
    pub(crate) py_os_chdir: bool,
    pub(crate) py_os_mkdir: bool,
    pub(crate) py_os_listdir: bool,
    pub(crate) py_os_rmdir: bool,
    pub(crate) py_os_rename: bool,
    pub(crate) py_os_replace: bool,
    pub(crate) py_os_makedirs: bool,
    pub(crate) py_os_getenv: bool,
    pub(crate) py_os_environ: bool,
    pub(crate) py_os_name: bool,
    pub(crate) py_os_path_join: bool,
    pub(crate) py_os_path_exists: bool,
    pub(crate) py_os_path_is_file: bool,
    pub(crate) py_os_path_is_dir: bool,
    pub(crate) py_os_path_basename: bool,
    pub(crate) py_os_path_dirname: bool,
    pub(crate) py_os_path_split: bool,
    pub(crate) py_os_path_abspath: bool,
    pub(crate) py_sys_argv: bool,
    pub(crate) py_sys_intern: bool,
    pub(crate) py_re_search: bool,
    pub(crate) py_re_match: bool,
    pub(crate) py_re_sub: bool,
    pub(crate) py_re_group: bool,
    pub(crate) py_re_span: bool,
    pub(crate) py_json_dumps: bool,
    pub(crate) py_json_loads: bool,
    pub(crate) py_json_dump: bool,
    pub(crate) py_json_load: bool,
    pub(crate) py_math_factorial: bool,
    pub(crate) py_math_gcd: bool,
    pub(crate) py_math_lcm: bool,
    pub(crate) py_math_comb: bool,
    pub(crate) py_math_perm: bool,
    pub(crate) py_time_monotonic: bool,
    pub(crate) py_time_monotonic_ns: bool,
    pub(crate) py_time_perf_counter: bool,
    pub(crate) py_time_perf_counter_ns: bool,
    pub(crate) py_time_process_time: bool,
    pub(crate) py_time_process_time_ns: bool,
    pub(crate) py_time_sleep: bool,
    pub(crate) py_time_localtime: bool,
    pub(crate) py_time_gmtime: bool,
    pub(crate) py_time_strftime: bool,
    pub(crate) py_time_strptime: bool,
    pub(crate) py_subprocess_run: bool,
    pub(crate) py_urllib_urlparse: bool,
    pub(crate) py_urllib_quote: bool,
    pub(crate) py_urllib_unquote: bool,
    pub(crate) py_urllib_urljoin: bool,
    pub(crate) py_urllib_urlencode: bool,
    pub(crate) py_urllib_parse_qs: bool,
    pub(crate) py_urllib_parse_geturl: bool,
    pub(crate) py_urllib_urlopen: bool,
    pub(crate) py_urllib_response_read: bool,
    pub(crate) py_urllib_response_getcode: bool,
    pub(crate) py_urllib_response_geturl: bool,
    pub(crate) py_iter: bool,
    pub(crate) py_repr: bool,
    pub(crate) py_int: bool,
    pub(crate) py_bytes_from_len: bool,
    pub(crate) py_bytes_from_str: bool,
    /// Force-emits `PyError` support for generated control-flow that references it directly.
    pub(crate) py_error: bool,
    pub(crate) py_random_seed: bool,
    pub(crate) py_random_shuffle: bool,
    pub(crate) py_random_gauss: bool,
    pub(crate) py_random_choices: bool,
    /// Emit `NEXT_PY_ID` atomic counter for identity-based Hash/Eq on custom classes.
    pub(crate) needs_py_id: bool,
    /// Emit `use std::sync::atomic::*` for `Arc<Atomic*>` shared mutable scalar fields.
    pub(crate) shared_mutable_fields: bool,
}

/// Storage strategy for list values in generated Rust.
///
/// Local lists are represented as `Vec<T>` for zero-cost mutation,
/// while shared lists use either `Rc<RefCell<Vec<T>>>` for single-threaded
/// local aliasing or `Arc<Mutex<Vec<T>>>` for global/sync paths.
///
/// # Decision Strategy
///
/// A list is stored locally (`Vec<T>`) when:
/// - It's declared and used only within a single function scope
/// - It's not passed to functions that might store a reference
/// - It's not returned from functions
/// - It's not assigned to another variable that could alias it
///
/// A list uses shared cell storage (`Rc<RefCell<Vec<T>>>`) when:
/// - It escapes via return, function argument, or aliased assignment,
/// - but does not participate in global/static access.
///
/// A list uses shared sync storage (`Arc<Mutex<Vec<T>>>`) when:
/// - It's a global variable (accessed from multiple scopes), or
/// - It aliases with a global-bound list.
///
/// See `ListStorageAnalyzer` in `analysis.rs` for the escape analysis implementation.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum ListStorage {
    /// Non-escaping list stored as `Vec<T>`. Zero mutex overhead.
    Local,
    /// Shared list stored as `Rc<RefCell<Vec<T>>>` for local aliasing.
    SharedCell,
    /// Shared list stored as `Arc<Mutex<Vec<T>>>` for global/sync aliasing.
    SharedSync,
}

/// Storage strategy for dict values in generated Rust.
///
/// Local dicts are represented as `IndexMap<K, V>` for insertion-ordered semantics,
/// while shared dicts use either `Rc<RefCell<IndexMap<K, V>>>` for single-threaded
/// local aliasing or `Arc<Mutex<IndexMap<K, V>>>` for global/sync paths.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum DictStorage {
    /// Non-escaping dict stored as `IndexMap<K, V>`.
    Local,
    /// Shared dict stored as `Rc<RefCell<IndexMap<K, V>>>` for local aliasing.
    SharedCell,
    /// Shared dict stored as `Arc<Mutex<IndexMap<K, V>>>` for global/sync aliasing.
    SharedSync,
}

/// Shared interface for container storage strategies (list and dict).
///
/// Both `ListStorage` and `DictStorage` have identical Local/SharedCell/SharedSync
/// variants. This trait allows generic marker functions that operate on either type.
pub(crate) trait ContainerStorage: Copy + PartialEq + Eq {
    fn local() -> Self;
    fn shared_cell() -> Self;
    fn shared_sync() -> Self;
    fn is_shared_sync(self) -> bool;
}

impl ContainerStorage for ListStorage {
    fn local() -> Self {
        Self::Local
    }
    fn shared_cell() -> Self {
        Self::SharedCell
    }
    fn shared_sync() -> Self {
        Self::SharedSync
    }
    fn is_shared_sync(self) -> bool {
        self == Self::SharedSync
    }
}

impl ContainerStorage for DictStorage {
    fn local() -> Self {
        Self::Local
    }
    fn shared_cell() -> Self {
        Self::SharedCell
    }
    fn shared_sync() -> Self {
        Self::SharedSync
    }
    fn is_shared_sync(self) -> bool {
        self == Self::SharedSync
    }
}

/// Mark a variable as shared with single-threaded cell storage.
pub(crate) fn mark_shared_cell<S: ContainerStorage>(
    name: &str,
    storage: &mut HashMap<String, S>,
) {
    storage.insert(name.to_string(), S::shared_cell());
}

/// Mark a variable as shared with sync storage.
pub(crate) fn mark_shared_sync<S: ContainerStorage>(
    name: &str,
    storage: &mut HashMap<String, S>,
) {
    storage.insert(name.to_string(), S::shared_sync());
}

/// Mark a variable as shared based on whether it is global/sync-bound.
pub(crate) fn mark_shared_by_scope<S: ContainerStorage>(
    name: &str,
    shared_globals: &HashSet<String>,
    storage: &mut HashMap<String, S>,
) {
    if shared_globals.contains(name) {
        mark_shared_sync(name, storage);
    } else {
        mark_shared_cell(name, storage);
    }
}

/// Promote alias-connected variables; sync storage wins over cell storage.
pub(crate) fn promote_alias<S: ContainerStorage>(
    lhs: &str,
    rhs: &str,
    shared_globals: &HashSet<String>,
    storage: &mut HashMap<String, S>,
) {
    let lhs_sync =
        shared_globals.contains(lhs) || storage.get(lhs).copied().is_some_and(S::is_shared_sync);
    let rhs_sync =
        shared_globals.contains(rhs) || storage.get(rhs).copied().is_some_and(S::is_shared_sync);
    if lhs_sync || rhs_sync {
        mark_shared_sync(lhs, storage);
        mark_shared_sync(rhs, storage);
    } else {
        mark_shared_cell(lhs, storage);
        mark_shared_cell(rhs, storage);
    }
}

/// Mark a variable as local if it hasn't already been forced shared.
pub(crate) fn mark_local_if_absent<S: ContainerStorage>(
    name: &str,
    storage: &mut HashMap<String, S>,
) {
    storage.entry(name.to_string()).or_insert(S::local());
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
    /// Shared globals that are write-once scalars — use `OnceLock<T>` without Mutex.
    pub(crate) readonly_globals: HashSet<String>,
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
    /// Inferred dict key/value hints for the current function scope, if any.
    pub(crate) inferred_dict_kv: Option<HashMap<String, (Type, Type)>>,
    /// Inferred dict key/value hints for top-level statements.
    pub(crate) main_dict_kv: HashMap<String, (Type, Type)>,
    /// Storage strategy for list locals in the current function.
    pub(crate) local_list_storage: Option<HashMap<String, ListStorage>>,
    /// Storage strategy for list locals at top level (inside main).
    pub(crate) main_list_storage: HashMap<String, ListStorage>,
    /// Storage strategy for dict locals in the current function.
    pub(crate) local_dict_storage: Option<HashMap<String, DictStorage>>,
    /// Storage strategy for dict locals at top level (inside main).
    pub(crate) main_dict_storage: HashMap<String, DictStorage>,
    /// Lambda default expressions keyed by variable name.
    /// Populated when processing let/assign of lambda expressions.
    pub(crate) lambda_defaults: HashMap<String, Vec<Option<Expr>>>,
    /// Extra capture parameters for recursive nested functions.
    /// Maps function name -> list of captured variable names to pass as `&mut` args.
    pub(crate) recursive_fn_captures: HashMap<String, Vec<String>>,
    /// Capture params that are already `&mut` refs in the current scope.
    /// When set, recursive calls should pass these directly (not `&mut name`).
    pub(crate) already_mut_ref_captures: HashSet<String>,
    /// When true, empty lists with Unknown element types should omit the type
    /// suffix (`Vec::new()` instead of `Vec::<PyRepr>::new()`) to let Rust infer
    /// the element type from context (e.g. inside comprehension push() calls).
    pub(crate) infer_empty_list_type: bool,
    /// Functions that always return freshly-constructed lists (literals,
    /// comprehensions, list concat, etc.). These functions use `Vec<T>` return
    /// type instead of `Arc<Mutex<Vec<T>>>` to avoid unnecessary wrapping.
    pub(crate) fresh_return_functions: HashSet<String>,
    /// When true, list expressions should be generated with `ListStorage::Local`
    /// (i.e., plain `Vec<T>`) regardless of the normal storage strategy.
    /// Set temporarily during Return codegen for fresh-return functions.
    pub(crate) force_local_list_storage: bool,
    /// Functions with read-only list parameters that can be emitted as `&[T]`
    /// instead of `Arc<Mutex<Vec<T>>>`. Maps function name → set of read-only
    /// list param names.
    pub(crate) readonly_list_params: HashMap<String, HashSet<String>>,
}

/// Cached program-level analysis and item partitions used during emission.
struct ProgramFacts<'program> {
    unions: Vec<&'program UnionDef>,
    classes: Vec<&'program ClassDef>,
    functions: Vec<&'program Function>,
    top_level_stmts: Vec<&'program Stmt>,
    shared_globals: HashSet<String>,
    /// Shared globals that are assigned exactly once and have a scalar (Copy) type.
    /// These use `OnceLock<T>` without Mutex for zero-overhead reads.
    readonly_globals: HashSet<String>,
    /// Functions that always return freshly-constructed lists and can use `Vec<T>`
    /// return type instead of `Arc<Mutex<Vec<T>>>`.
    fresh_return_functions: HashSet<String>,
    /// Functions with read-only list parameters that can be emitted as `&[T]`.
    readonly_list_params: HashMap<String, HashSet<String>>,
    name_compare_only: bool,
    main_list_elems: HashMap<String, Type>,
    main_dict_kv: HashMap<String, (Type, Type)>,
    main_list_storage: HashMap<String, ListStorage>,
    main_dict_storage: HashMap<String, DictStorage>,
    /// Auto-generated inline union enums collected from the type context.
    inline_unions: Vec<Vec<Type>>,
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
            readonly_globals: HashSet::new(),
            global_overrides: Vec::new(),
            lambda_depth: 0,
            lambda_return_types: Vec::new(),
            class_defs: HashMap::new(),
            function_defs: HashMap::new(),
            name_overrides: Vec::new(),
            inferred_list_elems: None,
            main_list_elems: HashMap::new(),
            inferred_dict_kv: None,
            main_dict_kv: HashMap::new(),
            local_list_storage: None,
            main_list_storage: HashMap::new(),
            local_dict_storage: None,
            main_dict_storage: HashMap::new(),
            lambda_defaults: HashMap::new(),
            recursive_fn_captures: HashMap::new(),
            already_mut_ref_captures: HashSet::new(),
            infer_empty_list_type: false,
            fresh_return_functions: HashSet::new(),
            force_local_list_storage: false,
            readonly_list_params: HashMap::new(),
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

    /// Return the `Type` of a class field if it is a shared mutable field
    /// (i.e., one that needs `Arc<Atomic*>` storage).
    ///
    /// Returns `None` when the field is either unknown, not a shared mutable field,
    /// or the class itself is not found.
    pub(crate) fn shared_mutable_field_ty(&self, class_name: &str, field: &str) -> Option<Type> {
        let ci = self.ctx.classes.get(class_name)?;
        if ci.shared_mutable_fields.contains(field) {
            ci.fields.get(field).cloned()
        } else {
            None
        }
    }

    /// Collect lambda default expressions from a statement, storing them keyed
    /// by the assigned variable name for later use in call codegen.
    fn collect_lambda_defaults_from_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                if let ExprKind::Lambda { defaults, .. } = &value.kind {
                    if defaults.iter().any(|d| d.is_some()) {
                        self.lambda_defaults.insert(name.clone(), defaults.clone());
                    }
                }
            }
            StmtKind::Assign { target, value, .. } => {
                if let AssignTarget::Name(name) = target.as_ref() {
                    if let ExprKind::Lambda { defaults, .. } = &value.kind {
                        if defaults.iter().any(|d| d.is_some()) {
                            self.lambda_defaults.insert(name.clone(), defaults.clone());
                        }
                    }
                }
            }
            _ => {}
        }
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
                | "SyntaxError"
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
    ///    - Necessary imports (IndexMap, HashMap, HashSet, etc.)
    ///    - Global constant declarations (__NAME__)
    ///    - Helper function definitions
    ///
    /// Why generate code first, then inject headers?
    /// - We don't know which imports/helpers are needed until we scan the code
    /// - String building is easier when we can append, then prepend headers
    pub fn emit_program(mut self, program: &Program) -> Result<String, CompileError> {
        let facts = self.collect_program_facts(program);

        // Phase 1: Scan to determine which helpers are needed
        self.collect_uses(program)?;
        self.shared_globals = facts.shared_globals;
        self.readonly_globals = facts.readonly_globals;
        self.fresh_return_functions = facts.fresh_return_functions;
        self.readonly_list_params = facts.readonly_list_params;
        self.name_compare_only = facts.name_compare_only;

        // Phase 3: Generate code for all items
        // Generate unions first (they're enum definitions)
        for def in &facts.unions {
            self.emit_union(def)?;
        }

        // Generate auto-generated inline union enums
        for members in &facts.inline_unions {
            self.emit_inline_union(members)?;
        }

        // Generate classes (struct definitions + impl blocks)
        for class_def in &facts.classes {
            self.emit_class(class_def)?;
        }

        // Generate top-level functions
        for func in &facts.functions {
            self.emit_function(func, None)?;
        }

        self.main_list_elems = facts.main_list_elems;
        self.main_dict_kv = facts.main_dict_kv;
        self.main_list_storage = facts.main_list_storage;
        self.main_dict_storage = facts.main_dict_storage;
        self.emit_main(program, &facts.top_level_stmts)?;

        // Phase 4: Inject header and helpers before the generated code
        let generated_code = mem::take(&mut self.out);
        self.emit_header();
        self.emit_globals();
        self.emit_helpers();
        self.out.push_str(&generated_code);

        Ok(self.out)
    }

    /// Collect one-shot program facts and partitions used by code emission.
    fn collect_program_facts<'program>(
        &mut self,
        program: &'program Program,
    ) -> ProgramFacts<'program> {
        self.class_defs.clear();
        self.function_defs.clear();

        let mut unions = Vec::new();
        let mut classes = Vec::new();
        let mut functions = Vec::new();
        let mut top_level_stmts = Vec::new();

        for item in &program.items {
            match item {
                Item::Union(def) => unions.push(def),
                Item::Class(def) => {
                    self.class_defs.insert(def.name.clone(), def.clone());
                    classes.push(def);
                }
                Item::Function(func) => {
                    self.function_defs.insert(func.name.clone(), func.clone());
                    functions.push(func);
                }
                Item::Stmt(stmt) => top_level_stmts.push(stmt.as_ref()),
            }
        }

        // Collect lambda default expressions from top-level assignments.
        for stmt in &top_level_stmts {
            self.collect_lambda_defaults_from_stmt(stmt);
        }

        let shared_globals = self.collect_shared_globals(program);
        let readonly_globals = self.collect_readonly_globals(program, &shared_globals);
        let fresh_return_functions = self.detect_fresh_return_functions(program);
        let readonly_list_params = self.detect_readonly_list_params(program);
        let name_compare_only = self.analyze_name_compare_only(program);
        let main_list_elems = self.collect_list_elem_types_for_stmt_refs(&top_level_stmts);
        let main_dict_kv = self.collect_dict_kv_types_for_stmt_refs(&top_level_stmts);
        let main_list_storage =
            self.collect_list_storage_for_stmt_refs(&top_level_stmts, &shared_globals);
        let main_dict_storage =
            self.collect_dict_storage_for_stmt_refs(&top_level_stmts, &shared_globals);
        let inline_unions = Self::collect_inline_unions(self.ctx);

        ProgramFacts {
            unions,
            classes,
            functions,
            top_level_stmts,
            shared_globals,
            readonly_globals,
            fresh_return_functions,
            readonly_list_params,
            name_compare_only,
            main_list_elems,
            main_dict_kv,
            main_list_storage,
            main_dict_storage,
            inline_unions,
        }
    }

    fn collect_inline_unions(ctx: &TypeContext) -> Vec<Vec<Type>> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for sig in ctx.functions.values() {
            for param in &sig.params {
                Self::walk_type_for_inline_unions(param, &mut seen, &mut result);
            }
            Self::walk_type_for_inline_unions(&sig.ret, &mut seen, &mut result);
        }
        for ty in ctx.globals.values() {
            Self::walk_type_for_inline_unions(ty, &mut seen, &mut result);
        }
        for class_info in ctx.classes.values() {
            for ty in class_info.fields.values() {
                Self::walk_type_for_inline_unions(ty, &mut seen, &mut result);
            }
        }
        result
    }

    fn walk_type_for_inline_unions(ty: &Type, seen: &mut HashSet<String>, result: &mut Vec<Vec<Type>>) {
        match ty {
            Type::InlineUnion(members) => {
                let name = Type::inline_union_name(members);
                if seen.insert(name) {
                    result.push(members.clone());
                    for member in members {
                        Self::walk_type_for_inline_unions(member, seen, result);
                    }
                }
            }
            Type::List(inner) | Type::Set(inner) | Type::Option(inner) | Type::Iterator(inner)
            | Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => {
                Self::walk_type_for_inline_unions(inner, seen, result);
            }
            Type::Dict(k, v) | Type::Result(k, v) => {
                Self::walk_type_for_inline_unions(k, seen, result);
                Self::walk_type_for_inline_unions(v, seen, result);
            }
            Type::Tuple(items) => {
                for item in items {
                    Self::walk_type_for_inline_unions(item, seen, result);
                }
            }
            Type::Lambda { params, ret, .. } => {
                for p in params {
                    Self::walk_type_for_inline_unions(p, seen, result);
                }
                Self::walk_type_for_inline_unions(ret, seen, result);
            }
            _ => {}
        }
    }
}
