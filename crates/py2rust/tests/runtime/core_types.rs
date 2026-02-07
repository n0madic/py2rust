//! Runtime tests for core types and operator behavior.

use crate::common::run_py;

#[test]
fn runtime_core_types_comprehensive() {
    // Keep this test source authoritative and run it end-to-end without expected-output pinning.
    run_py("core_types", include_str!("core_types.py"), None);
}
