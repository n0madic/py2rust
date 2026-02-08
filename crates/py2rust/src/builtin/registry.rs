//! Centralized registry for supported Python builtins.
//!
//! This module is the single source of truth for:
//! - builtin name resolution,
//! - keyword-argument policy,
//! - arity policy,
//! - and shared builtin identifiers used across type checking and codegen scans.

use crate::callspec::{AritySpec, CallShape, KeywordPolicy};

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
    Input,
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
    /// Unified call-shape policy (arity + keyword support).
    pub shape: CallShape,
}

const PRINT_KEYWORDS: &[&str] = &["sep", "end"];
const SORTED_KEYWORDS: &[&str] = &["key", "reverse"];
const KEY_ONLY_KEYWORDS: &[&str] = &["key"];

const BUILTIN_SPECS: [BuiltinSpec; 44] = [
    BuiltinSpec {
        id: BuiltinId::Abs,
        name: "abs",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::All,
        name: "all",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Any,
        name: "any",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Ascii,
        name: "ascii",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Bin,
        name: "bin",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Bool,
        name: "bool",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Bytes,
        name: "bytes",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 2 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Chr,
        name: "chr",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Dict,
        name: "dict",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Divmod,
        name: "divmod",
        shape: CallShape {
            arity: AritySpec::Exact(2),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Enumerate,
        name: "enumerate",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Exit,
        name: "exit",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Filter,
        name: "filter",
        shape: CallShape {
            arity: AritySpec::Exact(2),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Float,
        name: "float",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Hash,
        name: "hash",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Hex,
        name: "hex",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Id,
        name: "id",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Input,
        name: "input",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Int,
        name: "int",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::IsInstance,
        name: "isinstance",
        shape: CallShape {
            arity: AritySpec::Exact(2),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Iter,
        name: "iter",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Len,
        name: "len",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::List,
        name: "list",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Map,
        name: "map",
        shape: CallShape {
            // Support tutorial-compatible two-iterable map(func, it1, it2).
            arity: AritySpec::Range { min: 2, max: 3 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Max,
        name: "max",
        shape: CallShape {
            arity: AritySpec::AtLeast(1),
            keywords: KeywordPolicy::Named(KEY_ONLY_KEYWORDS),
        },
    },
    BuiltinSpec {
        id: BuiltinId::Min,
        name: "min",
        shape: CallShape {
            arity: AritySpec::AtLeast(1),
            keywords: KeywordPolicy::Named(KEY_ONLY_KEYWORDS),
        },
    },
    BuiltinSpec {
        id: BuiltinId::Next,
        name: "next",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Oct,
        name: "oct",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Open,
        name: "open",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Ord,
        name: "ord",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Pow,
        name: "pow",
        shape: CallShape {
            arity: AritySpec::Exact(2),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Print,
        name: "print",
        shape: CallShape {
            arity: AritySpec::Any,
            keywords: KeywordPolicy::Named(PRINT_KEYWORDS),
        },
    },
    BuiltinSpec {
        id: BuiltinId::Range,
        name: "range",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 3 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Repr,
        name: "repr",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Reversed,
        name: "reversed",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Round,
        name: "round",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Set,
        name: "set",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Sorted,
        name: "sorted",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::Named(SORTED_KEYWORDS),
        },
    },
    BuiltinSpec {
        id: BuiltinId::Str,
        name: "str",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Sum,
        name: "sum",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Super,
        name: "super",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Tuple,
        name: "tuple",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Type,
        name: "type",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: KeywordPolicy::None,
        },
    },
    BuiltinSpec {
        id: BuiltinId::Zip,
        name: "zip",
        shape: CallShape {
            arity: AritySpec::Exact(2),
            keywords: KeywordPolicy::None,
        },
    },
];

/// Return all builtin specs.
pub fn all_builtins() -> &'static [BuiltinSpec] {
    &BUILTIN_SPECS
}

/// Resolve a builtin name to metadata.
pub fn find_builtin(name: &str) -> Option<&'static BuiltinSpec> {
    BUILTIN_SPECS.iter().find(|spec| spec.name == name)
}
