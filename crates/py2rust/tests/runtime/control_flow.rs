//! Runtime tests for control flow: loops, conditionals, break, continue.

use crate::common::run_py;

#[test]
fn runtime_control_flow_comprehensive() {
    run_py(
        "control_flow",
        include_str!("control_flow.py"),
        Some("All control flow tests passed!"),
    );
}
