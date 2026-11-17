//! `guiraffe dev`: watch the app's sources, rebuild on change, and relaunch the app.
//!
//! A failed build keeps the previous version running and prints the compiler errors, so the
//! edit–see loop never leaves you without a window. Watching polls modification times (portable,
//! no platform watcher), debounced so a burst of saves triggers one rebuild.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};

/// What counts as a source change: everything under these paths.
pub const WATCHED: &[&str] = &["src", "assets", "ui", "Cargo.toml", "build.rs"];
const POLL: Duration = Duration::from_millis(250);
/// Quiet period after the last change before rebuilding.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// (modified time, length) per file; length catches edits within mtime granularity.
pub type Snapshot = BTreeMap<PathBuf, (Option<SystemTime>, u64)>;

/// Snapshot every file under the watched paths of `root`.
pub fn snapshot(root: &Path) -> Snapshot {
    let mut out = Snapshot::new();
    for entry in WATCHED {
        walk(&root.join(entry), &mut out);
    }
    out
}

fn walk(path: &Path, out: &mut Snapshot) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.is_dir() {
        let Ok(rd) = std::fs::read_dir(path) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            // Skip hidden files and editor swap/backup files.
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name.ends_with('~') || name.ends_with(".swp") {
                continue;
            }
            walk(&p, out);
        }
    } else {
        out.insert(path.to_path_buf(), (meta.modified().ok(), meta.len()));
    }
}

/// Files added, removed or modified between two snapshots.
pub fn changed(old: &Snapshot, new: &Snapshot) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = new
        .iter()
        .filter(|(p, v)| old.get(*p) != Some(v))
        .map(|(p, _)| p.clone())
        .collect();
    out.extend(old.keys().filter(|p| !new.contains_key(*p)).cloned());
    out
}

fn build(root: &Path, release: bool) -> Result<bool> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").current_dir(root);
    if release {
        cmd.arg("--release");
    }
    Ok(cmd.status().context("running cargo build")?.success())
}

fn launch(root: &Path, release: bool) -> Result<Child> {
    let mut cmd = Command::new("cargo");
    // Already built: `cargo run` only resolves the binary and starts it.
    cmd.args(["run", "-q"]).current_dir(root);
    if release {
        cmd.arg("--release");
    }
    cmd.spawn().context("starting the app")
}

fn stop(child: &mut Option<Child>) {
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// Run the watch–rebuild–relaunch loop for the app in `root` until interrupted.
pub fn run(root: &Path, release: bool) -> Result<()> {
    anyhow::ensure!(
        root.join("Cargo.toml").is_file(),
        "no Cargo.toml in {}; run inside an app",
        root.display()
    );
    println!(
        "guiraffe dev: watching {} (Ctrl-C to stop)",
        WATCHED.join(", ")
    );
    let mut app = None;
    if build(root, release)? {
        app = Some(launch(root, release)?);
    } else {
        println!("build failed; fix the errors above and save to retry");
    }
    let mut last = snapshot(root);
    loop {
        thread::sleep(POLL);
        let now = snapshot(root);
        let diff = changed(&last, &now);
        if diff.is_empty() {
            // The app exited on its own (closed window): wait for the next change.
            if let Some(c) = app.as_mut()
                && c.try_wait().ok().flatten().is_some()
            {
                app = None;
                println!("app exited; save a file to relaunch");
            }
            continue;
        }
        // Debounce: wait until saves settle.
        let mut settled = now;
        loop {
            thread::sleep(DEBOUNCE);
            let again = snapshot(root);
            if again == settled {
                break;
            }
            settled = again;
        }
        last = settled;
        let shown: Vec<String> = diff
            .iter()
            .take(3)
            .map(|p| p.strip_prefix(root).unwrap_or(p).display().to_string())
            .collect();
        println!(
            "\nchanged: {}{}",
            shown.join(", "),
            if diff.len() > 3 { " …" } else { "" }
        );
        if build(root, release)? {
            stop(&mut app);
            app = Some(launch(root, release)?);
            println!("relaunched");
        } else {
            println!("build failed; keeping the previous version running");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_added_modified_and_removed_files_and_ignores_noise() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(src.join("main.rs"), "fn main() {}").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.path().join("README.md"), "not watched")
            .unwrap_or_else(|e| panic!("{e}"));
        let a = snapshot(dir.path());
        assert_eq!(a.len(), 1, "only watched paths: {a:?}");

        std::fs::write(src.join(".main.rs.swp"), "x").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(src.join("main.rs~"), "x").unwrap_or_else(|e| panic!("{e}"));
        assert!(
            changed(&a, &snapshot(dir.path())).is_empty(),
            "editor noise ignored"
        );

        std::fs::write(src.join("main.rs"), "fn main() { /* edit */ }")
            .unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(src.join("app.rs"), "").unwrap_or_else(|e| panic!("{e}"));
        let b = snapshot(dir.path());
        let mut c = changed(&a, &b);
        c.sort();
        assert_eq!(c, vec![src.join("app.rs"), src.join("main.rs")]);

        std::fs::remove_file(src.join("app.rs")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(changed(&b, &snapshot(dir.path())), vec![src.join("app.rs")]);
    }
}
