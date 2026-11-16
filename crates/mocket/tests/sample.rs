//! End to end: Mocket's own sample project → GUIraffe document → compiled → rendered.

use std::sync::Arc;

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
};
use gpui_kit::{px, size};
use guiraffe_mocket::{Coverage, import, screens};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use guiraffe_ui::{DataUi, NoLogic, Registry, Severity, Template};

/// An app body around the imported screen (as a real shell would provide).
struct Body(Entity<DataUi>);

impl Render for Body {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = guiraffe_core::GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .p(px(24.))
            .child(self.0.clone())
    }
}

const SAMPLE: &str = include_str!("fixtures/sample.mocket.json");

#[test]
fn every_sample_screen_imports_and_compiles_without_errors() {
    let list = screens(SAMPLE).unwrap_or_else(|e| panic!("{e}"));
    assert!(list.len() >= 2, "{list:?}");
    for (id, name) in &list {
        let imp = import(SAMPLE, Some(id)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let t = Template::compile(&imp.document, &Registry::with_builtins())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let errors: Vec<_> = t
            .diagnostics()
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{name}: {errors:?}");
        let (m, p, u) = (
            imp.count(Coverage::Mapped),
            imp.count(Coverage::Partial),
            imp.count(Coverage::Unsupported),
        );
        eprintln!("{name}: {m} mapped, {p} partial, {u} unsupported");
        assert!(m > u, "{name}: most of the screen maps");
        // Designer text is the default, and every text is addressable by its Mocket id.
        assert!(
            t.names().slot_names().iter().any(|s| s.ends_with(".text")),
            "{name}: text slots by id"
        );
    }
}

#[test]
fn imported_screen_renders() {
    let (id, name) = screens(SAMPLE)
        .ok()
        .and_then(|l| l.last().cloned())
        .unwrap_or_default();
    let imp = import(SAMPLE, Some(&id)).unwrap_or_else(|e| panic!("{e}"));
    let doc = imp.document.clone();
    let mut snap = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .with_setup(guiraffe_components::init);
    let image = snap
        .render(size(px(1100.), px(820.)), move |_, cx| {
            let ui = DataUi::new(&doc, Arc::new(Registry::with_builtins()), NoLogic, cx)
                .unwrap_or_else(|e| panic!("{e}"));
            cx.new(|_| Body(ui))
        })
        .unwrap_or_else(|e| panic!("{e:#}"));
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    assert_snapshot(
        &format!("mocket-{}", name.to_lowercase().replace(' ', "-")),
        &image,
        &cfg,
    )
    .unwrap_or_else(|e| panic!("{e:#}"));
}
