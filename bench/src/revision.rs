//! Detached benchmark snapshots and scoped recovery of crashed benchmark runs.
use crate::process::{check_interrupt, checked};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
};

const PREFIX: &str = "guiraffe-bench-";
const CHECKOUT: &str = "guiraffe-bench-checkout";
fn storage(root: &Path) -> PathBuf {
    root.join("target/bench-compare/worktrees")
}
fn lock(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}
fn registry_lock(root: &Path) -> Result<File> {
    fs::create_dir_all(storage(root))?;
    let file = lock(&storage(root).join(".registry.lock"))?;
    FileExt::lock_exclusive(&file)?;
    Ok(file)
}

/// Only recover directories with our prefix, our marker, and an unlocked owner file.
/// A live comparison keeps its owner lock even while its child is executing.
pub fn recover(root: &Path) -> Result<()> {
    let _registry = registry_lock(root)?;
    for entry in fs::read_dir(storage(root))? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_dir() || !entry.file_name().to_string_lossy().starts_with(PREFIX)
        {
            continue;
        }
        check_interrupt()?;
        if fs::read(path.join(".repository")).ok().as_deref()
            != Some(root.as_os_str().as_encoded_bytes())
        {
            continue;
        }
        let owner = lock(&path.join(".owner.lock"))?;
        if FileExt::try_lock_exclusive(&owner).is_err() {
            continue;
        }
        let checkout = path.join(CHECKOUT);
        if checkout.exists() {
            let output = Command::new("git")
                .current_dir(root)
                .args(["worktree", "remove", "--force"])
                .arg(&checkout)
                .output()?;
            ensure!(
                output.status.success(),
                "cannot recover {}: {}",
                checkout.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fs::remove_dir_all(&path)?;
        eprintln!("recovered stale benchmark directory {}", path.display());
    }
    checked(
        Command::new("git")
            .current_dir(root)
            .args(["worktree", "prune"]),
    )?;
    Ok(())
}

pub fn resolve(root: &Path, reference: &str, head: &str, dirty: bool) -> Result<String> {
    let output = checked(Command::new("git").current_dir(root).args([
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{reference}^{{commit}}"),
    ]))?;
    let commit = String::from_utf8(output.stdout)?.trim().to_owned();
    if dirty && commit == head {
        eprintln!(
            "warning: ref {reference} resolves to HEAD but source tree is dirty; comparing committed HEAD, excluding uncommitted changes"
        );
    }
    for required in ["bench/Cargo.toml", "bench/src/main.rs"] {
        checked(Command::new("git").current_dir(root).args([
            "cat-file",
            "-e",
            &format!("{commit}:{required}"),
        ]))
        .with_context(|| {
            format!("ref {reference} ({commit}) has no benchmark CLI support: missing {required}")
        })?;
    }
    Ok(commit)
}

pub struct Revision {
    directory: tempfile::TempDir,
    root: PathBuf,
    pub path: PathBuf,
    _owner: File,
}
impl Revision {
    pub fn create(root: &Path, commit: &str) -> Result<Self> {
        check_interrupt()?;
        let _registry = registry_lock(root)?;
        let directory = tempfile::Builder::new()
            .prefix(PREFIX)
            .tempdir_in(storage(root))?;
        let owner = lock(&directory.path().join(".owner.lock"))?;
        FileExt::lock_exclusive(&owner)?;
        fs::write(
            directory.path().join(".repository"),
            root.as_os_str().as_encoded_bytes(),
        )?;
        let path = directory.path().join(CHECKOUT);
        let revision = Self {
            directory,
            root: root.into(),
            path,
            _owner: owner,
        };
        // Install the RAII guard BEFORE git add: cancellation during checkout also cleans up.
        checked(
            Command::new("git")
                .current_dir(root)
                .args(["worktree", "add", "--detach"])
                .arg(&revision.path)
                .arg(commit),
        )?;
        Ok(revision)
    }
    pub fn build(&self) -> Result<PathBuf> {
        let target = self.root.join("target/bench-compare");
        let output = checked(
            Command::new("cargo")
                .current_dir(&self.path)
                .args([
                    "build",
                    "--release",
                    "--locked",
                    "-p",
                    "guiraffe-bench",
                    "--message-format=json",
                    "--target-dir",
                ])
                .arg(&target),
        )?;
        let artifact = executable(&output.stdout)?;
        let binary = self
            .directory
            .path()
            .join(format!("guiraffe-bench{}", std::env::consts::EXE_SUFFIX));
        fs::copy(&artifact, &binary)
            .with_context(|| format!("copying Cargo artifact {}", artifact.display()))?;
        Ok(binary)
    }
}
pub fn executable(stdout: &[u8]) -> Result<PathBuf> {
    for line in stdout.split(|b| *b == b'\n') {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if value["reason"] == "compiler-artifact"
            && value["target"]["name"] == "guiraffe-bench"
            && value["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
            && let Some(path) = value["executable"].as_str()
        {
            return Ok(PathBuf::from(path));
        }
    }
    anyhow::bail!("Cargo did not report an executable artifact for guiraffe-bench")
}
impl Drop for Revision {
    fn drop(&mut self) {
        // Cleanup must ignore the interrupt flag. Source paths are exclusively owned.
        if let Ok(output) = Command::new("git")
            .current_dir(&self.root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .output()
            && !output.status.success()
            && self.path.exists()
        {
            eprintln!(
                "warning: temporary worktree cleanup failed for {}: {}",
                self.path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
