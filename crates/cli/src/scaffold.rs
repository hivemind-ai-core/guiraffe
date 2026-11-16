//! `guiraffe new`: write an app scaffold from the bundled templates.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

const CARGO: &str = include_str!("../templates/app/Cargo.toml.tmpl");
const MAIN: &str = include_str!("../templates/app/src/main.rs.tmpl");

/// Where the app gets GUIraffe from: a local checkout. (A published-crates source will be
/// added when GUIraffe is released.)
#[derive(Debug, Clone)]
pub enum Source {
    Path(PathBuf),
}

/// Valid Cargo package name: ASCII letters, digits, `-`, `_`; starts with a letter.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && name.len() <= 64;
    if !ok {
        bail!("invalid app name {name:?}: use letters, digits, '-' or '_', starting with a letter");
    }
    Ok(())
}

pub fn render(template: &str, name: &str, source: &Source) -> Result<String> {
    let Source::Path(path) = source;
    let mut rendered = template.replace("{{name}}", name);
    for krate in [
        "core",
        "components",
        "motion",
        "state",
        "router",
        "windows",
        "overlays",
    ] {
        let path = path.join("crates").join(krate);
        let path = path.to_str().context("checkout path must be UTF-8")?;
        // JSON string escaping is also valid for TOML basic strings.
        rendered = rendered.replace(&format!("{{{{{krate}}}}}"), &serde_json::to_string(path)?);
    }
    Ok(rendered)
}

/// Create `<parent>/<name>/` with the app scaffold. Refuses to overwrite an existing directory.
pub fn new_app(parent: &Path, name: &str, source: &Source) -> Result<PathBuf> {
    validate_name(name)?;
    let Source::Path(path) = source;
    let path = path
        .canonicalize()
        .context("GUIraffe checkout does not exist")?;
    if !path.join("crates/core/Cargo.toml").is_file() {
        bail!("{} is not a GUIraffe checkout", path.display());
    }
    let source = Source::Path(path);
    let manifest = render(CARGO, name, &source)?;
    let main = render(MAIN, name, &source)?;
    let dir = parent.join(name);
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }
    std::fs::create_dir(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::create_dir(dir.join("src"))?;
    std::fs::write(dir.join("Cargo.toml"), manifest)?;
    std::fs::write(dir.join("src/main.rs"), main)?;
    std::fs::write(dir.join(".gitignore"), "target\n")?;
    // Pin exactly the dependency versions GUIraffe was tested with. A fresh resolve can pick
    // newer upstream patch releases (e.g. gpui-fast 0.1.16) that break gpui-base 0.9.1.
    let Source::Path(checkout) = source;
    let lock = checkout.join("Cargo.lock");
    if lock.exists() {
        std::fs::copy(&lock, dir.join("Cargo.lock")).with_context(|| format!("copying {}", lock.display()))?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated() {
        for ok in ["app", "my-app", "a1_b"] {
            assert!(validate_name(ok).is_ok(), "{ok}");
        }
        for bad in ["", "1app", "../evil", "a b", "x/y", &"a".repeat(65)] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn scaffold_writes_a_project_with_gpui_fast_and_local_paths() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap_or_else(|e| panic!("{e}"));
        let src = Source::Path(checkout.clone());
        let dir = new_app(tmp.path(), "hello", &src).unwrap_or_else(|e| panic!("{e}"));
        let cargo = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        assert!(cargo.contains("name = \"hello\""));
        assert!(
            cargo.contains("\"gpui-fast\""),
            "apps default to gpui-fast (DESIGN §2c)"
        );
        assert!(
            cargo.contains(&format!(
                "guiraffe-router = {{ path = {:?} }}",
                checkout.join("crates/router")
            )),
            "{cargo}"
        );
        let main = std::fs::read_to_string(dir.join("src/main.rs")).unwrap_or_default();
        assert!(
            main.contains("Title::new(\"hello\")") && !main.contains("{{"),
            "placeholders filled"
        );
        assert!(
            new_app(tmp.path(), "hello", &src).is_err(),
            "never overwrites"
        );
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn manifest_escapes_paths_and_rejects_invalid_checkouts() -> Result<()> {
        let path = PathBuf::from("/tmp/checkout with \"quotes\" and \\slashes");
        let cargo = render(CARGO, "demo", &Source::Path(path.clone()))?;
        let parsed: toml::Value = toml::from_str(&cargo)?;
        assert_eq!(
            parsed["dependencies"]["guiraffe-core"]["path"].as_str(),
            path.join("crates/core").to_str()
        );
        let tmp = tempfile::tempdir()?;
        assert!(new_app(tmp.path(), "demo", &Source::Path(tmp.path().into())).is_err());
        assert!(!tmp.path().join("demo").exists());
        Ok(())
    }
}
