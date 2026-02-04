mod emit;
mod expr;
mod scan;
mod stmt;
mod types;
mod util;

use crate::diagnostic::CompileError;
use crate::hir::*;
use crate::span::Span;
use crate::typecheck::{ClassInfo, TypeContext};
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
    pub(crate) py_dict_get: bool,
    pub(crate) py_chr: bool,
    pub(crate) py_ord: bool,
    pub(crate) py_next: bool,
    pub(crate) py_str_slice: bool,
    pub(crate) py_str_slice_step: bool,
    pub(crate) py_list_slice_step: bool,
    pub(crate) py_iter: bool,
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
    /// Parameters that have been converted to borrowed types (e.g., &[T], &str, &HashMap)
    pub(crate) borrowed_params: HashSet<String>,
    /// Current function being emitted (for tracking if returns should be wrapped in Ok)
    pub(crate) current_function: Option<String>,
    /// Return type of current function (resolved), if any
    pub(crate) current_function_ret: Option<Type>,
    /// Return type when inside a try block with value returns
    pub(crate) try_block_return_type: Option<Type>,
    /// Local variable types for current function (function scope)
    pub(crate) local_vars: Option<HashMap<String, Type>>,
    /// Whether top-level main has exception handling
    pub(crate) top_level_can_throw: bool,
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
            local_vars: None,
            top_level_can_throw: false,
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
        // Phase 1: Scan to determine which helpers are needed
        self.collect_uses(program)?;

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
        self.emit_main(&top_level)?;

        // Phase 4: Inject header and helpers before the generated code
        let generated_code = mem::take(&mut self.out);
        self.emit_header();
        self.emit_globals();
        self.emit_helpers();
        self.out.push_str(&generated_code);

        Ok(self.out)
    }
}
