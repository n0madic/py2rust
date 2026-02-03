use py2rust::toolchain::{compile_rustc, run_binary, RustcOptions};
use py2rust::{compile, CompileOptions};
use std::fs;

/// Helper to run a Python source through py2rust, compile with rustc, and execute.
/// If expected_output is Some, also verify stdout matches.
fn run_py(name: &str, python_source: &str, expected_output: Option<&str>) {
    let out = compile(
        python_source,
        &format!("{name}.py"),
        &CompileOptions::default(),
    )
    .expect("py2rust compilation failed");

    let tmp_dir = std::env::temp_dir().join(format!("py2rust_test_{name}"));
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).expect("failed to create temp dir");

    let rs_path = tmp_dir.join("main.rs");
    let bin_path = tmp_dir.join(name);

    fs::write(&rs_path, &out.rust).expect("failed to write Rust source");

    // Compile with rustc
    let compile_output =
        compile_rustc(&rs_path, &bin_path, &RustcOptions::default()).expect("failed to run rustc");

    assert!(
        compile_output.status.success(),
        "rustc compilation failed for {name}.\nGenerated Rust:\n{}\nstderr: {}",
        out.rust,
        String::from_utf8_lossy(&compile_output.stderr)
    );

    // Execute the binary
    let run_output = run_binary(&bin_path).expect("failed to execute binary");

    let stdout = String::from_utf8_lossy(&run_output.stdout);
    let stderr = String::from_utf8_lossy(&run_output.stderr);

    assert!(
        run_output.status.success(),
        "runtime execution failed for {name} (exit code: {:?}).\nstdout: {stdout}\nstderr: {stderr}",
        run_output.status.code()
    );

    // Check expected output if provided
    if let Some(expected) = expected_output {
        assert_eq!(
            stdout.trim(),
            expected.trim(),
            "output mismatch for {name}.\nExpected: {expected}\nGot: {stdout}"
        );
    }

    // Cleanup
    let _ = fs::remove_dir_all(&tmp_dir);
}

#[test]
fn runtime_simple_arithmetic() {
    run_py(
        "arithmetic",
        r#"
def add(a: int, b: int) -> int:
    return a + b

x: int = add(2, 3)
assert x == 5, "x should be 5"
"#,
        None,
    );
}

#[test]
fn runtime_factorial() {
    run_py(
        "factorial",
        r#"
def factorial(n: int) -> int:
    if n <= 1:
        return 1
    else:
        return n * factorial(n - 1)

assert factorial(0) == 1
assert factorial(1) == 1
assert factorial(5) == 120
assert factorial(10) == 3628800
"#,
        None,
    );
}

#[test]
fn runtime_fibonacci() {
    run_py(
        "fibonacci",
        r#"
def fib(n: int) -> int:
    if n <= 1:
        return n
    else:
        return fib(n - 1) + fib(n - 2)

assert fib(0) == 0
assert fib(1) == 1
assert fib(10) == 55
assert fib(15) == 610
"#,
        None,
    );
}

#[test]
fn runtime_class_basic() {
    run_py(
        "class_basic",
        r#"
class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x: int = x
        self.y: int = y

    def sum(self) -> int:
        return self.x + self.y

p: Point = Point(3, 4)
assert p.x == 3
assert p.y == 4
assert p.sum() == 7
"#,
        None,
    );
}

#[test]
fn runtime_while_loop() {
    run_py(
        "while_loop",
        r#"
def sum_to(n: int) -> int:
    total: int = 0
    i: int = 1
    while i <= n:
        total = total + i
        i = i + 1
    return total

assert sum_to(0) == 0
assert sum_to(1) == 1
assert sum_to(10) == 55
assert sum_to(100) == 5050
"#,
        None,
    );
}

#[test]
fn runtime_list_operations() {
    run_py(
        "list_ops",
        r#"
def sum_list(xs: list[int]) -> int:
    total: int = 0
    for x in xs:
        total = total + x
    return total

nums: list[int] = [1, 2, 3, 4, 5]
assert sum_list(nums) == 15
assert sum_list([]) == 0
assert sum_list([10, 20, 30]) == 60
"#,
        None,
    );
}

#[test]
fn runtime_assert_with_message() {
    run_py(
        "assert_msg",
        r#"
x: int = 42
assert x == 42, "x should be 42"
"#,
        None,
    );
}

#[test]
fn runtime_boolean_logic() {
    run_py(
        "boolean_logic",
        r#"
assert True
assert not False
assert True and True
assert True or False
assert not (True and False)
"#,
        None,
    );
}

#[test]
fn runtime_comparison() {
    run_py(
        "comparison",
        r#"
assert 1 < 2
assert 2 <= 2
assert 3 > 2
assert 3 >= 3
assert 5 == 5
assert 5 != 6
"#,
        None,
    );
}

#[test]
fn runtime_print_output() {
    run_py(
        "print_output",
        r#"
print(42)
print("hello")
"#,
        Some("42\nhello"),
    );
}

#[test]
fn runtime_string_operations() {
    run_py(
        "string_ops",
        r#"
s: str = "hello"
assert len(s) == 5
print(len(s))
"#,
        Some("5"),
    );
}
