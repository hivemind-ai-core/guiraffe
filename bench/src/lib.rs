//! Headless suite harness, explicit host metadata collection, and comparison statistics.
pub mod allocator;
mod data_ui;
mod production_frame;
pub mod stats;
pub mod suites;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use stats::Summary;
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Reproducible bootstrap configuration used by baseline and compare.
pub const RESAMPLES: usize = 10_000;
/// Fixed seed: repeated analysis of identical samples produces identical output.
pub const SEED: u64 = 0x6a09_e667_f3bc_c909;

/// A registered workload; setup belongs outside its measured `run` function.
#[derive(Clone, Copy)]
pub struct Suite {
    pub name: &'static str,
    pub run: fn(),
}

/// Complete context recorded with every measurement. Missing OS information is explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub commit: String,
    pub dirty: bool,
    pub rustc: String,
    pub cpu_model: String,
    pub governor: String,
    pub load_average: String,
    pub profile: String,
    pub build_profile: String,
    pub unix_seconds: u64,
    pub notes: String,
}

fn command_text(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    ensure!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// Explicitly collect machine information. No I/O occurs until called by the application.
pub fn metadata(root: &Path, profile: &str, notes: &str) -> Result<Metadata> {
    validate_profile(profile)?;
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name")
                    .and_then(|v| v.split_once(':'))
                    .map(|(_, v)| v.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "unavailable (no Linux /proc/cpuinfo)".into());
    Ok(Metadata {
        commit: command_text(root, "git", &["rev-parse", "HEAD"])?,
        dirty: !command_text(root, "git", &["status", "--porcelain"])?.is_empty(),
        rustc: command_text(root, "rustc", &["--version", "--verbose"])?,
        cpu_model: cpu,
        governor: fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .unwrap_or_else(|_| "unavailable".into())
            .trim()
            .into(),
        load_average: fs::read_to_string("/proc/loadavg")
            .unwrap_or_else(|_| "unavailable".into())
            .trim()
            .into(),
        profile: profile.into(),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        unix_seconds: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        notes: notes.into(),
    })
}

/// Validate a portable single path component before using a profile in output paths.
pub fn validate_profile(profile: &str) -> Result<()> {
    ensure!(
        !profile.is_empty()
            && profile != "."
            && profile != ".."
            && profile
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid profile: use letters, digits, - or _"
    );
    Ok(())
}

/// Serializable measurements. Values are nanoseconds per workload invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub schema_version: u32,
    pub suite: String,
    pub metric: String,
    pub metadata: Metadata,
    pub iterations: usize,
    pub warmup_iterations: usize,
    pub bootstrap_resamples: usize,
    pub bootstrap_seed: u64,
    pub samples: Vec<f64>,
    pub summary: Summary,
}

/// Run one workload with warmup. Public runs require at least ten samples.
pub fn run(suite: Suite, meta: Metadata, samples: usize, iterations: usize) -> Result<Record> {
    ensure!(samples >= 10, "at least ten samples required");
    let values = measure(suite, samples, iterations)?;
    record(suite.name, meta, values, iterations)
}

/// Low-level sampler used by interleaved comparison (one sample per subprocess).
pub fn measure(suite: Suite, samples: usize, iterations: usize) -> Result<Vec<f64>> {
    measure_cancellable(suite, samples, iterations, || false)
}

/// Sample with cancellation checks outside measured batches. Each sample runs at
/// least 1 ms, repeating `iterations`-sized batches to avoid clock quantization.
/// `iterations` is a minimum batch size, not a fixed final invocation count.
pub fn measure_cancellable(
    suite: Suite,
    samples: usize,
    iterations: usize,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<f64>> {
    ensure!(
        iterations > 0 && samples > 0,
        "iterations and samples must be positive"
    );
    ensure!(!cancelled(), "benchmark interrupted");
    for _ in 0..iterations {
        (suite.run)();
    }
    let mut values = Vec::with_capacity(samples);
    for _ in 0..samples {
        ensure!(!cancelled(), "benchmark interrupted");
        let mut elapsed = Duration::ZERO;
        let mut completed = 0_usize;
        while elapsed < Duration::from_millis(1) {
            ensure!(!cancelled(), "benchmark interrupted");
            let start = Instant::now();
            for _ in 0..iterations {
                (suite.run)();
            }
            elapsed += start.elapsed();
            completed = completed
                .checked_add(iterations)
                .context("iteration count overflow")?;
        }
        values.push(elapsed.as_secs_f64() * 1e9 / completed as f64);
    }
    Ok(values)
}

/// Construct a validated record from externally collected samples.
pub fn record(
    suite: &str,
    metadata: Metadata,
    samples: Vec<f64>,
    iterations: usize,
) -> Result<Record> {
    ensure!(
        samples.len() >= 10 && iterations > 0,
        "records need ten samples and positive iterations"
    );
    let summary = stats::summarize(&samples, RESAMPLES, SEED)?;
    Ok(Record {
        schema_version: 1,
        suite: suite.into(),
        metric: "wall_ns".into(),
        metadata,
        iterations,
        warmup_iterations: iterations,
        bootstrap_resamples: RESAMPLES,
        bootstrap_seed: SEED,
        samples,
        summary,
    })
}

/// Save a JSON record, creating the containing directory if needed.
pub fn write_record(path: &Path, record: &Record) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(record)?)
        .with_context(|| format!("writing {}", path.display()))
}

/// Reject mismatched measurement environments before comparing.
pub fn compatible(old: &Record, new: &Record, force: bool) -> Result<()> {
    ensure!(
        old.schema_version == 1 && new.schema_version == 1,
        "unsupported record schema"
    );
    ensure!(
        old.suite == new.suite && old.metric == new.metric && old.iterations == new.iterations,
        "incompatible workload, metric or iterations"
    );
    ensure!(
        force
            || (old.metadata.profile == new.metadata.profile
                && old.metadata.build_profile == new.metadata.build_profile),
        "refusing comparison across profiles; pass --force to override"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_paths_cannot_escape() {
        for profile in ["", ".", "..", "../x", "/tmp", "a/b", "a\\b"] {
            assert!(validate_profile(profile).is_err());
        }
        assert!(validate_profile("dev-laptop_1").is_ok());
    }
    #[test]
    fn run_rejects_short_or_empty_workloads() {
        let suite = Suite {
            name: "noop",
            run: || {},
        };
        assert!(measure(suite, 0, 1).is_err());
        assert!(measure(suite, 1, 0).is_err());
        assert_eq!(measure(suite, 10, 1).map(|v| v.len()).ok(), Some(10));
    }
    #[test]
    fn tiny_workloads_extend_samples_and_remain_positive() -> Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn tiny() {
            CALLS.fetch_add(1, Ordering::Relaxed);
        }
        let start = Instant::now();
        let values = measure(
            Suite {
                name: "tiny",
                run: tiny,
            },
            10,
            1,
        )?;
        assert!(start.elapsed() >= Duration::from_millis(10));
        assert!(CALLS.load(Ordering::Relaxed) > 11);
        assert!(values.iter().all(|v| v.is_finite() && *v > 0.));
        assert!(
            measure_cancellable(
                Suite {
                    name: "noop",
                    run: || {}
                },
                10,
                1,
                || true
            )
            .is_err()
        );
        Ok(())
    }
}
