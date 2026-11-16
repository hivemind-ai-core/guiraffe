use std::process::Command;

#[test]
fn help_import_and_invalid_arguments_have_useful_exit_statuses() -> anyhow::Result<()> {
    let binary = env!("CARGO_BIN_EXE_guiraffe");
    let help = Command::new(binary).arg("--help").output()?;
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("preview"));

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mocket/tests/fixtures/sample.mocket.json");
    let imported = Command::new(binary)
        .arg("import")
        .arg(&fixture)
        .args(["--screen", "Order detail"])
        .output()?;
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let doc: guiraffe_ui::Document = serde_json::from_slice(&imported.stdout)?;
    assert_eq!(doc.version, 1);
    assert!(String::from_utf8_lossy(&imported.stderr).contains("Unsupported"));

    let bad = Command::new(binary)
        .arg("import")
        .arg(fixture)
        .arg("--screen")
        .output()?;
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("--screen <value>"));
    Ok(())
}
