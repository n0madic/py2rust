//! Runtime tests for Python string behavior.

use crate::common::run_py;

#[test]
fn runtime_strings_comprehensive() {
    // Keep this test source in Python to validate transpilation end-to-end.
    run_py("strings", include_str!("strings.py"), None);
}
