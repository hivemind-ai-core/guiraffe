//! Embed every vendored Lucide SVG as a sorted `(path, bytes)` table.
use std::{fmt::Write, path::Path};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/lucide");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.ends_with(".svg")).collect())
        .unwrap_or_default();
    names.sort();
    let mut out = String::from("pub(crate) static ICONS: &[(&str, &[u8])] = &[\n");
    for n in &names {
        let _ = writeln!(out, "    (\"icons/{n}\", include_bytes!(\"{}\")),", dir.join(n).display());
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into())).join("icons.rs");
    if let Err(e) = std::fs::write(&dest, out) {
        println!("cargo:warning=writing icon table: {e}");
    }
}
