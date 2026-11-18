//! `guiraffe` — the GUIraffe developer CLI.
//!
//! ```text
//! guiraffe new <name> [--path <guiraffe checkout>]   scaffold an app (gpui-fast, router, store, shell)
//! guiraffe preview <file.json|design.mocket.json> [--screen <id|name>]   live preview with hot reload
//! guiraffe import <design.mocket.json> [--screen <id|name>]              Mocket → GUIraffe document JSON
//! guiraffe add [<crate>…] [--path <checkout>]       add GUIraffe crates to this app (no args: list)
//! guiraffe dev [--release]                         watch, rebuild and relaunch the app in this directory
//! guiraffe doctor                                    check toolchain, system libraries and app pins
//! guiraffe mcp [--document <file>]                    serve the inspect and drive API as MCP tools
//! guiraffe test --script <file> [--document <file>]  run a recorded interaction script again
//! ```

mod add;
mod dev;
mod doctor;
mod inspect;
mod preview;
mod scaffold;

use guiraffe_ui::catalogue_artifacts::{
    CATALOGUE_JSON as CATALOGUE, REFERENCE_MARKDOWN as CATALOGUE_REFERENCE, SCREEN_SCHEMA,
};

use std::path::{Path, PathBuf};

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
        Some("catalog") => {
            match args.get(1..).unwrap_or_default() {
                [flag] if flag == "--json" => print!("{CATALOGUE}"),
                [flag] if flag == "--schema" => print!("{SCREEN_SCHEMA}"),
                [flag] if flag == "--markdown" => print!("{CATALOGUE_REFERENCE}"),
                _ => bail!("usage: guiraffe catalog --json | --schema | --markdown"),
            }
            Ok(())
        }
        Some("preview") => {
            let Some(file) = args.get(1) else {
                bail!("usage: guiraffe preview <file> [--screen <id>] [--inspect <socket>]")
            };
            // With --inspect the preview runs headless and the API is served
            // on the socket, for an agent that wants to look at it.
            match option(&args[2..], "--inspect")?.map(PathBuf::from) {
                Some(socket) => inspect::serve(&socket, Some(Path::new(file))),
                None => preview::run(PathBuf::from(file), option(&args[2..], "--screen")?),
            }
        }
        Some("mcp") => {
            let document = option(&args[1..], "--document")?.map(PathBuf::from);
            inspect::mcp(document.as_deref())
        }
        Some("test") => {
            let Some(script) = args.get(1) else {
                bail!("usage: guiraffe test --script <file> [--document <file>]")
            };
            let document = option(&args[2..], "--document")?.map(PathBuf::from);
            if !inspect::script(Path::new(script), document.as_deref())? {
                bail!("the script failed")
            }
            Ok(())
        }
        Some("import") => {
            let Some(file) = args.get(1) else {
                bail!("usage: guiraffe import <design.mocket.json> [--screen <id>]")
            };
            let text = std::fs::read_to_string(file)?;
            let imp = guiraffe_mocket::import(&text, option(&args[2..], "--screen")?.as_deref())?;
            preview::require_valid(imp.validation_issues())?;
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
        Some("add") => {
            let (names, path): (Vec<String>, Option<String>) =
                match args.iter().position(|a| a == "--path") {
                    Some(i) => (args[1..i].to_vec(), args.get(i + 1).cloned()),
                    None => (args[1..].to_vec(), None),
                };
            let checkout = path
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")));
            let checkout = checkout.canonicalize().unwrap_or(checkout);
            add::run(&std::env::current_dir()?, &checkout, &names)
        }
        Some("dev") => {
            let release = args[1..].iter().any(|a| a == "--release");
            dev::run(&std::env::current_dir()?, release)
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
                "guiraffe new <name> [--path <checkout>]\nguiraffe preview <file> [--screen <id|name>]\nguiraffe import <design.mocket.json> [--screen <id|name>]\nguiraffe add [<crate>…]\nguiraffe catalog --json | --schema | --markdown\nguiraffe doctor"
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

#[cfg(test)]
mod catalogue_tests;
