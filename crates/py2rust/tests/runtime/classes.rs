//! Runtime tests for classes and objects.

use crate::common::run_py;

#[test]
fn runtime_classes_comprehensive() {
    run_py("classes", include_str!("classes.py"), None);
}

#[test]
fn runtime_union_method_calls() {
    run_py("classes_union", include_str!("classes_union.py"), None);
}
