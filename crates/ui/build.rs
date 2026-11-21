use std::{env, fs, io, path::PathBuf};

// The positional schemas are consumed by DataUi. The build only exports metadata.
#[allow(dead_code)]
#[path = "src/catalog.rs"]
mod catalog;
#[allow(dead_code)]
#[path = "../core/src/widget_spec.rs"]
mod widget_spec;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src/catalog.rs");
    println!("cargo:rerun-if-changed=../core/src/widget_spec.rs");
    println!("cargo:rerun-if-changed=../inputs/src/widget_specs.rs");
    let output =
        PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("missing OUT_DIR"))?);
    for (name, mut value) in [
        ("catalogue.json", catalog::catalogue()),
        ("screen.schema.json", catalog::screen_schema()),
    ] {
        value.sort_all_objects();
        fs::write(
            output.join(name),
            serde_json::to_string_pretty(&value)? + "\n",
        )?;
    }
    fs::write(output.join("catalogue.md"), catalog::reference_markdown())?;
    Ok(())
}
