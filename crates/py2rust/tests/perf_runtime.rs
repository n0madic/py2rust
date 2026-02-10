//! Ignored runtime performance harness for generated binaries.
//!
//! This test is intentionally ignored by default and is meant to be run
//! manually when collecting performance baselines.

use py2rust::toolchain::{compile_rustc, run_binary, RustcOptions};
use py2rust::{compile, CompileOptions};
use std::fs;
use std::process::Command;
use std::time::Instant;

const RUNS_PER_SCENARIO: usize = 11;

/// One perf scenario source used by the runtime benchmark harness.
struct Scenario {
    name: &'static str,
    source: &'static str,
}

/// Collected timing metrics for one benchmark scenario.
struct ScenarioMetrics {
    name: &'static str,
    compile_ms: f64,
    rustc_ms: f64,
    median_ms: f64,
    p90_ms: f64,
}

#[test]
#[ignore = "manual runtime benchmark harness"]
fn perf_runtime_harness() {
    let scenarios = vec![
        Scenario {
            name: "list-heavy",
            source: r#"
def main_work() -> int:
    xs: list[int] = []
    i: int = 0
    while i < 40000:
        xs.append(i % 97)
        if len(xs) > 256:
            xs.pop(0)
        i = i + 1
    total: int = 0
    for value in xs:
        total = total + value
    return total

print(main_work())
"#,
        },
        Scenario {
            name: "dict-heavy",
            source: r#"
def main_work() -> int:
    counts: dict[str, int] = {}
    i: int = 0
    while i < 30000:
        key: str = str(i % 257)
        if key in counts:
            prev: int = counts[key]
            counts[key] = prev + 1
        else:
            counts[key] = 1
        i = i + 1
    total: int = 0
    for key in counts:
        total = total + counts[key]
    return total

print(main_work())
"#,
        },
        Scenario {
            name: "iterator-pipeline",
            source: r#"
def main_work() -> int:
    xs: list[int] = []
    i: int = 0
    while i < 20000:
        xs.append(i)
        i = i + 1
    ys: list[int] = list(filter(lambda x: x % 3 == 0, map(lambda x: x + 1, xs)))
    total: int = 0
    for idx, value in enumerate(reversed(sorted(ys))):
        if idx >= 1000:
            break
        total = total + value
    return total

print(main_work())
"#,
        },
        Scenario {
            name: "numeric-loop",
            source: r#"
def main_work() -> float:
    acc: float = 0.0
    i: int = 1
    while i < 500000:
        acc = acc + float(i) * 0.000001
        acc = acc - float(i % 7) * 0.0000001
        i = i + 1
    return round(acc, 6)

print(main_work())
"#,
        },
        Scenario {
            name: "mixed-exception-path",
            source: r#"
def maybe_fail(v: int) -> int:
    if v % 19 == 0:
        raise ValueError("bad value")
    return v * 2

def main_work() -> int:
    total: int = 0
    i: int = 1
    while i < 40000:
        try:
            total = total + maybe_fail(i)
        except ValueError:
            total = total + 1
        i = i + 1
    return total

print(main_work())
"#,
        },
    ];

    let mut results = Vec::new();
    for scenario in &scenarios {
        results.push(run_scenario(scenario));
    }

    println!("scenario,compile_ms,rustc_ms,median_ms,p90_ms");
    for result in &results {
        println!(
            "{},{:.3},{:.3},{:.3},{:.3}",
            result.name, result.compile_ms, result.rustc_ms, result.median_ms, result.p90_ms
        );
    }
    println!("cpu_info={}", detect_cpu_info());
}

fn run_scenario(scenario: &Scenario) -> ScenarioMetrics {
    let compile_start = Instant::now();
    let output = compile(
        scenario.source,
        &format!("{}.py", scenario.name),
        &CompileOptions::default(),
    )
    .unwrap_or_else(|err| panic!("py2rust compile failed for {}: {err}", scenario.name));
    let compile_ms = compile_start.elapsed().as_secs_f64() * 1_000.0;

    let tmp_dir = std::env::temp_dir().join(format!(
        "py2rust_perf_runtime_{}_{}",
        scenario.name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir)
        .unwrap_or_else(|err| panic!("failed to create temp dir for {}: {err}", scenario.name));

    let rs_path = tmp_dir.join("main.rs");
    let bin_path = tmp_dir.join("bench_bin");
    fs::write(&rs_path, &output.rust)
        .unwrap_or_else(|err| panic!("failed to write rust source for {}: {err}", scenario.name));

    let rustc_start = Instant::now();
    let compile_output = compile_rustc(&rs_path, &bin_path, &RustcOptions::default())
        .unwrap_or_else(|err| panic!("failed to invoke rustc for {}: {err}", scenario.name));
    let rustc_ms = rustc_start.elapsed().as_secs_f64() * 1_000.0;
    assert!(
        compile_output.status.success(),
        "rustc failed for {}:\n{}",
        scenario.name,
        String::from_utf8_lossy(&compile_output.stderr)
    );

    let mut run_times_ms = Vec::with_capacity(RUNS_PER_SCENARIO);
    for _ in 0..RUNS_PER_SCENARIO {
        let start = Instant::now();
        let run_output = run_binary(&bin_path)
            .unwrap_or_else(|err| panic!("run failed for {}: {err}", scenario.name));
        let elapsed_ms = start.elapsed().as_secs_f64() * 1_000.0;
        assert!(
            run_output.status.success(),
            "binary failed for {}:\nstdout={}\nstderr={}",
            scenario.name,
            String::from_utf8_lossy(&run_output.stdout),
            String::from_utf8_lossy(&run_output.stderr)
        );
        run_times_ms.push(elapsed_ms);
    }

    let median_ms = percentile(&run_times_ms, 0.50);
    let p90_ms = percentile(&run_times_ms, 0.90);

    let _ = fs::remove_dir_all(&tmp_dir);

    ScenarioMetrics {
        name: scenario.name,
        compile_ms,
        rustc_ms,
        median_ms,
        p90_ms,
    }
}

/// Compute percentile using nearest-rank over sorted samples.
fn percentile(samples: &[f64], percentile: f64) -> f64 {
    assert!(
        !samples.is_empty(),
        "percentile requires at least one sample"
    );
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| {
        a.partial_cmp(b)
            .expect("sample comparison should not be NaN")
    });
    let rank = ((sorted.len() as f64) * percentile).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

/// Detect host CPU model for benchmark metadata.
fn detect_cpu_info() -> String {
    if let Ok(output) = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
    {
        if output.status.success() {
            let cpu = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !cpu.is_empty() {
                return cpu;
            }
        }
    }
    if let Ok(output) = Command::new("lscpu").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = text.lines().find(|line| line.starts_with("Model name:")) {
                return line.trim().to_string();
            }
        }
    }
    if let Ok(output) = Command::new("uname").arg("-m").output() {
        if output.status.success() {
            let arch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !arch.is_empty() {
                return format!("arch={arch} (model unavailable)");
            }
        }
    }
    "unknown (sandbox restricted)".to_string()
}
