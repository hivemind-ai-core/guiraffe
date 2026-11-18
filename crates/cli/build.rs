use std::{env, fs, io, path::PathBuf};

// The positional schemas are consumed by DataUi. The build only exports metadata.
#[allow(dead_code)]
#[path = "../ui/src/catalog.rs"]
mod catalog;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../ui/src/catalog.rs");
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
