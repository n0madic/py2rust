//! Runtime integration tests - organized by category.
//!
//! Each category is in a separate file under runtime/ and contains
//! a single comprehensive test to minimize compilation overhead.

mod common;

/// Generate a standard runtime integration test body around `run_py`.
macro_rules! runtime_case {
    ($test_name:ident, $suite_name:literal, $source_file:literal) => {
        #[test]
        fn $test_name() {
            crate::common::run_py($suite_name, include_str!($source_file), None);
        }
    };
    ($test_name:ident, $suite_name:literal, $source_file:literal, $expected:expr) => {
        #[test]
        fn $test_name() {
            crate::common::run_py($suite_name, include_str!($source_file), Some($expected));
        }
    };
}

pub(crate) use runtime_case;

#[path = "runtime/assert.rs"]
mod assert_tests;
#[path = "runtime/builtins.rs"]
mod builtins;
#[path = "runtime/classes.rs"]
mod classes;
#[path = "runtime/collections.rs"]
mod collections;
#[path = "runtime/comprehensions.rs"]
mod comprehensions;
#[path = "runtime/control_flow.rs"]
mod control_flow;
#[path = "runtime/core_types.rs"]
mod core_types;
#[path = "runtime/exceptions.rs"]
mod exceptions;
#[path = "runtime/file_io.rs"]
mod file_io;
#[path = "runtime/functions.rs"]
mod functions;
#[path = "runtime/generators.rs"]
mod generators;
#[path = "runtime/global_scoping.rs"]
mod global_scoping;
#[path = "runtime/import.rs"]
mod import;
#[path = "runtime/io.rs"]
mod io;
#[path = "runtime/iteration.rs"]
mod iteration;
#[path = "runtime/match.rs"]
mod match_tests;
#[path = "runtime/operators.rs"]
mod operators;
#[path = "runtime/strings.rs"]
mod strings;
#[path = "runtime/types_system.rs"]
mod types_system;
