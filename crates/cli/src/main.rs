//! `guiraffe` — the GUIraffe developer CLI.
//!
//! ```text
//! guiraffe new <name> [--path <guiraffe checkout>]   scaffold an app (gpui-fast, router, store, shell)
//! guiraffe preview <file.json|design.mocket.json> [--screen <id|name>]   live preview with hot reload
//! guiraffe import <design.mocket.json> [--screen <id|name>]              Mocket → GUIraffe document JSON
//! guiraffe doctor                                    check toolchain, system libraries and app pins
//! ```

mod doctor;
mod preview;
mod scaffold;

use std::path::PathBuf;

use anyhow::{Result, bail};

fn option(args: &[String], allowed: &str) -> Result<Option<String>> {
    match args {
        [] => Ok(None),
        [flag, value] if flag == allowed && !value.starts_with("--") => Ok(Some(value.clone())),
        _ => bail!("expected optional {allowed} <value>, got {args:?}"),
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("new") => {
            let Some(name) = args.get(1) else {
                bail!("usage: guiraffe new <name> [--path <checkout>]")
            };
            let checkout = option(&args[2..], "--path")?
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")));
            let checkout = checkout.canonicalize().unwrap_or(checkout);
            let dir = scaffold::new_app(
                &std::env::current_dir()?,
                name,
                &scaffold::Source::Path(checkout),
            )?;
            println!("Created {}\n  cd {name} && cargo run", dir.display());
            Ok(())
        }
        Some("preview") => {
            let Some(file) = args.get(1) else {
                bail!("usage: guiraffe preview <file> [--screen <id>]")
            };
            preview::run(PathBuf::from(file), option(&args[2..], "--screen")?)
        }
        Some("import") => {
            let Some(file) = args.get(1) else {
                bail!("usage: guiraffe import <design.mocket.json> [--screen <id>]")
            };
            let text = std::fs::read_to_string(file)?;
            let imp = guiraffe_mocket::import(&text, option(&args[2..], "--screen")?.as_deref())?;
            for r in imp
                .report
                .iter()
                .filter(|r| r.coverage != guiraffe_mocket::Coverage::Mapped)
            {
                eprintln!("{:?}: {} ({})", r.coverage, r.mocket_id, r.mocket_type);
            }
            println!("{}", serde_json::to_string_pretty(&imp.document)?);
            Ok(())
        }
        Some("doctor") => {
            let ok = doctor::report(&doctor::gather(&std::env::current_dir()?));
            if ok {
                Ok(())
            } else {
                bail!("doctor found problems")
            }
        }
        Some("--help" | "-h") | None => {
            println!(
                "guiraffe new <name> [--path <checkout>]\nguiraffe preview <file> [--screen <id|name>]\nguiraffe import <design.mocket.json> [--screen <id|name>]\nguiraffe doctor"
            );
            Ok(())
        }
        _ => bail!(
            "usage: guiraffe new <name> | preview <file> | import <design.mocket.json> | doctor"
        ),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => 0.into(),
        Err(e) => {
            eprintln!("error: {e:#}");
            1.into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_reject_missing_values_unknown_flags_and_extra_arguments() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(option(&[], "--path").ok(), Some(None));
        assert_eq!(
            option(&args(&["--path", "a b"]), "--path").ok(),
            Some(Some("a b".into()))
        );
        for bad in [
            vec!["--path"],
            vec!["--screen", "a"],
            vec!["--path", "--screen"],
            vec!["a"],
            vec!["--path", "a", "b"],
        ] {
            assert!(option(&args(&bad), "--path").is_err());
        }
    }
}
