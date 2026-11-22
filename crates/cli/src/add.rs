//! `guiraffe add <crate>…`: add GUIraffe crates to the app in the current directory.
//!
//! Edits `Cargo.toml` with `toml_edit`, so comments and formatting survive. Idempotent: a crate
//! already present is left as is.

use std::path::Path;

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, InlineTable, Item, Value};

/// Crates in a GUIraffe checkout that apps can depend on (`crates/<name>` with a library).
pub fn available(checkout: &Path) -> Vec<String> {
    const NOT_FOR_APPS: &[&str] = &["test", "bench", "cli", "showcase"];
    let mut names: Vec<String> = std::fs::read_dir(checkout.join("crates"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("src/lib.rs").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !NOT_FOR_APPS.contains(&n.as_str()))
        .collect();
    names.sort();
    names
}

/// Add `guiraffe-<name> = { path = "<checkout>/crates/<name>" }` to `[dependencies]`.
/// Returns the new manifest and whether it changed.
pub fn add_dependency(manifest: &str, name: &str, crate_dir: &Path) -> Result<(String, bool)> {
    let mut doc: DocumentMut = manifest.parse().context("Cargo.toml is not valid TOML")?;
    let deps = doc
        .entry("dependencies")
        .or_insert_with(|| Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .context("[dependencies] is not a table")?;
    let key = format!("guiraffe-{name}");
    if deps.contains_key(&key) {
        return Ok((doc.to_string(), false));
    }
    let path = crate_dir.to_str().context("crate path must be UTF-8")?;
    let mut table = InlineTable::new();
    table.insert("path", Value::from(path));
    deps.insert(&key, Item::Value(Value::InlineTable(table)));
    Ok((doc.to_string(), true))
}

/// The GUIraffe checkout an app already depends on, from any existing `guiraffe-*` path
/// dependency (`<checkout>/crates/<name>`). Mixing checkouts would put two copies of
/// guiraffe-core in one build, so `add` must reuse it.
pub fn checkout_of(manifest: &str) -> Option<std::path::PathBuf> {
    let doc: DocumentMut = manifest.parse().ok()?;
    let deps = doc.get("dependencies")?.as_table_like()?;
    deps.iter()
        .filter(|(k, _)| k.starts_with("guiraffe-"))
        .find_map(|(_, v)| {
            let path = v.as_table_like()?.get("path")?.as_str()?;
            let crate_dir = Path::new(path);
            (crate_dir.parent()?.file_name()? == "crates")
                .then(|| crate_dir.parent()?.parent().map(Path::to_path_buf))?
        })
}

/// Add each of `names` to the app manifest in `app_dir`, using crates from `checkout`.
pub fn run(app_dir: &Path, checkout: &Path, names: &[String]) -> Result<()> {
    if names.is_empty() {
        println!("Available crates:\n  {}", available(checkout).join("\n  "));
        return Ok(());
    }
    let manifest_path = app_dir.join("Cargo.toml");
    let mut manifest = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("no Cargo.toml in {}", app_dir.display()))?;
    // Prefer the checkout the app already uses over the default.
    let inferred = checkout_of(&manifest);
    let checkout = inferred.as_deref().unwrap_or(checkout);
    let known = available(checkout);
    for raw in names {
        let name = raw.strip_prefix("guiraffe-").unwrap_or(raw);
        if !known.iter().any(|k| k == name) {
            bail!("unknown crate `{raw}`; available: {}", known.join(", "));
        }
        let (next, changed) = add_dependency(&manifest, name, &checkout.join("crates").join(name))?;
        manifest = next;
        println!(
            "{} guiraffe-{name}",
            if changed { "added" } else { "already present:" }
        );
    }
    std::fs::write(&manifest_path, manifest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str =
        "[package]\nname = \"app\"\n\n[dependencies]\n# keep me\ngpui-kit = \"=0.9.1\"\n";

    #[test]
    fn adds_once_and_preserves_comments() {
        let (once, changed) =
            add_dependency(APP, "markdown", Path::new("/g/crates/markdown")).unwrap_or_default();
        assert!(changed);
        assert!(once.contains("# keep me"), "{once}");
        assert!(
            once.contains("guiraffe-markdown = { path = \"/g/crates/markdown\" }"),
            "{once}"
        );
        let (twice, changed) =
            add_dependency(&once, "markdown", Path::new("/g/crates/markdown")).unwrap_or_default();
        assert!(!changed);
        assert_eq!(once, twice);
    }

    #[test]
    fn creates_dependencies_table_when_missing() {
        let (out, changed) = add_dependency(
            "[package]\nname = \"a\"\n",
            "flow",
            Path::new("/g/crates/flow"),
        )
        .unwrap_or_default();
        assert!(
            changed && out.contains("[dependencies]") && out.contains("guiraffe-flow"),
            "{out}"
        );
    }

    #[test]
    fn infers_the_checkout_the_app_already_uses() {
        let app = "[dependencies]\nguiraffe-core = { path = \"/src/guiraffe/crates/core\" }\n";
        assert_eq!(
            checkout_of(app),
            Some(std::path::PathBuf::from("/src/guiraffe"))
        );
        assert_eq!(checkout_of("[dependencies]\ngpui-kit = \"=0.9.1\"\n"), None);
    }

    #[test]
    fn lists_real_crates_but_not_tools() {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let names = available(&checkout);
        assert!(
            names.contains(&"markdown".to_string()) && names.contains(&"timeline".to_string()),
            "{names:?}"
        );
        assert!(!names.contains(&"cli".to_string()) && !names.contains(&"bench".to_string()));
    }
}
