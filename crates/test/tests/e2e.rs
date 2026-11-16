//! Real-renderer spike. Build the example first, run this test inside xvfb-run.
#![cfg(target_os = "linux")]
use anyhow::{Context, Result, bail};
use guiraffe_test::e2e::Client;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

struct App(Child);
impl Drop for App {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "requires built probe_app, X11 server, renderer, import and xdotool; see spikes/visual-snapshots.md"]
fn real_app_input_and_five_fresh_captures() -> Result<()> {
    let root = PathBuf::from(format!("/tmp/guiraffe-visual-{}", std::process::id()));
    fs::create_dir(&root)?;
    eprintln!("artifacts: {}", root.display());
    let example = std::env::current_exe()?
        .parent()
        .context("test parent")?
        .parent()
        .context("debug directory")?
        .join("examples/probe_app");
    if !example.is_file() {
        bail!(
            "build example first: cargo build -p guiraffe-test --features e2e --example probe_app"
        );
    }
    let mut baseline = None;
    for run in 0..5 {
        let output = root.join(format!("run-{run}"));
        fs::create_dir(&output)?;
        let socket = output.join("probe.sock");
        let mut app = App(Command::new(&example).arg(&socket).arg(&output).spawn()?);
        let mut client = Client::new(socket);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = app.0.try_wait()? {
                bail!(
                    "GPUI app exited before ready: {status}; artifacts: {}",
                    root.display()
                );
            }
            if client.call("query", json!({"selector":"save"})).is_ok() {
                break;
            }
            if Instant::now() >= deadline {
                bail!("app not ready; artifacts: {}", root.display());
            }
            thread::sleep(Duration::from_millis(25));
        }
        let windows = client.call("windows", Value::Null)?;
        assert_eq!(windows.as_array().map(Vec::len), Some(1));
        let target = client.call("query", json!({"selector":"save"}))?;
        assert_eq!(target["bounds"][2], 200.0);
        assert_eq!(target["bounds"][3], 64.0);
        assert_eq!(target["bounds"][0], 16.0);
        assert_eq!(target["bounds"][1], 16.0);
        client.call("wait_idle", Value::Null)?;
        // A CPU paint callback is not a GPU/presentation fence. Poll an explicit
        // visual readiness condition; never accept identical blank frames.
        let ready_deadline = Instant::now() + Duration::from_secs(10);
        let (path, pixels) = loop {
            let capture = client.call("capture", Value::Null)?;
            let path = capture["path"].as_str().context("capture path")?.to_owned();
            let pixels = image::open(&path)?.to_rgba8();
            assert_eq!(pixels.dimensions(), (320, 200));
            if pixels.pixels().any(|p| p.0[..3] == [0x24, 0x71, 0xa3])
                && pixels.pixels().any(|p| p.0[..3] == [0xe6, 0x7e, 0x22])
            {
                break (path, pixels);
            }
            if Instant::now() >= ready_deadline {
                bail!("no presented scene before deadline; last capture {path}");
            }
            thread::sleep(Duration::from_millis(100));
        };
        if let Some(first) = &baseline {
            assert!(first == &pixels, "different pixels on launch {run}");
        } else {
            baseline = Some(pixels);
        }
        client.call("click", json!({"selector":"save"}))?;
        client.call("type", json!({"text":"hello"}))?;
        client.call("key", json!({"key":"BackSpace"}))?;
        client.call("wait_idle", Value::Null)?;
        let state = client.call("query", json!({"selector":"save"}))?;
        assert_eq!(state["state"]["clicks"], 1);
        assert_eq!(state["state"]["typed"], "hell");
        assert!(client.call("query", json!({"selector":"missing"})).is_err());
        eprintln!("launch {run}: identical baseline, input verified, capture {path}");
    }
    eprintln!("artifacts retained at {}", root.display());
    Ok(())
}
