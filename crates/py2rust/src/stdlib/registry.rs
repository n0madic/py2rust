//! Centralized registry for supported standard-library modules and members.
//!
//! The registry is the single source of truth for:
//! - which stdlib modules are recognized,
//! - which module members are callable,
//! - and basic call-shape constraints (arity, keyword support).

use crate::codegen::Codegen;
use crate::diagnostic::CompileError;
use crate::hir::Expr;

/// Identifier for a supported stdlib module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StdlibModuleId {
    /// Python `os` module.
    Os,
    /// Python `sys` module.
    Sys,
}

/// Identifier for a supported stdlib callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StdlibMethodId {
    /// `os.remove(path)`
    OsRemove,
    /// `sys.exit([code])`
    SysExit,
}

/// Function pointer used to emit method-specific Rust calls in codegen.
pub type StdlibCodegenHandler =
    for<'a> fn(&mut Codegen<'a>, &[Expr]) -> Result<String, CompileError>;

/// Static method metadata used by type checking and code generation.
#[derive(Debug, Clone, Copy)]
pub struct StdlibMethodSpec {
    /// Stable method identifier.
    pub method_id: StdlibMethodId,
    /// Python module name (e.g. `"os"`).
    pub module_name: &'static str,
    /// Python member/callable name (e.g. `"remove"`).
    pub method_name: &'static str,
    /// Minimum supported positional argument count.
    pub min_args: usize,
    /// Maximum supported positional argument count.
    pub max_args: usize,
    /// Whether keyword arguments are accepted.
    pub allow_keywords: bool,
    /// Codegen callback for emitting this stdlib call.
    pub codegen_handler: StdlibCodegenHandler,
}

const OS_REMOVE_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsRemove,
    module_name: "os",
    method_name: "remove",
    min_args: 1,
    max_args: 1,
    allow_keywords: false,
    codegen_handler: codegen_os_remove,
};

const SYS_EXIT_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::SysExit,
    module_name: "sys",
    method_name: "exit",
    min_args: 0,
    max_args: 1,
    allow_keywords: false,
    codegen_handler: codegen_sys_exit,
};

/// Resolve a module name to a known stdlib module id.
pub fn resolve_module(name: &str) -> Option<StdlibModuleId> {
    match name {
        "os" => Some(StdlibModuleId::Os),
        "sys" => Some(StdlibModuleId::Sys),
        _ => None,
    }
}

/// Resolve a module method by module id and method name.
pub fn resolve_method(module: StdlibModuleId, method: &str) -> Option<&'static StdlibMethodSpec> {
    match (module, method) {
        (StdlibModuleId::Os, "remove") => Some(&OS_REMOVE_SPEC),
        (StdlibModuleId::Sys, "exit") => Some(&SYS_EXIT_SPEC),
        _ => None,
    }
}

/// Resolve an importable module member to a stable method id.
pub fn resolve_member(module: StdlibModuleId, member: &str) -> Option<StdlibMethodId> {
    resolve_method(module, member).map(|spec| spec.method_id)
}

/// Look up method metadata by stable method id.
pub fn method_spec(method_id: StdlibMethodId) -> &'static StdlibMethodSpec {
    match method_id {
        StdlibMethodId::OsRemove => &OS_REMOVE_SPEC,
        StdlibMethodId::SysExit => &SYS_EXIT_SPEC,
    }
}

/// Emit code for `os.remove(path)` after generic validation has passed.
fn codegen_os_remove(codegen: &mut Codegen<'_>, args: &[Expr]) -> Result<String, CompileError> {
    codegen.uses.py_os_remove = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_remove(&{})", path_expr)))
}

/// Emit code for `sys.exit([code])` after generic validation has passed.
fn codegen_sys_exit(codegen: &mut Codegen<'_>, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() {
        return Ok("std::process::exit(0)".to_string());
    }
    let code_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("std::process::exit({} as i32)", code_expr))
}
