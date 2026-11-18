use anyhow::{Context, Result};
use guiraffe_bench::{Record, compatible};
use std::{fs, process::Command};
fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_guiraffe-bench"))
}
#[test]
fn json_roundtrip_report_escaping_and_profile_guard() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let records = directory.path().join("records.json");
    let output = binary()
        .args([
            "run",
            "--suite",
            "core-variants",
            "--samples",
            "10",
            "--iterations",
            "20",
            "--notes",
            "heavily loaded <script> & host",
            "--output",
        ])
        .arg(&records)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut records: Vec<Record> = serde_json::from_slice(&fs::read(records)?)?;
    assert_eq!(records.len(), 1);
    let old = records.first().context("record missing")?.clone();
    assert_eq!(old.samples.len(), 10);
    assert!(!old.metadata.commit.is_empty());
    let new = records.first_mut().context("record missing")?;
    new.metadata.profile = "different-machine".into();
    assert!(compatible(&old, new, false).is_err());
    assert!(compatible(&old, new, true).is_ok());
    new.metric = "instructions".into();
    assert!(compatible(&old, new, true).is_err());
    let html = directory.path().join("trends.html");
    let output = binary()
        .arg("report")
        .arg("--input")
        .arg(directory.path().join("records.json"))
        .arg("--output")
        .arg(&html)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(html)?;
    assert!(text.contains("heavily loaded &lt;script&gt; &amp; host"));
    assert!(!text.contains("<script>"));
    assert!(text.contains("<polyline"));
    Ok(())
}
#[test]
fn invalid_options_fail_before_mutation() -> Result<()> {
    for args in [
        vec!["baseline", "--profile", "../escape"],
        vec!["run", "--samples", "9"],
        vec!["run", "--iterations", "0"],
        vec!["run", "--suite", "nonexistent"],
        vec!["compare", "HEAD", "HEAD", "--new-profile", "other"],
        vec!["compare", "HEAD", "HEAD", "--threshold", "NaN"],
    ] {
        assert_eq!(binary().args(args).output()?.status.code(), Some(2));
    }
    Ok(())
}
