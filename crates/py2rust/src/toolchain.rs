use std::path::Path;
use std::process::{Command, Output};

#[derive(Debug, Clone)]
pub struct RustcOptions {
    pub edition: &'static str,
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
        cmd.arg("-C").arg("strip=symbols");
    }
    cmd.output()
}

pub fn run_binary(bin_path: &Path) -> std::io::Result<Output> {
    Command::new(bin_path).output()
}
