//! Runtime tests for collections: lists, strings, tuples, dictionaries.

use crate::common::run_py;

#[test]
fn runtime_collections_comprehensive() {
    run_py(
        "collections",
        include_str!("collections.py"),
        Some("All collection tests passed!"),
    );
}
