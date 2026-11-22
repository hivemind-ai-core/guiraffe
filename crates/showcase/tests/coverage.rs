//! Fails when a public component in any GUIraffe crate has no showcase entry.
//!
//! A "component" is a `pub struct` with a `Render` or `RenderOnce` impl in a library crate's
//! `src/`. Each must appear in some catalog entry's `covers`, or in `catalog::INTERNAL` with a
//! reason. Stale names (covered but no longer existing) fail too, so the catalog cannot rot.

use std::{collections::BTreeSet, fs, path::Path};

/// Catalog sections that showcase third-party types (GPUI Controls and its charts), which the
/// scanner cannot see; their entries are exempt from the covered/stale check.
const EXTERNAL_SECTIONS: &[&str] = &["kit", "kit-charts"];

/// Crates that are tools or harnesses, not component libraries.
const NOT_COMPONENT_CRATES: &[&str] = &["showcase", "test", "bench", "cli", "mocket"];

fn ident_after<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    let rest = &line[line.find(kw)? + kw.len()..];
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

fn scan(dir: &Path, krate: &str, out: &mut BTreeSet<(String, String)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan(&p, krate, out);
            continue;
        }
        if p.extension().and_then(|x| x.to_str()) != Some("rs") {
            continue;
        }
        let Ok(src) = fs::read_to_string(&p) else {
            continue;
        };
        let pubs: BTreeSet<&str> = src
            .lines()
            .filter(|l| l.trim_start().starts_with("pub struct "))
            .filter_map(|l| ident_after(l, "pub struct "))
            .collect();
        for l in src.lines() {
            let t = l.trim_start();
            if !t.starts_with("impl")
                || !(t.contains("Render for ") || t.contains("RenderOnce for "))
            {
                continue;
            }
            if let Some(name) = ident_after(t, " for ")
                && pubs.contains(name)
            {
                out.insert((krate.to_string(), name.to_string()));
            }
        }
    }
}

#[test]
fn every_public_component_has_a_showcase_entry() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = BTreeSet::new();
    for e in fs::read_dir(&crates_dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !NOT_COMPONENT_CRATES.contains(&name.as_str()) {
            scan(&e.path().join("src"), &name, &mut found);
        }
    }
    assert!(
        found.len() > 20,
        "scanner found too little ({}); is the layout unchanged?",
        found.len()
    );

    let mut covered = BTreeSet::new();
    for e in guiraffe_showcase::catalog::entries() {
        if EXTERNAL_SECTIONS.contains(&e.krate) {
            continue;
        }
        for c in e.covers {
            covered.insert((e.krate.to_string(), (*c).to_string()));
        }
    }
    for (k, n, why) in guiraffe_showcase::catalog::INTERNAL {
        assert!(!why.trim().is_empty(), "{k}::{n} needs a reason");
        covered.insert(((*k).to_string(), (*n).to_string()));
    }

    let missing: Vec<_> = found
        .difference(&covered)
        .map(|(k, n)| format!("{k}::{n}"))
        .collect();
    let stale: Vec<_> = covered
        .difference(&found)
        .map(|(k, n)| format!("{k}::{n}"))
        .collect();
    assert!(
        missing.is_empty(),
        "{} component(s) without a showcase entry:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "catalog names that no longer exist:\n  {}",
        stale.join("\n  ")
    );
}
