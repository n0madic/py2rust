//! Runtime tests for assertions.

use crate::common::run_py;

#[test]
fn runtime_assert_comprehensive() {
    run_py(
        "assert",
        include_str!("assert.py"),
        Some("All assertions passed!"),
    );
}
