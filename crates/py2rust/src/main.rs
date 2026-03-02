#![forbid(unsafe_code)]

use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// Command-line arguments for the py2rust transpiler.
///
/// We use clap's derive API for simple, declarative argument parsing.
/// The workflow supports several modes:
/// 1. Basic: `py2rust input.py` → generates input.rs
/// 2. Custom output: `py2rust input.py --output custom.rs`
/// 3. Compile: `py2rust input.py --compile` → generates and compiles to binary
/// 4. Run: `py2rust input.py --run` → generates, compiles, and executes
/// 5. Debug: `--emit-hir`, `--emit-types`, `--pretty` for development
#[derive(Parser, Debug)]
#[command(name = "py2rust")]
#[command(about = "Transpile a restricted Python subset to Rust")]
struct Cli {
    /// Input Python file to transpile
    input: String,

    /// Output Rust file path (default: input.rs)
    #[arg(short, long)]
    output: Option<String>,

    /// Compile the generated Rust code to an executable
    #[arg(long)]
    compile: bool,

    /// Compile and run the generated executable
    #[arg(long)]
    run: bool,

    /// Emit debug HIR representation
    #[arg(long)]
    emit_hir: bool,

    /// Emit debug type information
    #[arg(long)]
    emit_types: bool,

    /// Format generated code with rustfmt
    #[arg(long)]
    pretty: bool,
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    // Read input Python source
    let source = fs::read_to_string(&cli.input)
        .map_err(|err| miette::miette!("failed to read {}: {}", cli.input, err))?;

    // Determine output path
    // Default behavior: input.py → input.rs in the same directory
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

    // Compile Python to Rust
    let opts = py2rust::CompileOptions {
        emit_hir: cli.emit_hir,
        emit_types: cli.emit_types,
        pretty: cli.pretty,
    };
    let out = py2rust::compile(&source, &cli.input, &opts)?;

    // Print warnings (errors are returned as Err and handled by miette)
    for warning in out.warnings {
        eprintln!("{:?}", miette::Report::new(warning));
    }

    // Write generated Rust code
    fs::write(&output_path, out.rust)
        .map_err(|err| miette::miette!("failed to write {}: {}", output_path, err))?;

    // Convert output path to absolute for rustfmt and rustc
    // This avoids issues with relative paths when invoking external tools
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

    // Optionally format with rustfmt
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

    // Optionally compile and/or run
    if cli.compile || cli.run {
        let bin_path = output_rs.with_extension("");
        let compile_output = py2rust::toolchain::compile_rustc(
            &output_rs,
            &bin_path,
            &py2rust::toolchain::RustcOptions {
                strip_symbols: true,
                opt_level: 3,
                ..Default::default()
            },
        )
        .map_err(|err| miette::miette!("failed to invoke rustc: {}", err))?;

        // Forward rustc's output to the user
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

        // Optionally run the compiled binary
        if cli.run {
            let run_output = py2rust::toolchain::run_binary(&bin_path)
                .map_err(|err| miette::miette!("failed to run {}: {}", bin_path.display(), err))?;

            // Forward program output to the user
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

    // Print debug information if requested
    if let Some(hir) = out.hir {
        println!("=== HIR ===\n{hir}");
    }
    if let Some(types) = out.types {
        println!("=== TYPES ===\n{types}");
    }
    Ok(())
}
