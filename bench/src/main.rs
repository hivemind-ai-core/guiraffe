mod process;
mod revision;

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use guiraffe_bench::{
    RESAMPLES, Record, SEED, metadata, record, stats, suites, validate_profile, write_record,
};
use process::{check_interrupt, checked};
use revision::Revision;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

#[derive(Parser)]
#[command(about = "GUIraffe reproducible headless benchmarks")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Args, Clone)]
struct Options {
    #[arg(long, default_value = "dev-laptop")]
    profile: String,
    #[arg(long, default_value_t = 20)]
    samples: usize,
    #[arg(long, default_value_t = 10_000)]
    iterations: usize,
    #[arg(long)]
    suite: Option<String>,
    #[arg(long, default_value = "")]
    notes: String,
}
#[derive(Subcommand)]
enum Action {
    Run {
        #[command(flatten)]
        options: Options,
        #[arg(long)]
        output: Option<PathBuf>,
        /// Run iai-callgrind instead; skip explicitly when Valgrind is absent.
        #[arg(long)]
        instructions: bool,
    },
    Baseline {
        #[command(flatten)]
        options: Options,
    },
    Compare {
        old_ref: String,
        new_ref: String,
        #[command(flatten)]
        options: Options,
        #[arg(long)]
        new_profile: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value_t = 0.10)]
        threshold: f64,
        /// CPU in this process's allowed affinity mask (default: first allowed CPU).
        #[arg(long)]
        cpu: Option<usize>,
        #[arg(long, default_value = "bench/reports/compare.json")]
        output: PathBuf,
    },
    Report {
        #[arg(long, default_value = "bench/baselines")]
        input: PathBuf,
        #[arg(long, default_value = "bench/reports/trends.html")]
        output: PathBuf,
    },
    #[command(hide = true)]
    Sample {
        #[arg(long)]
        suite: String,
        #[arg(long)]
        iterations: usize,
    },
}
fn root() -> Result<PathBuf> {
    let output = checked(Command::new("git").args(["rev-parse", "--show-toplevel"]))?;
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}
fn selected(name: Option<&str>) -> Result<Vec<guiraffe_bench::Suite>> {
    let list: Vec<_> = suites::registered()
        .into_iter()
        .filter(|s| name.is_none_or(|name| name == s.name))
        .collect();
    ensure!(!list.is_empty(), "no matching suite");
    Ok(list)
}
fn run(options: Options, output: Option<PathBuf>, baseline: bool) -> Result<()> {
    ensure!(options.samples >= 10, "at least ten samples required");
    let root = root()?;
    let meta = metadata(&root, &options.profile, &options.notes)?;
    let mut records = Vec::new();
    for suite in selected(options.suite.as_deref())? {
        let values = guiraffe_bench::measure_cancellable(
            suite,
            options.samples,
            options.iterations,
            process::interrupted,
        )?;
        let result = record(suite.name, meta.clone(), values, options.iterations)?;
        if baseline {
            write_record(
                &root
                    .join("bench/baselines")
                    .join(&options.profile)
                    .join(format!("{}.json", suite.name)),
                &result,
            )?;
        }
        records.push(result);
    }
    let json = serde_json::to_vec_pretty(&records)?;
    if let Some(path) = output {
        save(&path, &json)?;
    }
    println!("{}", String::from_utf8(json)?);
    Ok(())
}
fn save(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn affinity(requested: Option<usize>) -> Result<Option<usize>> {
    if !Command::new("taskset")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("taskset unavailable: comparison will run without CPU pinning");
        return Ok(None);
    }
    let status = fs::read_to_string("/proc/self/status")?;
    let allowed = status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .context("missing CPU affinity mask")?
        .trim();
    let first = allowed
        .split([',', '-'])
        .next()
        .context("empty affinity mask")?
        .parse::<usize>()?;
    let cpu = requested.unwrap_or(first);
    checked(Command::new("taskset").args(["-c", &cpu.to_string(), "true"]))?;
    eprintln!("comparison pinned to CPU {cpu}");
    Ok(Some(cpu))
}
fn sample(binary: &Path, suite: &str, iterations: usize, cpu: Option<usize>) -> Result<f64> {
    let mut command = if let Some(cpu) = cpu {
        let mut command = Command::new("taskset");
        command.args(["-c", &cpu.to_string()]).arg(binary);
        command
    } else {
        Command::new(binary)
    };
    command.args([
        "sample",
        "--suite",
        suite,
        "--iterations",
        &iterations.to_string(),
    ]);
    let output = checked(&mut command)?;
    Ok(serde_json::from_slice(&output.stdout)?)
}
#[derive(serde::Serialize)]
struct Comparison {
    old: Record,
    new: Record,
    relative_ci95: stats::Interval,
    threshold: f64,
    regression: bool,
    status: stats::Change,
    cpu: Option<usize>,
    order: &'static str,
}
struct CompareOptions {
    old_ref: String,
    new_ref: String,
    options: Options,
    new_profile: Option<String>,
    force: bool,
    threshold: f64,
    cpu: Option<usize>,
    output: PathBuf,
}
fn compare(args: CompareOptions) -> Result<()> {
    let CompareOptions {
        old_ref,
        new_ref,
        options,
        new_profile,
        force,
        threshold,
        cpu,
        output,
    } = args;
    ensure!(
        options.samples >= 10 && options.iterations > 0,
        "need at least ten samples and positive iterations"
    );
    ensure!(
        threshold.is_finite() && threshold >= 0.,
        "invalid regression threshold"
    );
    let new_profile = new_profile.as_deref().unwrap_or(&options.profile);
    validate_profile(&options.profile)?;
    validate_profile(new_profile)?;
    ensure!(
        force || options.profile == new_profile,
        "refusing comparison across profiles; pass --force"
    );
    let root = root()?;
    let source = metadata(&root, &options.profile, &options.notes)?;
    let old_commit = revision::resolve(&root, &old_ref, &source.commit, source.dirty)?;
    let new_commit = revision::resolve(&root, &new_ref, &source.commit, source.dirty)?;
    revision::recover(&root)?;
    let old = Revision::create(&root, &old_commit)?;
    eprintln!("building old revision {old_ref}");
    let old_binary = old.build()?;
    let mut old_meta = metadata(&old.path, &options.profile, &options.notes)?;
    // Cargo can retain paths to the first checkout in shared-cache fingerprints.
    // Remove those inputs before the second build, retaining the copied binary.
    old.remove_checkout()?;
    let new = Revision::create(&root, &new_commit)?;
    eprintln!("building new revision {new_ref}");
    let new_binary = new.build()?;
    let mut new_meta = metadata(&new.path, new_profile, &options.notes)?;
    // Dirty describes the invoking source root, not the clean detached snapshots.
    old_meta.dirty = source.dirty;
    new_meta.dirty = source.dirty;
    old_meta.build_profile = "release".into();
    new_meta.build_profile = "release".into();
    let cpu = affinity(cpu)?;
    let mut comparisons = Vec::new();
    println!(
        "suite                    old ns       new ns       change      95% CI change       status"
    );
    for suite in selected(options.suite.as_deref())? {
        check_interrupt()?;
        let mut a = Vec::new();
        let mut b = Vec::new();
        // Alternate AB/BA across rounds, not just all of A followed by all of B.
        for round in 0..options.samples {
            check_interrupt()?;
            let (av, bv) = if round.is_multiple_of(2) {
                (
                    sample(&old_binary, suite.name, options.iterations, cpu)?,
                    sample(&new_binary, suite.name, options.iterations, cpu)?,
                )
            } else {
                let bv = sample(&new_binary, suite.name, options.iterations, cpu)?;
                (
                    sample(&old_binary, suite.name, options.iterations, cpu)?,
                    bv,
                )
            };
            a.push(av);
            b.push(bv);
        }
        let old = record(suite.name, old_meta.clone(), a, options.iterations)?;
        let new = record(suite.name, new_meta.clone(), b, options.iterations)?;
        guiraffe_bench::compatible(&old, &new, force)?;
        let ci = stats::relative_change(&old.samples, &new.samples, RESAMPLES, SEED)?;
        let status = stats::classify(ci, threshold);
        let regression = status == stats::Change::Regression;
        println!(
            "{:<24} {:>10.2} {:>12.2} {:>9.2}% [{:>6.2}%, {:>6.2}%] {}",
            suite.name,
            old.summary.median,
            new.summary.median,
            (new.summary.median / old.summary.median - 1.) * 100.,
            ci.low * 100.,
            ci.high * 100.,
            status.label()
        );
        comparisons.push(Comparison {
            old,
            new,
            relative_ci95: ci,
            threshold,
            regression,
            status,
            cpu,
            order: "AB, BA, repeated; warmup before every sample",
        });
    }
    save(&output, &serde_json::to_vec_pretty(&comparisons)?)?;
    if comparisons.iter().any(|c| c.regression) {
        return Err(Regression.into());
    }
    Ok(())
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn read_records(path: &Path, records: &mut Vec<Record>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            read_records(&entry?.path(), records)?;
        }
    } else if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        let bytes = fs::read(path)?;
        if let Ok(record) = serde_json::from_slice::<Record>(&bytes) {
            records.push(record);
        } else {
            records.extend(
                serde_json::from_slice::<Vec<Record>>(&bytes)
                    .with_context(|| format!("invalid record {}", path.display()))?,
            );
        }
    }
    Ok(())
}
fn report(input: &Path, output: &Path) -> Result<()> {
    let mut records = Vec::new();
    read_records(input, &mut records)?;
    ensure!(!records.is_empty(), "no benchmark records found");
    records.sort_by_key(|r| r.metadata.unix_seconds);
    let mut groups: BTreeMap<(String, String, String), Vec<&Record>> = BTreeMap::new();
    for record in &records {
        groups
            .entry((
                record.metadata.profile.clone(),
                record.suite.clone(),
                record.metadata.build_profile.clone(),
            ))
            .or_default()
            .push(record);
    }
    let mut html = String::from(
        "<!doctype html><html lang=en><meta charset=utf-8><title>GUIraffe benchmark trends</title><style>body{font:16px system-ui;margin:3rem;max-width:1200px}table{border-collapse:collapse;width:100%}td,th{padding:.7rem;text-align:left;border-bottom:1px solid #ddd}svg{width:100%;height:150px}small{color:#555}</style><h1>GUIraffe benchmark trends</h1><p>Wall time is nanoseconds per workload; spread is unscaled MAD. Intervals are 95% percentile bootstrap confidence intervals. Compare only matching machine and build profiles.</p>",
    );
    for ((profile, suite, build), values) in groups {
        html.push_str(&format!(
            "<h2>{} / {} ({})</h2>",
            escape(&profile),
            escape(&suite),
            escape(&build)
        ));
        let maximum = values
            .iter()
            .map(|r| r.summary.ci95.high)
            .fold(1.0, f64::max);
        html.push_str("<svg viewBox='0 0 1000 150' role='img' aria-label='Median timing trend with confidence intervals'>");
        let mut points = Vec::new();
        for (index, result) in values.iter().enumerate() {
            let x = 20. + index as f64 * 960. / values.len().saturating_sub(1).max(1) as f64;
            let y = 140. - result.summary.median / maximum * 120.;
            points.push(format!("{x},{y}"));
            html.push_str(&format!("<path d='M{x},{}V{}' stroke='#738' /><circle cx='{x}' cy='{y}' r='4' fill='#385'><title>{}: {:.2} ns</title></circle>",140. - result.summary.ci95.low / maximum * 120.,140. - result.summary.ci95.high / maximum * 120.,escape(&result.metadata.commit),result.summary.median));
        }
        html.push_str(&format!("<polyline points='{}' stroke='#385' fill='none'/></svg><table><tr><th>Time / commit</th><th>Median ± MAD</th><th>95% CI</th><th>Host context</th></tr>", points.join(" ")));
        for result in values {
            html.push_str(&format!("<tr><td>{}<br><small>{}{}</small></td><td>{:.2} ± {:.2}</td><td>[{:.2}, {:.2}]</td><td>{}<br>load: {}<br>{}</td></tr>", result.metadata.unix_seconds,escape(&result.metadata.commit),if result.metadata.dirty { " (dirty)" } else { "" },result.summary.median,result.summary.mad,result.summary.ci95.low,result.summary.ci95.high,escape(&result.metadata.cpu_model),escape(&result.metadata.load_average),escape(&result.metadata.notes)));
        }
        html.push_str("</table>");
    }
    html.push_str("</html>");
    save(output, html.as_bytes())?;
    println!("wrote {}", output.display());
    Ok(())
}
fn instructions() -> Result<()> {
    if !Command::new("valgrind")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!(
            "SKIP instruction counts: Valgrind is not installed or cannot run; install valgrind and iai-callgrind-runner 0.16.1"
        );
        return Ok(());
    }
    let output = checked(Command::new("cargo").current_dir(root()?).args([
        "bench",
        "--locked",
        "-p",
        "guiraffe-bench",
        "--features",
        "instruction-counts",
        "--bench",
        "instructions",
        "--",
        "--save-summary=json",
    ]))
    .context(
        "instruction benchmarks failed (install iai-callgrind-runner --version 0.16.1 if missing)",
    )?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}
fn dispatch(action: Action) -> Result<()> {
    match action {
        Action::Run {
            options,
            output,
            instructions: true,
        } => {
            if output.is_some() || options.suite.is_some() {
                bail!(
                    "instruction mode uses its own summary output and all registered instruction suites"
                );
            }
            instructions()
        }
        Action::Run {
            options,
            output,
            instructions: false,
        } => run(options, output, false),
        Action::Baseline { options } => run(options, None, true),
        Action::Compare {
            old_ref,
            new_ref,
            options,
            new_profile,
            force,
            threshold,
            cpu,
            output,
        } => compare(CompareOptions {
            old_ref,
            new_ref,
            options,
            new_profile,
            force,
            threshold,
            cpu,
            output,
        }),
        Action::Report { input, output } => report(&input, &output),
        Action::Sample { suite, iterations } => {
            let suite = selected(Some(&suite))?
                .into_iter()
                .next()
                .context("missing suite")?;
            let values =
                guiraffe_bench::measure_cancellable(suite, 1, iterations, process::interrupted)?;
            println!(
                "{}",
                serde_json::to_string(values.first().context("missing sample")?)?
            );
            Ok(())
        }
    }
}

#[derive(Debug)]
struct Regression;
impl std::fmt::Display for Regression {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("regression exceeds threshold with 95% confidence")
    }
}
impl std::error::Error for Regression {}
fn main() -> ExitCode {
    let result = process::install_handler().and_then(|()| dispatch(Cli::parse().command));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::from(if error.is::<Regression>() { 1 } else { 2 })
        }
    }
}
