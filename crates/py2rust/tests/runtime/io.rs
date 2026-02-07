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
fn runtime_print_comprehensive() {
    run_py(
        "print",
        include_str!("print.py"),
        Some(
            "42\n-7\n0\n3.14\n1\n0\ntrue\nfalse\nNone\nhello\n\n\n1 2 3\na b c\n1 hello true 3.14\n1 None 2\n1-2-3\na, b, c\n12\nhello world\nline1\nline2\n1-2-3!\n[1, 2, 3]\n['a', 'b']\n[]\n(1, 2, 3)\n(42,)\n(\"hello\", \"world\")\n[[1, 2], [3, 4]]\n{}\n{\"x\": 1}\n{}\n{42}\n[104, 101, 108, 108, 111]\n[]\n30\n200\n0\n1\n2\nHello World",
        ),
    );
}

#[test]
fn runtime_sys_exit() {
    run_py(
        "sys_exit",
        include_str!("sys_exit.py"),
        Some("before sys.exit"),
    );
}
