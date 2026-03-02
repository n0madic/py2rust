use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Descriptor for an external crate that may appear in generated Rust.
struct ExternalDependency {
    /// Rust crate name used in `--extern` and `lib<crate>-*.rlib` lookup.
    crate_name: &'static str,
    /// Token used to detect whether generated code references this crate.
    marker: &'static str,
    /// Human-friendly crate label for diagnostics.
    display_name: &'static str,
}

/// Registry of external dependencies that can be auto-linked for generated code.
const EXTERNAL_DEPENDENCIES: &[ExternalDependency] = &[
    ExternalDependency {
        crate_name: "regex_lite",
        marker: "regex_lite::",
        display_name: "regex-lite",
    },
    ExternalDependency {
        crate_name: "indexmap",
        marker: "indexmap::",
        display_name: "indexmap",
    },
    ExternalDependency {
        crate_name: "chrono",
        marker: "chrono::",
        display_name: "chrono",
    },
    ExternalDependency {
        crate_name: "ureq",
        marker: "ureq::",
        display_name: "ureq",
    },
];

/// Options for rustc compilation.
///
/// We use rustc directly rather than cargo because:
/// 1. We're generating single-file programs, not multi-crate projects
/// 2. Direct rustc invocation is faster (no dependency resolution)
/// 3. It simplifies the CLI (no Cargo.toml needed)
///
/// Note: generated Rust may reference external helper crates (for example
/// `regex-lite`, `indexmap`, `chrono`, or `ureq`) that must be linked
/// explicitly when invoking rustc.
#[derive(Debug, Clone)]
pub struct RustcOptions {
    /// Rust edition to use (2021 is the latest stable as of writing)
    pub edition: &'static str,
    /// Strip debug symbols from the binary to reduce size.
    /// Enabled by default in the CLI for cleaner output.
    pub strip_symbols: bool,
    /// Optimization level (0 = none/debug, 1 = basic, 2 = standard release, 3 = aggressive).
    /// The CLI defaults to 3 for fast binaries; tests keep 0 for fast compile times.
    pub opt_level: u8,
}

impl Default for RustcOptions {
    fn default() -> Self {
        Self {
            edition: "2021",
            strip_symbols: false,
            opt_level: 0,
        }
    }
}

/// Compile a Rust source file to an executable using rustc.
///
/// This is a thin wrapper around `rustc` that sets up the necessary flags.
/// We always use the 2021 edition and optionally strip symbols / apply optimization.
///
/// Returns the command output (stdout, stderr, exit status) for error handling.
pub fn compile_rustc(
    rs_path: &Path,
    bin_path: &Path,
    opts: &RustcOptions,
) -> std::io::Result<Output> {
    let source = fs::read_to_string(rs_path)?;
    let mut cmd = Command::new("rustc");
    cmd.arg(rs_path)
        .arg(format!("--edition={}", opts.edition))
        .arg("-o")
        .arg(bin_path);
    link_external_dependencies(&mut cmd, &source)?;
    if opts.strip_symbols {
        // -C strip=symbols removes debug info, reducing binary size significantly
        cmd.arg("-C").arg("strip=symbols");
    }
    if opts.opt_level > 0 {
        cmd.arg("-C").arg(format!("opt-level={}", opts.opt_level));
    }
    cmd.output()
}

/// Link all known external crates referenced by generated Rust.
fn link_external_dependencies(cmd: &mut Command, source: &str) -> std::io::Result<()> {
    for dependency in EXTERNAL_DEPENDENCIES {
        maybe_link_dependency(cmd, source, dependency)?;
    }
    Ok(())
}

/// Link one external crate when generated code references it.
fn maybe_link_dependency(
    cmd: &mut Command,
    source: &str,
    dependency: &ExternalDependency,
) -> std::io::Result<()> {
    if !source.contains(dependency.marker) {
        return Ok(());
    }
    let lib_path = find_dependency_rlib(dependency.crate_name).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "generated code requires {}, but lib{}*.rlib was not found",
                dependency.display_name, dependency.crate_name
            ),
        )
    })?;
    if let Some(parent) = lib_path.parent() {
        cmd.arg("-L")
            .arg(format!("dependency={}", parent.display()));
    }
    cmd.arg("--extern")
        .arg(format!("{}={}", dependency.crate_name, lib_path.display()));
    Ok(())
}

/// Resolve a dependency rlib path from common Cargo target locations.
fn find_dependency_rlib(crate_name: &str) -> Option<PathBuf> {
    let prefix = format!("lib{crate_name}-");
    for dir in dependency_search_dirs() {
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
            if name.starts_with(&prefix) && name.ends_with(".rlib") {
                return Some(path);
            }
        }
    }
    None
}

/// Candidate target directories where Cargo stores dependency rlibs.
fn dependency_search_dirs() -> Vec<PathBuf> {
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
