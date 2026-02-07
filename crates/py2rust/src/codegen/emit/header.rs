// Header emission for generated Rust files.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit file header with necessary imports and suppressions.
    ///
    /// We use #![allow(unused)] liberally because:
    /// 1. Generated code may have unused variables (Python allows this)
    /// 2. We don't want to burden users with clippy warnings on generated code
    /// 3. Dead code is harmless in generated output
    ///
    /// Map/set imports are only emitted when actually used (tracked by scan pass).
    pub(crate) fn emit_header(&mut self) {
        // Import names are namespace-mangled for collision safety and may violate
        // Rust style lints in generated code.
        self.push_line("#![allow(unused, non_snake_case, non_camel_case_types)]");
        if self.uses.hash_map {
            self.push_line("use std::collections::HashMap;");
        }
        if self.uses.index_map || self.uses.py_dict_get {
            self.push_line("use indexmap::IndexMap;");
        }
        if self.uses.hash_set {
            self.push_line("use std::collections::HashSet;");
        }
        self.push_line("use std::cell::RefCell;");
        self.push_line("use std::rc::Rc;");
        // Arc/Mutex are required for list semantics and globals.
        self.push_line("use std::sync::{Arc, Mutex, OnceLock};");
        self.push_line("const __NAME__: &str = \"__main__\";");
        self.push_line("");
    }
}
