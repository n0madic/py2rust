use std::path::Path;
use std::process::{Command, Output};

/// Options for rustc compilation.
///
/// We use rustc directly rather than cargo because:
/// 1. We're generating single-file programs, not multi-crate projects
/// 2. Direct rustc invocation is faster (no dependency resolution)
/// 3. We don't need any external dependencies (all helpers are injected)
/// 4. It simplifies the CLI (no Cargo.toml needed)
#[derive(Debug, Clone)]
pub struct RustcOptions {
    /// Rust edition to use (2021 is the latest stable as of writing)
    pub edition: &'static str,
    /// Strip debug symbols from the binary to reduce size.
    /// Enabled by default in the CLI for cleaner output.
    pub strip_symbols: bool,
}

impl Default for RustcOptions {
    fn default() -> Self {
        Self {
            edition: "2021",
            strip_symbols: false,
        }
    }
}

/// Compile a Rust source file to an executable using rustc.
///
/// This is a thin wrapper around `rustc` that sets up the necessary flags.
/// We always use the 2021 edition and optionally strip symbols.
///
/// Returns the command output (stdout, stderr, exit status) for error handling.
pub fn compile_rustc(
    rs_path: &Path,
    bin_path: &Path,
    opts: &RustcOptions,
) -> std::io::Result<Output> {
    let mut cmd = Command::new("rustc");
    cmd.arg(rs_path)
        .arg(format!("--edition={}", opts.edition))
        .arg("-o")
        .arg(bin_path);
    if opts.strip_symbols {
        // -C strip=symbols removes debug info, reducing binary size significantly
        cmd.arg("-C").arg("strip=symbols");
    }
    cmd.output()
}

/// Run a compiled binary and capture its output.
///
/// Used by the `--run` flag to execute the generated program and display results.
pub fn run_binary(bin_path: &Path) -> std::io::Result<Output> {
    Command::new(bin_path).output()
}
