//! Embed every vendored Lucide SVG as a sorted `(path, bytes)` table.
use std::{fmt::Write, path::Path};

fn main() {
    // Read the manifest dir at run time: `env!` would bake in the path of whichever checkout
    // first compiled this script, and a cached script from a deleted worktree then silently
    // embedded zero icons.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let dir = Path::new(&manifest).join("assets/lucide");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".svg"))
            .collect(),
        Err(e) => panic!("guiraffe-icons: cannot read {}: {e}", dir.display()),
    };
    assert!(
        names.len() > 1000,
        "guiraffe-icons: only {} icons found in {}",
        names.len(),
        dir.display()
    );
    names.sort();
    let mut out = String::from("pub(crate) static ICONS: &[(&str, &[u8])] = &[\n");
    for n in &names {
        let _ = writeln!(
            out,
            "    (\"icons/{n}\", include_bytes!(\"{}\")),",
            dir.join(n).display()
        );
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into())).join("icons.rs");
    if let Err(e) = std::fs::write(&dest, out) {
        println!("cargo:warning=writing icon table: {e}");
    }
}
