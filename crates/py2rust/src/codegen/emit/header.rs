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
    /// HashMap/HashSet are only imported if actually used (tracked by scan pass).
    pub(crate) fn emit_header(&mut self) {
        self.push_line("#![allow(unused)]");
        if self.uses.hash_map {
            self.push_line("use std::collections::HashMap;");
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
