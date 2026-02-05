// Runtime tests for Python builtins.

use crate::common::run_py;

#[test]
fn runtime_builtins_comprehensive() {
    run_py("builtins", include_str!("builtins.py"), None);
}
