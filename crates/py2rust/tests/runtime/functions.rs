//! Runtime tests for functions and recursion.

use crate::common::run_py;

#[test]
fn runtime_functions_comprehensive() {
    run_py("functions", include_str!("functions.py"), None);
}
