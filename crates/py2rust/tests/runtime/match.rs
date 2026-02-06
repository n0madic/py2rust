//! Runtime tests for match/case pattern matching.

use crate::common::run_py;

#[test]
fn runtime_match_comprehensive() {
    run_py(
        "match",
        include_str!("match.py"),
        Some("All match tests passed!"),
    );
}
