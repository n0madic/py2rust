//! Runtime tests for iteration builtins and protocols.

use crate::common::run_py;

#[test]
fn runtime_iteration_comprehensive() {
    run_py("iteration", include_str!("iteration.py"), None);
}
