use std::sync::Arc;

use gpui_kit::{
    Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};
use guiraffe_ui::*;

const DOC: &str = r#"{
  "version": 1,
  "root": { "kind": "stack", "children": [
    { "kind": "title", "props": { "text": "Inbox", "order": 2 } },
    { "kind": "text", "id": "status", "props": { "text": "loading…" }, "bind": { "text": "status" } },
    { "kind": "text", "id": "clock", "bind": { "text": "clock" } },
    { "kind": "text", "id": "live", "props": { "text": "LIVE", "color": "gray" }, "bind": { "color": "tone" } },
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
    fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
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

fn doc() -> Document {
    Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"))
}

fn compile(d: &Document) -> Template {
    Template::compile(d, &Registry::with_builtins()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn compile_resolves_names_and_reports_problems() {
    let t = compile(&doc());
    assert_eq!(t.names().slot("status"), Some(SlotId(0)));
    assert_eq!(t.names().event("refresh"), Some(EventId(0)));
    assert_eq!(
        t.slot_defaults()[0].as_str(),
        Some("loading…"),
        "literal seeds the bound slot"
    );
    let msgs: Vec<&str> = t.diagnostics().iter().map(|d| d.message.as_str()).collect();
    assert_eq!(msgs, vec!["unknown widget kind `fancy-chart`"]);
    assert!(t.find("refresh").is_some());
    assert!(Document::from_json(r#"{"version": 99, "root": {"kind":"stack"}}"#).is_err());
    assert!(Document::from_json("not json").is_err());
}

#[test]
fn typos_duplicates_and_objects_are_diagnosed() {
    let d = Document::new(
        Node::new("stack")
            .child(Node::new("button").prop("lable", "x").on("clik", "save"))
            .child(Node::new("text").prop("text", serde_json::json!({"nested": true})))
            .child(Node {
                id: Some("a".into()),
                ..Node::new("text")
            })
            .child(Node {
                id: Some("a".into()),
                ..Node::new("text")
            }),
    );
    let t = compile(&d);
    let msgs: Vec<String> = t.diagnostics().iter().map(|d| d.message.clone()).collect();
    assert!(
        msgs.iter().any(|m| m.contains("no prop `lable`")),
        "{msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("no event `clik`")),
        "{msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("objects are not supported")),
        "{msgs:?}"
    );
    let dup = t
        .diagnostics()
        .iter()
        .find(|d| d.message.contains("duplicate id"));
    assert_eq!(dup.map(|d| d.severity), Some(Severity::Error));
}

#[test]
fn depth_limit_is_the_same_for_json_and_builders() {
    let mut n = Node::new("stack");
    for _ in 0..MAX_DEPTH + 5 {
        n = Node::new("stack").child(n);
    }
    assert!(matches!(
        Template::compile(&Document::new(n.clone()), &Registry::with_builtins()),
        Err(CompileError::TooDeep { .. })
    ));
    let json = serde_json::to_string(&Document::new(n)).unwrap_or_default();
    let via_json = Document::from_json(&json)
        .map_err(|e| e.to_string())
        .and_then(|d| {
            Template::compile(&d, &Registry::with_builtins())
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
    assert!(
        via_json.is_err(),
        "too-deep JSON is rejected gracefully, not overflowed"
    );
}

#[test]
fn templates_compile_off_the_ui_thread() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Template>();
    let d = doc();
    let registry = Arc::new(Registry::with_builtins());
    let n = std::thread::spawn(move || Template::compile(&d, &registry).map(|t| t.len())).join();
    assert!(matches!(n, Ok(Ok(7))));
}

#[test]
fn ir_round_trips_through_json() {
    let d = doc();
    let json = serde_json::to_string(&d).unwrap_or_default();
    assert_eq!(Document::from_json(&json).ok(), Some(d));
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

fn mount<'a>(
    cx: &'a mut TestAppContext,
    d: &Document,
) -> (Entity<DataUi>, &'a mut VisualTestContext) {
    cx.update(guiraffe_components::init);
    let mut ui = None;
    let d = d.clone();
    let (_shell, vcx) = cx.add_window_view(|_, cx| {
        let u = DataUi::new(
            &d,
            Arc::new(Registry::with_builtins()),
            Inbox::default(),
            cx,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        ui = Some(u.clone());
        Shell { ui: u }
    });
    vcx.run_until_parked();
    (ui.unwrap_or_else(|| unreachable!()), vcx)
}

fn status(ui: &Entity<DataUi>, vcx: &mut VisualTestContext) -> String {
    vcx.update(|_, cx| {
        ui.read(cx)
            .slot_value("status")
            .map(|v| v.display().to_string())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn real_click_flows_through_logic_into_view_data(cx: &mut TestAppContext) {
    let (ui, vcx) = mount(cx, &doc());
    assert_eq!(
        status(&ui, vcx),
        "3 unread",
        "init overrides the design default"
    );
    let center = vcx
        .debug_bounds("refresh")
        .map(|b| b.center())
        .unwrap_or_else(|| panic!("button addressable by its document id"));
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(status(&ui, vcx), "refreshed 1×");
}

#[gpui_kit::test]
fn a_patch_re_renders_only_the_nodes_bound_to_that_slot(cx: &mut TestAppContext) {
    let (ui, vcx) = mount(cx, &doc());
    let t = vcx.update(|_, cx| ui.read(cx).template());
    let (root, status_node, live_node) = (
        t.root(),
        t.find("status").unwrap_or(0),
        t.find("live").unwrap_or(0),
    );
    let h = vcx.update(|_, cx| ui.read(cx).handle());
    let counts = |vcx: &mut VisualTestContext| {
        vcx.update(|_, cx| {
            let u = ui.read(cx);
            (
                u.node_renders(root),
                u.node_renders(status_node),
                u.node_renders(live_node),
            )
        })
    };
    // A layout-neutral change (colour) on one bound node.
    let before = counts(vcx);
    vcx.update(|_, cx| {
        h.patch(
            |n, out| out.set(n.slot("tone").unwrap_or(SlotId(0)), "red"),
            cx,
        )
    });
    vcx.run_until_parked();
    let after = counts(vcx);
    assert!(after.2 > before.2, "the bound node re-rendered");
    if gpui_kit::GPUI_FAST {
        assert_eq!(after.2, before.2 + 1, "exactly once");
        assert_eq!(after.1, before.1, "a sibling bound node did not");
        assert_eq!(after.0, before.0, "the document root did not");
    }
    // Upstream GPUI re-renders ancestors (and their child views) on a child notify (DESIGN §2c).
    // On either backend, a change that alters a node's *size* may re-layout its ancestors.
    vcx.update(|_, cx| {
        h.patch(
            |n, out| out.set(n.slot("tone").unwrap_or(SlotId(0)), "red"),
            cx,
        )
    });
    vcx.run_until_parked();
    assert_eq!(counts(vcx).2, after.2, "an unchanged value is a no-op");
}

#[gpui_kit::test]
fn stale_events_after_hot_reload_are_dropped(cx: &mut TestAppContext) {
    let (ui, vcx) = mount(cx, &doc());
    // Reload a document whose first event is now "delete": EventId(0) changes meaning.
    let v2 = Document::new(
        Node::new("stack")
            .child(Node {
                id: Some("danger".into()),
                ..Node::new("button")
                    .prop("label", "Delete")
                    .on("click", "delete")
            })
            .child(Node::new("text").bind("text", "status")),
    );
    vcx.update(|_, cx| ui.update(cx, |u, cx| u.set_document(&v2, cx)))
        .unwrap_or_else(|e| panic!("{e}"));
    // A handler captured before the reload fires afterwards (stamped with the old generation).
    vcx.update(|_, cx| {
        let h = ui.read(cx).handle();
        h.dispatch_stale_for_test(EventId(0), cx)
    });
    assert_eq!(vcx.update(|_, cx| ui.read(cx).stale_events()), 1);
    assert_eq!(
        vcx.update(|_, cx| ui.read(cx).events_handled()),
        0,
        "the new document's handler did not run"
    );
    assert_eq!(status(&ui, vcx), "3 unread");
}

#[gpui_kit::test]
fn hot_reload_keeps_live_values_and_lets_logic_fill_new_slots(cx: &mut TestAppContext) {
    struct Filler;
    impl Logic for Filler {
        fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
            if let Some(s) = names.slot("subtitle") {
                out.set(s, "filled on bind");
            }
        }
        fn on_event(&mut self, _: EventId, _: &Value, _: &mut PatchBuf) {}
    }
    cx.update(guiraffe_components::init);
    let ui = cx
        .update(|cx| {
            DataUi::new(
                &Document::new(Node::new("text").bind("text", "title")),
                Arc::new(Registry::with_builtins()),
                Filler,
                cx,
            )
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let h = cx.update(|cx| ui.read(cx).handle());
    cx.update(|cx| {
        h.patch(
            |n, out| out.set(n.slot("title").unwrap_or(SlotId(0)), "live"),
            cx,
        )
    });
    let v2 = Document::new(
        Node::new("stack")
            .child(Node::new("text").bind("text", "title"))
            .child(Node::new("text").bind("text", "subtitle")),
    );
    cx.update(|cx| ui.update(cx, |u, cx| u.set_document(&v2, cx)))
        .unwrap_or_else(|e| panic!("{e}"));
    let (title, sub) = cx.update(|cx| {
        (
            ui.read(cx).slot_value("title"),
            ui.read(cx).slot_value("subtitle"),
        )
    });
    assert_eq!(
        title.and_then(|v| v.as_str().map(str::to_owned)).as_deref(),
        Some("live")
    );
    assert_eq!(
        sub.and_then(|v| v.as_str().map(str::to_owned)).as_deref(),
        Some("filled on bind")
    );
}

#[gpui_kit::test]
fn crate_owned_password_meter_validates_compiles_and_renders(cx: &mut TestAppContext) {
    cx.update(guiraffe_inputs::init);
    let document = Document::new(
        Node::new("password-strength")
            .prop("password", "A7!example")
            .bind("password", "password"),
    );
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let issues = validation::Validation::default().check_with_registry(&document, &registry);
        assert!(issues.is_empty(), "{issues:?}");
        let template = Arc::new(Template::compile(&document, &registry).expect("compile meter"));
        assert!(
            template.diagnostics().is_empty(),
            "{:?}",
            template.diagnostics()
        );
        Shell {
            ui: DataUi::from_template(template, registry, NoLogic, cx),
        }
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_the_password_meter_constructor() {
    use guiraffe_inputs::PasswordStrength;
    let meter = ui! { PasswordStrength("A7!example") };
    let _ = meter.into_element();
}
