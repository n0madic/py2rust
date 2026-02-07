//! Centralized method metadata for list/dict/set attribute calls.
//!
//! This registry keeps the supported surface and arity policy in one place so
//! type checking and codegen dispatch cannot drift independently.

/// Identifier for a supported container family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerId {
    List,
    Dict,
    Set,
}

impl ContainerId {
    /// Python-visible container name used in diagnostics.
    fn as_str(self) -> &'static str {
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
    /// Minimum positional argument count.
    pub min_args: usize,
    /// Maximum positional argument count.
    pub max_args: usize,
}

impl ContainerMethodSpec {
    /// Return whether this method accepts the provided positional arg count.
    pub fn accepts_arity(self, arg_count: usize) -> bool {
        self.min_args <= arg_count && arg_count <= self.max_args
    }

    /// Build the canonical arity error message for this method.
    pub fn arity_error(self) -> String {
        format!(
            "{}.{}() expects {}",
            self.container.as_str(),
            self.name,
            describe_arity(self.min_args, self.max_args)
        )
    }
}

const CONTAINER_METHOD_SPECS: [ContainerMethodSpec; 22] = [
    // list
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "append",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "extend",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "pop",
        min_args: 0,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "insert",
        min_args: 2,
        max_args: 2,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "clear",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "copy",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "reverse",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "index",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "sort",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::List,
        name: "count",
        min_args: 1,
        max_args: 1,
    },
    // dict
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "get",
        min_args: 1,
        max_args: 2,
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "pop",
        min_args: 1,
        max_args: 2,
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "clear",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "copy",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::Dict,
        name: "update",
        min_args: 1,
        max_args: 1,
    },
    // set
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "add",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "remove",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "discard",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "clear",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "copy",
        min_args: 0,
        max_args: 0,
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "extend",
        min_args: 1,
        max_args: 1,
    },
    ContainerMethodSpec {
        container: ContainerId::Set,
        name: "pop",
        min_args: 0,
        max_args: 0,
    },
];

/// Return all supported container method specs.
pub fn container_method_specs() -> &'static [ContainerMethodSpec] {
    &CONTAINER_METHOD_SPECS
}

/// Resolve a supported container method by container family and name.
pub fn resolve_container_method(
    container: ContainerId,
    name: &str,
) -> Option<&'static ContainerMethodSpec> {
    CONTAINER_METHOD_SPECS
        .iter()
        .find(|spec| spec.container == container && spec.name == name)
}

fn describe_arity(min_args: usize, max_args: usize) -> &'static str {
    match (min_args, max_args) {
        (0, 0) => "no arguments",
        (1, 1) => "one argument",
        (2, 2) => "two arguments",
        (0, 1) => "zero or one argument",
        (1, 2) => "one or two arguments",
        _ => "a valid number of arguments",
    }
}
