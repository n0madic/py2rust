//! Differential compatibility checks against a local CPython interpreter.
//!
//! These tests intentionally cover a small deterministic subset and compare
//! CPython stdout with transpiled-binary stdout. They are designed to grow
//! over time as compatibility coverage expands.

use py2rust::toolchain::{compile_rustc, run_binary, RustcOptions};
use py2rust::{compile, CompileOptions};
use std::fs;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// Resolve a local Python executable suitable for differential checks.
fn discover_python() -> Option<String> {
    for candidate in ["python3.12", "python3", "python"] {
        let Ok(output) = Command::new(candidate).arg("--version").output() else {
            continue;
        };
        if output.status.success() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Execute source directly with CPython.
fn run_on_cpython(python_exe: &str, source: &str) -> std::io::Result<Output> {
    Command::new(python_exe).arg("-c").arg(source).output()
}

/// Execute source through py2rust + rustc.
fn run_on_py2rust(test_name: &str, source: &str) -> std::io::Result<Output> {
    let out = compile(
        source,
        &format!("{test_name}.py"),
        &CompileOptions::default(),
    )
    .expect("py2rust compilation failed");

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("py2rust_diff_{test_name}_{unique}"));
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir)?;

    let rs_path = tmp_dir.join("main.rs");
    let bin_path = tmp_dir.join(test_name);
    fs::write(&rs_path, &out.rust)?;

    let compile_output = compile_rustc(&rs_path, &bin_path, &RustcOptions::default())?;
    assert!(
        compile_output.status.success(),
        "rustc failed for {test_name}.\nstderr: {}",
        String::from_utf8_lossy(&compile_output.stderr)
    );

    let run_output = run_binary(&bin_path);
    let _ = fs::remove_dir_all(&tmp_dir);
    run_output
}

/// Compare stdout/exit status between CPython and transpiled output.
fn assert_stdout_parity(test_name: &str, source: &str) {
    let Some(python_exe) = discover_python() else {
        eprintln!("Skipping differential test `{test_name}`: no Python interpreter found");
        return;
    };

    let py = run_on_cpython(&python_exe, source).expect("failed to run CPython");
    let rust = run_on_py2rust(test_name, source).expect("failed to run transpiled binary");

    assert_eq!(
        py.status.success(),
        rust.status.success(),
        "exit status mismatch for {test_name}"
    );
    assert_eq!(
        String::from_utf8_lossy(&py.stdout),
        String::from_utf8_lossy(&rust.stdout),
        "stdout mismatch for {test_name}"
    );
}

#[test]
fn differential_core_arithmetic_and_control_flow() {
    let source = r#"
def accumulate(n: int) -> int:
    total: int = 0
    for i in range(n):
        if i % 2 == 0:
            total = total + i
    return total

print(accumulate(10))
"#;
    assert_stdout_parity("core_arithmetic_and_control_flow", source);
}

#[test]
fn differential_positional_only_calls() {
    let source = r#"
def mix(a: int, /, b: int, *, c: int = 0) -> int:
    return a + b + c

print(mix(1, 2))
print(mix(1, b=3, c=4))
"#;
    assert_stdout_parity("positional_only_calls", source);
}
