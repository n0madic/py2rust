//! Runtime tests for exception handling: try/except/finally/raise.

use crate::common::run_py;

#[test]
fn runtime_exceptions_comprehensive() {
    run_py(
        "exceptions",
        include_str!("exceptions.py"),
        Some("All exception tests passed!"),
    );
}
