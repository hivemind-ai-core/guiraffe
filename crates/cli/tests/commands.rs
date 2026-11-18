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

#[test]
fn the_mcp_server_answers_a_request_on_stdin() -> anyhow::Result<()> {
    let binary = env!("CARGO_BIN_EXE_guiraffe");
    let mut child = Command::new(binary)
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin");
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list"}}"#)?;
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("inspect_nodes"), "{stdout}");
    assert!(stdout.contains("drive"), "{stdout}");
    Ok(())
}

#[test]
fn a_script_that_fails_makes_the_command_fail() -> anyhow::Result<()> {
    let binary = env!("CARGO_BIN_EXE_guiraffe");
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("broken.json");
    std::fs::write(
        &script,
        r#"{"name":"broken","steps":[{"action":{"action":"click","id":"nope"},"assert":[]}]}"#,
    )?;
    let output = Command::new(binary)
        .arg("test")
        .arg("--script")
        .arg(&script)
        .output()?;
    assert!(!output.status.success(), "the script should fail");
    Ok(())
}

#[test]
fn the_inspected_preview_answers_on_the_socket() -> anyhow::Result<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let binary = env!("CARGO_BIN_EXE_guiraffe");
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("inspect.sock");
    let document = dir.path().join("doc.json");
    std::fs::write(
        &document,
        r#"{"version":1,"root":{"kind":"text","props":{"text":"hi"}}}"#,
    )?;
    let mut child = Command::new(binary)
        .arg("preview")
        .arg(&document)
        .arg("--inspect")
        .arg(&socket)
        .stdout(std::process::Stdio::null())
        .spawn()?;

    // The socket appears when the preview is up.
    let mut stream = None;
    for _ in 0..100 {
        if socket.exists() {
            stream = UnixStream::connect(&socket).ok();
            if stream.is_some() {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let Some(mut stream) = stream else {
        let _ = child.kill();
        anyhow::bail!("the inspect socket never came up");
    };
    stream.set_read_timeout(Some(std::time::Duration::from_secs(20)))?;
    writeln!(stream, r#"{{"method":"style"}}"#)?;
    stream.flush()?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut answer = String::new();
    reader.read_line(&mut answer)?;
    let _ = child.kill();
    assert!(answer.contains("\"ok\":true"), "{answer}");
    assert!(answer.contains("control_size"), "{answer}");
    Ok(())
}
