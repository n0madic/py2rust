//! Centralized registry for supported Python builtins.
//!
//! This module is the single source of truth for:
//! - builtin name resolution,
//! - keyword-argument support policy,
//! - and shared builtin identifiers used across type checking and codegen scans.

/// Stable identifier for a supported Python builtin function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinId {
    Abs,
    All,
    Any,
    Ascii,
    Bin,
    Bool,
    Bytes,
    Chr,
    Dict,
    Divmod,
    Enumerate,
    Exit,
    Filter,
    Float,
    Hash,
    Hex,
    Id,
    Int,
    IsInstance,
    Iter,
    Len,
    List,
    Map,
    Max,
    Min,
    Next,
    Oct,
    Open,
    Ord,
    Pow,
    Print,
    Range,
    Repr,
    Reversed,
    Round,
    Set,
    Sorted,
    Str,
    Sum,
    Super,
    Tuple,
    Type,
    Zip,
}

/// Metadata for one supported builtin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSpec {
    /// Stable builtin identifier.
    pub id: BuiltinId,
    /// Python-visible builtin name.
    pub name: &'static str,
    /// Whether keyword arguments are accepted.
    pub allow_keywords: bool,
}

const BUILTIN_SPECS: [BuiltinSpec; 43] = [
    BuiltinSpec {
        id: BuiltinId::Abs,
        name: "abs",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::All,
        name: "all",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Any,
        name: "any",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Ascii,
        name: "ascii",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Bin,
        name: "bin",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Bool,
        name: "bool",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Bytes,
        name: "bytes",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Chr,
        name: "chr",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Dict,
        name: "dict",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Divmod,
        name: "divmod",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Enumerate,
        name: "enumerate",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Exit,
        name: "exit",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Filter,
        name: "filter",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Float,
        name: "float",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Hash,
        name: "hash",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Hex,
        name: "hex",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Id,
        name: "id",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Int,
        name: "int",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::IsInstance,
        name: "isinstance",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Iter,
        name: "iter",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Len,
        name: "len",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::List,
        name: "list",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Map,
        name: "map",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Max,
        name: "max",
        allow_keywords: true,
    },
    BuiltinSpec {
        id: BuiltinId::Min,
        name: "min",
        allow_keywords: true,
    },
    BuiltinSpec {
        id: BuiltinId::Next,
        name: "next",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Oct,
        name: "oct",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Open,
        name: "open",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Ord,
        name: "ord",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Pow,
        name: "pow",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Print,
        name: "print",
        allow_keywords: true,
    },
    BuiltinSpec {
        id: BuiltinId::Range,
        name: "range",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Repr,
        name: "repr",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Reversed,
        name: "reversed",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Round,
        name: "round",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Set,
        name: "set",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Sorted,
        name: "sorted",
        allow_keywords: true,
    },
    BuiltinSpec {
        id: BuiltinId::Str,
        name: "str",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Sum,
        name: "sum",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Super,
        name: "super",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Tuple,
        name: "tuple",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Type,
        name: "type",
        allow_keywords: false,
    },
    BuiltinSpec {
        id: BuiltinId::Zip,
        name: "zip",
        allow_keywords: false,
    },
];

/// Return all builtin specs.
pub fn builtin_specs() -> &'static [BuiltinSpec] {
    &BUILTIN_SPECS
}

/// Resolve a builtin name to metadata.
pub fn resolve_builtin(name: &str) -> Option<&'static BuiltinSpec> {
    BUILTIN_SPECS.iter().find(|spec| spec.name == name)
}
