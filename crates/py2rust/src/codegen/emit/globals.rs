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
        if self.shared_globals.is_empty() {
            return;
        }
        for (name, ty) in &self.ctx.globals {
            // Only emit globals that are actually shared with functions/helpers.
            if !self.shared_globals.contains(name) {
                continue;
            }
            if matches!(ty, Type::Module(_) | Type::StdlibFunction { .. }) {
                // Import bindings are compile-time only and must not be emitted as runtime globals.
                continue;
            }
            let ty_str = self.rust_type_for_global(ty);
            let gname = self.global_name(name);
            if self.readonly_globals.contains(name) {
                // Write-once scalar globals skip the Mutex wrapper entirely.
                self.push_line(&format!(
                    "static {}: OnceLock<{}> = OnceLock::new();",
                    gname, ty_str
                ));
            } else {
                self.push_line(&format!(
                    "static {}: OnceLock<Mutex<{}>> = OnceLock::new();",
                    gname, ty_str
                ));
            }
            if self.is_default_global_name(name) && matches!(ty, Type::List(_) | Type::Dict(_, _)) {
                let cache_name = self.default_cache_name(name);
                let local_ty = self.rust_type(ty);
                self.push_line(&format!(
                    "thread_local! {{ static {}: RefCell<Option<{}>> = RefCell::new(None); }}",
                    cache_name, local_ty
                ));
            }
        }
        self.push_line("");
    }
}
