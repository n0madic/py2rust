//! Runtime tests for generators and generator expressions.

use crate::common::run_py;

#[test]
fn runtime_generators_comprehensive() {
    run_py("generators", include_str!("generators.py"), None);
}
