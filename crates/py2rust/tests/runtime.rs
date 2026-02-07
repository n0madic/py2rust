//! Runtime integration tests - organized by category.
//!
//! Each category is in a separate file under runtime/ and contains
//! a single comprehensive test to minimize compilation overhead.

mod common;

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
#[path = "runtime/global_scoping.rs"]
mod global_scoping;
#[path = "runtime/import.rs"]
mod import;
#[path = "runtime/io.rs"]
mod io;
#[path = "runtime/match.rs"]
mod match_tests;
#[path = "runtime/operators.rs"]
mod operators;
#[path = "runtime/strings.rs"]
mod strings;
#[path = "runtime/types_system.rs"]
mod types_system;
