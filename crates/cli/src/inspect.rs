//! `guiraffe mcp` and `guiraffe test --script`: the inspect and drive API of a
//! running app, over MCP or a local socket (spec/21 §2).
//!
//! The app runs headless (the snapshot renderer), so the tools work without a
//! display server.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, Render, TestAppContext, TestDispatcher, Window, div};
use guiraffe_test::drive::Driver;
use guiraffe_test::inspect::EventSink;
use guiraffe_test::replay::Script;
use guiraffe_test::transport::Server;
use guiraffe_ui::{DataUi, Document, NoLogic, Registry};

/// Serves the API as MCP tools over stdio.
///
/// The app is the document given, or an empty screen.
pub fn mcp(document: Option<&Path>) -> Result<()> {
    with_app(document, |server| {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line.context("read a request")?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(answer) = guiraffe_test::transport::mcp(&line, server) {
                writeln!(stdout, "{answer}")?;
                stdout.flush()?;
            }
        }
        Ok(())
    })
}

/// Runs a recorded script again and answers with the failures.
///
/// The answer is `true` when every assertion held.
pub fn script(path: &Path, document: Option<&Path>) -> Result<bool> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let script = Script::from_json(&text).context("the script is not a script")?;
    with_app(document, |server| {
        let result = guiraffe_test::replay::run(server.driver_mut(), &script);
        for step in &result.steps {
            for failure in &step.failures {
                println!("{}: {failure}", step.action);
            }
        }
        Ok(result.ok())
    })
}

/// Serves the API on a local socket, one JSON document per line.
pub fn serve(socket: &Path, document: Option<&Path>) -> Result<()> {
    with_app(document, |server| {
        guiraffe_test::transport::serve(socket, server).context("serve the socket")
    })
}

/// Runs the app headless and hands the server to `f`.
fn with_app<R>(document: Option<&Path>, f: impl FnOnce(&mut Server) -> Result<R>) -> Result<R> {
    let mut cx = TestAppContext::build(TestDispatcher::new(0), None);
    let text = document
        .map(std::fs::read_to_string)
        .transpose()
        .with_context(|| "reading the document")?;
    let doc = text
        .as_deref()
        .and_then(|text| Document::from_json(text).ok());
    let events = EventSink::new();
    let app_events = events.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| AppRoot {
        ui: doc.and_then(|doc| {
            DataUi::new(&doc, Arc::new(Registry::with_builtins()), NoLogic, cx).ok()
        }),
        events: app_events,
    });
    vcx.run_until_parked();
    let driver = Driver::new(vcx, events);
    let mut server = Server::new(driver);
    f(&mut server)
}

/// The root of the headless app: the document, when there is one.
struct AppRoot {
    ui: Option<Entity<DataUi>>,
    events: EventSink,
}

impl Render for AppRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        let _ = &self.events;
        match &self.ui {
            Some(ui) => div().size_full().child(ui.clone()),
            None => div().size_full(),
        }
    }
}
