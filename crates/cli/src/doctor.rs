//! `guiraffe doctor`: check that this machine (and the app in the current directory, if any)
//! can build and run GUIraffe apps, with an actionable fix for every problem.
//!
//! The checks are pure functions over text (tool output, manifests, lockfiles) so they are unit
//! tested; [`run`] only gathers that text from the system.

use std::{path::Path, process::Command};

/// Minimum Rust version (workspace `rust-version`).
pub const MIN_RUST: (u32, u32) = (1, 90);
/// gpui-fast 0.1.16 does not compile with gpui-kit 0.9.1 (KNOWN_ISSUES #3).
pub const GPUI_FAST_PIN: &str = "0.1.15";
pub const GPUI_KIT_PIN: &str = "0.9.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
    /// What to do about it (empty when `Ok`).
    pub fix: String,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: Status::Ok,
            detail: detail.into(),
            fix: String::new(),
        }
    }
    fn bad(name: &str, status: Status, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
            fix: fix.into(),
        }
    }
}

/// Parse `rustc --version` output ("rustc 1.98.0 (…)") into (major, minor).
pub fn parse_rustc(version: &str) -> Option<(u32, u32)> {
    let v = version.split_whitespace().nth(1)?;
    let mut parts = v.split(['.', '-']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

pub fn check_rustc(output: Option<&str>) -> Check {
    match output.and_then(parse_rustc) {
        Some(v) if v >= MIN_RUST => Check::ok("Rust toolchain", format!("rustc {}.{}", v.0, v.1)),
        Some(v) => Check::bad(
            "Rust toolchain",
            Status::Fail,
            format!(
                "rustc {}.{} is older than {}.{}",
                v.0, v.1, MIN_RUST.0, MIN_RUST.1
            ),
            "rustup update stable",
        ),
        None => Check::bad(
            "Rust toolchain",
            Status::Fail,
            "rustc not found",
            "install Rust from https://rustup.rs",
        ),
    }
}

/// System libraries GPUI needs on Linux: (pkg-config name, human name, Debian/Ubuntu package,
/// Fedora package, Arch package).
pub const LINUX_LIBS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "xkbcommon",
        "xkbcommon",
        "libxkbcommon-dev",
        "libxkbcommon-devel",
        "libxkbcommon",
    ),
    (
        "xkbcommon-x11",
        "xkbcommon (X11)",
        "libxkbcommon-x11-dev",
        "libxkbcommon-x11-devel",
        "libxkbcommon-x11",
    ),
    (
        "wayland-client",
        "Wayland client",
        "libwayland-dev",
        "wayland-devel",
        "wayland",
    ),
    ("xcb", "XCB", "libxcb1-dev", "libxcb-devel", "libxcb"),
    (
        "fontconfig",
        "fontconfig",
        "libfontconfig-dev",
        "fontconfig-devel",
        "fontconfig",
    ),
    (
        "freetype2",
        "FreeType",
        "libfreetype-dev",
        "freetype-devel",
        "freetype2",
    ),
];

/// One library check from whether `pkg-config --exists` succeeded.
pub fn check_lib(entry: &(&str, &str, &str, &str, &str), found: bool) -> Check {
    let (_, human, deb, fedora, arch) = *entry;
    if found {
        Check::ok(human, "found")
    } else {
        Check::bad(
            human,
            Status::Fail,
            "not found by pkg-config",
            format!(
                "Debian/Ubuntu: sudo apt install {deb} · Fedora: sudo dnf install {fedora} · Arch: sudo pacman -S {arch}"
            ),
        )
    }
}

/// The Vulkan loader (GPUI renders with Vulkan on Linux), from `ldconfig -p` output.
pub fn check_vulkan(ldconfig: Option<&str>) -> Check {
    match ldconfig {
        Some(out) if out.contains("libvulkan.so") => Check::ok("Vulkan loader", "libvulkan found"),
        Some(_) => Check::bad(
            "Vulkan loader",
            Status::Fail,
            "libvulkan.so not found",
            "install your GPU's Vulkan driver and loader (e.g. apt install libvulkan1 mesa-vulkan-drivers)",
        ),
        None => Check::bad(
            "Vulkan loader",
            Status::Warn,
            "could not run ldconfig",
            "make sure a Vulkan driver is installed",
        ),
    }
}

/// Dependency pins in an app's `Cargo.toml` (text). `None` when this isn't a GUIraffe app.
pub fn check_manifest(manifest: &str) -> Option<Vec<Check>> {
    let uses_guiraffe = manifest.contains("guiraffe-") || manifest.contains("gpui-kit");
    if !uses_guiraffe {
        return None;
    }
    let line = |dep: &str| {
        manifest
            .lines()
            .find(|l| l.trim_start().starts_with(dep))
            .map(str::to_owned)
    };
    let mut out = Vec::new();
    out.push(match line("gpui-kit") {
        Some(l) if l.contains(&format!("\"={GPUI_KIT_PIN}\"")) => {
            Check::ok("gpui-kit pin", format!("={GPUI_KIT_PIN}"))
        }
        Some(l) => Check::bad(
            "gpui-kit pin",
            Status::Fail,
            format!("expected =\"{GPUI_KIT_PIN}\": {}", l.trim()),
            format!("pin it exactly: gpui-kit = {{ version = \"={GPUI_KIT_PIN}\", … }}"),
        ),
        None => Check::bad(
            "gpui-kit pin",
            Status::Warn,
            "no gpui-kit dependency",
            "add gpui-kit (guiraffe add gpui-kit)",
        ),
    });
    if manifest.contains("gpui-fast") {
        out.push(match line("gpui-fast") {
            Some(l) if l.contains(&format!("\"={GPUI_FAST_PIN}\"")) => Check::ok("gpui-fast pin", format!("={GPUI_FAST_PIN}")),
            _ => Check::bad(
                "gpui-fast pin",
                Status::Fail,
                format!("gpui-fast must be pinned to ={GPUI_FAST_PIN} (0.1.16 breaks gpui-kit {GPUI_KIT_PIN}; KNOWN_ISSUES #3)"),
                format!("add: gpui-fast = \"={GPUI_FAST_PIN}\""),
            ),
        });
    }
    Some(out)
}

/// The resolved gpui-fast version in `Cargo.lock` (text), if any.
pub fn check_lock(lock: &str) -> Option<Check> {
    let mut lines = lock.lines();
    while let Some(l) = lines.next() {
        if l.trim() == "name = \"gpui-fast\"" {
            let version = lines
                .next()?
                .trim()
                .strip_prefix("version = \"")?
                .trim_end_matches('"')
                .to_owned();
            return Some(if version == GPUI_FAST_PIN {
                Check::ok("gpui-fast lock", version)
            } else {
                Check::bad(
                    "gpui-fast lock",
                    Status::Fail,
                    format!("Cargo.lock resolves gpui-fast {version}"),
                    format!("cargo update -p gpui-fast --precise {GPUI_FAST_PIN}"),
                )
            });
        }
    }
    None
}

fn output(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Gather every check for this machine and the app in `dir` (if it is one).
pub fn gather(dir: &Path) -> Vec<Check> {
    let mut checks = vec![check_rustc(output("rustc", &["--version"]).as_deref())];
    if cfg!(target_os = "linux") {
        let have_pkg_config = output("pkg-config", &["--version"]).is_some();
        if have_pkg_config {
            for entry in LINUX_LIBS {
                let found = Command::new("pkg-config")
                    .args(["--exists", entry.0])
                    .status()
                    .is_ok_and(|s| s.success());
                checks.push(check_lib(entry, found));
            }
        } else {
            checks.push(Check::bad(
                "pkg-config",
                Status::Fail,
                "not found",
                "install pkg-config (apt install pkg-config)",
            ));
        }
        checks.push(check_vulkan(
            output("ldconfig", &["-p"])
                .or_else(|| output("/sbin/ldconfig", &["-p"]))
                .as_deref(),
        ));
    }
    if let Ok(manifest) = std::fs::read_to_string(dir.join("Cargo.toml")) {
        checks.extend(check_manifest(&manifest).unwrap_or_default());
        if let Ok(lock) = std::fs::read_to_string(dir.join("Cargo.lock")) {
            checks.extend(check_lock(&lock));
        }
    }
    checks
}

/// Print a report; returns whether everything required passed (warnings are allowed).
pub fn report(checks: &[Check]) -> bool {
    for c in checks {
        let mark = match c.status {
            Status::Ok => "✓",
            Status::Warn => "!",
            Status::Fail => "✗",
        };
        println!("{mark} {:<20} {}", c.name, c.detail);
        if !c.fix.is_empty() {
            println!("    fix: {}", c.fix);
        }
    }
    let failed = checks.iter().filter(|c| c.status == Status::Fail).count();
    if failed == 0 {
        println!("\nAll checks passed.");
    } else {
        println!("\n{failed} problem(s) found.");
    }
    failed == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustc_versions() {
        assert_eq!(
            parse_rustc("rustc 1.98.0 (88d9e12ae 2026-08-18)"),
            Some((1, 98))
        );
        assert_eq!(parse_rustc("rustc 1.90.0-nightly (x)"), Some((1, 90)));
        assert_eq!(check_rustc(Some("rustc 1.89.2 (x)")).status, Status::Fail);
        assert_eq!(check_rustc(Some("rustc 1.90.0 (x)")).status, Status::Ok);
        assert_eq!(check_rustc(None).status, Status::Fail);
    }

    #[test]
    fn manifest_pins() {
        let good = "[dependencies]\ngpui-kit = { version = \"=0.9.1\", features = [\"gpui-fast\"] }\ngpui-fast = \"=0.1.15\"\nguiraffe-core = { path = \"x\" }\n";
        assert!(check_manifest(good).is_some_and(|c| c.iter().all(|c| c.status == Status::Ok)));
        let loose = "gpui-kit = \"0.9\"\ngpui-fast = \"0.1\"\nguiraffe-core = \"0.1\"\n";
        let c = check_manifest(loose).unwrap_or_default();
        assert_eq!(
            c.iter().filter(|c| c.status == Status::Fail).count(),
            2,
            "{c:?}"
        );
        assert!(
            check_manifest("[package]\nname = \"other\"\n").is_none(),
            "not a GUIraffe app"
        );
    }

    #[test]
    fn lock_versions() {
        let lock = "[[package]]\nname = \"gpui-fast\"\nversion = \"0.1.16\"\n";
        let c = check_lock(lock);
        assert!(
            c.as_ref()
                .is_some_and(|c| c.status == Status::Fail && c.fix.contains("--precise 0.1.15")),
            "{c:?}"
        );
        assert!(
            check_lock("[[package]]\nname = \"gpui-fast\"\nversion = \"0.1.15\"\n")
                .is_some_and(|c| c.status == Status::Ok)
        );
        assert!(check_lock("").is_none());
    }

    #[test]
    fn libs_and_vulkan_give_fixes() {
        let c = check_lib(&LINUX_LIBS[0], false);
        assert_eq!(c.status, Status::Fail);
        assert!(c.fix.contains("libxkbcommon-dev"));
        assert_eq!(
            check_vulkan(Some(
                "libvulkan.so.1 (libc6,x86-64) => /usr/lib/libvulkan.so.1"
            ))
            .status,
            Status::Ok
        );
        assert_eq!(check_vulkan(Some("libc.so.6")).status, Status::Fail);
    }
}
