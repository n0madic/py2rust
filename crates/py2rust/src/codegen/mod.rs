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
    pub(crate) py_str_slice: bool,
    pub(crate) py_str_slice_step: bool,
    pub(crate) py_list_slice_step: bool,
    pub(crate) py_iter: bool,
}

pub struct Codegen<'a> {
    pub(crate) ctx: &'a TypeContext,
    pub(crate) source: &'a str,
    pub(crate) filename: &'a str,
    pub(crate) out: String,
    pub(crate) indent: usize,
    pub(crate) tmp_counter: usize,
    pub(crate) uses: Uses,
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

    pub fn emit_program(mut self, program: &Program) -> Result<String, CompileError> {
        self.collect_uses(program)?;
        self.name_compare_only = self.analyze_name_compare_only(program);

        // First pass: generate all code to collect uses flags
        for item in &program.items {
            if let Item::Union(def) = item {
                self.emit_union(def)?;
            }
        }

        for item in &program.items {
            if let Item::Class(class_def) = item {
                self.emit_class(class_def)?;
            }
        }

        for item in &program.items {
            if let Item::Function(func) = item {
                self.emit_function(func, None)?;
            }
        }

        let mut top_level = Vec::new();
        for item in &program.items {
            if let Item::Stmt(stmt) = item {
                top_level.push(stmt.as_ref().clone());
            }
        }
        self.emit_main(&top_level)?;

        // Save generated code and emit header + helpers before it
        let generated_code = mem::take(&mut self.out);
        self.emit_header();
        self.emit_globals();
        self.emit_helpers();
        self.out.push_str(&generated_code);

        Ok(self.out)
    }
}
