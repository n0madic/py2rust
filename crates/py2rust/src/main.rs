#![forbid(unsafe_code)]

use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "py2rust")]
#[command(about = "Transpile a restricted Python subset to Rust")]
struct Cli {
    input: String,
    #[arg(short, long)]
    output: Option<String>,
    #[arg(long)]
    compile: bool,
    #[arg(long)]
    run: bool,
    #[arg(long)]
    emit_hir: bool,
    #[arg(long)]
    emit_types: bool,
    #[arg(long)]
    pretty: bool,
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    let source = fs::read_to_string(&cli.input)
        .map_err(|err| miette::miette!("failed to read {}: {}", cli.input, err))?;
    let output_path = match cli.output {
        Some(path) => path,
        None => {
            let input_path = Path::new(&cli.input);
            let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
            let stem = input_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            parent
                .join(format!("{stem}.rs"))
                .to_string_lossy()
                .to_string()
        }
    };
    let opts = py2rust::CompileOptions {
        emit_hir: cli.emit_hir,
        emit_types: cli.emit_types,
        pretty: cli.pretty,
    };
    let out = py2rust::compile(&source, &cli.input, &opts)?;
    // Print warnings
    for warning in out.warnings {
        eprintln!("{:?}", miette::Report::new(warning));
    }
    fs::write(&output_path, out.rust)
        .map_err(|err| miette::miette!("failed to write {}: {}", output_path, err))?;
    let output_rs = {
        let path = Path::new(&output_path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|err| miette::miette!("failed to get current dir: {}", err))?
                .join(path)
        }
    };
    if cli.pretty {
        let status = Command::new("rustfmt")
            .arg("--edition=2021")
            .arg(&output_rs)
            .status()
            .map_err(|err| miette::miette!("failed to invoke rustfmt: {}", err))?;
        if !status.success() {
            return Err(miette::miette!("rustfmt failed"));
        }
    }
    if cli.compile || cli.run {
        let bin_path = output_rs.with_extension("");
        let compile_output = py2rust::toolchain::compile_rustc(
            &output_rs,
            &bin_path,
            &py2rust::toolchain::RustcOptions {
                strip_symbols: true,
                ..Default::default()
            },
        )
        .map_err(|err| miette::miette!("failed to invoke rustc: {}", err))?;
        if !compile_output.stdout.is_empty() {
            std::io::stdout()
                .write_all(&compile_output.stdout)
                .map_err(|err| miette::miette!("failed to write rustc stdout: {}", err))?;
        }
        if !compile_output.stderr.is_empty() {
            std::io::stderr()
                .write_all(&compile_output.stderr)
                .map_err(|err| miette::miette!("failed to write rustc stderr: {}", err))?;
        }
        if !compile_output.status.success() {
            return Err(miette::miette!("rustc failed"));
        }
        if cli.run {
            let run_output = py2rust::toolchain::run_binary(&bin_path)
                .map_err(|err| miette::miette!("failed to run {}: {}", bin_path.display(), err))?;
            if !run_output.stdout.is_empty() {
                std::io::stdout()
                    .write_all(&run_output.stdout)
                    .map_err(|err| miette::miette!("failed to write program stdout: {}", err))?;
            }
            if !run_output.stderr.is_empty() {
                std::io::stderr()
                    .write_all(&run_output.stderr)
                    .map_err(|err| miette::miette!("failed to write program stderr: {}", err))?;
            }
            if !run_output.status.success() {
                return Err(miette::miette!(
                    "program exited with status {}",
                    run_output.status
                ));
            }
        }
    }
    if let Some(hir) = out.hir {
        println!("=== HIR ===\n{hir}");
    }
    if let Some(types) = out.types {
        println!("=== TYPES ===\n{types}");
    }
    Ok(())
}
