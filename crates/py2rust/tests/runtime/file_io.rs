//! Runtime tests for file I/O operations.

use crate::common::run_py;

#[test]
fn runtime_file_io_comprehensive() {
    run_py(
        "file_io",
        include_str!("file_io.py"),
        Some("All file I/O tests passed!"),
    );
}
