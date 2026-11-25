//! Finding changed goldens: parse `git diff --name-status` and untracked files, classify
//! snapshot paths by backend and theme, and read both sides. The parsing is pure.
use std::{path::PathBuf, process::Command, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Change {
    Added,
    Modified,
    Deleted,
}

impl Change {
    pub fn label(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Modified => "modified",
            Change::Deleted => "deleted",
        }
    }
}

/// The two GPUI backends keep separate goldens (`snapshots/<backend>/...`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    Upstream,
    Fast,
    Other,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Upstream => "upstream",
            Backend::Fast => "gpui-fast",
            Backend::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Theme {
    Light,
    Dark,
    Other,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::Other => "—",
        }
    }
}

/// One changed golden.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Golden {
    /// Repository-relative path; the identity of the golden.
    pub path: Arc<str>,
    /// The snapshot name within its backend directory, e.g. `inputs/inputs-dark`.
    pub name: Arc<str>,
    pub backend: Backend,
    pub theme: Theme,
    pub change: Change,
}

/// Classify a repository path as a golden, or `None` for anything else (including the
/// `.actual.png` and `.diff.png` files a failing snapshot writes).
pub fn classify(path: &str, change: Change) -> Option<Golden> {
    let stem = path.strip_suffix(".png")?;
    if stem.ends_with(".actual") || stem.ends_with(".diff") {
        return None;
    }
    let (_, rest) = stem.split_once("/snapshots/")?;
    let (backend, name) = match rest.split_once('/') {
        Some(("upstream", name)) => (Backend::Upstream, name),
        Some(("gpui-fast", name)) => (Backend::Fast, name),
        _ => (Backend::Other, rest),
    };
    let leaf = name.rsplit('/').next().unwrap_or(name);
    let theme = if leaf == "dark" || leaf.ends_with("-dark") || leaf.ends_with("_dark") {
        Theme::Dark
    } else if leaf == "light" || leaf.ends_with("-light") || leaf.ends_with("_light") {
        Theme::Light
    } else {
        Theme::Other
    };
    Some(Golden {
        path: path.into(),
        name: name.into(),
        backend,
        theme,
        change,
    })
}

/// Parse `git diff --name-status` output (renames count as a delete and an add).
pub fn parse_name_status(text: &str) -> Vec<(Change, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        let (Some(status), Some(first)) = (parts.next(), parts.next()) else {
            continue;
        };
        match status.chars().next() {
            Some('A') => out.push((Change::Added, first.to_owned())),
            Some('M') | Some('T') => out.push((Change::Modified, first.to_owned())),
            Some('D') => out.push((Change::Deleted, first.to_owned())),
            Some('R') | Some('C') => {
                if status.starts_with('R') {
                    out.push((Change::Deleted, first.to_owned()));
                }
                if let Some(second) = parts.next() {
                    out.push((Change::Added, second.to_owned()));
                }
            }
            _ => {}
        }
    }
    out
}

/// Untracked paths from `git status --porcelain` (new goldens a `--regen` wrote).
pub fn parse_untracked(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("?? "))
        .map(str::to_owned)
        .collect()
}

/// Changed goldens from both listings, deduplicated and sorted by backend, theme, name.
pub fn collect(name_status: &str, untracked: &str) -> Vec<Golden> {
    let mut out: Vec<Golden> = parse_name_status(name_status)
        .into_iter()
        .chain(
            parse_untracked(untracked)
                .into_iter()
                .map(|p| (Change::Added, p)),
        )
        .filter_map(|(change, path)| classify(&path, change))
        .collect();
    out.sort_by(|a, b| (&a.name, a.backend, a.theme).cmp(&(&b.name, b.backend, b.theme)));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Where the two sides of a golden come from. `Send + Sync`: loads run on a background thread.
pub trait Source: Send + Sync + 'static {
    /// The golden at the base ref; `None` when it did not exist.
    fn before(&self, golden: &Golden) -> Option<Vec<u8>>;
    /// The golden in the working tree; `None` when it was deleted.
    fn after(&self, golden: &Golden) -> Option<Vec<u8>>;
}

/// A git working tree compared against a base ref.
pub struct Git {
    pub repo: PathBuf,
    pub base: String,
}

impl Git {
    fn git(&self, args: &[&str]) -> anyhow::Result<Vec<u8>> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()?;
        anyhow::ensure!(
            out.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(out.stdout)
    }
    /// Changed goldens: committed and uncommitted changes since `base`, plus untracked PNGs.
    pub fn changed(&self) -> anyhow::Result<Vec<Golden>> {
        let diff = self.git(&["diff", "--name-status", "-M", &self.base, "--", "*.png"])?;
        let status = self.git(&[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            "*.png",
        ])?;
        Ok(collect(
            &String::from_utf8_lossy(&diff),
            &String::from_utf8_lossy(&status),
        ))
    }
}

impl Source for Git {
    fn before(&self, golden: &Golden) -> Option<Vec<u8>> {
        if golden.change == Change::Added {
            return None;
        }
        self.git(&["show", &format!("{}:{}", self.base, golden.path)])
            .ok()
    }
    fn after(&self, golden: &Golden) -> Option<Vec<u8>> {
        if golden.change == Change::Deleted {
            return None;
        }
        std::fs::read(self.repo.join(&*golden.path)).ok()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::*;

    #[test]
    fn classifies_backend_theme_and_skips_failure_artifacts() {
        let g = classify(
            "crates/calendar/tests/snapshots/gpui-fast/day/dark.png",
            Change::Modified,
        )
        .unwrap();
        assert_eq!(
            (g.backend, g.theme, &*g.name),
            (Backend::Fast, Theme::Dark, "day/dark")
        );
        let g = classify(
            "crates/inputs/tests/snapshots/upstream/inputs-light.png",
            Change::Added,
        )
        .unwrap();
        assert_eq!((g.backend, g.theme), (Backend::Upstream, Theme::Light));
        let g = classify("bench/snapshots/frame.png", Change::Added).unwrap();
        assert_eq!((g.backend, g.theme), (Backend::Other, Theme::Other));
        assert!(
            classify(
                "crates/x/tests/snapshots/upstream/a.actual.png",
                Change::Added
            )
            .is_none()
        );
        assert!(
            classify(
                "crates/x/tests/snapshots/upstream/a.diff.png",
                Change::Added
            )
            .is_none()
        );
        assert!(classify("docs/logo.png", Change::Added).is_none());
        assert!(classify("crates/x/tests/snapshots/upstream/a.txt", Change::Added).is_none());
    }

    #[test]
    fn parses_git_listings_and_merges_untracked() {
        let diff = "M\tc/tests/snapshots/upstream/b-dark.png\nD\tc/tests/snapshots/gpui-fast/b-dark.png\nR087\tc/tests/snapshots/upstream/old-light.png\tc/tests/snapshots/upstream/new-light.png\nM\tREADME.md\n";
        let untracked = "?? c/tests/snapshots/gpui-fast/z-light.png\n?? c/tests/snapshots/upstream/z.actual.png\n M c/tests/snapshots/upstream/b-dark.png\n";
        let found = collect(diff, untracked);
        let rows: Vec<_> = found
            .iter()
            .map(|g| (&*g.name, g.backend, g.change))
            .collect();
        assert_eq!(
            rows,
            [
                ("b-dark", Backend::Upstream, Change::Modified),
                ("b-dark", Backend::Fast, Change::Deleted),
                ("new-light", Backend::Upstream, Change::Added),
                ("old-light", Backend::Upstream, Change::Deleted),
                ("z-light", Backend::Fast, Change::Added),
            ]
        );
    }
}
