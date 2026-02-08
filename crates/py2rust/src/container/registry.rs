//! Centralized method metadata for list/dict/set attribute calls.
//!
//! This registry keeps the supported surface and call-shape policy in one place so
//! type checking and codegen dispatch cannot drift independently.

use crate::callspec::{validate_call_shape, AritySpec, CallShape, CallShapeError, KeywordPolicy};

/// Identifier for a supported container family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerId {
    List,
    Dict,
    Set,
}

impl ContainerId {
    /// Python-visible container name used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Dict => "dict",
            Self::Set => "set",
        }
    }
}

/// Metadata for one supported container method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerMethodSpec {
    /// Container family this method belongs to.
    pub container: ContainerId,
    /// Python-visible method name.
    pub name: &'static str,
    /// Unified call-shape policy.
    pub shape: CallShape,
}

impl ContainerMethodSpec {
    /// Return a canonical callable display name for diagnostics.
    pub fn callable_name(self) -> String {
        format!("{}.{}()", self.container.as_str(), self.name)
    }

    /// Validate a call against this method shape.
    pub fn validate(
        self,
        positional: usize,
        keywords: &[Option<&str>],
    ) -> Result<(), CallShapeError> {
        validate_call_shape(&self.callable_name(), self.shape, positional, keywords)
    }
}

const NO_KW: KeywordPolicy = KeywordPolicy::None;

const CONTAINER_METHOD_SPECS: [ContainerMethodSpec; 26] = [
    // list
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "append",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "extend",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "pop",
        shape: CallShape {
            arity: AritySpec::Range { min: 0, max: 1 },
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "insert",
        shape: CallShape {
            arity: AritySpec::Exact(2),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "clear",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "copy",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "reverse",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "index",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "sort",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "count",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "remove",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    // dict
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "get",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "pop",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "clear",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "copy",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "update",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "keys",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "values",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "setdefault",
        shape: CallShape {
            arity: AritySpec::Range { min: 1, max: 2 },
            keywords: NO_KW,
        },
    },
    // set
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "add",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "remove",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "discard",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "clear",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "copy",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "extend",
        shape: CallShape {
            arity: AritySpec::Exact(1),
            keywords: NO_KW,
        },
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "pop",
        shape: CallShape {
            arity: AritySpec::Exact(0),
            keywords: NO_KW,
        },
    },
];

/// Return all supported container method specs.
pub fn all_container_methods() -> &'static [ContainerMethodSpec] {
    &CONTAINER_METHOD_SPECS
}

/// Resolve a supported container method by container family and name.
pub fn find_container_method(
    container: ContainerId,
    name: &str,
) -> Option<&'static ContainerMethodSpec> {
    CONTAINER_METHOD_SPECS
        .iter()
        .find(|spec| spec.container == container && spec.name == name)
}
