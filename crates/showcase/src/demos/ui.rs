//! guiraffe-ui: data-driven UI. A document (as Mocket would export it) is compiled once into
//! a template and hosted by `DataUi`; logic answers events with slot patches.

use std::sync::Arc;

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div,
};
use guiraffe_components::Text;
use guiraffe_core::ColorName;
use guiraffe_ui::{DataUi, Document, EventId, Logic, Names, PatchBuf, Registry, SlotId, Value, ui};

use super::{column, example};
use crate::catalog::Entry;

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "ui",
            name: "DataUi",
            summary: "A UI defined as data (JSON from Mocket), compiled once; bindings and events resolve by id, logic replies with minimal patches. Each node renders as its own BoundNode, so a patch re-renders only what it touches.",
            covers: &["DataUi", "BoundNode"],
            demo: data_ui,
        },
        Entry {
            krate: "ui",
            name: "DataUi widget reference",
            summary: "Document widgets and their properties.",
            covers: &[],
            demo: |_, cx| crate::catalog::stateless(widget_reference, cx),
        },
        Entry {
            krate: "ui",
            name: "ui! macro",
            summary: "Declarative Rust syntax for GPUI elements and builder props.",
            covers: &[],
            demo: |_, cx| crate::catalog::stateless(macro_demo, cx),
        },
    ]
}

fn widget_reference(_: &mut Window, _: &mut App) -> gpui_kit::AnyElement {
    ui! {
        div {
            flex flex_col gap: gpui_kit::px(12.)
            for widget in guiraffe_ui::catalog::WIDGETS {
                {
                    let props = widget.props.iter().map(|prop| prop.name).collect::<Vec<_>>();
                    ui! {
                        div {
                            flex flex_col
                            guiraffe_components::Title(widget.id) { order: 4 }
                            Text(widget.summary)
                            if !props.is_empty() { Text(props.join(", ")) { dimmed: true } }
                        }
                    }
                }
            }
        }
    }
    .into_any_element()
}

fn macro_demo(_: &mut Window, _: &mut App) -> gpui_kit::AnyElement {
    let source = r#"ui! {
    div {
        flex flex_col gap_2
        Title("Build status") { order: 3 }
        Text("All checks passed") { color: ColorName::Teal }
    }
}"#;
    ui! {
        div {
            flex gap_6
            div { font_family: "monospace" {source} }
            div {
                flex flex_col gap_2
                guiraffe_components::Title("Build status") { order: 3 }
                Text("All checks passed") { color: ColorName::Teal }
            }
        }
    }
    .into_any_element()
}

const DOC: &str = r#"{
  "version": 1,
  "root": { "kind": "card", "children": [
    { "kind": "stack", "children": [
      { "kind": "group", "children": [
        { "kind": "title", "props": { "text": "Storage quota", "order": 3 } },
        { "kind": "spacer" },
        { "kind": "hug", "children": [
          { "kind": "pill", "id": "plan", "props": { "label": "Pro plan" }, "bind": { "label": "plan" } }
        ] }
      ] },
      { "kind": "group", "children": [
        { "kind": "ring", "id": "usage", "props": { "value": 0, "color": "blue", "diameter": 96, "label": "" },
          "bind": { "value": "percent", "label": "percent_label", "color": "tone" } },
        { "kind": "stack", "children": [
          { "kind": "text", "id": "used", "bind": { "text": "used" } },
          { "kind": "text", "props": { "text": "Uploads pause at 100%. Old snapshots can be pruned safely.", "dimmed": true, "size": "sm" } },
          { "kind": "group", "children": [
            { "kind": "button", "id": "upload", "props": { "label": "Upload 5 GB" }, "on": { "click": "upload" } },
            { "kind": "button", "id": "prune", "props": { "label": "Prune snapshots" }, "on": { "click": "prune" } }
          ] }
        ] }
      ] }
    ] }
  ] }
}"#;

/// The document's logic: plain Rust, no GPUI, talking to the view only through slot patches.
#[derive(Default)]
struct Quota {
    used_gb: f64,
    percent: Option<SlotId>,
    label: Option<SlotId>,
    tone: Option<SlotId>,
    used: Option<SlotId>,
    plan: Option<SlotId>,
    upload: Option<EventId>,
    prune: Option<EventId>,
}

const CAPACITY_GB: f64 = 50.0;

impl Quota {
    fn publish(&self, out: &mut PatchBuf) {
        let pct = (self.used_gb / CAPACITY_GB * 100.0).clamp(0.0, 100.0);
        let tone = match pct {
            p if p >= 90.0 => "red",
            p if p >= 70.0 => "orange",
            _ => "blue",
        };
        let mut set = |slot: Option<SlotId>, v: Value| {
            if let Some(s) = slot {
                out.set(s, v);
            }
        };
        set(self.percent, pct.into());
        set(self.label, format!("{pct:.0}%").into());
        set(self.tone, tone.into());
        set(
            self.used,
            format!("{:.1} GB of {CAPACITY_GB:.0} GB used", self.used_gb).into(),
        );
        set(
            self.plan,
            if pct >= 90.0 {
                "Upgrade soon"
            } else {
                "Pro plan"
            }
            .into(),
        );
    }
}

impl Logic for Quota {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        self.percent = names.slot("percent");
        self.label = names.slot("percent_label");
        self.tone = names.slot("tone");
        self.used = names.slot("used");
        self.plan = names.slot("plan");
        self.upload = names.event("upload");
        self.prune = names.event("prune");
        self.publish(out);
    }
    fn init(&mut self, out: &mut PatchBuf) {
        self.used_gb = 31.4;
        self.publish(out);
    }
    fn on_event(&mut self, event: EventId, _: &Value, out: &mut PatchBuf) {
        if Some(event) == self.upload {
            self.used_gb = (self.used_gb + 5.0).min(CAPACITY_GB);
        } else if Some(event) == self.prune {
            self.used_gb = (self.used_gb - 12.0).max(4.0);
        }
        self.publish(out);
    }
}

struct DataUiDemo {
    ui: Option<Entity<DataUi>>,
    error: Option<SharedString>,
}

fn data_ui(_: &mut Window, cx: &mut App) -> AnyView {
    let built = Document::from_json(DOC).and_then(|doc| {
        DataUi::new(
            &doc,
            Arc::new(Registry::with_builtins()),
            Quota::default(),
            cx,
        )
        .map_err(|e| e.to_string())
    });
    let demo = match built {
        Ok(ui) => DataUiDemo {
            ui: Some(ui),
            error: None,
        },
        Err(e) => DataUiDemo {
            ui: None,
            error: Some(e.into()),
        },
    };
    cx.new(|_| demo).into()
}

impl Render for DataUiDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut body = div().max_w(gpui_kit::px(520.));
        if let Some(ui) = &self.ui {
            body = body.child(ui.clone());
        }
        if let Some(e) = &self.error {
            body = body.child(Text::new(e.clone()).color(ColorName::Red));
        }
        column([example(
            "Card compiled from a JSON document; buttons drive Rust logic",
            body,
        )])
    }
}
