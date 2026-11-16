//! guiraffe-mocket: Mocket → GUIraffe.
//!
//! - `guiraffe-mocket import <design.mocket.json> [screen]` → GUIraffe document JSON on stdout
//! - `guiraffe-mocket screens <design.mocket.json>`         → list screens
//! - `guiraffe-mocket plugin`                               → mocket.plugin/1 (stdin → stdout)

use std::io::Read;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = match args.first().map(String::as_str) {
        Some("plugin") => {
            let mut req = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut req) {
                eprintln!("reading request: {e}");
                return 2.into();
            }
            println!("{}", guiraffe_mocket::plugin_run(&req));
            return 0.into();
        }
        Some(cmd @ ("import" | "screens")) => match args.get(1).map(std::fs::read_to_string) {
            Some(Ok(json)) if cmd == "screens" => guiraffe_mocket::screens(&json)
                .map(|l| {
                    l.into_iter()
                        .map(|(id, n)| format!("{id}\t{n}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .map_err(|e| e.to_string()),
            Some(Ok(json)) => guiraffe_mocket::import(&json, args.get(2).map(String::as_str))
                .map(|imp| {
                    for r in imp
                        .report
                        .iter()
                        .filter(|r| r.coverage != guiraffe_mocket::Coverage::Mapped)
                    {
                        eprintln!("{:?}: {} ({})", r.coverage, r.mocket_id, r.mocket_type);
                    }
                    serde_json::to_string_pretty(&imp.document).unwrap_or_default()
                })
                .map_err(|e| e.to_string()),
            Some(Err(e)) => Err(format!("reading file: {e}")),
            None => Err("missing file argument".into()),
        },
        _ => Err("usage: guiraffe-mocket import <file> [screen] | screens <file> | plugin".into()),
    };
    match out {
        Ok(s) => {
            println!("{s}");
            0.into()
        }
        Err(e) => {
            eprintln!("{e}");
            1.into()
        }
    }
}
