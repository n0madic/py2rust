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
    /// Python `re` module.
    Re,
    /// Python `json` module.
    Json,
    /// Python `math` module.
    Math,
    /// Python `time` module.
    Time,
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
    /// `re.search(pattern, string)`
    ReSearch,
    /// `re.match(pattern, string)`
    ReMatch,
    /// `re.sub(pattern, repl, string)`
    ReSub,
    /// `json.dumps(value)`
    JsonDumps,
    /// `json.loads(text)`
    JsonLoads,
    /// `json.dump(value, file)`
    JsonDump,
    /// `json.load(file)`
    JsonLoad,
    /// `math.sqrt(x)`
    MathSqrt,
    /// `math.sin(x)`
    MathSin,
    /// `math.cos(x)`
    MathCos,
    /// `math.tan(x)`
    MathTan,
    /// `math.ceil(x)`
    MathCeil,
    /// `math.floor(x)`
    MathFloor,
    /// `math.factorial(n)`
    MathFactorial,
    /// `math.log(x)`
    MathLog,
    /// `math.log2(x)`
    MathLog2,
    /// `math.log10(x)`
    MathLog10,
    /// `math.exp(x)`
    MathExp,
    /// `math.asin(x)`
    MathAsin,
    /// `math.acos(x)`
    MathAcos,
    /// `math.atan(x)`
    MathAtan,
    /// `math.sinh(x)`
    MathSinh,
    /// `math.cosh(x)`
    MathCosh,
    /// `math.tanh(x)`
    MathTanh,
    /// `math.fabs(x)`
    MathFabs,
    /// `math.degrees(x)`
    MathDegrees,
    /// `math.radians(x)`
    MathRadians,
    /// `math.trunc(x)`
    MathTrunc,
    /// `math.isnan(x)`
    MathIsNan,
    /// `math.isinf(x)`
    MathIsInf,
    /// `math.isfinite(x)`
    MathIsFinite,
    /// `math.atan2(y, x)`
    MathAtan2,
    /// `math.fmod(x, y)`
    MathFmod,
    /// `math.copysign(x, y)`
    MathCopySign,
    /// `math.hypot(x, y)`
    MathHypot,
    /// `math.pow(x, y)`
    MathPow,
    /// `math.gcd(a, b)`
    MathGcd,
    /// `math.lcm(a, b)`
    MathLcm,
    /// `math.comb(n, k)`
    MathComb,
    /// `math.perm(n, k)`
    MathPerm,
    /// `time.time()`
    TimeTime,
    /// `time.time_ns()`
    TimeTimeNs,
    /// `time.monotonic()`
    TimeMonotonic,
    /// `time.monotonic_ns()`
    TimeMonotonicNs,
    /// `time.perf_counter()`
    TimePerfCounter,
    /// `time.perf_counter_ns()`
    TimePerfCounterNs,
    /// `time.process_time()`
    TimeProcessTime,
    /// `time.process_time_ns()`
    TimeProcessTimeNs,
    /// `time.sleep(seconds)`
    TimeSleep,
    /// `time.localtime([secs])`
    TimeLocaltime,
    /// `time.gmtime([secs])`
    TimeGmtime,
    /// `time.strftime(format, t)`
    TimeStrftime,
    /// `time.strptime(string, format)`
    TimeStrptime,
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

const RE_SEARCH_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::ReSearch,
    module_name: "re",
    method_name: "search",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_re_search,
};

const RE_MATCH_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::ReMatch,
    module_name: "re",
    method_name: "match",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_re_match,
};

const RE_SUB_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::ReSub,
    module_name: "re",
    method_name: "sub",
    shape: CallShape {
        arity: AritySpec::Exact(3),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_re_sub,
};

const JSON_DUMPS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::JsonDumps,
    module_name: "json",
    method_name: "dumps",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_json_dumps,
};

const JSON_LOADS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::JsonLoads,
    module_name: "json",
    method_name: "loads",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_json_loads,
};

const JSON_DUMP_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::JsonDump,
    module_name: "json",
    method_name: "dump",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_json_dump,
};

const JSON_LOAD_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::JsonLoad,
    module_name: "json",
    method_name: "load",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_json_load,
};

const MATH_PI_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "math",
    attribute_name: "pi",
    type_resolver: type_math_pi_attr,
    codegen_handler: codegen_math_pi_attr,
};

const MATH_E_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "math",
    attribute_name: "e",
    type_resolver: type_math_e_attr,
    codegen_handler: codegen_math_e_attr,
};

const MATH_TAU_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "math",
    attribute_name: "tau",
    type_resolver: type_math_tau_attr,
    codegen_handler: codegen_math_tau_attr,
};

const MATH_INF_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "math",
    attribute_name: "inf",
    type_resolver: type_math_inf_attr,
    codegen_handler: codegen_math_inf_attr,
};

const MATH_NAN_ATTR_SPEC: StdlibAttributeSpec = StdlibAttributeSpec {
    module_name: "math",
    attribute_name: "nan",
    type_resolver: type_math_nan_attr,
    codegen_handler: codegen_math_nan_attr,
};

const MATH_SQRT_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathSqrt,
    module_name: "math",
    method_name: "sqrt",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_sqrt,
};

const MATH_SIN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathSin,
    module_name: "math",
    method_name: "sin",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_sin,
};

const MATH_COS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathCos,
    module_name: "math",
    method_name: "cos",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_cos,
};

const MATH_TAN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathTan,
    module_name: "math",
    method_name: "tan",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_tan,
};

const MATH_CEIL_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathCeil,
    module_name: "math",
    method_name: "ceil",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_ceil,
};

const MATH_FLOOR_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathFloor,
    module_name: "math",
    method_name: "floor",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_floor,
};

const MATH_FACTORIAL_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathFactorial,
    module_name: "math",
    method_name: "factorial",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_factorial,
};

const MATH_LOG_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathLog,
    module_name: "math",
    method_name: "log",
    shape: CallShape {
        arity: AritySpec::Range { min: 1, max: 2 },
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_log,
};

const MATH_LOG2_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathLog2,
    module_name: "math",
    method_name: "log2",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_log2,
};

const MATH_LOG10_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathLog10,
    module_name: "math",
    method_name: "log10",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_log10,
};

const MATH_EXP_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathExp,
    module_name: "math",
    method_name: "exp",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_exp,
};

const MATH_ASIN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathAsin,
    module_name: "math",
    method_name: "asin",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_asin,
};

const MATH_ACOS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathAcos,
    module_name: "math",
    method_name: "acos",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_acos,
};

const MATH_ATAN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathAtan,
    module_name: "math",
    method_name: "atan",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_atan,
};

const MATH_SINH_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathSinh,
    module_name: "math",
    method_name: "sinh",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_sinh,
};

const MATH_COSH_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathCosh,
    module_name: "math",
    method_name: "cosh",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_cosh,
};

const MATH_TANH_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathTanh,
    module_name: "math",
    method_name: "tanh",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_tanh,
};

const MATH_FABS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathFabs,
    module_name: "math",
    method_name: "fabs",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_fabs,
};

const MATH_DEGREES_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathDegrees,
    module_name: "math",
    method_name: "degrees",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_degrees,
};

const MATH_RADIANS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathRadians,
    module_name: "math",
    method_name: "radians",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_radians,
};

const MATH_TRUNC_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathTrunc,
    module_name: "math",
    method_name: "trunc",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_trunc,
};

const MATH_ISNAN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathIsNan,
    module_name: "math",
    method_name: "isnan",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_isnan,
};

const MATH_ISINF_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathIsInf,
    module_name: "math",
    method_name: "isinf",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_isinf,
};

const MATH_ISFINITE_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathIsFinite,
    module_name: "math",
    method_name: "isfinite",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_isfinite,
};

const MATH_ATAN2_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathAtan2,
    module_name: "math",
    method_name: "atan2",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_atan2,
};

const MATH_FMOD_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathFmod,
    module_name: "math",
    method_name: "fmod",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_fmod,
};

const MATH_COPYSIGN_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathCopySign,
    module_name: "math",
    method_name: "copysign",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_copysign,
};

const MATH_HYPOT_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathHypot,
    module_name: "math",
    method_name: "hypot",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_hypot,
};

const MATH_POW_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathPow,
    module_name: "math",
    method_name: "pow",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_pow,
};

const MATH_GCD_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathGcd,
    module_name: "math",
    method_name: "gcd",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_gcd,
};

const MATH_LCM_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathLcm,
    module_name: "math",
    method_name: "lcm",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_lcm,
};

const MATH_COMB_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathComb,
    module_name: "math",
    method_name: "comb",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_comb,
};

const MATH_PERM_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::MathPerm,
    module_name: "math",
    method_name: "perm",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_math_perm,
};

const TIME_TIME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeTime,
    module_name: "time",
    method_name: "time",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_time,
};

const TIME_TIME_NS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeTimeNs,
    module_name: "time",
    method_name: "time_ns",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_time_ns,
};

const TIME_MONOTONIC_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeMonotonic,
    module_name: "time",
    method_name: "monotonic",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_monotonic,
};

const TIME_MONOTONIC_NS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeMonotonicNs,
    module_name: "time",
    method_name: "monotonic_ns",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_monotonic_ns,
};

const TIME_PERF_COUNTER_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimePerfCounter,
    module_name: "time",
    method_name: "perf_counter",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_perf_counter,
};

const TIME_PERF_COUNTER_NS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimePerfCounterNs,
    module_name: "time",
    method_name: "perf_counter_ns",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_perf_counter_ns,
};

const TIME_PROCESS_TIME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeProcessTime,
    module_name: "time",
    method_name: "process_time",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_process_time,
};

const TIME_PROCESS_TIME_NS_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeProcessTimeNs,
    module_name: "time",
    method_name: "process_time_ns",
    shape: CallShape {
        arity: AritySpec::Exact(0),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_process_time_ns,
};

const TIME_SLEEP_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeSleep,
    module_name: "time",
    method_name: "sleep",
    shape: CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_sleep,
};

const TIME_LOCALTIME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeLocaltime,
    module_name: "time",
    method_name: "localtime",
    shape: CallShape {
        arity: AritySpec::Range { min: 0, max: 1 },
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_localtime,
};

const TIME_GMTIME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeGmtime,
    module_name: "time",
    method_name: "gmtime",
    shape: CallShape {
        arity: AritySpec::Range { min: 0, max: 1 },
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_gmtime,
};

const TIME_STRFTIME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeStrftime,
    module_name: "time",
    method_name: "strftime",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_strftime,
};

const TIME_STRPTIME_SPEC: StdlibMethodSpec = StdlibMethodSpec {
    method_id: StdlibMethodId::TimeStrptime,
    module_name: "time",
    method_name: "strptime",
    shape: CallShape {
        arity: AritySpec::Exact(2),
        keywords: KeywordPolicy::None,
    },
    codegen_handler: codegen_time_strptime,
};

/// Resolve a module name to a known stdlib module id.
pub fn resolve_module(name: &str) -> Option<StdlibModuleId> {
    match name {
        "os" => Some(StdlibModuleId::Os),
        "os.path" => Some(StdlibModuleId::OsPath),
        "sys" => Some(StdlibModuleId::Sys),
        "re" => Some(StdlibModuleId::Re),
        "json" => Some(StdlibModuleId::Json),
        "math" => Some(StdlibModuleId::Math),
        "time" => Some(StdlibModuleId::Time),
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
        (StdlibModuleId::Re, "search") => Some(&RE_SEARCH_SPEC),
        (StdlibModuleId::Re, "match") => Some(&RE_MATCH_SPEC),
        (StdlibModuleId::Re, "sub") => Some(&RE_SUB_SPEC),
        (StdlibModuleId::Json, "dumps") => Some(&JSON_DUMPS_SPEC),
        (StdlibModuleId::Json, "loads") => Some(&JSON_LOADS_SPEC),
        (StdlibModuleId::Json, "dump") => Some(&JSON_DUMP_SPEC),
        (StdlibModuleId::Json, "load") => Some(&JSON_LOAD_SPEC),
        (StdlibModuleId::Math, "sqrt") => Some(&MATH_SQRT_SPEC),
        (StdlibModuleId::Math, "sin") => Some(&MATH_SIN_SPEC),
        (StdlibModuleId::Math, "cos") => Some(&MATH_COS_SPEC),
        (StdlibModuleId::Math, "tan") => Some(&MATH_TAN_SPEC),
        (StdlibModuleId::Math, "ceil") => Some(&MATH_CEIL_SPEC),
        (StdlibModuleId::Math, "floor") => Some(&MATH_FLOOR_SPEC),
        (StdlibModuleId::Math, "factorial") => Some(&MATH_FACTORIAL_SPEC),
        (StdlibModuleId::Math, "log") => Some(&MATH_LOG_SPEC),
        (StdlibModuleId::Math, "log2") => Some(&MATH_LOG2_SPEC),
        (StdlibModuleId::Math, "log10") => Some(&MATH_LOG10_SPEC),
        (StdlibModuleId::Math, "exp") => Some(&MATH_EXP_SPEC),
        (StdlibModuleId::Math, "asin") => Some(&MATH_ASIN_SPEC),
        (StdlibModuleId::Math, "acos") => Some(&MATH_ACOS_SPEC),
        (StdlibModuleId::Math, "atan") => Some(&MATH_ATAN_SPEC),
        (StdlibModuleId::Math, "sinh") => Some(&MATH_SINH_SPEC),
        (StdlibModuleId::Math, "cosh") => Some(&MATH_COSH_SPEC),
        (StdlibModuleId::Math, "tanh") => Some(&MATH_TANH_SPEC),
        (StdlibModuleId::Math, "fabs") => Some(&MATH_FABS_SPEC),
        (StdlibModuleId::Math, "degrees") => Some(&MATH_DEGREES_SPEC),
        (StdlibModuleId::Math, "radians") => Some(&MATH_RADIANS_SPEC),
        (StdlibModuleId::Math, "trunc") => Some(&MATH_TRUNC_SPEC),
        (StdlibModuleId::Math, "isnan") => Some(&MATH_ISNAN_SPEC),
        (StdlibModuleId::Math, "isinf") => Some(&MATH_ISINF_SPEC),
        (StdlibModuleId::Math, "isfinite") => Some(&MATH_ISFINITE_SPEC),
        (StdlibModuleId::Math, "atan2") => Some(&MATH_ATAN2_SPEC),
        (StdlibModuleId::Math, "fmod") => Some(&MATH_FMOD_SPEC),
        (StdlibModuleId::Math, "copysign") => Some(&MATH_COPYSIGN_SPEC),
        (StdlibModuleId::Math, "hypot") => Some(&MATH_HYPOT_SPEC),
        (StdlibModuleId::Math, "pow") => Some(&MATH_POW_SPEC),
        (StdlibModuleId::Math, "gcd") => Some(&MATH_GCD_SPEC),
        (StdlibModuleId::Math, "lcm") => Some(&MATH_LCM_SPEC),
        (StdlibModuleId::Math, "comb") => Some(&MATH_COMB_SPEC),
        (StdlibModuleId::Math, "perm") => Some(&MATH_PERM_SPEC),
        (StdlibModuleId::Time, "time") => Some(&TIME_TIME_SPEC),
        (StdlibModuleId::Time, "time_ns") => Some(&TIME_TIME_NS_SPEC),
        (StdlibModuleId::Time, "monotonic") => Some(&TIME_MONOTONIC_SPEC),
        (StdlibModuleId::Time, "monotonic_ns") => Some(&TIME_MONOTONIC_NS_SPEC),
        (StdlibModuleId::Time, "perf_counter") => Some(&TIME_PERF_COUNTER_SPEC),
        (StdlibModuleId::Time, "perf_counter_ns") => Some(&TIME_PERF_COUNTER_NS_SPEC),
        (StdlibModuleId::Time, "process_time") => Some(&TIME_PROCESS_TIME_SPEC),
        (StdlibModuleId::Time, "process_time_ns") => Some(&TIME_PROCESS_TIME_NS_SPEC),
        (StdlibModuleId::Time, "sleep") => Some(&TIME_SLEEP_SPEC),
        (StdlibModuleId::Time, "localtime") => Some(&TIME_LOCALTIME_SPEC),
        (StdlibModuleId::Time, "gmtime") => Some(&TIME_GMTIME_SPEC),
        (StdlibModuleId::Time, "strftime") => Some(&TIME_STRFTIME_SPEC),
        (StdlibModuleId::Time, "strptime") => Some(&TIME_STRPTIME_SPEC),
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
        (StdlibModuleId::Math, "pi") => Some(&MATH_PI_ATTR_SPEC),
        (StdlibModuleId::Math, "e") => Some(&MATH_E_ATTR_SPEC),
        (StdlibModuleId::Math, "tau") => Some(&MATH_TAU_ATTR_SPEC),
        (StdlibModuleId::Math, "inf") => Some(&MATH_INF_ATTR_SPEC),
        (StdlibModuleId::Math, "nan") => Some(&MATH_NAN_ATTR_SPEC),
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
        StdlibMethodId::ReSearch => &RE_SEARCH_SPEC,
        StdlibMethodId::ReMatch => &RE_MATCH_SPEC,
        StdlibMethodId::ReSub => &RE_SUB_SPEC,
        StdlibMethodId::JsonDumps => &JSON_DUMPS_SPEC,
        StdlibMethodId::JsonLoads => &JSON_LOADS_SPEC,
        StdlibMethodId::JsonDump => &JSON_DUMP_SPEC,
        StdlibMethodId::JsonLoad => &JSON_LOAD_SPEC,
        StdlibMethodId::MathSqrt => &MATH_SQRT_SPEC,
        StdlibMethodId::MathSin => &MATH_SIN_SPEC,
        StdlibMethodId::MathCos => &MATH_COS_SPEC,
        StdlibMethodId::MathTan => &MATH_TAN_SPEC,
        StdlibMethodId::MathCeil => &MATH_CEIL_SPEC,
        StdlibMethodId::MathFloor => &MATH_FLOOR_SPEC,
        StdlibMethodId::MathFactorial => &MATH_FACTORIAL_SPEC,
        StdlibMethodId::MathLog => &MATH_LOG_SPEC,
        StdlibMethodId::MathLog2 => &MATH_LOG2_SPEC,
        StdlibMethodId::MathLog10 => &MATH_LOG10_SPEC,
        StdlibMethodId::MathExp => &MATH_EXP_SPEC,
        StdlibMethodId::MathAsin => &MATH_ASIN_SPEC,
        StdlibMethodId::MathAcos => &MATH_ACOS_SPEC,
        StdlibMethodId::MathAtan => &MATH_ATAN_SPEC,
        StdlibMethodId::MathSinh => &MATH_SINH_SPEC,
        StdlibMethodId::MathCosh => &MATH_COSH_SPEC,
        StdlibMethodId::MathTanh => &MATH_TANH_SPEC,
        StdlibMethodId::MathFabs => &MATH_FABS_SPEC,
        StdlibMethodId::MathDegrees => &MATH_DEGREES_SPEC,
        StdlibMethodId::MathRadians => &MATH_RADIANS_SPEC,
        StdlibMethodId::MathTrunc => &MATH_TRUNC_SPEC,
        StdlibMethodId::MathIsNan => &MATH_ISNAN_SPEC,
        StdlibMethodId::MathIsInf => &MATH_ISINF_SPEC,
        StdlibMethodId::MathIsFinite => &MATH_ISFINITE_SPEC,
        StdlibMethodId::MathAtan2 => &MATH_ATAN2_SPEC,
        StdlibMethodId::MathFmod => &MATH_FMOD_SPEC,
        StdlibMethodId::MathCopySign => &MATH_COPYSIGN_SPEC,
        StdlibMethodId::MathHypot => &MATH_HYPOT_SPEC,
        StdlibMethodId::MathPow => &MATH_POW_SPEC,
        StdlibMethodId::MathGcd => &MATH_GCD_SPEC,
        StdlibMethodId::MathLcm => &MATH_LCM_SPEC,
        StdlibMethodId::MathComb => &MATH_COMB_SPEC,
        StdlibMethodId::MathPerm => &MATH_PERM_SPEC,
        StdlibMethodId::TimeTime => &TIME_TIME_SPEC,
        StdlibMethodId::TimeTimeNs => &TIME_TIME_NS_SPEC,
        StdlibMethodId::TimeMonotonic => &TIME_MONOTONIC_SPEC,
        StdlibMethodId::TimeMonotonicNs => &TIME_MONOTONIC_NS_SPEC,
        StdlibMethodId::TimePerfCounter => &TIME_PERF_COUNTER_SPEC,
        StdlibMethodId::TimePerfCounterNs => &TIME_PERF_COUNTER_NS_SPEC,
        StdlibMethodId::TimeProcessTime => &TIME_PROCESS_TIME_SPEC,
        StdlibMethodId::TimeProcessTimeNs => &TIME_PROCESS_TIME_NS_SPEC,
        StdlibMethodId::TimeSleep => &TIME_SLEEP_SPEC,
        StdlibMethodId::TimeLocaltime => &TIME_LOCALTIME_SPEC,
        StdlibMethodId::TimeGmtime => &TIME_GMTIME_SPEC,
        StdlibMethodId::TimeStrftime => &TIME_STRFTIME_SPEC,
        StdlibMethodId::TimeStrptime => &TIME_STRPTIME_SPEC,
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

/// Resolve the static type for `math.pi`.
fn type_math_pi_attr() -> Type {
    Type::Float
}

/// Resolve the static type for `math.e`.
fn type_math_e_attr() -> Type {
    Type::Float
}

/// Resolve the static type for `math.tau`.
fn type_math_tau_attr() -> Type {
    Type::Float
}

/// Resolve the static type for `math.inf`.
fn type_math_inf_attr() -> Type {
    Type::Float
}

/// Resolve the static type for `math.nan`.
fn type_math_nan_attr() -> Type {
    Type::Float
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

/// Emit `math.pi` attribute expression.
fn codegen_math_pi_attr(_codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    Ok("std::f64::consts::PI".to_string())
}

/// Emit `math.e` attribute expression.
fn codegen_math_e_attr(_codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    Ok("std::f64::consts::E".to_string())
}

/// Emit `math.tau` attribute expression.
fn codegen_math_tau_attr(_codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    Ok("std::f64::consts::TAU".to_string())
}

/// Emit `math.inf` attribute expression.
fn codegen_math_inf_attr(_codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    Ok("f64::INFINITY".to_string())
}

/// Emit `math.nan` attribute expression.
fn codegen_math_nan_attr(_codegen: &mut Codegen<'_>, _span: Span) -> Result<String, CompileError> {
    Ok("f64::NAN".to_string())
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

/// Emit code for `re.search(pattern, string)`.
fn codegen_re_search(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_re_search = true;
    let pattern_expr = codegen.gen_expr(&args[0])?;
    let value_expr = codegen.gen_expr(&args[1])?;
    Ok(format!(
        "py_re_search(&({}), &({}))",
        pattern_expr, value_expr
    ))
}

/// Emit code for `re.match(pattern, string)`.
fn codegen_re_match(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_re_match = true;
    let pattern_expr = codegen.gen_expr(&args[0])?;
    let value_expr = codegen.gen_expr(&args[1])?;
    Ok(format!(
        "py_re_match(&({}), &({}))",
        pattern_expr, value_expr
    ))
}

/// Emit code for `re.sub(pattern, repl, string)`.
fn codegen_re_sub(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_re_sub = true;
    let pattern_expr = codegen.gen_expr(&args[0])?;
    let repl_expr = codegen.gen_expr(&args[1])?;
    let value_expr = codegen.gen_expr(&args[2])?;
    Ok(format!(
        "py_re_sub(&({}), &({}), &({}))",
        pattern_expr, repl_expr, value_expr
    ))
}

/// Emit code for `json.dumps(value)`.
fn codegen_json_dumps(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_json_dumps = true;
    let value_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_json_dumps(&({}))", value_expr)))
}

/// Emit code for `json.loads(text)`.
fn codegen_json_loads(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_json_loads = true;
    let text_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_json_loads(&({}))", text_expr)))
}

/// Emit code for `json.dump(value, file)`.
fn codegen_json_dump(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_json_dump = true;
    let value_expr = codegen.gen_expr(&args[0])?;
    let file_expr = codegen.gen_expr(&args[1])?;
    Ok(codegen.wrap_result(format!(
        "py_json_dump(&({}), &mut ({}))",
        value_expr, file_expr
    )))
}

/// Emit code for `json.load(file)`.
fn codegen_json_load(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_json_load = true;
    let file_expr = codegen.gen_expr(&args[0])?;
    Ok(codegen.wrap_result(format!("py_json_load(&mut ({}))", file_expr)))
}

/// Render one numeric argument as `f64` for `math` unary calls.
fn gen_math_float_arg(codegen: &mut Codegen<'_>, arg: &Expr) -> Result<String, CompileError> {
    codegen.gen_expr_with_expected(arg, Some(&Type::Float))
}

/// Render one numeric argument as `i64` for integer-only `math` calls.
fn gen_math_int_arg(codegen: &mut Codegen<'_>, arg: &Expr) -> Result<String, CompileError> {
    codegen.gen_expr_with_expected(arg, Some(&Type::Int))
}

/// Render two numeric arguments as `f64`.
fn gen_math_float_args(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
) -> Result<(String, String), CompileError> {
    let left = gen_math_float_arg(codegen, &args[0])?;
    let right = gen_math_float_arg(codegen, &args[1])?;
    Ok((left, right))
}

/// Render two numeric arguments as `i64`.
fn gen_math_int_args(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
) -> Result<(String, String), CompileError> {
    let left = gen_math_int_arg(codegen, &args[0])?;
    let right = gen_math_int_arg(codegen, &args[1])?;
    Ok((left, right))
}

/// Emit code for `math.sqrt(x)`.
fn codegen_math_sqrt(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).sqrt()", value))
}

/// Emit code for `math.sin(x)`.
fn codegen_math_sin(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).sin()", value))
}

/// Emit code for `math.cos(x)`.
fn codegen_math_cos(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).cos()", value))
}

/// Emit code for `math.tan(x)`.
fn codegen_math_tan(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).tan()", value))
}

/// Emit code for `math.ceil(x)`.
fn codegen_math_ceil(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).ceil() as i64", value))
}

/// Emit code for `math.floor(x)`.
fn codegen_math_floor(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).floor() as i64", value))
}

/// Emit code for `math.factorial(n)`.
fn codegen_math_factorial(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_math_factorial = true;
    let value = gen_math_int_arg(codegen, &args[0])?;
    Ok(codegen.wrap_result(format!("py_math_factorial({})", value)))
}

/// Emit code for `math.log(x[, base])`.
fn codegen_math_log(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    if args.len() == 2 {
        let base = gen_math_float_arg(codegen, &args[1])?;
        return Ok(format!("({}).log({})", value, base));
    }
    Ok(format!("({}).ln()", value))
}

/// Emit code for `math.log2(x)`.
fn codegen_math_log2(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).log2()", value))
}

/// Emit code for `math.log10(x)`.
fn codegen_math_log10(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).log10()", value))
}

/// Emit code for `math.exp(x)`.
fn codegen_math_exp(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).exp()", value))
}

/// Emit code for `math.asin(x)`.
fn codegen_math_asin(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).asin()", value))
}

/// Emit code for `math.acos(x)`.
fn codegen_math_acos(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).acos()", value))
}

/// Emit code for `math.atan(x)`.
fn codegen_math_atan(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).atan()", value))
}

/// Emit code for `math.sinh(x)`.
fn codegen_math_sinh(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).sinh()", value))
}

/// Emit code for `math.cosh(x)`.
fn codegen_math_cosh(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).cosh()", value))
}

/// Emit code for `math.tanh(x)`.
fn codegen_math_tanh(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).tanh()", value))
}

/// Emit code for `math.fabs(x)`.
fn codegen_math_fabs(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).abs()", value))
}

/// Emit code for `math.degrees(x)`.
fn codegen_math_degrees(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).to_degrees()", value))
}

/// Emit code for `math.radians(x)`.
fn codegen_math_radians(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).to_radians()", value))
}

/// Emit code for `math.trunc(x)`.
fn codegen_math_trunc(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).trunc() as i64", value))
}

/// Emit code for `math.isnan(x)`.
fn codegen_math_isnan(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).is_nan()", value))
}

/// Emit code for `math.isinf(x)`.
fn codegen_math_isinf(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).is_infinite()", value))
}

/// Emit code for `math.isfinite(x)`.
fn codegen_math_isfinite(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let value = gen_math_float_arg(codegen, &args[0])?;
    Ok(format!("({}).is_finite()", value))
}

/// Emit code for `math.atan2(y, x)`.
fn codegen_math_atan2(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let (y, x) = gen_math_float_args(codegen, args)?;
    Ok(format!("({}).atan2({})", y, x))
}

/// Emit code for `math.fmod(x, y)`.
fn codegen_math_fmod(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let (left, right) = gen_math_float_args(codegen, args)?;
    Ok(format!("({}) % ({})", left, right))
}

/// Emit code for `math.copysign(x, y)`.
fn codegen_math_copysign(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let (left, right) = gen_math_float_args(codegen, args)?;
    Ok(format!("({}).copysign({})", left, right))
}

/// Emit code for `math.hypot(x, y)`.
fn codegen_math_hypot(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let (left, right) = gen_math_float_args(codegen, args)?;
    Ok(format!("({}).hypot({})", left, right))
}

/// Emit code for `math.pow(x, y)`.
fn codegen_math_pow(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    let (left, right) = gen_math_float_args(codegen, args)?;
    Ok(format!("({}).powf({})", left, right))
}

/// Emit code for `math.gcd(a, b)`.
fn codegen_math_gcd(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_math_gcd = true;
    let (left, right) = gen_math_int_args(codegen, args)?;
    Ok(format!("py_math_gcd({}, {})", left, right))
}

/// Emit code for `math.lcm(a, b)`.
fn codegen_math_lcm(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_math_lcm = true;
    let (left, right) = gen_math_int_args(codegen, args)?;
    Ok(codegen.wrap_result(format!("py_math_lcm({}, {})", left, right)))
}

/// Emit code for `math.comb(n, k)`.
fn codegen_math_comb(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_math_comb = true;
    let (left, right) = gen_math_int_args(codegen, args)?;
    Ok(codegen.wrap_result(format!("py_math_comb({}, {})", left, right)))
}

/// Emit code for `math.perm(n, k)`.
fn codegen_math_perm(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_math_perm = true;
    let (left, right) = gen_math_int_args(codegen, args)?;
    Ok(codegen.wrap_result(format!("py_math_perm({}, {})", left, right)))
}

/// Emit code for `time.time()`.
fn codegen_time_time(
    _codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    Ok("std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_secs_f64()".to_string())
}

/// Emit code for `time.time_ns()`.
fn codegen_time_time_ns(
    _codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    Ok("std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_nanos() as i64".to_string())
}

/// Emit code for `time.monotonic()`.
fn codegen_time_monotonic(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_monotonic = true;
    Ok("py_time_monotonic()".to_string())
}

/// Emit code for `time.monotonic_ns()`.
fn codegen_time_monotonic_ns(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_monotonic_ns = true;
    Ok("py_time_monotonic_ns()".to_string())
}

/// Emit code for `time.perf_counter()`.
fn codegen_time_perf_counter(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_perf_counter = true;
    Ok("py_time_perf_counter()".to_string())
}

/// Emit code for `time.perf_counter_ns()`.
fn codegen_time_perf_counter_ns(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_perf_counter_ns = true;
    Ok("py_time_perf_counter_ns()".to_string())
}

/// Emit code for `time.process_time()`.
fn codegen_time_process_time(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_process_time = true;
    Ok("py_time_process_time()".to_string())
}

/// Emit code for `time.process_time_ns()`.
fn codegen_time_process_time_ns(
    codegen: &mut Codegen<'_>,
    _args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_process_time_ns = true;
    Ok("py_time_process_time_ns()".to_string())
}

/// Emit code for `time.sleep(seconds)`.
fn codegen_time_sleep(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_sleep = true;
    let seconds_expr = codegen.gen_expr_with_expected(&args[0], Some(&Type::Float))?;
    Ok(format!("py_time_sleep({})", seconds_expr))
}

/// Emit code for `time.localtime([secs])`.
fn codegen_time_localtime(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_localtime = true;
    if args.is_empty() {
        return Ok("py_time_localtime(None)".to_string());
    }
    let seconds_expr = codegen.gen_expr_with_expected(&args[0], Some(&Type::Float))?;
    Ok(format!("py_time_localtime(Some({}))", seconds_expr))
}

/// Emit code for `time.gmtime([secs])`.
fn codegen_time_gmtime(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_gmtime = true;
    if args.is_empty() {
        return Ok("py_time_gmtime(None)".to_string());
    }
    let seconds_expr = codegen.gen_expr_with_expected(&args[0], Some(&Type::Float))?;
    Ok(format!("py_time_gmtime(Some({}))", seconds_expr))
}

/// Emit code for `time.strftime(format, t)`.
fn codegen_time_strftime(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_strftime = true;
    let format_expr = codegen.gen_expr_with_expected(&args[0], Some(&Type::Str))?;
    let tuple_expr = codegen.gen_expr(&args[1])?;
    Ok(format!(
        "py_time_strftime(&({}), &({}))",
        format_expr, tuple_expr
    ))
}

/// Emit code for `time.strptime(string, format)`.
fn codegen_time_strptime(
    codegen: &mut Codegen<'_>,
    args: &[Expr],
    _keywords: &[KeywordArg],
) -> Result<String, CompileError> {
    codegen.uses.py_time_strptime = true;
    let text_expr = codegen.gen_expr_with_expected(&args[0], Some(&Type::Str))?;
    let format_expr = codegen.gen_expr_with_expected(&args[1], Some(&Type::Str))?;
    Ok(format!(
        "py_time_strptime(&({}), &({}))",
        text_expr, format_expr
    ))
}
