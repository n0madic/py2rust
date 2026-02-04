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
        }
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
