use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Options for rustc compilation.
///
/// We use rustc directly rather than cargo because:
/// 1. We're generating single-file programs, not multi-crate projects
/// 2. Direct rustc invocation is faster (no dependency resolution)
/// 3. It simplifies the CLI (no Cargo.toml needed)
///
/// Note: when generated code uses `re` helpers, we link `regex-lite`
/// as an external crate for rustc.
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
    maybe_link_regex_lite(&mut cmd, rs_path)?;
    if opts.strip_symbols {
        // -C strip=symbols removes debug info, reducing binary size significantly
        cmd.arg("-C").arg("strip=symbols");
    }
    cmd.output()
}

/// Link `regex-lite` when generated code references it.
fn maybe_link_regex_lite(cmd: &mut Command, rs_path: &Path) -> std::io::Result<()> {
    let source = fs::read_to_string(rs_path)?;
    if !source.contains("regex_lite::") {
        return Ok(());
    }
    let lib_path = find_regex_lite_rlib().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "generated code requires regex-lite, but libregex_lite*.rlib was not found",
        )
    })?;
    if let Some(parent) = lib_path.parent() {
        cmd.arg("-L")
            .arg(format!("dependency={}", parent.display()));
    }
    cmd.arg("--extern")
        .arg(format!("regex_lite={}", lib_path.display()));
    Ok(())
}

/// Resolve the built `regex-lite` rlib path from common Cargo target locations.
fn find_regex_lite_rlib() -> Option<PathBuf> {
    for dir in regex_lite_search_dirs() {
        if !dir.exists() || !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with("libregex_lite-") && name.ends_with(".rlib") {
                return Some(path);
            }
        }
    }
    None
}

/// Candidate target directories where Cargo stores dependency rlibs.
fn regex_lite_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            dirs.push(exe_dir.to_path_buf());
            dirs.push(exe_dir.join("deps"));
            if let Some(parent) = exe_dir.parent() {
                dirs.push(parent.join("deps"));
            }
        }
    }

    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        let target_dir = PathBuf::from(target_dir);
        dirs.push(target_dir.join("debug").join("deps"));
        dirs.push(target_dir.join("release").join("deps"));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dirs.push(
        manifest_dir
            .join("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("deps"),
    );
    dirs.push(
        manifest_dir
            .join("..")
            .join("..")
            .join("target")
            .join("release")
            .join("deps"),
    );

    let mut unique: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if !unique.iter().any(|existing| existing == &dir) {
            unique.push(dir);
        }
    }
    unique
}

/// Run a compiled binary and capture its output.
///
/// Used by the `--run` flag to execute the generated program and display results.
pub fn run_binary(bin_path: &Path) -> std::io::Result<Output> {
    Command::new(bin_path).output()
}
