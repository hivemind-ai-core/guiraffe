use std::sync::Arc;

use gpui_kit::{
    Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div, px,
};
use guiraffe_ui::*;

const DOC: &str = r#"{
  "version": 1,
  "root": { "kind": "stack", "children": [
    { "kind": "title", "props": { "text": "Inbox", "order": 2 } },
    { "kind": "text", "id": "status", "bind": { "text": "status" } },
    { "kind": "button", "id": "refresh", "props": { "label": "Refresh" }, "on": { "click": "refresh" } },
    { "kind": "fancy-chart" }
  ] }
}"#;

#[derive(Default)]
struct Inbox {
    status: Option<SlotId>,
    refresh: Option<EventId>,
    refreshed: u32,
}

impl Logic for Inbox {
    fn bind(&mut self, names: &Names) {
        self.status = names.slot("status");
        self.refresh = names.event("refresh");
    }
    fn init(&mut self, out: &mut PatchBuf) {
        if let Some(s) = self.status {
            out.set(s, "3 unread");
        }
    }
    fn on_event(&mut self, event: EventId, _: &Value, out: &mut PatchBuf) {
        if Some(event) == self.refresh {
            self.refreshed += 1;
            if let Some(s) = self.status {
                out.set(s, format!("refreshed {}×", self.refreshed));
            }
        }
    }
}

#[test]
fn compile_resolves_names_and_reports_unknown_kinds() {
    let doc = Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"));
    let t = Template::compile(&doc, &Registry::with_builtins()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.names.slot("status"), Some(SlotId(0)));
    assert_eq!(t.names.event("refresh"), Some(EventId(0)));
    assert_eq!(
        t.diagnostics,
        vec![Diagnostic {
            path: "/root/children/3".into(),
            message: "unknown widget kind `fancy-chart`".into()
        }]
    );
    assert!(t.find("refresh").is_some());
    assert!(Document::from_json(r#"{"version": 99, "root": {"kind":"stack"}}"#).is_err());
    assert!(Document::from_json("not json").is_err());
}

#[test]
fn absurd_nesting_is_rejected_not_overflowed() {
    let mut n = Node::new("stack");
    for _ in 0..400 {
        n = Node::new("stack").child(n);
    }
    let r = Template::compile(&Document::new(n), &Registry::with_builtins());
    assert!(matches!(r, Err(CompileError::TooDeep { .. })));
}

struct Shell {
    ui: Entity<DataUi>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        // A dynamic document embedded inside a static layout.
        div()
            .flex()
            .child(div().w(px(120.)).child("static sidebar"))
            .child(self.ui.clone())
    }
}

#[gpui_kit::test]
fn click_flows_through_logic_into_patched_view_data(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let doc = Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"));
    let mut ui = None;
    let (_shell, vcx) = cx.add_window_view(|_, cx| {
        let u = DataUi::new(
            &doc,
            Arc::new(Registry::with_builtins()),
            Inbox::default(),
            cx,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        ui = Some(u.clone());
        Shell { ui: u }
    });
    let ui = ui.unwrap_or_else(|| unreachable!());
    vcx.run_until_parked();
    let status = |vcx: &mut gpui_kit::VisualTestContext| {
        vcx.update(|_, cx| {
            let ui = ui.read(cx);
            let slot = ui
                .template()
                .names
                .slot("status")
                .unwrap_or(SlotId(u32::MAX));
            ui.slots().get(slot).display().to_string()
        })
    };
    assert_eq!(status(vcx), "3 unread", "init populated view data");
    let center = vcx
        .debug_bounds("refresh")
        .map(|b| b.center())
        .unwrap_or_else(|| panic!("button addressable by its document id"));
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(
        status(vcx),
        "refreshed 1×",
        "real click → event id → logic → slot patch"
    );
}

#[gpui_kit::test]
fn unchanged_patches_do_not_notify_and_hot_reload_keeps_slots(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let doc = Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"));
    let ui = cx
        .update(|cx| {
            DataUi::new(
                &doc,
                Arc::new(Registry::with_builtins()),
                Inbox::default(),
                cx,
            )
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let n = notified.clone();
    let _sub = cx.update(|cx| cx.observe(&ui, move |_, _| n.set(n.get() + 1)));
    cx.update(|cx| {
        ui.update(cx, |u, cx| {
            u.patch(
                |names, out| out.set(names.slot("status").unwrap_or(SlotId(0)), "3 unread"),
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert_eq!(notified.get(), 0, "same value: no re-render");
    // Hot reload: a new document that still binds `status` keeps its value.
    let v2 = Document::new(
        Node::new("group")
            .child(Node::new("pill").bind("label", "status"))
            .child(Node::new("text").bind("text", "new_slot")),
    );
    cx.update(|cx| ui.update(cx, |u, cx| u.set_document(&v2, cx)))
        .unwrap_or_else(|e| panic!("{e}"));
    let kept = cx.update(|cx| {
        let u = ui.read(cx);
        u.slots()
            .get(u.template().names.slot("status").unwrap_or(SlotId(9)))
            .display()
            .to_string()
    });
    assert_eq!(kept, "3 unread");
}

#[test]
fn ir_round_trips_through_json() {
    let doc = Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"));
    let json = serde_json::to_string(&doc).unwrap_or_default();
    assert_eq!(Document::from_json(&json).ok(), Some(doc));
}

#[test]
fn templates_compile_off_the_ui_thread() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Template>();
    let doc = Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"));
    let registry = Arc::new(Registry::with_builtins());
    let t = std::thread::spawn(move || Template::compile(&doc, &registry).map(|t| t.nodes.len())).join();
    assert!(matches!(t, Ok(Ok(5))));
}
