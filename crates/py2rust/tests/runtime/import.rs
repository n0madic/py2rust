//! Runtime tests for import statements.

use crate::common::run_py;

#[test]
fn runtime_import_comprehensive() {
    run_py(
        "import",
        include_str!("import.py"),
        Some("All import tests passed!"),
    );
}
