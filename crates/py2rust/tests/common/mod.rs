//! Common test utilities for py2rust tests.

use py2rust::toolchain::{compile_rustc, run_binary, RustcOptions};
use py2rust::{compile, CompileOptions};
use std::fs;

/// Helper to run a Python source through py2rust, compile with rustc, and execute.
/// If expected_output is Some, also verify stdout matches.
pub fn run_py(name: &str, python_source: &str, expected_output: Option<&str>) {
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
