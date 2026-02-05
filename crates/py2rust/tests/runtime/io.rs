//! Runtime tests for input/output operations.

use crate::common::run_py;

#[test]
fn runtime_io_comprehensive() {
    run_py(
        "io",
        include_str!("io.py"),
        Some("42\nhello\ntrue\nfalse\nworld\n30\nmessage from function\n1\n2\n3"),
    );
}

#[test]
fn runtime_print_core_types() {
    run_py(
        "print_core_types",
        include_str!("print_core_types.py"),

        Some(
            "42\n3.14\ntrue\nfalse\nNone\nHello, World!\n1 2 3\n42 3.14 true hello\n30\n0\n1\n2\n12",
        ),
    );
}
