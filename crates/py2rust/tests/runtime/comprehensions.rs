//! Runtime tests for comprehensions: list, set, and dict comprehensions.

use crate::common::run_py;

#[test]
fn runtime_comprehensions() {
    run_py(
        "comprehensions",
        include_str!("comprehensions.py"),
        Some("All comprehension tests passed!"),
    );
}
