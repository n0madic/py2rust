// Global variable emission for generated Rust files.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit global variable declarations.
    ///
    /// Python allows mutable global variables. In Rust, this requires special handling:
    /// - Globals must be static (compile-time constants)
    /// - But we need runtime mutability
    /// - Solution: OnceLock<Mutex<T>>
    ///
    /// OnceLock: Initialized once at first use (lazy initialization)
    /// Mutex: Provides interior mutability and thread safety
    ///
    /// Access pattern in generated code:
    /// - Read: `GLOBAL_X.get().expect("global not initialized").lock().expect("global mutex poisoned").clone()`
    /// - Write: `*GLOBAL_X.get().expect("global not initialized").lock().expect("global mutex poisoned") = value`
    ///
    /// This is verbose but necessary for Rust's safety guarantees.
    pub(crate) fn emit_globals(&mut self) {
        if self.ctx.globals.is_empty() {
            return;
        }
        for (name, ty) in &self.ctx.globals {
            let ty_str = self.rust_type_for_global(ty);
            let gname = self.global_name(name);
            self.push_line(&format!(
                "static {}: OnceLock<Mutex<{}>> = OnceLock::new();",
                gname, ty_str
            ));
        }
        self.push_line("");
    }
}
