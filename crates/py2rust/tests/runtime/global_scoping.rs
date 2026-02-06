//! Runtime tests for global variable scoping.

use crate::common::run_py;

#[test]
fn runtime_global_scoping_comprehensive() {
    run_py(
        "global_scoping",
        include_str!("global_scoping.py"),
        Some("All global scoping tests passed!"),
    );
}
