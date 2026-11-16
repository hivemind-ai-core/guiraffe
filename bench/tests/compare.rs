#![cfg(unix)]
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    time::{Duration, Instant},
};

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    tools: PathBuf,
    old: String,
    new: String,
}
impl Fixture {
    fn new(old: f64, new: f64) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("repo");
        let tools = directory.path().join("tools");
        fs::create_dir_all(root.join("bench/src"))?;
        fs::create_dir_all(&tools)?;
        let mut fixture = Self {
            _directory: directory,
            root,
            tools,
            old: String::new(),
            new: String::new(),
        };
        fixture.git(&["init", "--quiet"])?;
        fixture.git(&["config", "user.email", "bench-test@example.invalid"])?;
        fixture.git(&["config", "user.name", "Bench fixture"])?;
        fs::write(fixture.root.join(".gitignore"), "target/\n")?;
        fs::write(
            fixture.root.join("bench/Cargo.toml"),
            "# benchmark fixture\n",
        )?;
        fs::write(
            fixture.root.join("bench/src/main.rs"),
            "// fixture protocol\n",
        )?;
        fixture.old = fixture.commit(old)?;
        fixture.new = fixture.commit(new)?;
        let cargo = fixture.tools.join("cargo");
        fs::write(
            &cargo,
            r#"#!/usr/bin/env python3
import json, os, pathlib, sys, time
args = sys.argv[1:]
assert '--message-format=json' in args
base = pathlib.Path(args[args.index('--target-dir') + 1])
assert base.name == 'bench-compare'
if os.environ.get('BENCH_FIXTURE_MARKER'):
    pathlib.Path(os.environ['BENCH_FIXTURE_MARKER']).write_text(str(os.getpid()))
    time.sleep(30)
# Deliberately unrelated to the host target triple and conventional release path.
exe = base / 'custom-target' / 'unexpected-artifact'
exe.parent.mkdir(parents=True, exist_ok=True)
value = float(pathlib.Path('timing').read_text())
exe.write_text('#!/bin/sh\nprintf \'%s\\n\' \'' + str(value) + '\'\n')
exe.chmod(0o755)
print(json.dumps({'reason': 'compiler-message'}))
print(json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'guiraffe-bench', 'kind': ['bin']}, 'executable': str(exe)}))
"#,
        )?;
        fs::set_permissions(cargo, fs::Permissions::from_mode(0o755))?;
        Ok(fixture)
    }
    fn git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()?;
        ensure!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn commit(&self, value: f64) -> Result<String> {
        fs::write(self.root.join("timing"), value.to_string())?;
        self.git(&["add", "."])?;
        self.git(&["commit", "--quiet", "--allow-empty", "-m", "fixture"])?;
        self.git(&["rev-parse", "HEAD"])
    }
    fn command(&self) -> Result<Command> {
        let mut paths = vec![self.tools.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").context("missing PATH")?,
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_guiraffe-bench"));
        command
            .current_dir(&self.root)
            .env("PATH", std::env::join_paths(paths)?)
            .args([
                "compare",
                &self.old,
                &self.new,
                "--samples",
                "10",
                "--iterations",
                "1",
                "--suite",
                "core-variants",
            ]);
        Ok(command)
    }
    fn compare(&self) -> Result<Output> {
        Ok(self.command()?.output()?)
    }
    fn assert_clean_worktrees(&self) -> Result<()> {
        assert_eq!(
            self.git(&["worktree", "list", "--porcelain"])?
                .lines()
                .filter(|line| line.starts_with("worktree "))
                .count(),
            1
        );
        let storage = self.root.join("target/bench-compare/worktrees");
        if storage.exists() {
            assert!(!fs::read_dir(storage)?.filter_map(Result::ok).any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("guiraffe-bench-")
            }));
        }
        Ok(())
    }
}

#[test]
fn real_comparison_exit_codes_and_improvement_output() -> Result<()> {
    for (new, code, label) in [
        (100., 0, "pass"),
        (150., 1, "REGRESSION"),
        (50., 0, "improvement"),
    ] {
        let fixture = Fixture::new(100., new)?;
        let output = fixture.compare()?;
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains(label));
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.root.join("bench/reports/compare.json"))?)?;
        assert_eq!(json[0]["regression"], code == 1);
        if new == 50. {
            assert_eq!(json[0]["status"], "improvement");
        }
        fixture.assert_clean_worktrees()?;
    }
    Ok(())
}

#[test]
fn warns_for_dirty_head_and_records_source_root_state() -> Result<()> {
    let fixture = Fixture::new(100., 100.)?;
    fs::write(fixture.root.join("timing"), "99999")?;
    let output = fixture.compare()?;
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("source tree is dirty"));
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("bench/reports/compare.json"))?)?;
    assert_eq!(json[0]["new"]["metadata"]["dirty"], true);
    assert_eq!(json[0]["new"]["summary"]["median"], 100.);
    fixture.assert_clean_worktrees()?;
    Ok(())
}

#[test]
fn unsupported_ref_fails_before_building_with_tool_error_code() -> Result<()> {
    let mut fixture = Fixture::new(100., 100.)?;
    fixture.git(&["rm", "bench/src/main.rs"])?;
    fixture.git(&["commit", "--quiet", "-m", "remove support"])?;
    fixture.old = fixture.git(&["rev-parse", "HEAD"])?;
    let output = fixture.compare()?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no benchmark CLI support"));
    assert!(!stderr.contains("building old revision"));
    fixture.assert_clean_worktrees()?;
    Ok(())
}

#[test]
fn ctrl_c_kills_build_group_and_removes_checkouts() -> Result<()> {
    use nix::{
        sys::signal::{Signal, kill},
        unistd::Pid,
    };
    let fixture = Fixture::new(100., 100.)?;
    let marker = fixture._directory.path().join("started");
    let mut child = fixture
        .command()?
        .env("BENCH_FIXTURE_MARKER", &marker)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !marker.exists() {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("build did not start");
    }
    let build_pid = fs::read_to_string(marker)?.parse::<i32>()?;
    kill(Pid::from_raw(i32::try_from(child.id())?), Signal::SIGINT)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            let _ = kill(Pid::from_raw(build_pid), Signal::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("interrupted compare did not exit");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("interrupted"));
    assert!(
        kill(Pid::from_raw(build_pid), None).is_err(),
        "build child was not reaped"
    );
    fixture.assert_clean_worktrees()?;
    Ok(())
}

#[test]
fn startup_recovers_stale_prefixed_checkout_but_preserves_locked_owner() -> Result<()> {
    use fs2::FileExt;
    let fixture = Fixture::new(100., 100.)?;
    let storage = fixture.root.join("target/bench-compare/worktrees");
    for name in ["guiraffe-bench-stale", "guiraffe-bench-live"] {
        let directory = storage.join(name);
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join(".repository"),
            fixture.root.to_str().context("non-utf8 root")?,
        )?;
        fs::write(directory.join(".owner.lock"), "")?;
        let checkout = directory.join("guiraffe-bench-checkout");
        fixture.git(&[
            "worktree",
            "add",
            "--detach",
            checkout.to_str().context("non-utf8 checkout")?,
            &fixture.old,
        ])?;
    }
    let owner = fs::File::open(storage.join("guiraffe-bench-live/.owner.lock"))?;
    FileExt::lock_exclusive(&owner)?;
    let output = fixture.compare()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!storage.join("guiraffe-bench-stale").exists());
    assert!(
        storage
            .join("guiraffe-bench-live/guiraffe-bench-checkout/.git")
            .exists()
    );
    drop(owner);
    let output = fixture.compare()?;
    assert_eq!(output.status.code(), Some(0));
    fixture.assert_clean_worktrees()?;
    Ok(())
}
