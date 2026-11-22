//! Grow independent mounted fixtures until the first budget failure.
use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use std::{
    fmt::Write as _,
    io::{Read, Seek, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const SCENARIOS: &[(&str, &str, u64)] = &[
    ("grid", "rows", 50 * 8),
    ("code", "lines", 20),
    ("chart-line", "points", 16),
    ("chart-scatter", "points", 16),
    ("chart-stream", "retained points", 16),
    ("flow", "nodes (two edges each)", 74),
    ("timeline", "keys per channel (100 channels)", 800),
    ("gantt", "tasks", 36),
    ("gantt-fan-out", "tasks", 36),
    ("gantt-chain", "tasks", 36),
    ("tracks", "clips per track (100 tracks)", 1600),
    ("curve-editor", "keys", 49),
    ("bar-editor", "bars", 8),
    ("flamegraph", "frames", 50),
    ("tree", "branches (nine leaves each)", 148),
    ("property-grid", "groups (nine properties each)", 224),
    ("markdown", "minimum lines", 0),
    ("chat", "messages", 0),
    ("mermaid", "nodes", 0),
];

#[derive(Parser)]
struct Args {
    /// Probe just this fixture. By default all dataset scenarios run.
    component: Option<String>,
    #[arg(long, default_value_t = 16)]
    start: usize,
    /// Safety cap. Reaching it without a failure gives a lower bound.
    #[arg(long, default_value_t = 16_777_216)]
    max: usize,
    #[arg(long, default_value = "target/PERF-LIMITS.md")]
    output: PathBuf,
}

struct Measurement {
    mount_us: u64,
    p99_us: u64,
    rss_kib: u64,
    raw: u64,
}

impl Measurement {
    /// `rss_kib` is peak RSS; the budget applies to its increase over an empty window in the
    /// same harness (docs/performance.md).
    fn failures(&self, empty_kib: u64) -> Vec<&'static str> {
        let mut failures = Vec::new();
        if self.mount_us > 200_000 {
            failures.push("mount");
        }
        if self.p99_us > 8_000 {
            failures.push("p99");
        }
        if self.rss_kib.saturating_sub(empty_kib) * 1_024 > self.raw * 5 / 4 + 64 * 1_024 * 1_024 {
            failures.push("RSS");
        }
        failures
    }
}

fn probe(executable: &Path, component: &str, n: usize, bytes_per_item: u64) -> Result<Measurement> {
    let log_dir = executable.parent().context("missing example directory")?;
    let mut log = tempfile::tempfile_in(log_dir)?;
    let mut child = Command::new(executable)
        .arg(component)
        .env("SOAK_LIMIT_PROBE", component)
        .env("SOAK_ITEMS", n.to_string())
        .env_remove("SOAK_FRAMES")
        .env_remove("SOAK_SCALE")
        .env_remove("SOAK_FLOW_WARM_REPLAY")
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log.try_clone()?))
        .spawn()
        .with_context(|| format!("launching {}", executable.display()))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(15) {
            child.kill()?;
            child.wait()?;
            bail!("probe timed out after 15 s");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    log.rewind()?;
    let mut output = String::new();
    log.read_to_string(&mut output)?;
    ensure!(status.success(), "child {status}: {}", output.trim());
    let record = output
        .lines()
        .find_map(|line| line.strip_prefix("LIMIT "))
        .context("missing limit telemetry")?;
    let values: Vec<u64> = record
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let &[mount_us, p99_us, rss_kib] = values.as_slice() else {
        bail!("invalid limit telemetry: {record}");
    };
    let raw = match output.lines().find_map(|line| line.strip_prefix("RAW ")) {
        Some(bytes) => bytes.parse()?,
        None if component == "code" => 20 * n as u64 - 1,
        None if bytes_per_item > 0 => bytes_per_item * n as u64,
        None => bail!("{component} did not report raw input size"),
    };
    Ok(Measurement {
        mount_us,
        p99_us,
        rss_kib,
        raw,
    })
}

/// Median peak RSS of five empty windows, the baseline for every memory verdict.
fn empty_baseline(executable: &Path) -> Result<u64> {
    let mut peaks = (0..5)
        .map(|_| probe(executable, "empty", 1, 1).map(|measurement| measurement.rss_kib))
        .collect::<Result<Vec<_>>>()?;
    peaks.sort_unstable();
    Ok(peaks[peaks.len() / 2])
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        args.start >= 2 && args.max >= args.start && args.max <= 16_777_216,
        "require 2 <= start <= max <= 16,777,216"
    );
    let executable = std::env::current_exe()?.with_file_name("soak");
    ensure!(
        executable.is_file(),
        "build the soak example alongside limits"
    );
    let scenarios: Vec<_> = SCENARIOS
        .iter()
        .filter(|(name, _, _)| {
            args.component
                .as_deref()
                .is_none_or(|wanted| wanted == *name)
        })
        .collect();
    ensure!(!scenarios.is_empty(), "unknown component");
    let empty_kib = empty_baseline(&executable).context("measuring the empty-window baseline")?;
    let mut table = String::from(
        "| Component | Size unit | Largest passing | First failing | Budget or stop reason | Mount ms | p99 ms | RSS over empty MiB | Raw MiB |\n|---|---|---:|---:|---|---:|---:|---:|---:|\n",
    );
    let mut incomplete = false;
    for &(component, unit, bytes_per_item) in scenarios {
        let mut size = args.start;
        let mut passing = None;
        let (failing, reason, measurement) = loop {
            eprintln!("probing {component} at {size} {unit}");
            let measurement = match probe(&executable, component, size, bytes_per_item) {
                Ok(measurement) => measurement,
                Err(error) => {
                    eprintln!("{component} at {size}: {error:#}");
                    incomplete = true;
                    break (None, format!("probe error at {size}"), None);
                }
            };
            let failures = measurement.failures(empty_kib);
            if !failures.is_empty() {
                break (Some(size), failures.join(", "), Some(measurement));
            }
            passing = Some(size);
            if size * 2 > args.max {
                break (None, "size cap".into(), Some(measurement));
            }
            size *= 2;
        };
        let display_size = |n: Option<usize>| n.map_or_else(|| "—".into(), |n| n.to_string());
        let metrics = measurement.map_or_else(
            || "— | — | — | —".into(),
            |m| {
                let p99 = if m.mount_us > 200_000 {
                    "—".into()
                } else {
                    format!("{:.3}", m.p99_us as f64 / 1_000.)
                };
                format!(
                    "{:.3} | {p99} | {:.2} | {:.2}",
                    m.mount_us as f64 / 1_000.,
                    m.rss_kib.saturating_sub(empty_kib) as f64 / 1_024.,
                    m.raw as f64 / 1_048_576.
                )
            },
        );
        writeln!(
            table,
            "| {component} | {unit} | {} | {} | {reason} | {metrics} |",
            display_size(passing),
            display_size(failing)
        )?;
    }
    println!("{table}");
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|result| result.status.success())
        .map_or_else(
            || "unknown".into(),
            |result| String::from_utf8_lossy(&result.stdout).trim().to_owned(),
        );
    let backend = if cfg!(feature = "gpui-fast") {
        "gpui-fast"
    } else {
        "upstream"
    };
    let directory = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(directory)?;
    writeln!(
        output,
        "# GUIraffe performance limits\n\nCheckout {revision}, backend {backend}. Start {}, doubling cap {}.\n\nEach size ran in a fresh process. Headless CPU mount includes fixture construction. Measurements describe the first failure or the last tested size. Raw bytes approximate fixture payloads before derived indexes and caches. The grid payload models 50 i64 cells per row, generated on demand. A probe error leaves the threshold unknown. RSS is the peak above an empty window ({:.2} MiB, median of 5).\n\n{table}",
        args.start,
        args.max,
        empty_kib as f64 / 1_024.
    )?;
    output.persist(&args.output)?;
    ensure!(
        !incomplete,
        "one or more scenarios ended with a probe error"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_include_boundary_and_report_each_exceeded_limit() {
        let mut measurement = Measurement {
            mount_us: 200_000,
            p99_us: 8_000,
            rss_kib: 64 * 1_024 + 5,
            raw: 4 * 1_024,
        };
        assert!(measurement.failures(0).is_empty());
        measurement.mount_us += 1;
        measurement.p99_us += 1;
        measurement.rss_kib += 1;
        assert_eq!(measurement.failures(0), ["mount", "p99", "RSS"]);
        // The empty window's own memory is not charged to the component.
        assert_eq!(measurement.failures(1), ["mount", "p99"]);
    }
}
