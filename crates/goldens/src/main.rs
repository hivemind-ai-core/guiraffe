//! `cargo run -p guiraffe-goldens -- [--repo <path>] [--base <ref>] [--demo]`
//!
//! Lists the goldens changed since `--base` (default `HEAD`: the working tree's changes,
//! which is what `scripts/gate.sh --regen` produces) and opens the review window.
use std::{path::PathBuf, sync::Arc};

fn main() {
    let mut repo = PathBuf::from(".");
    let mut base = "HEAD".to_owned();
    let mut demo = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo = args.next().map(PathBuf::from).unwrap_or(repo),
            "--base" => base = args.next().unwrap_or(base),
            "--demo" => demo = true,
            "-h" | "--help" => {
                println!("guiraffe-goldens [--repo <path>] [--base <ref>] [--demo]");
                return;
            }
            other => {
                eprintln!("unknown argument {other:?}; see --help");
                std::process::exit(2);
            }
        }
    }
    let (source, goldens): (Arc<dyn guiraffe_goldens::discover::Source>, _) = if demo {
        let (source, goldens) = guiraffe_goldens::demo();
        (Arc::new(source), goldens)
    } else {
        let git = guiraffe_goldens::discover::Git { repo, base };
        match git.changed() {
            Ok(goldens) => (Arc::new(git), goldens),
            Err(e) => {
                eprintln!("could not list changed goldens: {e:#}");
                std::process::exit(1);
            }
        }
    };
    gpui_kit::application().run(move |cx| {
        guiraffe_core::init(cx);
        guiraffe_windows::WindowManager::init(guiraffe_windows::WindowConfig::default(), cx);
        if let Err(e) = guiraffe_goldens::open(source, goldens, cx) {
            eprintln!("failed to open the review window: {e:#}");
            cx.quit();
        }
    });
}
