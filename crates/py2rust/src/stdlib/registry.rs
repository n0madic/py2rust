//! Centralized registry for supported standard-library modules and members.
//!
//! The registry is the single source of truth for:
//! - which stdlib modules are recognized,
//! - which module members are callable,
//! - which module attributes are supported as values,
//! - call-shape constraints (arity, keyword support),
//! - and method-specific codegen handlers.

use crate::callspec::{AritySpec, CallShape, KeywordPolicy};
use crate::codegen::Codegen;
use crate::diagnostic::CompileError;
use crate::hir::{Expr, KeywordArg};
use crate::span::Span;
use crate::types::Type;

/// Identifier for a supported stdlib module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StdlibModuleId {
    /// Python `os` module.
    Os,
    /// Python `os.path` module.
    OsPath,
    /// Python `sys` module.
    Sys,
}

/// Identifier for a supported stdlib callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StdlibMethodId {
    /// `os.remove(path)`
    OsRemove,
    /// `os.getcwd()`
    OsGetcwd,
    /// `os.chdir(path)`
    OsChdir,
    /// `os.mkdir(path)`
    OsMkdir,
    /// `os.listdir(path)`
    OsListdir,
    /// `os.rmdir(path)`
    OsRmdir,
    /// `os.rename(src, dst)`
    OsRename,
    /// `os.replace(src, dst)`
    OsReplace,
    /// `os.makedirs(path, exist_ok=...)`
    OsMakedirs,
    /// `os.getenv(key, [default])`
    OsGetenv,
    /// `os.path.join(*parts)`
    OsPathJoin,
    /// `os.path.exists(path)`
    OsPathExists,
    /// `os.path.basename(path)`
    OsPathBasename,
    /// `os.path.dirname(path)`
    OsPathDirname,
    /// `os.path.split(path)`
    OsPathSplit,
    /// `os.path.isdir(path)`
    OsPathIsDir,
    /// `os.path.isfile(path)`
    OsPathIsFile,
    /// `os.path.abspath(path)`
    OsPathAbspath,
    /// `sys.intern(string)`
    SysIntern,
    /// `sys.exit([code])`
    SysExit,
}

/// Function pointer used to emit method-specific Rust calls in codegen.
pub type StdlibCodegenHandler =
    for<'a> fn(&mut Codegen<'a>, &[Expr], &[KeywordArg]) -> Result<String, CompileError>;

/// Function pointer used to materialize a module-attribute type.
pub type StdlibAttrTypeResolver = fn() -> Type;

/// Function pointer used to emit module-attribute expressions in codegen.
pub type StdlibAttrCodegenHandler =
    for<'a> fn(&mut Codegen<'a>, Span) -> Result<String, CompileError>;

/// Static method metadata used by type checking and code generation.
#[derive(Debug, Clone, Copy)]
pub struct StdlibMethodSpec {
    /// Stable method identifier.
    pub method_id: StdlibMethodId,
    /// Python module name (e.g. `"os"`).
    pub module_name: &'static str,
    /// Python member/callable name (e.g. `"remove"`).
    pub method_name: &'static str,
    /// Unified call-shape policy.
    pub shape: CallShape,
    /// Codegen callback for emitting this stdlib call.
    pub codegen_handler: StdlibCodegenHandler,
}

impl StdlibMethodSpec {
    /// Return canonical callable name used in diagnostics.
    pub fn callable_name(self) -> String {
        format!("{}.{}()", self.module_name, self.method_name)
    }
}

/// Static module-attribute metadata used by type checking and code generation.
#[derive(Debug, Clone, Copy)]
pub struct StdlibAttributeSpec {
    /// Python module name (e.g. `"os"`).
    pub module_name: &'static str,
    /// Python attribute name (e.g. `"environ"`).
    pub attribute_name: &'static str,
    /// Type resolver for this attribute.
    pub type_resolver: StdlibAttrTypeResolver,
    /// Codegen callback for emitting this attribute access.
    pub codegen_handler: StdlibAttrCodegenHandler,
}

const OS_REMOVE_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsRemove,
    module_name: "os",
    method_name: "remove",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_remove,
};

const OS_GETCWD_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsGetcwd,
    module_name: "os",
    method_name: "getcwd",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_getcwd,
};

const OS_CHDIR_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsChdir,
    module_name: "os",
    method_name: "chdir",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_chdir,
};

const OS_MKDIR_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsMkdir,
    module_name: "os",
    method_name: "mkdir",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_mkdir,
};

const OS_LISTDIR_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsListdir,
    module_name: "os",
    method_name: "listdir",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_listdir,
};

const OS_RMDIR_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsRmdir,
    module_name: "os",
    method_name: "rmdir",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_rmdir,
};

const OS_RENAME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsRename,
    module_name: "os",
    method_name: "rename",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_rename,
};

const OS_REPLACE_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsReplace,
    module_name: "os",
    method_name: "replace",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_replace,
};

const OS_MAKEDIRS_KEYWORDS: &[&str] = &["exist_ok"];
const OS_MAKEDIRS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsMakedirs,
    module_name: "os",
    method_name: "makedirs",
    shape: CallShape {
        arity: AritySpec::Range { min: 1, max: 2 },
        keywords: KeywordPolicy::Named(OS_MAKEDIRS_KEYWORDS),
    },
    codegen_handler: codegen_os_makedirs,
};

const OS_GETENV_KEYWORDS: &[&str] = &["default"];
const OS_GETENV_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsGetenv,
    module_name: "os",
    method_name: "getenv",
    shape: CallShape {
        arity: AritySpec::Range { min: 1, max: 2 },
        keywords: KeywordPolicy::Named(OS_GETENV_KEYWORDS),
    },
    codegen_handler: codegen_os_getenv,
};

const OS_PATH_JOIN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathJoin,
    module_name: "os.path",
    method_name: "join",
    shape: CallShape {
        arity: AritySpec::AtLeast(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_join,
};

const OS_PATH_EXISTS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathExists,
    module_name: "os.path",
    method_name: "exists",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_exists,
};

const OS_PATH_BASENAME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathBasename,
    module_name: "os.path",
    method_name: "basename",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_basename,
};

const OS_PATH_DIRNAME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathDirname,
    module_name: "os.path",
    method_name: "dirname",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_dirname,
};

const OS_PATH_SPLIT_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathSplit,
    module_name: "os.path",
    method_name: "split",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_split,
};

const OS_PATH_ISDIR_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathIsDir,
    module_name: "os.path",
    method_name: "isdir",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_isdir,
};

const OS_PATH_ISFILE_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathIsFile,
    module_name: "os.path",
    method_name: "isfile",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_isfile,
};

const OS_PATH_ABSPATH_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::OsPathAbspath,
    module_name: "os.path",
    method_name: "abspath",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_os_path_abspath,
};

const OS_PATH_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "os",
    attribute_name: "path",
    type_resolver: type_os_path_attr,
    codegen_handler: codegen_os_path_attr,
};

const OS_ENVIRON_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "os",
    attribute_name: "environ",
    type_resolver: type_os_environ_attr,
    codegen_handler: codegen_os_environ_attr,
};

const OS_NAME_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "os",
    attribute_name: "name",
    type_resolver: type_os_name_attr,
    codegen_handler: codegen_os_name_attr,
};

const SYS_ARGV_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "sys",
    attribute_name: "argv",
    type_resolver: type_sys_argv_attr,
    codegen_handler: codegen_sys_argv_attr,
};

const SYS_EXIT_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::SysExit,
    module_name: "sys",
    method_name: "exit",
    shape: CallShape {
        arity: AritySpec::Range { min: 0, max: 1 },
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_sys_exit,
};

const SYS_INTERN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::SysIntern,
    module_name: "sys",
    method_name: "intern",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_sys_intern,
};

/// Resolve a module name to a known stdlib module id.
pub fn resolve_module(name: &str) -> Option<StdlibModuleId> {
    match name {
        "os" => Some(StdlibModuleId::Os),
        "os.path" => Some(StdlibModuleId::OsPath),
        "sys" => Some(StdlibModuleId::Sys),
        _ => None,
    }
}

/// Resolve a module method by module id and method name.
pub fn find_stdlib_method(
    module: StdlibModuleId,
    method: &str,
) -> Option<&'static StdlibMethodSpec> {
    match (module, method) {
        (StdlibModuleId::Os, "remove") => Some(&OS_REMOVE_SPEC),
        (StdlibModuleId::Os, "getcwd") => Some(&OS_GETCWD_SPEC),
        (StdlibModuleId::Os, "chdir") => Some(&OS_CHDIR_SPEC),
        (StdlibModuleId::Os, "mkdir") => Some(&OS_MKDIR_SPEC),
        (StdlibModuleId::Os, "listdir") => Some(&OS_LISTDIR_SPEC),
        (StdlibModuleId::Os, "rmdir") => Some(&OS_RMDIR_SPEC),
        (StdlibModuleId::Os, "rename") => Some(&OS_RENAME_SPEC),
        (StdlibModuleId::Os, "replace") => Some(&OS_REPLACE_SPEC),
        (StdlibModuleId::Os, "makedirs") => Some(&OS_MAKEDIRS_SPEC),
        (StdlibModuleId::Os, "getenv") => Some(&OS_GETENV_SPEC),
        (StdlibModuleId::OsPath, "join") => Some(&OS_PATH_JOIN_SPEC),
        (StdlibModuleId::OsPath, "exists") => Some(&OS_PATH_EXISTS_SPEC),
        (StdlibModuleId::OsPath, "basename") => Some(&OS_PATH_BASENAME_SPEC),
        (StdlibModuleId::OsPath, "dirname") => Some(&OS_PATH_DIRNAME_SPEC),
        (StdlibModuleId::OsPath, "split") => Some(&OS_PATH_SPLIT_SPEC),
        (StdlibModuleId::OsPath, "isdir") => Some(&OS_PATH_ISDIR_SPEC),
        (StdlibModuleId::OsPath, "isfile") => Some(&OS_PATH_ISFILE_SPEC),
        (StdlibModuleId::OsPath, "abspath") => Some(&OS_PATH_ABSPATH_SPEC),
        (StdlibModuleId::Sys, "intern") => Some(&SYS_INTERN_SPEC),
        (StdlibModuleId::Sys, "exit") => Some(&SYS_EXIT_SPEC),
        _ => None,
    }
}

/// Resolve a module attribute by module id and attribute name.
pub fn find_stdlib_attribute(
    module: StdlibModuleId,
    attribute: &str,
) -> Option<&'static StdlibAttributeSpec> {
    match (module, attribute) {
        (StdlibModuleId::Os, "path") => Some(&OS_PATH_ATTR_SPEC),
        (StdlibModuleId::Os, "environ") => Some(&OS_ENVIRON_ATTR_SPEC),
        (StdlibModuleId::Os, "name") => Some(&OS_NAME_ATTR_SPEC),
        (StdlibModuleId::Sys, "argv") => Some(&SYS_ARGV_ATTR_SPEC),
        _ => None,
    }
}

/// Resolve an importable module member to a stable method id.
pub fn find_imported_member(module: StdlibModuleId, member: &str) -> Option<StdlibMethodId> {
    find_stdlib_method(module, member).map(|spec| spec.method_id)
}

/// Look up method metadata by stable method id.
pub fn method_spec(method_id: StdlibMethodId) -> &'static StdlibMethodSpec {
    match method_id {
        StdlibMethodId::OsRemove => &OS_REMOVE_SPEC,
        StdlibMethodId::OsGetcwd => &OS_GETCWD_SPEC,
        StdlibMethodId::OsChdir => &OS_CHDIR_SPEC,
        StdlibMethodId::OsMkdir => &OS_MKDIR_SPEC,
        StdlibMethodId::OsListdir => &OS_LISTDIR_SPEC,
        StdlibMethodId::OsRmdir => &OS_RMDIR_SPEC,
        StdlibMethodId::OsRename => &OS_RENAME_SPEC,
        StdlibMethodId::OsReplace => &OS_REPLACE_SPEC,
        StdlibMethodId::OsMakedirs => &OS_MAKEDIRS_SPEC,
        StdlibMethodId::OsGetenv => &OS_GETENV_SPEC,
        StdlibMethodId::OsPathJoin => &OS_PATH_JOIN_SPEC,
        StdlibMethodId::OsPathExists => &OS_PATH_EXISTS_SPEC,
        StdlibMethodId::OsPathBasename => &OS_PATH_BASENAME_SPEC,
        StdlibMethodId::OsPathDirname => &OS_PATH_DIRNAME_SPEC,
        StdlibMethodId::OsPathSplit => &OS_PATH_SPLIT_SPEC,
        StdlibMethodId::OsPathIsDir => &OS_PATH_ISDIR_SPEC,
        StdlibMethodId::OsPathIsFile => &OS_PATH_ISFILE_SPEC,
        StdlibMethodId::OsPathAbspath => &OS_PATH_ABSPATH_SPEC,
        StdlibMethodId::SysIntern => &SYS_INTERN_SPEC,
        StdlibMethodId::SysExit => &SYS_EXIT_SPEC,
    }
}

/// Look up a keyword argument value by name after call-shape validation.
fn keyword_value<'kw>(keywords: &'kw [KeywordArg], keyword: &str) -> Option<&'kw Expr> {
    keywords
        .iter()
        .find(|kw| kw.name.as_deref() == Some(keyword))
        .map(|kw| &kw.value)
}

/// Resolve the static type for `os.path`.
fn type_os_path_attr() -> Type {
    Type::Module("os.path".to_string())
}

/// Resolve the static type for `os.environ`.
fn type_os_environ_attr() -> Type {
    Type::Dict(Box::new(Type::Str), Box::new(Type::Str))
}

/// Resolve the static type for `os.name`.
fn type_os_name_attr() -> Type {
    Type::Str
}

/// Resolve the static type for `sys.argv`.
fn type_sys_argv_attr() -> Type {
    Type::List(Box::new(Type::Str))
}

/// Emit `os.path` attribute expression (module namespace marker only).
fn codegen_os_path_attr(codegen: &mut Codegen<'_>, span: Span) -> Result<String, CompileError> {
    Err(codegen.error(
        span,
        "module 'os.path' is not a runtime value; use os.path.<member>(...)",
    ))
}

/// Emit `os.environ` attribute expression.
fn codegen_os_environ_attr(codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    codegen.uses.py_os_environ = true;
    Ok("py_os_environ()".to_string())
}

/// Emit `os.name` attribute expression.
fn codegen_os_name_attr(codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    codegen.uses.py_os_name = true;
    Ok("py_os_name()".to_string())
}

/// Emit `sys.argv` attribute expression.
fn codegen_sys_argv_attr(codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    codegen.uses.py_sys_argv = true;
    Ok("py_sys_argv()".to_string())
}

/// Emit code for `os.remove(path)` after generic validation has passed.
fn codegen_os_remove(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_remove = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_remove(&({}))", path_expr)))
}

/// Emit code for `os.getcwd()`.
fn codegen_os_getcwd(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_getcwd = true;
    Ok(codegen.wrap_result("py_os_getcwd()".to_string()))
}

/// Emit code for `os.chdir(path)`.
fn codegen_os_chdir(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_chdir = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_chdir(&({}))", path_expr)))
}

/// Emit code for `os.mkdir(path)`.
fn codegen_os_mkdir(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_mkdir = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_mkdir(&({}))", path_expr)))
}

/// Emit code for `os.listdir(path)`.
fn codegen_os_listdir(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_listdir = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_listdir(&({}))", path_expr)))
}

/// Emit code for `os.rmdir(path)`.
fn codegen_os_rmdir(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_rmdir = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_rmdir(&({}))", path_expr)))
}

/// Emit code for `os.rename(src, dst)`.
fn codegen_os_rename(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_rename = true;
    let src_expr = codegen.gen_expr(&args[0])?;
    let dst_expr = codegen.gen_expr(&args[1])?;
    Ok(codegen.wrap_result(format!("py_os_rename(&({}), &({}))", src_expr, dst_expr)))
}

/// Emit code for `os.replace(src, dst)`.
fn codegen_os_replace(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_replace = true;
    let src_expr = codegen.gen_expr(&args[0])?;
    let dst_expr = codegen.gen_expr(&args[1])?;
    Ok(codegen.wrap_result(format!("py_os_replace(&({}), &({}))", src_expr, dst_expr)))
}

/// Emit code for `os.makedirs(path, exist_ok=...)`.
fn codegen_os_makedirs(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_makedirs = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    if args.len() == 2 && keyword_value(keywords, "exist_ok").is_some() {
        return Err(codegen.error(
            args[1].span,
            "Multiple values for keyword argument `exist_ok`",
        ));
    }
    let exist_ok = if let Some(exist_ok_kw) = keyword_value(keywords, "exist_ok") {
        codegen.gen_expr(exist_ok_kw)?
    } else if args.len() == 2 {
        codegen.gen_expr(&args[1])?
    } else {
        "false".to_string()
    };
    Ok(codegen.wrap_result(format!("py_os_makedirs(&({}), {})", path_expr, exist_ok)))
}

/// Emit code for `os.getenv(key, [default])`.
fn codegen_os_getenv(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_getenv = true;
    let key_expr = codegen.gen_expr(&args[0])?;
    if args.len() == 2 && keyword_value(keywords, "default").is_some() {
        return Err(codegen.error(
            args[1].span,
            "Multiple values for keyword argument `default`",
        ));
    }
    // Preserve Python's `None` default behavior without forcing a String conversion.
    let render_default = |codegen: &mut Codegen<'_>, expr: &Expr| -> Result<String, CompileError> {
        if matches!(expr.ty.as_ref(), Some(Type::None)) {
            return Ok("None".to_string());
        }
        let default_value = codegen.gen_expr(expr)?;
        Ok(format!("Some(({}).clone())", default_value))
    };
    let default_expr = if let Some(default_kw) = keyword_value(keywords, "default") {
        render_default(codegen, default_kw)?
    } else if args.len() == 2 {
        render_default(codegen, &args[1])?
    } else {
        "None".to_string()
    };
    Ok(format!("py_os_getenv(&({}), {})", key_expr, default_expr))
}

/// Emit code for `os.path.join(*parts)`.
fn codegen_os_path_join(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_join = true;
    let rendered: Vec<String> = args
        .iter()
        .map(|arg| codegen.gen_expr(arg).map(|expr| format!("&({})", expr)))
        .collect::<Result<_, _>>()?;
    Ok(format!("py_os_path_join(&[{}])", rendered.join(", ")))
}

/// Emit code for `os.path.exists(path)`.
fn codegen_os_path_exists(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_exists = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_os_path_exists(&({}))", path_expr))
}

/// Emit code for `os.path.basename(path)`.
fn codegen_os_path_basename(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_basename = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_os_path_basename(&({}))", path_expr))
}

/// Emit code for `os.path.dirname(path)`.
fn codegen_os_path_dirname(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_dirname = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_os_path_dirname(&({}))", path_expr))
}

/// Emit code for `os.path.split(path)`.
fn codegen_os_path_split(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_split = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_os_path_split(&({}))", path_expr))
}

/// Emit code for `os.path.isdir(path)`.
fn codegen_os_path_isdir(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_is_dir = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_os_path_is_dir(&({}))", path_expr))
}

/// Emit code for `os.path.isfile(path)`.
fn codegen_os_path_isfile(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_is_file = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_os_path_is_file(&({}))", path_expr))
}

/// Emit code for `os.path.abspath(path)`.
fn codegen_os_path_abspath(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_os_path_abspath = true;
    let path_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_os_path_abspath(&({}))", path_expr)))
}

/// Emit code for `sys.exit([code])` after generic validation has passed.
fn codegen_sys_exit(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    if args.is_empty() {
        return Ok("std::process::exit(0)".to_string());
    }
    let code_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("std::process::exit({} as i32)", code_expr))
}

/// Emit code for `sys.intern(string)`.
fn codegen_sys_intern(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_sys_intern = true;
    let value_expr = codegen.gen_expr(&args[0])?;
    Ok(format!("py_sys_intern(&({}))", value_expr))
}
