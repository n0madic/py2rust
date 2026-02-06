//! Runtime tests for type-system features and narrowing behavior.

use crate::common::run_py;

#[test]
fn runtime_types_system_comprehensive() {
    run_py("types_system", include_str!("types_system.py"), None);
}
