//! `guiraffe preview <file>`: render a data-UI document (or a Mocket design) in a window and
//! hot-reload it on save, keeping live slot values (spec/10).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context as _, Result};
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px,
};
use guiraffe_ui::{DataUi, Document, NoLogic, Registry};

/// Load a document from `.json` (GUIraffe IR) or `.mocket.json` / any Mocket file.
pub fn load(path: &Path, screen: Option<&str>) -> Result<Document> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text, screen)
}

/// Mocket documents are recognised by their `screens` + `nodes` shape.
pub fn parse(text: &str, screen: Option<&str>) -> Result<Document> {
    let probe: serde_json::Value = serde_json::from_str(text).context("not JSON")?;
    if probe.get("screens").is_some() && probe.get("nodes").is_some() {
        let imported = guiraffe_mocket::import(text, screen)?;
        require_valid(imported.validation_issues())?;
        return Ok(imported.document);
    }
    let document = Document::from_json(text).map_err(anyhow::Error::msg)?;
    validate_structure(&document, true)?;
    Ok(document)
}

pub(crate) fn validate(document: &Document, mocket_placeholders: bool) -> Result<()> {
    require_valid(
        guiraffe_ui::validation::Validation {
            mocket_placeholders,
            bound_events: Some(&std::collections::BTreeSet::new()),
        }
        .check(document),
    )
}

pub(crate) fn validate_structure(document: &Document, mocket_placeholders: bool) -> Result<()> {
    require_valid(
        guiraffe_ui::validation::Validation {
            mocket_placeholders,
            ..Default::default()
        }
        .check(document),
    )
}

pub(crate) fn require_valid(issues: Vec<guiraffe_ui::validation::Issue>) -> Result<()> {
    if !issues.is_empty() {
        anyhow::bail!(
            "document validation failed\n{}",
            serde_json::to_string_pretty(&issues)?
        );
    }
    Ok(())
}

/// Change detection by modification time + length (cheap; no extra dependencies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        Some(Stamp {
            modified: m.modified().ok(),
            len: m.len(),
        })
    }
}

pub(crate) struct Preview {
    path: PathBuf,
    screen: Option<String>,
    ui: Option<Entity<DataUi>>,
    error: Option<String>,
    stamp: Option<Stamp>,
    reloads: u32,
}

impl Preview {
    pub(crate) fn apply(&mut self, loaded: Result<Document>, cx: &mut Context<Self>) {
        match loaded {
            Ok(doc) => {
                self.error = validate(&doc, true).err().map(|error| error.to_string());
                match &self.ui {
                    Some(ui) => {
                        if let Err(e) = ui.update(cx, |u, cx| u.set_document(&doc, cx)) {
                            self.error = Some(e.to_string());
                        }
                    }
                    None => match DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx) {
                        Ok(ui) => self.ui = Some(ui),
                        Err(e) => self.error = Some(e.to_string()),
                    },
                }
                self.reloads += 1;
            }
            // Keep showing the last good document; report the error on top.
            Err(e) => self.error = Some(format!("{e:#}")),
        }
        cx.notify();
    }

    pub(crate) fn watch(&self, cx: &mut Context<Self>) {
        let path = self.path.clone();
        let screen = self.screen.clone();
        let mut stamp = self.stamp;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                if !this.is_upgradable() {
                    break;
                }
                let path = path.clone();
                let screen = screen.clone();
                let next = cx
                    .background_executor()
                    .spawn(async move {
                        let next_stamp = Stamp::of(&path);
                        (next_stamp != stamp).then(|| (next_stamp, load(&path, screen.as_deref())))
                    })
                    .await;
                if let Some((next_stamp, loaded)) = next {
                    stamp = next_stamp;
                    if this.update(cx, |p, cx| p.apply(loaded, cx)).is_err() {
                        break;
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = guiraffe_core::GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .when_some(self.error.clone(), |el, e| {
                el.child(
                    div()
                        .p(px(8.))
                        .bg(guiraffe_core::ColorName::Red.shade(1).to_hsla())
                        .text_color(guiraffe_core::ColorName::Red.shade(9).to_hsla())
                        .child(e),
                )
            })
            .child(
                div()
                    .flex_1()
                    .p(px(24.))
                    .when_some(self.ui.clone(), |el, ui| el.child(ui)),
            )
    }
}

use gpui_kit::prelude::FluentBuilder as _;

/// Runs the preview and serves the API on the socket.
///
/// The preview runs headless (the renderer of the snapshots) and the API
/// serves the live document: the preview reloads it when the file changes,
/// and the inspection follows.
pub fn run_inspect(path: PathBuf, screen: Option<String>, socket: &Path) -> Result<()> {
    let stamp = Stamp::of(&path);
    let doc = load(&path, screen.as_deref())?;
    let mut cx = gpui_kit::HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        std::sync::Arc::new(guiraffe_icons::GuiraffeAssets),
        gpui_kit::platform::current_headless_renderer,
    );
    let window = cx.open_window(
        gpui_kit::size(gpui_kit::px(1280.), gpui_kit::px(800.)),
        move |_, app| {
            app.new(|cx| {
                let mut preview = Preview {
                    path,
                    screen,
                    ui: None,
                    error: None,
                    stamp,
                    reloads: 0,
                };
                preview.apply(Ok(doc), cx);
                preview.watch(cx);
                preview
            })
        },
    )?;
    let calls = guiraffe_test::transport::CallSocket::start(socket)?;
    let mut target = guiraffe_test::inspect::HeadlessTarget::new(&mut cx, window.into());
    let events = guiraffe_test::inspect::EventSink::new();
    let driver = guiraffe_test::drive::Driver::new(&mut target, events);
    let mut server = guiraffe_test::transport::Server::new(driver);
    loop {
        server.park();
        while let Some((id, call)) = calls.next_call() {
            let answer = match serde_json::from_str::<guiraffe_test::transport::Call>(&call) {
                Ok(call) => server.call(call),
                Err(error) => guiraffe_test::transport::Answer {
                    ok: false,
                    error: Some(format!("the call does not parse: {error}")),
                    ..Default::default()
                },
            };
            let text = serde_json::to_string(&answer).unwrap_or_default();
            calls.answer(id, &text);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

pub fn run(path: PathBuf, screen: Option<String>) -> Result<()> {
    // Fail fast on an unreadable file before opening a window.
    let stamp = Stamp::of(&path);
    let doc = load(&path, screen.as_deref())?;
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(move |cx: &mut App| {
            guiraffe_inputs::init(cx);
            guiraffe_layout::init(cx);
            guiraffe_code::init(cx);
            guiraffe_markdown::init(cx);
            guiraffe_mermaid::init(cx);
            guiraffe_overlays::init(cx);
            guiraffe_tree::init(cx);
            let title = format!("GUIraffe preview — {}", path.display());
            let opened = cx.open_window(
                gpui_kit::WindowOptions {
                    titlebar: Some(gpui_kit::TitlebarOptions {
                        title: Some(title.into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| {
                        let mut p = Preview {
                            path,
                            screen,
                            ui: None,
                            error: None,
                            stamp,
                            reloads: 0,
                        };
                        p.apply(Ok(doc), cx);
                        p.watch(cx);
                        p
                    })
                },
            );
            if let Err(e) = opened {
                eprintln!("could not open window: {e}");
                cx.quit();
            }
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ir_and_mocket_and_reports_garbage() {
        let ir = r#"{"version":1,"root":{"kind":"text","props":{"text":"hi"}}}"#;
        assert!(parse(ir, None).is_ok());
        let mocket = r#"{"screens":[{"id":"s","name":"S","root":"a"}],"nodes":{"a":{"id":"a","type":"text","props":{"text":"x"}}}}"#;
        assert_eq!(
            parse(mocket, None).map(|d| d.root.kind).ok().as_deref(),
            Some("text")
        );
        assert!(parse("{", None).is_err());
        assert!(parse(r#"{"neither":1}"#, None).is_err());
    }

    #[test]
    fn validation_reports_unbound_handlers() {
        let doc = Document::new(
            guiraffe_ui::Node::new("button")
                .prop("label", "Save")
                .on("click", "save"),
        );
        let error = validate(&doc, false).expect_err("preview has no save handler");
        assert!(
            error
                .to_string()
                .contains("Bind this event to a named host handler.")
        );
    }

    #[test]
    fn imported_validation_uses_source_screen_and_node_ids() {
        let mocket = r#"{"screens":[{"id":"settings","root":"r"}],"nodes":{"r":{"type":"stack","id":"save-btn","children":["b"]},"b":{"type":"button","id":"save-btn"}}}"#;
        let error = parse(mocket, None).expect_err("duplicate Mocket id");
        assert!(
            error
                .to_string()
                .contains("/screens/settings/nodes/save-btn"),
            "{error}"
        );
    }

    #[test]
    fn stamps_change_when_the_file_changes() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let f = tmp.path().join("d.json");
        std::fs::write(&f, "a").unwrap_or_else(|e| panic!("{e}"));
        let a = Stamp::of(&f);
        std::fs::write(&f, "abc").unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(
            a,
            Stamp::of(&f),
            "length change detected even within mtime granularity"
        );
        assert_eq!(Stamp::of(&tmp.path().join("missing")), None);
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    use guiraffe_ui::{Node, Value};

    #[gpui_kit::test]
    fn reload_preserves_live_slots_and_last_good_document(cx: &mut gpui_kit::TestAppContext) {
        cx.update(guiraffe_components::init);
        let doc = Document::new(
            Node::new("text")
                .prop("text", "initial")
                .bind("text", "title"),
        );
        let preview = cx.new(|cx| {
            let mut p = Preview {
                path: PathBuf::new(),
                screen: None,
                ui: None,
                error: None,
                stamp: None,
                reloads: 0,
            };
            p.apply(Ok(doc.clone()), cx);
            p
        });
        preview.update(cx, |p, cx| {
            let ui = p.ui.clone().unwrap_or_else(|| panic!("missing host"));
            ui.read(cx).handle().patch(
                |names, out| {
                    if let Some(slot) = names.slot("title") {
                        out.set(slot, "live value");
                    }
                },
                cx,
            );
            p.apply(Err(anyhow::anyhow!("unfinished JSON")), cx);
            assert!(p.error.is_some());
            assert_eq!(p.ui.as_ref().map(|u| u.entity_id()), Some(ui.entity_id()));
            p.apply(Ok(doc), cx);
            assert!(p.error.is_none());
            assert_eq!(
                ui.read(cx).slot_value("title"),
                Some(Value::from("live value"))
            );
            assert_eq!(p.reloads, 2);
        });
    }
}
