//! Runtime tests for exception handling: try/except/finally/raise.

use crate::common::run_py;

#[test]
fn runtime_exceptions_comprehensive() {
    // The comprehensive script emits per-case progress logs; only successful
    // execution matters because each scenario is validated via assertions.
    run_py("exceptions", include_str!("exceptions.py"), None);
}
