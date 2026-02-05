//! Runtime tests for operators: arithmetic, comparison, logical.

use crate::common::run_py;

#[test]
fn runtime_operators_comprehensive() {
    run_py(
        "operators",
        include_str!("operators.py"),
        Some("All operator tests passed!"),
    );
}
