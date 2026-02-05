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
    pub(crate) py_list_index: bool,
    pub(crate) py_list_count: bool,
    pub(crate) py_dict_get: bool,
    pub(crate) py_chr: bool,
    pub(crate) py_ord: bool,
    pub(crate) py_next: bool,
    pub(crate) py_insert_index: bool,
    pub(crate) py_list_str: bool,
    pub(crate) py_str_slice: bool,
    pub(crate) py_str_slice_step: bool,
    pub(crate) py_list_slice_step: bool,
    pub(crate) py_iter: bool,
    pub(crate) py_repr: bool,
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

    /// Collect globals that must be emitted because they are shared with functions or helpers.
    pub(crate) fn collect_shared_globals(&self, program: &Program) -> HashSet<String> {
        let module_vars = self.collect_module_vars(program);
        let mut used_by_functions = HashSet::new();

        for item in &program.items {
            match item {
                Item::Function(func) => {
                    self.collect_used_globals_in_function(
                        func,
                        &module_vars,
                        &mut used_by_functions,
                    );
                }
                Item::Class(class_def) => {
                    for method in &class_def.methods {
                        self.collect_used_globals_in_function(
                            method,
                            &module_vars,
                            &mut used_by_functions,
                        );
                    }
                }
                Item::Stmt(_) | Item::Union(_) => {}
            }
        }

        // Internal globals (defaults, class attrs) aren't module vars but must be emitted.
        for name in self.ctx.globals.keys() {
            if !module_vars.contains(name) {
                used_by_functions.insert(name.clone());
            }
        }

        used_by_functions
    }

    /// Collect module-scope variable names from top-level statements.
    fn collect_module_vars(&self, program: &Program) -> HashSet<String> {
        let mut vars = HashSet::new();
        for item in &program.items {
            if let Item::Stmt(stmt) = item {
                self.collect_module_vars_from_stmt(stmt, &mut vars);
            }
        }
        vars
    }

    /// Walk a statement and record any names bound at module scope.
    fn collect_module_vars_from_stmt(&self, stmt: &Stmt, vars: &mut HashSet<String>) {
        fn record_target(target: &AssignTarget, vars: &mut HashSet<String>) {
            match target {
                AssignTarget::Name(name) => {
                    vars.insert(name.clone());
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    for item in items {
                        record_target(item, vars);
                    }
                }
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
            }
        }

        match &stmt.kind {
            StmtKind::Let { name, .. } => {
                vars.insert(name.clone());
            }
            StmtKind::Assign { target, .. } => {
                record_target(target, vars);
            }
            StmtKind::For { target, body, .. } => {
                for name in target.names() {
                    vars.insert(name.to_string());
                }
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::If { body, orelse, .. } => {
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
                for stmt in orelse {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::While { body, .. } => {
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    for binding in &case.bindings {
                        vars.insert(binding.clone());
                    }
                    for stmt in &case.body {
                        self.collect_module_vars_from_stmt(stmt, vars);
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for stmt in body {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
                for handler in handlers {
                    if let Some(name) = &handler.name {
                        vars.insert(name.clone());
                    }
                    for stmt in &handler.body {
                        self.collect_module_vars_from_stmt(stmt, vars);
                    }
                }
                for stmt in orelse {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
                for stmt in finalbody {
                    self.collect_module_vars_from_stmt(stmt, vars);
                }
            }
            StmtKind::Return { .. }
            | StmtKind::Global { .. }
            | StmtKind::Nonlocal { .. }
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Expr(_)
            | StmtKind::Assert { .. }
            | StmtKind::Raise { .. } => {}
        }
    }

    /// Collect local/global declarations for a statement list within a scope.
    fn collect_scope_locals(
        &self,
        stmts: &[Stmt],
        locals: &mut HashSet<String>,
        globals: &mut HashSet<String>,
    ) {
        fn record_target(
            target: &AssignTarget,
            locals: &mut HashSet<String>,
            globals: &HashSet<String>,
        ) {
            match target {
                AssignTarget::Name(name) => {
                    if !globals.contains(name) {
                        locals.insert(name.clone());
                    }
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    for item in items {
                        record_target(item, locals, globals);
                    }
                }
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
            }
        }

        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, .. } => {
                    if !globals.contains(name) {
                        locals.insert(name.clone());
                    }
                }
                StmtKind::Assign { target, .. } => {
                    record_target(target, locals, globals);
                }
                StmtKind::For { target, body, .. } => {
                    for name in target.names() {
                        if !globals.contains(name) {
                            locals.insert(name.to_string());
                        }
                    }
                    self.collect_scope_locals(body, locals, globals);
                }
                StmtKind::If { body, orelse, .. } => {
                    self.collect_scope_locals(body, locals, globals);
                    self.collect_scope_locals(orelse, locals, globals);
                }
                StmtKind::While { body, .. } => {
                    self.collect_scope_locals(body, locals, globals);
                }
                StmtKind::Match { cases, .. } => {
                    for case in cases {
                        for binding in &case.bindings {
                            if !globals.contains(binding) {
                                locals.insert(binding.clone());
                            }
                        }
                        self.collect_scope_locals(&case.body, locals, globals);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_scope_locals(body, locals, globals);
                    for handler in handlers {
                        if let Some(name) = &handler.name {
                            if !globals.contains(name) {
                                locals.insert(name.clone());
                            }
                        }
                        self.collect_scope_locals(&handler.body, locals, globals);
                    }
                    self.collect_scope_locals(orelse, locals, globals);
                    self.collect_scope_locals(finalbody, locals, globals);
                }
                StmtKind::Global { names } => {
                    for name in names {
                        globals.insert(name.clone());
                        // Global declarations override any prior local binding.
                        locals.remove(name);
                    }
                }
                StmtKind::Nonlocal { names } => {
                    for name in names {
                        // Nonlocal declarations remove the local binding in this scope.
                        locals.remove(name);
                    }
                }
                StmtKind::Return { .. }
                | StmtKind::Break
                | StmtKind::Continue
                | StmtKind::Expr(_)
                | StmtKind::Assert { .. }
                | StmtKind::Raise { .. } => {}
            }
        }
    }

    /// Find module globals referenced from a function or method body.
    fn collect_used_globals_in_function(
        &self,
        func: &Function,
        module_vars: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        let mut locals: HashSet<String> = func.params.iter().map(|p| p.name.clone()).collect();
        let mut globals = HashSet::new();
        self.collect_scope_locals(&func.body, &mut locals, &mut globals);

        // Explicit global declarations always require shared storage.
        for name in &globals {
            if module_vars.contains(name) {
                used.insert(name.clone());
            }
        }

        let outers = HashSet::new();
        self.collect_used_globals_in_stmts(
            &func.body,
            &locals,
            &outers,
            &globals,
            module_vars,
            used,
        );
    }

    /// Walk statements and record module globals used by expressions.
    fn collect_used_globals_in_stmts(
        &self,
        stmts: &[Stmt],
        locals: &HashSet<String>,
        outers: &HashSet<String>,
        globals: &HashSet<String>,
        module_vars: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { value, .. } => {
                    self.collect_used_globals_in_expr(
                        value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Assign { value, .. } => {
                    self.collect_used_globals_in_expr(
                        value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_used_globals_in_expr(
                        test,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        orelse,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::While { test, body } => {
                    self.collect_used_globals_in_expr(
                        test,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_used_globals_in_expr(
                        iter,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Expr(expr) => {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_used_globals_in_expr(
                        test,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    if let Some(expr) = msg {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_used_globals_in_expr(
                        subject,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    for case in cases {
                        self.collect_used_globals_in_stmts(
                            &case.body,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_used_globals_in_stmts(
                        body,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    for handler in handlers {
                        self.collect_used_globals_in_stmts(
                            &handler.body,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                    self.collect_used_globals_in_stmts(
                        orelse,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_stmts(
                        finalbody,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                    if let Some(expr) = cause {
                        self.collect_used_globals_in_expr(
                            expr,
                            locals,
                            outers,
                            globals,
                            module_vars,
                            used,
                        );
                    }
                }
                StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
            }
        }
    }

    /// Walk an expression tree and record module globals used by name resolution.
    fn collect_used_globals_in_expr(
        &self,
        expr: &Expr,
        locals: &HashSet<String>,
        outers: &HashSet<String>,
        globals: &HashSet<String>,
        module_vars: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        match &expr.kind {
            ExprKind::Name(name) => {
                // Treat module-level names as globals when referenced from non-local scopes.
                if module_vars.contains(name)
                    && (globals.contains(name)
                        || (!locals.contains(name) && !outers.contains(name)))
                {
                    used.insert(name.clone());
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                self.collect_used_globals_in_expr(func, locals, outers, globals, module_vars, used);
                for arg in args {
                    self.collect_used_globals_in_expr(
                        arg,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                for kw in keywords {
                    self.collect_used_globals_in_expr(
                        &kw.value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::Starred { value } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Attr { value, .. } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_used_globals_in_expr(left, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(
                    right,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.collect_used_globals_in_expr(
                    inner,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Compare { left, right, .. } => {
                self.collect_used_globals_in_expr(left, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(
                    right,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.collect_used_globals_in_expr(left, locals, outers, globals, module_vars, used);
                for cmp in comparators {
                    self.collect_used_globals_in_expr(
                        cmp,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for value in values {
                    self.collect_used_globals_in_expr(
                        value,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_used_globals_in_expr(
                        item,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_used_globals_in_expr(
                        k,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                    self.collect_used_globals_in_expr(
                        v,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
                self.collect_used_globals_in_expr(
                    index,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_used_globals_in_expr(
                    value,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
                if let Some(expr) = start {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                if let Some(expr) = end {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
                if let Some(expr) = step.as_deref() {
                    self.collect_used_globals_in_expr(
                        expr,
                        locals,
                        outers,
                        globals,
                        module_vars,
                        used,
                    );
                }
            }
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
            }
            | ExprKind::SetComp {
                elt,
                target,
                iter,
                ifs,
            } => {
                // The iterator expression is evaluated in the outer scope.
                self.collect_used_globals_in_expr(iter, locals, outers, globals, module_vars, used);
                // Comprehensions do not inherit `global` declarations.
                let empty_globals = HashSet::new();
                let mut comp_locals = HashSet::new();
                comp_locals.insert(target.clone());
                let mut comp_outers = outers.clone();
                comp_outers.extend(locals.iter().cloned());
                for cond in ifs {
                    self.collect_used_globals_in_expr(
                        cond,
                        &comp_locals,
                        &comp_outers,
                        &empty_globals,
                        module_vars,
                        used,
                    );
                }
                self.collect_used_globals_in_expr(
                    elt,
                    &comp_locals,
                    &comp_outers,
                    &empty_globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Lambda { params, body } => {
                let mut lambda_locals: HashSet<String> = params.iter().cloned().collect();
                let mut lambda_globals = HashSet::new();
                if let ExprKind::Block { stmts } = &body.kind {
                    self.collect_scope_locals(stmts, &mut lambda_locals, &mut lambda_globals);
                }
                let mut lambda_outers = outers.clone();
                lambda_outers.extend(locals.iter().cloned());
                self.collect_used_globals_in_expr(
                    body,
                    &lambda_locals,
                    &lambda_outers,
                    &lambda_globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_used_globals_in_expr(test, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(body, locals, outers, globals, module_vars, used);
                self.collect_used_globals_in_expr(
                    orelse,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Block { stmts } => {
                self.collect_used_globals_in_stmts(
                    stmts,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_used_globals_in_expr(
                    inner,
                    locals,
                    outers,
                    globals,
                    module_vars,
                    used,
                );
            }
            ExprKind::Literal(_) => {}
        }
    }

    /// Collect list element type hints for a block of statements.
    fn collect_list_elem_types_for_stmts(&self, stmts: &[Stmt]) -> HashMap<String, Type> {
        let mut inferred = HashMap::new();
        self.collect_list_elem_types_in_stmts(stmts, &mut inferred);
        inferred
    }

    /// Walk statements and record list element types inferred from assignments and calls.
    fn collect_list_elem_types_in_stmts(
        &self,
        stmts: &[Stmt],
        inferred: &mut HashMap<String, Type>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    self.note_list_assignment(name, value, inferred);
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_list_assignment(name, value, inferred);
                    }
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_list_elem_types_in_expr(test, inferred);
                    self.collect_list_elem_types_in_stmts(body, inferred);
                    self.collect_list_elem_types_in_stmts(orelse, inferred);
                }
                StmtKind::While { test, body } => {
                    self.collect_list_elem_types_in_expr(test, inferred);
                    self.collect_list_elem_types_in_stmts(body, inferred);
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_list_elem_types_in_expr(iter, inferred);
                    self.collect_list_elem_types_in_stmts(body, inferred);
                }
                StmtKind::Expr(expr) => {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_list_elem_types_in_expr(test, inferred);
                    if let Some(expr) = msg {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_list_elem_types_in_expr(subject, inferred);
                    for case in cases {
                        self.collect_list_elem_types_in_stmts(&case.body, inferred);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_list_elem_types_in_stmts(body, inferred);
                    for handler in handlers {
                        self.collect_list_elem_types_in_stmts(&handler.body, inferred);
                    }
                    self.collect_list_elem_types_in_stmts(orelse, inferred);
                    self.collect_list_elem_types_in_stmts(finalbody, inferred);
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                    if let Some(expr) = cause {
                        self.collect_list_elem_types_in_expr(expr, inferred);
                    }
                }
                StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
            }
        }
    }

    /// Track list element type assignments from direct list expressions.
    fn note_list_assignment(&self, name: &str, value: &Expr, inferred: &mut HashMap<String, Type>) {
        if let Some(Type::List(inner)) = value.ty.as_ref() {
            if !matches!(inner.as_ref(), Type::Unknown) && !inferred.contains_key(name) {
                inferred.insert(name.to_string(), (*inner.as_ref()).clone());
            }
        }
    }

    /// Walk expressions and record list element types inferred from list method calls.
    fn collect_list_elem_types_in_expr(&self, expr: &Expr, inferred: &mut HashMap<String, Type>) {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Attr { value, attr } = &func.kind {
                    if let ExprKind::Name(name) = &value.kind {
                        let elem_ty = match attr.as_str() {
                            "append" | "index" | "count" => {
                                args.first().and_then(|arg| arg.ty.clone())
                            }
                            "insert" => args.get(1).and_then(|arg| arg.ty.clone()),
                            "extend" => args
                                .first()
                                .and_then(|arg| arg.ty.as_ref())
                                .and_then(|ty| self.iter_item_type_hint(ty)),
                            _ => None,
                        };
                        if let Some(elem_ty) = elem_ty {
                            if !matches!(elem_ty, Type::Unknown) && !inferred.contains_key(name) {
                                inferred.insert(name.clone(), elem_ty);
                            }
                        }
                    }
                }
                self.collect_list_elem_types_in_expr(func, inferred);
                for arg in args {
                    self.collect_list_elem_types_in_expr(arg, inferred);
                }
                for kw in keywords {
                    self.collect_list_elem_types_in_expr(&kw.value, inferred);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_list_elem_types_in_expr(value, inferred);
            }
            ExprKind::Attr { value, .. } => {
                self.collect_list_elem_types_in_expr(value, inferred);
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_list_elem_types_in_expr(left, inferred);
                self.collect_list_elem_types_in_expr(right, inferred);
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.collect_list_elem_types_in_expr(inner, inferred);
            }
            ExprKind::Compare { left, right, .. } => {
                self.collect_list_elem_types_in_expr(left, inferred);
                self.collect_list_elem_types_in_expr(right, inferred);
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.collect_list_elem_types_in_expr(left, inferred);
                for cmp in comparators {
                    self.collect_list_elem_types_in_expr(cmp, inferred);
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for value in values {
                    self.collect_list_elem_types_in_expr(value, inferred);
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_list_elem_types_in_expr(item, inferred);
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_list_elem_types_in_expr(k, inferred);
                    self.collect_list_elem_types_in_expr(v, inferred);
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_list_elem_types_in_expr(value, inferred);
                self.collect_list_elem_types_in_expr(index, inferred);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_list_elem_types_in_expr(value, inferred);
                if let Some(expr) = start {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
                if let Some(expr) = end {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
                if let Some(expr) = step.as_deref() {
                    self.collect_list_elem_types_in_expr(expr, inferred);
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. }
            | ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_list_elem_types_in_expr(iter, inferred);
                self.collect_list_elem_types_in_expr(elt, inferred);
                for cond in ifs {
                    self.collect_list_elem_types_in_expr(cond, inferred);
                }
            }
            ExprKind::Lambda { body, .. } => {
                self.collect_list_elem_types_in_expr(body, inferred);
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_list_elem_types_in_expr(test, inferred);
                self.collect_list_elem_types_in_expr(body, inferred);
                self.collect_list_elem_types_in_expr(orelse, inferred);
            }
            ExprKind::Block { stmts } => {
                self.collect_list_elem_types_in_stmts(stmts, inferred);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_list_elem_types_in_expr(inner, inferred);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
    }

    /// Collect list storage strategies for a block of statements.
    ///
    /// This analysis is conservative: if a list can escape or be aliased, it
    /// is marked Shared and emitted as Arc<Mutex<Vec<T>>>. Only non-escaping
    /// lists initialized from fresh literals/comprehensions are marked Local.
    fn collect_list_storage_for_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
    ) -> HashMap<String, ListStorage> {
        let mut storage = HashMap::new();
        self.collect_list_storage_in_stmts(stmts, shared_globals, &mut storage);
        storage
    }

    /// Walk statements and record whether list locals can remain as Vec<T>.
    fn collect_list_storage_in_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    self.note_list_storage_assignment(name, value, shared_globals, storage);
                    // Alias assignment: let x = y
                    if let ExprKind::Name(src) = &value.kind {
                        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                            self.mark_list_shared(src, storage);
                            self.mark_list_shared(name, storage);
                        }
                    }
                    self.collect_list_storage_in_expr(
                        value,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_list_storage_assignment(name, value, shared_globals, storage);
                        if let ExprKind::Name(src) = &value.kind {
                            if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                                self.mark_list_shared(src, storage);
                                self.mark_list_shared(name, storage);
                            }
                        }
                    }
                    // Assigning a list into a container is an escape.
                    let ctx = match target {
                        AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {
                            ListUseContext::Escape
                        }
                        _ => ListUseContext::Value,
                    };
                    self.collect_list_storage_in_expr(value, ctx, shared_globals, storage);
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Escape,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_list_storage_in_expr(
                        test,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                    self.collect_list_storage_in_stmts(orelse, shared_globals, storage);
                }
                StmtKind::While { test, body } => {
                    self.collect_list_storage_in_expr(
                        test,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_list_storage_in_expr(
                        iter,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                }
                StmtKind::Expr(expr) => {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_list_storage_in_expr(
                        test,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    if let Some(expr) = msg {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_list_storage_in_expr(
                        subject,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    for case in cases {
                        self.collect_list_storage_in_stmts(&case.body, shared_globals, storage);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_list_storage_in_stmts(body, shared_globals, storage);
                    for handler in handlers {
                        self.collect_list_storage_in_stmts(&handler.body, shared_globals, storage);
                    }
                    self.collect_list_storage_in_stmts(orelse, shared_globals, storage);
                    self.collect_list_storage_in_stmts(finalbody, shared_globals, storage);
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    if let Some(expr) = cause {
                        self.collect_list_storage_in_expr(
                            expr,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
            }
        }
    }

    /// Record a list assignment and decide if it can stay local.
    fn note_list_storage_assignment(
        &self,
        name: &str,
        value: &Expr,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        if shared_globals.contains(name) {
            self.mark_list_shared(name, storage);
            return;
        }
        if !matches!(value.ty.as_ref(), Some(Type::List(_))) {
            return;
        }
        if self.is_fresh_list_expr(value) {
            self.mark_list_local_if_absent(name, storage);
        } else {
            self.mark_list_shared(name, storage);
        }
    }

    /// Determine if an expression creates a fresh list value.
    fn is_fresh_list_expr(&self, expr: &Expr) -> bool {
        matches!(expr.kind, ExprKind::List(_) | ExprKind::ListComp { .. })
    }

    /// Record list usage inside expressions, marking escapes conservatively.
    fn collect_list_storage_in_expr(
        &self,
        expr: &Expr,
        ctx: ListUseContext,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, ListStorage>,
    ) {
        match &expr.kind {
            ExprKind::Name(name) => {
                if matches!(ctx, ListUseContext::Escape)
                    && matches!(expr.ty.as_ref(), Some(Type::List(_)))
                {
                    self.mark_list_shared(name, storage);
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                let safe = self.call_is_list_safe(func);
                match &func.kind {
                    ExprKind::Attr { value, .. } => {
                        self.collect_list_storage_in_expr(
                            value,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    _ => {
                        self.collect_list_storage_in_expr(
                            func,
                            ListUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                let arg_ctx = if safe {
                    ListUseContext::Value
                } else {
                    ListUseContext::Escape
                };
                for arg in args {
                    self.collect_list_storage_in_expr(arg, arg_ctx, shared_globals, storage);
                }
                for kw in keywords {
                    self.collect_list_storage_in_expr(&kw.value, arg_ctx, shared_globals, storage);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Attr { value, .. } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_list_storage_in_expr(
                    left,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    right,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Unary { expr, .. } => {
                self.collect_list_storage_in_expr(
                    expr,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Compare { op, left, right } => {
                if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                    self.mark_identity_list_operand(left, storage);
                    self.mark_identity_list_operand(right, storage);
                }
                self.collect_list_storage_in_expr(
                    left,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    right,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::CompareChain {
                left,
                ops,
                comparators,
                ..
            } => {
                let mut prev = left.as_ref();
                for (op, cmp) in ops.iter().zip(comparators.iter()) {
                    if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                        self.mark_identity_list_operand(prev, storage);
                        self.mark_identity_list_operand(cmp, storage);
                    }
                    prev = cmp;
                }
                self.collect_list_storage_in_expr(
                    left,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cmp in comparators {
                    self.collect_list_storage_in_expr(
                        cmp,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for val in values {
                    self.collect_list_storage_in_expr(
                        val,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_list_storage_in_expr(
                        item,
                        ListUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_list_storage_in_expr(
                        k,
                        ListUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                    self.collect_list_storage_in_expr(
                        v,
                        ListUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    index,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_list_storage_in_expr(
                    value,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                if let Some(expr) = start {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = end {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = step {
                    self.collect_list_storage_in_expr(
                        expr,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.collect_list_storage_in_expr(
                    iter,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    elt,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_list_storage_in_expr(
                        cond,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_list_storage_in_expr(
                    iter,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    elt,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_list_storage_in_expr(
                        cond,
                        ListUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Lambda { body, .. } => {
                // Lambdas can escape; treat captured list uses as shared.
                self.collect_list_storage_in_expr(
                    body,
                    ListUseContext::Escape,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_list_storage_in_expr(
                    test,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    body,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_list_storage_in_expr(
                    orelse,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Block { stmts } => {
                self.collect_list_storage_in_stmts(stmts, shared_globals, storage);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_list_storage_in_expr(
                    inner,
                    ListUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Literal(_) => {}
        }
    }

    /// Mark list operands used in identity comparisons as shared.
    fn mark_identity_list_operand(&self, expr: &Expr, storage: &mut HashMap<String, ListStorage>) {
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            if let ExprKind::Name(name) = &expr.kind {
                self.mark_list_shared(name, storage);
            }
        }
    }

    /// Mark a list variable as shared.
    fn mark_list_shared(&self, name: &str, storage: &mut HashMap<String, ListStorage>) {
        storage.insert(name.to_string(), ListStorage::Shared);
    }

    /// Mark a list variable as local if it hasn't already been forced shared.
    fn mark_list_local_if_absent(&self, name: &str, storage: &mut HashMap<String, ListStorage>) {
        storage
            .entry(name.to_string())
            .or_insert(ListStorage::Local);
    }

    /// Decide whether a call is safe to treat list arguments as non-escaping.
    fn call_is_list_safe(&self, func: &Expr) -> bool {
        if let ExprKind::Name(name) = &func.kind {
            return matches!(
                name.as_str(),
                "len"
                    | "print"
                    | "enumerate"
                    | "zip"
                    | "map"
                    | "filter"
                    | "reversed"
                    | "all"
                    | "any"
                    | "min"
                    | "max"
                    | "sum"
                    | "list"
                    | "tuple"
                    | "set"
            );
        }
        false
    }

    /// Compute dict storage strategy for a statement list.
    fn collect_dict_storage_for_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
    ) -> HashMap<String, DictStorage> {
        let mut storage = HashMap::new();
        self.collect_dict_storage_in_stmts(stmts, shared_globals, &mut storage);
        storage
    }

    /// Walk statements and record whether dict locals can remain as HashMap.
    fn collect_dict_storage_in_stmts(
        &self,
        stmts: &[Stmt],
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Let { name, value, .. } => {
                    self.note_dict_storage_assignment(name, value, shared_globals, storage);
                    // Alias assignment: let x = y
                    if let ExprKind::Name(src) = &value.kind {
                        if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
                            self.mark_dict_shared(src, storage);
                            self.mark_dict_shared(name, storage);
                        }
                    }
                    self.collect_dict_storage_in_expr(
                        value,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Name(name) = target {
                        self.note_dict_storage_assignment(name, value, shared_globals, storage);
                        if let ExprKind::Name(src) = &value.kind {
                            if matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
                                self.mark_dict_shared(src, storage);
                                self.mark_dict_shared(name, storage);
                            }
                        }
                    }
                    // Assigning a dict into a container is an escape.
                    let ctx = match target {
                        AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {
                            DictUseContext::Escape
                        }
                        _ => DictUseContext::Value,
                    };
                    self.collect_dict_storage_in_expr(value, ctx, shared_globals, storage);
                }
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        self.collect_dict_storage_in_expr(
                            expr,
                            DictUseContext::Escape,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    self.collect_dict_storage_in_expr(
                        test,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_dict_storage_in_stmts(body, shared_globals, storage);
                    self.collect_dict_storage_in_stmts(orelse, shared_globals, storage);
                }
                StmtKind::While { test, body } => {
                    self.collect_dict_storage_in_expr(
                        test,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_dict_storage_in_stmts(body, shared_globals, storage);
                }
                StmtKind::For { iter, body, .. } => {
                    self.collect_dict_storage_in_expr(
                        iter,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    self.collect_dict_storage_in_stmts(body, shared_globals, storage);
                }
                StmtKind::Expr(expr) => {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                StmtKind::Assert { test, msg } => {
                    self.collect_dict_storage_in_expr(
                        test,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    if let Some(expr) = msg {
                        self.collect_dict_storage_in_expr(
                            expr,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::Match { subject, cases } => {
                    self.collect_dict_storage_in_expr(
                        subject,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                    for case in cases {
                        self.collect_dict_storage_in_stmts(&case.body, shared_globals, storage);
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    self.collect_dict_storage_in_stmts(body, shared_globals, storage);
                    for handler in handlers {
                        self.collect_dict_storage_in_stmts(&handler.body, shared_globals, storage);
                    }
                    self.collect_dict_storage_in_stmts(orelse, shared_globals, storage);
                    self.collect_dict_storage_in_stmts(finalbody, shared_globals, storage);
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        self.collect_dict_storage_in_expr(
                            expr,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    if let Some(expr) = cause {
                        self.collect_dict_storage_in_expr(
                            expr,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
            }
        }
    }

    /// Record a dict assignment and decide if it can stay local.
    fn note_dict_storage_assignment(
        &self,
        name: &str,
        value: &Expr,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        if shared_globals.contains(name) {
            self.mark_dict_shared(name, storage);
            return;
        }
        if !matches!(value.ty.as_ref(), Some(Type::Dict(_, _))) {
            return;
        }
        if self.is_fresh_dict_expr(value) {
            self.mark_dict_local_if_absent(name, storage);
        } else {
            self.mark_dict_shared(name, storage);
        }
    }

    /// Determine if an expression creates a fresh dict value.
    fn is_fresh_dict_expr(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Dict(_) => true,
            ExprKind::Call { func, .. } => matches!(
                func.kind,
                ExprKind::Name(ref name) if name == "dict"
            ),
            _ => false,
        }
    }

    /// Record dict usage inside expressions, marking escapes conservatively.
    fn collect_dict_storage_in_expr(
        &self,
        expr: &Expr,
        ctx: DictUseContext,
        shared_globals: &HashSet<String>,
        storage: &mut HashMap<String, DictStorage>,
    ) {
        match &expr.kind {
            ExprKind::Name(name) => {
                if matches!(ctx, DictUseContext::Escape)
                    && matches!(expr.ty.as_ref(), Some(Type::Dict(_, _)))
                {
                    self.mark_dict_shared(name, storage);
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                let safe = self.call_is_dict_safe(func);
                match &func.kind {
                    ExprKind::Attr { value, .. } => {
                        self.collect_dict_storage_in_expr(
                            value,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                    _ => {
                        self.collect_dict_storage_in_expr(
                            func,
                            DictUseContext::Value,
                            shared_globals,
                            storage,
                        );
                    }
                }
                let arg_ctx = if safe {
                    DictUseContext::Value
                } else {
                    DictUseContext::Escape
                };
                for arg in args {
                    self.collect_dict_storage_in_expr(arg, arg_ctx, shared_globals, storage);
                }
                for kw in keywords {
                    self.collect_dict_storage_in_expr(&kw.value, arg_ctx, shared_globals, storage);
                }
            }
            ExprKind::Starred { value } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Attr { value, .. } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_dict_storage_in_expr(
                    left,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    right,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Unary { expr, .. } => {
                self.collect_dict_storage_in_expr(
                    expr,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Compare { op, left, right } => {
                if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                    self.mark_identity_dict_operand(left, storage);
                    self.mark_identity_dict_operand(right, storage);
                }
                self.collect_dict_storage_in_expr(
                    left,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    right,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::CompareChain {
                left,
                ops,
                comparators,
                ..
            } => {
                let mut prev = left.as_ref();
                for (op, cmp) in ops.iter().zip(comparators.iter()) {
                    if matches!(op, CmpOp::Is | CmpOp::IsNot) {
                        self.mark_identity_dict_operand(prev, storage);
                        self.mark_identity_dict_operand(cmp, storage);
                    }
                    prev = cmp;
                }
                self.collect_dict_storage_in_expr(
                    left,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cmp in comparators {
                    self.collect_dict_storage_in_expr(
                        cmp,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for val in values {
                    self.collect_dict_storage_in_expr(
                        val,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.collect_dict_storage_in_expr(
                        item,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    self.collect_dict_storage_in_expr(
                        k,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                    self.collect_dict_storage_in_expr(
                        v,
                        DictUseContext::Escape,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Index { value, index } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    index,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.collect_dict_storage_in_expr(
                    value,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                if let Some(expr) = start {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = end {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
                if let Some(expr) = step {
                    self.collect_dict_storage_in_expr(
                        expr,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.collect_dict_storage_in_expr(
                    iter,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    elt,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_dict_storage_in_expr(
                        cond,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.collect_dict_storage_in_expr(
                    iter,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    elt,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                for cond in ifs {
                    self.collect_dict_storage_in_expr(
                        cond,
                        DictUseContext::Value,
                        shared_globals,
                        storage,
                    );
                }
            }
            ExprKind::Lambda { body, .. } => {
                // Lambdas can escape; treat captured dict uses as shared.
                self.collect_dict_storage_in_expr(
                    body,
                    DictUseContext::Escape,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.collect_dict_storage_in_expr(
                    test,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    body,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
                self.collect_dict_storage_in_expr(
                    orelse,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Block { stmts } => {
                self.collect_dict_storage_in_stmts(stmts, shared_globals, storage);
            }
            ExprKind::UnionCtor { inner, .. } => {
                self.collect_dict_storage_in_expr(
                    inner,
                    DictUseContext::Value,
                    shared_globals,
                    storage,
                );
            }
            ExprKind::Literal(_) => {}
        }
    }

    /// Mark dict operands used in identity comparisons as shared.
    fn mark_identity_dict_operand(&self, expr: &Expr, storage: &mut HashMap<String, DictStorage>) {
        if matches!(expr.ty.as_ref(), Some(Type::Dict(_, _))) {
            if let ExprKind::Name(name) = &expr.kind {
                self.mark_dict_shared(name, storage);
            }
        }
    }

    /// Mark a dict variable as shared.
    fn mark_dict_shared(&self, name: &str, storage: &mut HashMap<String, DictStorage>) {
        storage.insert(name.to_string(), DictStorage::Shared);
    }

    /// Mark a dict variable as local if it hasn't already been forced shared.
    fn mark_dict_local_if_absent(&self, name: &str, storage: &mut HashMap<String, DictStorage>) {
        storage
            .entry(name.to_string())
            .or_insert(DictStorage::Local);
    }

    /// Decide whether a call is safe to treat dict arguments as non-escaping.
    fn call_is_dict_safe(&self, func: &Expr) -> bool {
        if let ExprKind::Name(name) = &func.kind {
            return matches!(
                name.as_str(),
                "len"
                    | "print"
                    | "all"
                    | "any"
                    | "min"
                    | "max"
                    | "sum"
                    | "dict"
                    | "list"
                    | "tuple"
                    | "set"
            );
        }
        false
    }

    /// Analyze nonlocal declarations in a scope and determine which locals
    /// must be stored in `Rc<RefCell<_>>` for inner mutations.
    fn collect_nonlocal_info_for_stmts(&self, stmts: &[Stmt], params: &[String]) -> NonlocalInfo {
        fn collect_declares(
            stmts: &[Stmt],
            nonlocals: &mut HashSet<String>,
            globals: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Nonlocal { names } => {
                        for name in names {
                            nonlocals.insert(name.clone());
                        }
                    }
                    StmtKind::Global { names } => {
                        for name in names {
                            globals.insert(name.clone());
                        }
                    }
                    StmtKind::If { body, orelse, .. } => {
                        collect_declares(body, nonlocals, globals);
                        collect_declares(orelse, nonlocals, globals);
                    }
                    StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                        collect_declares(body, nonlocals, globals);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            collect_declares(&case.body, nonlocals, globals);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        collect_declares(body, nonlocals, globals);
                        for handler in handlers {
                            collect_declares(&handler.body, nonlocals, globals);
                        }
                        collect_declares(orelse, nonlocals, globals);
                        collect_declares(finalbody, nonlocals, globals);
                    }
                    _ => {}
                }
            }
        }

        fn record_target(
            target: &AssignTarget,
            locals: &mut HashSet<String>,
            skip: &HashSet<String>,
        ) {
            match target {
                AssignTarget::Name(name) => {
                    if !skip.contains(name) {
                        locals.insert(name.clone());
                    }
                }
                AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                    for item in items {
                        record_target(item, locals, skip);
                    }
                }
                AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
            }
        }

        fn collect_local_defs(
            stmts: &[Stmt],
            locals: &mut HashSet<String>,
            skip: &HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Let { name, .. } => {
                        if !skip.contains(name) {
                            locals.insert(name.clone());
                        }
                    }
                    StmtKind::Assign { target, .. } => {
                        record_target(target, locals, skip);
                    }
                    StmtKind::For { target, body, .. } => {
                        for name in target.names() {
                            if !skip.contains(name) {
                                locals.insert(name.to_string());
                            }
                        }
                        collect_local_defs(body, locals, skip);
                    }
                    StmtKind::If { body, orelse, .. } => {
                        collect_local_defs(body, locals, skip);
                        collect_local_defs(orelse, locals, skip);
                    }
                    StmtKind::While { body, .. } => {
                        collect_local_defs(body, locals, skip);
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            for binding in &case.bindings {
                                if !skip.contains(binding) {
                                    locals.insert(binding.clone());
                                }
                            }
                            collect_local_defs(&case.body, locals, skip);
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        collect_local_defs(body, locals, skip);
                        for handler in handlers {
                            if let Some(name) = &handler.name {
                                if !skip.contains(name) {
                                    locals.insert(name.clone());
                                }
                            }
                            collect_local_defs(&handler.body, locals, skip);
                        }
                        collect_local_defs(orelse, locals, skip);
                        collect_local_defs(finalbody, locals, skip);
                    }
                    _ => {}
                }
            }
        }

        fn visit_expr_for_lambdas(
            this: &Codegen,
            expr: &Expr,
            locals: &HashSet<String>,
            nonlocals: &HashSet<String>,
            globals: &HashSet<String>,
            cell_locals: &mut HashSet<String>,
            unresolved: &mut HashSet<String>,
        ) {
            match &expr.kind {
                ExprKind::Lambda { params, body } => {
                    if let ExprKind::Block { stmts } = &body.kind {
                        let info = this.collect_nonlocal_info_for_stmts(stmts, params);
                        for name in info.unresolved {
                            if locals.contains(&name) {
                                cell_locals.insert(name);
                            } else {
                                unresolved.insert(name);
                            }
                        }
                    }
                }
                ExprKind::Call {
                    func,
                    args,
                    keywords,
                } => {
                    visit_expr_for_lambdas(
                        this,
                        func,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    for arg in args {
                        visit_expr_for_lambdas(
                            this,
                            arg,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    for kw in keywords {
                        visit_expr_for_lambdas(
                            this,
                            &kw.value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Starred { value } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Attr { value, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        left,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        right,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Unary { expr, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        expr,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::CompareChain {
                    left, comparators, ..
                } => {
                    visit_expr_for_lambdas(
                        this,
                        left,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    for cmp in comparators {
                        visit_expr_for_lambdas(
                            this,
                            cmp,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::BoolOp { values, .. } => {
                    for value in values {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                    for item in items {
                        visit_expr_for_lambdas(
                            this,
                            item,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Dict(items) => {
                    for (k, v) in items {
                        visit_expr_for_lambdas(
                            this,
                            k,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_expr_for_lambdas(
                            this,
                            v,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::Index { value, index } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        index,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    visit_expr_for_lambdas(
                        this,
                        value,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    if let Some(start) = start.as_deref() {
                        visit_expr_for_lambdas(
                            this,
                            start,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    if let Some(end) = end.as_deref() {
                        visit_expr_for_lambdas(
                            this,
                            end,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    if let Some(step) = step.as_deref() {
                        visit_expr_for_lambdas(
                            this,
                            step,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::ListComp { elt, iter, ifs, .. }
                | ExprKind::SetComp { elt, iter, ifs, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        elt,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        iter,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    for cond in ifs {
                        visit_expr_for_lambdas(
                            this,
                            cond,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                }
                ExprKind::UnionCtor { inner, .. } => {
                    visit_expr_for_lambdas(
                        this,
                        inner,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::IfExpr { test, body, orelse } => {
                    visit_expr_for_lambdas(
                        this,
                        test,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        body,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                    visit_expr_for_lambdas(
                        this,
                        orelse,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Block { stmts } => {
                    visit_stmts_for_lambdas(
                        this,
                        stmts,
                        locals,
                        nonlocals,
                        globals,
                        cell_locals,
                        unresolved,
                    );
                }
                ExprKind::Name(_) | ExprKind::Literal(_) => {}
            }
        }

        fn visit_stmts_for_lambdas(
            this: &Codegen,
            stmts: &[Stmt],
            locals: &HashSet<String>,
            nonlocals: &HashSet<String>,
            globals: &HashSet<String>,
            cell_locals: &mut HashSet<String>,
            unresolved: &mut HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::Let { value, .. } => {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Assign { value, .. } => {
                        visit_expr_for_lambdas(
                            this,
                            value,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Return { value } => {
                        if let Some(expr) = value {
                            visit_expr_for_lambdas(
                                this,
                                expr,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::If { test, body, orelse } => {
                        visit_expr_for_lambdas(
                            this,
                            test,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            orelse,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::While { test, body } => {
                        visit_expr_for_lambdas(
                            this,
                            test,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::For { iter, body, .. } => {
                        visit_expr_for_lambdas(
                            this,
                            iter,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Expr(expr) => {
                        visit_expr_for_lambdas(
                            this,
                            expr,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Assert { test, msg } => {
                        visit_expr_for_lambdas(
                            this,
                            test,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        if let Some(msg) = msg {
                            visit_expr_for_lambdas(
                                this,
                                msg,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        visit_expr_for_lambdas(
                            this,
                            subject,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        for case in cases {
                            visit_stmts_for_lambdas(
                                this,
                                &case.body,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        visit_stmts_for_lambdas(
                            this,
                            body,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        for handler in handlers {
                            visit_stmts_for_lambdas(
                                this,
                                &handler.body,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                        visit_stmts_for_lambdas(
                            this,
                            orelse,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                        visit_stmts_for_lambdas(
                            this,
                            finalbody,
                            locals,
                            nonlocals,
                            globals,
                            cell_locals,
                            unresolved,
                        );
                    }
                    StmtKind::Raise { exc, cause } => {
                        if let Some(expr) = exc {
                            visit_expr_for_lambdas(
                                this,
                                expr,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                        if let Some(expr) = cause {
                            visit_expr_for_lambdas(
                                this,
                                expr,
                                locals,
                                nonlocals,
                                globals,
                                cell_locals,
                                unresolved,
                            );
                        }
                    }
                    StmtKind::Global { .. }
                    | StmtKind::Nonlocal { .. }
                    | StmtKind::Break
                    | StmtKind::Continue => {}
                }
            }
        }

        let mut nonlocal_decls = HashSet::new();
        let mut global_decls = HashSet::new();
        collect_declares(stmts, &mut nonlocal_decls, &mut global_decls);

        let mut local_defs: HashSet<String> = params.iter().cloned().collect();
        let mut skip: HashSet<String> = HashSet::new();
        for name in nonlocal_decls.iter().chain(global_decls.iter()) {
            skip.insert(name.clone());
        }
        collect_local_defs(stmts, &mut local_defs, &skip);

        let mut cell_locals = HashSet::new();
        let mut unresolved = HashSet::new();
        visit_stmts_for_lambdas(
            self,
            stmts,
            &local_defs,
            &nonlocal_decls,
            &global_decls,
            &mut cell_locals,
            &mut unresolved,
        );

        for name in nonlocal_decls.iter() {
            unresolved.insert(name.clone());
        }

        NonlocalInfo {
            nonlocal_decls,
            cell_locals,
            unresolved,
        }
    }
}

/// Nonlocal analysis result for a single scope.
#[derive(Default)]
struct NonlocalInfo {
    nonlocal_decls: HashSet<String>,
    cell_locals: HashSet<String>,
    unresolved: HashSet<String>,
}

/// List usage context for storage analysis.
#[derive(Copy, Clone, PartialEq, Eq)]
enum ListUseContext {
    /// Regular evaluation; list does not escape.
    Value,
    /// List value escapes and must be shared.
    Escape,
}

/// Dict usage context for storage analysis.
#[derive(Copy, Clone, PartialEq, Eq)]
enum DictUseContext {
    /// Regular evaluation; dict does not escape.
    Value,
    /// Dict value escapes and must be shared.
    Escape,
}
