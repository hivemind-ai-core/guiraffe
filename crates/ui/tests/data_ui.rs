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
    // Upstream GPUI re-renders ancestors (and their child views) on a child notify.
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
fn crate_owned_widgets_validate_compile_and_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_inputs::init);
    cx.update(guiraffe_layout::init);
    let document = Document::new(
        Node::new("center").prop("min-height", 100).child(
            Node::new("aspect-ratio")
                .prop("ratio", 2)
                .child(
                    Node::new("space")
                        .prop("width", "md")
                        .prop("height", "sm")
                        .prop("width-px", 32)
                        .prop("height-px", 24),
                )
                .child(
                    Node::new("password-strength")
                        .prop("password", "A7!example")
                        .bind("password", "password"),
                ),
        ),
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

#[test]
fn ui_macro_accepts_the_layout_builders() {
    use guiraffe_layout::{AspectRatio, Center, Size, Space};
    let layout = ui! {
        Center { min_height: px(100.)
            AspectRatio(1.0) { ratio: 2.0
                Space { width: Size::Md; height: Size::Sm; width_px: 32.0; height_px: 24.0 }
            }
        }
    };
    let _ = layout.into_element();
}

#[gpui_kit::test]
fn extra_component_properties_validate_and_render(cx: &mut TestAppContext) {
    let doc = Document::new(
        Node::new("card")
            .prop("shadow", "md")
            .prop("radius", "lg")
            .prop("padding", "sm")
            .prop("with-border", false)
            .child(
                Node::new("text")
                    .prop("text", "Example")
                    .prop("truncate", true)
                    .prop("font-weight", 700),
            )
            .child(
                Node::new("pill")
                    .prop("label", "Example")
                    .prop("size", "xl")
                    .on("remove", "remove-tag"),
            )
            .child(Node::new("ring").prop("thickness", 8).prop(
                "sections",
                serde_json::json!([
                    {"value": 25, "color": "blue"},
                    {"value": 50, "color": "teal"}
                ]),
            )),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    assert!(compile(&doc).diagnostics().is_empty());
    let _ = mount(cx, &doc);
}

#[gpui_kit::test]
fn chip_click_updates_a_bound_selection_and_delivers_an_event(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    #[derive(Default)]
    struct ChipLogic(
        Option<EventId>,
        std::rc::Rc<std::cell::RefCell<Option<Value>>>,
    );
    impl Logic for ChipLogic {
        fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
            self.0 = names.event("chips-changed");
        }
        fn on_event(&mut self, event: EventId, payload: &Value, _: &mut PatchBuf) {
            if self.0 == Some(event) {
                self.1.replace(Some(payload.clone()));
            }
        }
    }

    cx.update(guiraffe_inputs::init);
    let doc = Document::new(
        Node::new("chip-group")
            .prop("options", serde_json::json!(["Rust", "Zig"]))
            .prop("value", serde_json::json!([0]))
            .prop("multiple", true)
            .prop("color", "teal")
            .prop("disabled", false)
            .prop("focusable", true)
            .bind("value", "selection")
            .on("change", "chips-changed"),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let payload = std::rc::Rc::default();
    let mut host = None;
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let ui = DataUi::new(
            &doc,
            registry,
            ChipLogic(None, std::rc::Rc::clone(&payload)),
            cx,
        )
        .expect("compile chip group");
        host = Some(ui.clone());
        Shell { ui }
    });
    window.run_until_parked();
    window.update(|window, cx| window.click(("chip", 1usize), cx));
    window.run_until_parked();
    let host = host.expect("host mounted");
    let expected = Value::List(vec![Value::Num(0.0), Value::Num(1.0)].into());
    window.update(|_, cx| {
        assert_eq!(
            host.read(cx).slot_value("selection"),
            Some(expected.clone())
        )
    });
    assert_eq!(*payload.borrow(), Some(expected));
}

fn init_document_widgets(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    cx.update(|cx| {
        let _ = guiraffe_commands::init(cx);
    });
    cx.update(guiraffe_inputs::init);
    cx.update(guiraffe_layout::init);
    cx.update(guiraffe_code::init);
    cx.update(guiraffe_charts::init);
    cx.update(guiraffe_chat::init);
    cx.update(guiraffe_datagrid::init);
    cx.update(guiraffe_dnd::init);
    cx.update(guiraffe_flamegraph::init);
    cx.update(guiraffe_flow::init);
    cx.update(guiraffe_markdown::init);
    cx.update(guiraffe_mermaid::init);
    cx.update(guiraffe_overlays::init);
    cx.update(guiraffe_sheet::init);
    cx.update(guiraffe_tree::init);
    cx.update(guiraffe_timeline::init);
    cx.update(guiraffe_calendar::init);
}

#[gpui_kit::test]
fn installed_widget_metadata_matches_compiler_schemas(cx: &mut TestAppContext) {
    init_document_widgets(cx);
    cx.update(|cx| {
        let registry = Registry::for_app(cx);
        assert_eq!(registry.kinds().count(), catalog::all_widgets().count());
        for spec in catalog::all_widgets() {
            let registered = registry.get(spec.id).expect(spec.id);
            let props: Vec<_> = spec.props.iter().map(|p| p.name).collect();
            let events: Vec<_> = spec.events.iter().map(|e| e.id).collect();
            assert_eq!(registered.schema().props, props, "{}", spec.id);
            assert_eq!(registered.schema().events, events, "{}", spec.id);
        }
    });
}

#[gpui_kit::test]
fn every_catalogue_example_validates_compiles_and_renders(cx: &mut TestAppContext) {
    init_document_widgets(cx);
    let registry = cx.update(|cx| Arc::new(Registry::for_app(cx)));
    for spec in catalog::all_widgets() {
        let document = serde_json::from_value(spec.example()).expect(spec.id);
        let issues = validation::Validation::default().check(&document);
        assert!(issues.is_empty(), "{}: {issues:?}", spec.id);
        let template = Arc::new(Template::compile(&document, &registry).expect(spec.id));
        assert!(
            template.diagnostics().is_empty(),
            "{}: {:?}",
            spec.id,
            template.diagnostics()
        );
        let registry = registry.clone();
        let (_, window) = cx.add_window_view(|_, cx| Shell {
            ui: DataUi::from_template(template, registry, NoLogic, cx),
        });
        window.run_until_parked();
    }
}

#[gpui_kit::test]
fn overlay_document_validates_compiles_and_renders(cx: &mut TestAppContext) {
    cx.update(guiraffe_overlays::init);
    let doc = Document::new(
        Node::new("affix")
            .prop("position", "top-left")
            .prop("offset", "sm")
            .child(Node::new("text").prop("text", "Overlay content"))
            .child(
                Node::new("floating-tooltip")
                    .prop("text", "Details")
                    .prop("x", 100)
                    .prop("y", 80)
                    .prop("width", 40)
                    .prop("height", 20)
                    .prop("side", "right")
                    .prop("align", "start")
                    .prop("gap", 6)
                    .prop("padding", 10)
                    .prop("arrow-padding", 5)
                    .prop("flip", true)
                    .prop("shift", false),
            )
            .child(
                Node::new("loading-overlay")
                    .prop("visible", false)
                    .prop("size", "lg")
                    .prop("color", "orange")
                    .prop("variant", "outline"),
            ),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("overlay document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_overlay_builders() {
    use guiraffe_core::{ColorName, Size, Variant};
    use guiraffe_overlays::{
        Affix, FloatingTooltip, LoadingOverlay,
        position::{FloatingOptions, Rect, Side},
        toast::ToastPosition,
    };
    let tooltip_options = FloatingOptions {
        side: Side::Right,
        ..Default::default()
    };
    let overlay = ui! {
        Affix { position: ToastPosition::TopRight; offset: Size::Sm
            LoadingOverlay { visible: false; size: Size::Lg; color: ColorName::Orange; variant: Variant::Outline }
            FloatingTooltip("help", "Details", Rect::default()) { options: tooltip_options }
        }
    };
    let _ = overlay.into_element();
}

#[gpui_kit::test]
fn markdown_document_drives_source_and_view_options(cx: &mut TestAppContext) {
    cx.update(guiraffe_markdown::init);
    let doc = Document::new(
        Node::new("markdown")
            .prop("source", "# Title\n\n- [ ] Task")
            .bind("source", "body")
            .prop("selectable", false)
            .prop("scrollable", true)
            .prop("min-height", 120)
            .prop("max-height", 240)
            .prop("code-copy", false)
            .on("link-click", "navigate")
            .on("task-toggle", "toggle"),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("markdown document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_markdown_builders() {
    use guiraffe_markdown::MarkdownView;
    let view = ui! {
        MarkdownView("example") {
            source: "# Title"; selectable: false; scrollable: true;
            min_height: px(120.0); max_height: px(240.0); code_copy: false;
            .on_link_click(|_, _, _| {})
            .on_task_toggle(|_, _, _, _| {})
        }
    };
    let _ = view.into_element();
}

#[gpui_kit::test]
fn mermaid_document_validates_compiles_and_renders(cx: &mut TestAppContext) {
    cx.update(guiraffe_mermaid::init);
    let doc = Document::new(
        Node::new("mermaid")
            .prop("source", "flowchart LR\n  A --> B")
            .prop("message", "Ready")
            .bind("source", "diagram"),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("mermaid document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_mermaid_builders() {
    use guiraffe_mermaid::MermaidView;
    let view = ui! { MermaidView("example") { source: "flowchart LR\n  A --> B"; message: Some("Ready".into()) } };
    let _ = view.into_element();
}

#[gpui_kit::test]
fn code_document_drives_view_options_and_annotations(cx: &mut TestAppContext) {
    cx.update(guiraffe_code::init);
    let doc = Document::new(
        Node::new("code-viewer")
            .prop("source", "let x = {\n    42\n};")
            .bind("source", "code")
            .prop("language", "rust")
            .prop("size", "sm")
            .prop("line-numbers", false)
            .prop("minimap", true)
            .prop("margin-chars", 20)
            .prop("decoration-layers", serde_json::json!([{
                "id": 3, "zIndex": -2, "mergeIdentical": true,
                "decorations": [
                    {"id": 4, "line": 0, "start": 4, "end": 5,
                     "startBias": "left", "style": {"kind": "diagnostic", "severity": "warning",
                                 "pattern": "dotted", "message": "Check this binding"}}
                ]
            }]))
            .on("decoration", "inspect-binding")
            .prop("folds", serde_json::json!([{"start":0,"end":3,"collapsed":true}]))
            .prop("annotations", serde_json::json!([
                {"id":1,"line":0,"start":4,"end":5,"kind":"underline","payload":"A binding","color":"orange"}
            ])),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx).expect("code document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_a_code_viewer_handle() {
    use gpui_kit::App;
    use guiraffe_code::{CodeDocument, CodeHandle, CodeViewer};

    fn example(cx: &mut App) -> impl IntoElement {
        let handle = CodeHandle::new(1, Arc::new(CodeDocument::new("let x = 42;")), cx);
        ui! { CodeViewer(&handle) }
    }
    let _ = example;
}

#[test]
fn document_validation_rejects_a_fold_without_hidden_lines() {
    let doc = Document::new(
        Node::new("code-viewer")
            .prop("source", "first\nsecond")
            .prop("folds", serde_json::json!([{"start":0,"end":1}])),
    );
    let issues = validation::Validation::default().check(&doc);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].property, "folds");
}

#[gpui_kit::test]
fn tree_documents_validate_and_render_property_grid_and_tree_list(cx: &mut TestAppContext) {
    cx.update(guiraffe_tree::init);
    let nodes = serde_json::json!([
        {"id":11,"label":"Group","height":40},
        {"id":12,"depth":1,"label":"Width","checked":true,"badge":"Px","icon":"#",
         "property":{"type":"number","value":24,"default":16,"min":1,"max":100,"step":1,"units":"px",
                     "resetButton":true}}
    ]);
    let doc = Document::new(
        Node::new("group")
            .child(
                Node::new("tree-list")
                    .prop("nodes", nodes.clone())
                    .prop("expanded", serde_json::json!([11])),
            )
            .child(
                Node::new("property-grid")
                    .prop("nodes", nodes)
                    .prop("expanded", serde_json::json!([11]))
                    .prop("focusable", true)
                    .prop("density", "compact")
                    .prop("alternating", true)
                    .prop("grid-lines", true)
                    .prop("indent-guides", true)
                    .prop("group-headers", true)
                    .prop("sticky-headers", true)
                    .prop("searchable", true)
                    .prop("split-ratio", 0.55)
                    .prop("persist-key", "properties")
                    .prop("focus-row", false)
                    .prop("reset-button", false)
                    .prop("edit-buttons", "labels")
                    .on("edit", "property-edited"),
            ),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx).expect("tree document"),
    });
    window.run_until_parked();
}

#[gpui_kit::test]
fn range_tags_and_dropzone_documents_validate_and_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_inputs::init);
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("range-slider")
                    .prop("min", 1)
                    .prop("max", 20)
                    .prop("step", 0.5)
                    .prop("min-distance", 2)
                    .prop("value", serde_json::json!([4, 12]))
                    .bind("value", "range")
                    .prop("label", "Range")
                    .prop("disabled", false)
                    .prop("focusable", true),
            )
            .child(
                Node::new("tags-input")
                    .prop("value", serde_json::json!(["Rust"]))
                    .bind("value", "tags")
                    .prop("placeholder", "Add tag")
                    .prop("allow-duplicates", false)
                    .prop("split-chars", ",;")
                    .prop("max-tags", 3)
                    .prop("max-len", 10)
                    .on("reject", "invalid-tag"),
            )
            .child(
                Node::new("dropzone")
                    .prop("accept", serde_json::json!([".png", "text/*"]))
                    .prop("label", "Drop files here")
                    .prop("disabled", false)
                    .prop("focusable", true)
                    .on("accept", "accept")
                    .on("reject", "reject")
                    .on("browse", "browse"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("input document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_can_embed_stateful_input_builders() {
    use gpui_kit::App;
    use guiraffe_inputs::{ChipGroup, Dropzone, Range2, RangeSlider, TagRules, TagsInput};

    fn example(window: &mut Window, cx: &mut App) -> impl IntoElement {
        ui! {
            div {
                {ChipGroup::new(["Rust", "Zig"], true, cx)}
                {RangeSlider::new(Range2::new(0.0, 100.0, 1.0, 20.0, 40.0), cx)}
                {TagsInput::new(TagRules::default(), "Add tags", window, cx)}
                {Dropzone::new(vec![".png"], "Drop images", cx)}
            }
        }
    }
    let _ = example;
}

#[gpui_kit::test]
fn button_document_validates_and_renders_builder_options(cx: &mut TestAppContext) {
    let doc = Document::new(
        Node::new("button")
            .prop("label", "Open")
            .prop("accessibility-label", "Open item")
            .prop("accessibility-id", "open-item")
            .prop("size", "lg")
            .prop("variant", "primary")
            .prop("rounded", "lg")
            .prop("rounding-px", 12)
            .prop("disabled", false)
            .prop("loading", false)
            .prop("selected", true)
            .prop("open", false)
            .prop("outline", true)
            .prop("compact", true)
            .prop("toggled", true)
            .prop("dropdown-caret", true)
            .prop("focus-ring", true)
            .prop("tab-stop", true)
            .prop("tab-index", 1)
            .prop("tooltip", "Open item")
            .prop("tooltip-placement", "bottom")
            .prop("tooltip-show-delay-ms", 250)
            .on("hover", "hovered")
            .child(Node::new("text").prop("text", "Item")),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    assert!(compile(&doc).diagnostics().is_empty());
    let _ = mount(cx, &doc);
}

#[gpui_kit::test]
fn expanded_layout_documents_validate_and_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let doc = Document::new(
        Node::new("container")
            .prop("size", "lg")
            .prop("fluid", false)
            .prop("padding", "sm")
            .prop("padding-px", 8)
            .child(
                Node::new("scroll-area")
                    .prop("scroll-x", true)
                    .prop("scroll-y", true)
                    .child(
                        Node::new("flex")
                            .prop("direction", "column")
                            .prop("gap", "sm")
                            .prop("gap-px", 10)
                            .prop("align", "center")
                            .prop("justify", "between")
                            .prop("grow", false)
                            .prop("wrap", true)
                            .child(
                                Node::new("divider")
                                    .prop("orientation", "horizontal")
                                    .prop("label", "Section")
                                    .prop("label-position", "start")
                                    .prop("color", "teal")
                                    .prop("thickness", 2),
                            )
                            .child(
                                Node::new("simple-grid")
                                    .prop("cols", 3)
                                    .prop("spacing", 12)
                                    .prop("vertical-spacing", 8)
                                    .prop("min-child-width", 120)
                                    .child(Node::new("text").prop("text", "Cell")),
                            ),
                    ),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("layout document"),
    });
    window.run_until_parked();
}

#[gpui_kit::test]
fn stack_and_group_documents_drive_native_layout_builders(cx: &mut TestAppContext) {
    let doc = Document::new(
        Node::new("stack")
            .prop("gap", "xs")
            .prop("gap-px", 8)
            .prop("align", "center")
            .prop("justify", "between")
            .prop("grow", true)
            .child(
                Node::new("group")
                    .prop("gap", "md")
                    .prop("gap-px", 12)
                    .prop("align", "baseline")
                    .prop("justify", "evenly")
                    .prop("grow", true)
                    .prop("wrap", false)
                    .child(Node::new("text").prop("text", "Item")),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    assert!(compile(&doc).diagnostics().is_empty());
    let _ = mount(cx, &doc);
}

#[test]
fn ui_macro_accepts_native_stack_and_group_builders() {
    use guiraffe_core::Size;
    use guiraffe_layout::{Align, Group, Justify, Stack};

    let _ = ui! {
        Stack { gap: Size::Xs; gap_px: 8.; align: Align::Center; justify: Justify::Between; grow: true;
            Group { gap: Size::Md; gap_px: 12.; align: Align::Baseline; justify: Justify::Evenly; grow: true; wrap: false;
                "Item"
            }
        }
    };
}

#[gpui_kit::test]
fn datagrid_document_validates_and_renders_rows_and_columns(cx: &mut TestAppContext) {
    cx.update(guiraffe_datagrid::init);
    let doc = Document::new(
        Node::new("data-grid")
            .prop("rows", serde_json::json!([["Alice", true], ["Bob", false]]))
            .bind("rows", "grid-rows")
            .prop("columns", serde_json::json!([
                {"id":"name","title":"Name","width":110,"minWidth":40,"maxWidth":200,"pin":"left","editable":true},
                {"id":"ready","title":"Ready","source":1,"align":"center","sortable":false,"resizable":false,"visible":true}
            ]))
            .prop("selection", "range")
            .prop("zebra", false)
            .prop("size", "sm")
            .prop("density", "compact")
            .prop("focusable", true)
            .prop("overscan", 2)
            .prop("sync-below", 100)
            .prop("label", "Status")
            .on("cell-edit", "edited"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx).expect("grid document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_a_grid_handle() {
    use gpui_kit::App;
    use guiraffe_datagrid::{DataGrid, GridHandle, VecSource};

    fn example(cx: &mut App) -> impl IntoElement {
        let handle = GridHandle::new(VecSource::<()>::new(Vec::new()), Vec::new(), cx);
        ui! { DataGrid(&handle) }
    }
    let _ = example;
}

#[gpui_kit::test]
fn flame_graph_document_validates_and_renders_profile(cx: &mut TestAppContext) {
    cx.update(guiraffe_flamegraph::init);
    let doc = Document::new(
        Node::new("flame-graph")
            .prop(
                "frames",
                serde_json::json!([
                    {"id":1,"label":"main","package":"app","self":2,"total":10},
                    {"id":2,"parent":1,"label":"render","package":"ui","self":8,"total":8}
                ]),
            )
            .bind("frames", "profile")
            .prop("orientation", "icicle")
            .prop("colour-scheme", "heat")
            .prop("search", "render")
            .on("frame-click", "frame-selected"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("flame document"),
    });
    window.run_until_parked();
}

#[gpui_kit::test]
fn clip_timeline_document_validates_and_drives_tracks(cx: &mut TestAppContext) {
    cx.update(guiraffe_timeline::init);
    let doc = Document::new(
        Node::new("tracks-view")
            .prop("tracks", serde_json::json!([
                {"name":"Video","clips":[{"id":12,"start":2,"length":3,"sourceOffset":1,"locked":true}]},
                {"name":"Audio","clips":[{"id":5,"start":0,"length":2}]}
            ]))
            .bind("tracks", "tracks")
            .prop("time-format", "frames")
            .prop("frames-per-second", 24)
            .prop("length", 12)
            .prop("playhead", 3)
            .prop("snap", 0.25)
            .prop("resize-edges", "end")
            .prop("editable", true)
            .prop("focusable", true)
            .prop("lock-style", "dimmed")
            .prop("global-lock-style", "hatched")
            .prop("selected", serde_json::json!([0, 12]))
            .on("seek", "seek")
            .on("clip-change", "clip-change"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("track document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_can_embed_the_timeline_entity() {
    use gpui_kit::App;
    use guiraffe_timeline::{TimeFormat, TracksView};
    fn example(cx: &mut App) -> impl IntoElement {
        ui! { div { {TracksView::new(vec![], TimeFormat::Seconds, 10.0, cx)} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn sortable_board_document_validates_and_renders(cx: &mut TestAppContext) {
    cx.update(guiraffe_dnd::init);
    let doc = Document::new(
        Node::new("sortable-board")
            .prop("columns", serde_json::json!([
                {"title":"To do","cards":[{"id":1,"label":"Plan"},{"id":2,"label":"Implement"}]},
                {"title":"Done","cards":[{"id":3,"label":"Research"}]}
            ]))
            .bind("columns", "columns")
            .prop("cards-per-row", 2)
            .on("reorder", "board-changed"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("board document"),
    });
    window.run_until_parked();
}

#[gpui_kit::test]
fn chart_document_validates_and_drives_layered_series(cx: &mut TestAppContext) {
    cx.update(guiraffe_charts::init);
    let doc = Document::new(
        Node::new("chart")
            .prop("summary", "Chart")
            .prop("layers", serde_json::json!([
                {"mark":"line","title":"Rates","groupLabels":[{"id":1,"label":"Measured"}],"data":[
                    {"key":1,"x":0,"y":2,"group":1,"facet":1,"size":5,"colorValue":0.5},
                    {"key":2,"x":1,"y":4,"group":1,"facet":1}
                ]},
                {"mark":"bar","arrangement":"group","orientation":"vertical","secondary":true,
                 "data":[{"x":0,"y":1,"facet":1,"colorCategory":3}]}
            ]))
            .bind("layers", "layers")
            .prop("axes", serde_json::json!([
                {"axis":"x","scale":"linear","domain":[0,2],"title":"Time","grid":true,"nice":false},
                {"axis":"y","scale":"log"}
            ]))
            .prop("facet-labels", serde_json::json!([{"id":1,"label":"Primary"}]))
            .prop("height", 280)
            .prop("legend", true)
            .prop("crosshair", false)
            .prop("tooltip", false)
            .prop("navigation", true)
            .prop("keyboard", true)
            .on("select", "select")
            .on("viewport-change", "view-change")
            .on("brush", "brush")
            .on("series-visibility", "series-change"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("chart document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_can_embed_a_chart_entity() {
    use gpui_kit::App;
    use guiraffe_charts::{ChartOptions, ChartSpec, ChartView};
    fn example(cx: &mut App) -> impl IntoElement {
        ui! { div { {ChartView::from_spec(ChartSpec::new("Chart"), ChartOptions::default(), cx)} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn chat_document_drives_the_composer_and_receives_submits(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    struct ChatLogic(
        Option<EventId>,
        std::rc::Rc<std::cell::RefCell<Option<Value>>>,
    );
    impl Logic for ChatLogic {
        fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
            self.0 = names.event("submitted");
        }
        fn on_event(&mut self, event: EventId, payload: &Value, _: &mut PatchBuf) {
            if self.0 == Some(event) {
                self.1.replace(Some(payload.clone()));
            }
        }
    }

    cx.update(guiraffe_chat::init);
    let doc = Document::new(
        Node::new("chat")
            .prop("title", "Support")
            .prop("subtitle", "Conversation")
            .prop("messages", serde_json::json!([
                {"id":100,"role":"assistant","text":"Welcome","timestamp":"Today",
                 "reasoning":"A short greeting","tools":[{"id":"load","name":"Read log","status":"succeeded","result":"Ready"}]}
            ]))
            .bind("messages", "messages")
            .prop("draft", "Hello")
            .prop("attachments", serde_json::json!([{"id":"notes","name":"notes.txt"}]))
            .prop("commands", serde_json::json!([{"id":"help","label":"Help"}]))
            .prop("mentions", serde_json::json!([{"id":"assistant","label":"Assistant"}]))
            .on("submit", "submitted"),
    );
    assert!(validation::Validation::default().check(&doc).is_empty());
    let payload = std::rc::Rc::default();
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(
            &doc,
            Arc::new(Registry::for_app(cx)),
            ChatLogic(None, std::rc::Rc::clone(&payload)),
            cx,
        )
        .expect("chat document"),
    });
    window.run_until_parked();
    window.update(|window, cx| window.click("send-chat", cx));
    window.run_until_parked();
    let payload = payload.borrow();
    let payload = payload.as_ref().expect("submit payload");
    assert_eq!(payload.field(&["text".into()]).as_str(), Some("Hello"));
    assert_eq!(payload.field(&["attachments".into()]).list_len(), Some(1));
}

#[test]
fn ui_macro_accepts_a_conversation_entity() {
    use gpui_kit::Entity;
    use guiraffe_chat::{Chat, ChatState};
    fn example(chat: &Entity<ChatState>) -> impl IntoElement {
        ui! { Chat(chat) }
    }
    let _ = example;
}

#[gpui_kit::test]
fn native_date_and_time_documents_validate_and_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_inputs::init);
    let doc = Document::new(
        Node::new("stack")
            .child(Node::new("date-picker")
                .prop("value", serde_json::json!(["2026-11-21", "2026-11-25"]))
                .bind("value", "dates")
                .prop("range", true)
                .prop("placeholder", "Choose dates")
                .prop("cleanable", true)
                .prop("appearance", true)
                .prop("disabled", false)
                .prop("size", "lg")
                .prop("focus-ring", true)
                .prop("months", 2)
                .prop("date-format", "%Y-%m-%d")
                .prop("precision", "date")
                .prop("hour-cycle", "h23")
                .prop("default-time", "09:30")
                .prop("first-day", "mon")
                .prop("disabled-weekdays", serde_json::json!([0, 6]))
                .prop("min-date", "2026-11-01")
                .prop("max-date", "2026-12-01")
                .prop("years", serde_json::json!([2020, 2030]))
                .prop("presets", serde_json::json!([{"label":"This week","value":["2026-11-21","2026-11-25"]}]))
                .on("change", "date-change"))
            .child(Node::new("time-field")
                .prop("value", "09:30:15")
                .bind("value", "time")
                .prop("precision", "second")
                .prop("hour-cycle", "h12")
                .prop("disabled", false)
                .prop("invalid", true)
                .prop("size", "sm")
                .on("change", "time-change")),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("calendar document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_uses_native_date_and_time_builder_methods() {
    use gpui_kit::component::{
        Disableable, Sizable,
        date_picker::{DatePicker, DatePickerState},
        time_field::{TimeField, TimeFieldState},
    };

    fn example(date: &Entity<DatePickerState>, time: &Entity<TimeFieldState>) -> impl IntoElement {
        ui! {
            div {
                DatePicker(date) { placeholder: "Choose"; cleanable: true; appearance: false; number_of_months: 2; disabled: false }
                TimeField(time) { invalid: true; disabled: false; with_size: px(32.) }
            }
        }
    }
    let _ = example;
}

#[gpui_kit::test]
fn flow_document_accepts_and_renders_native_graph_records(cx: &mut TestAppContext) {
    cx.update(guiraffe_flow::init);
    let doc = Document::new(
        Node::new("flow")
            .prop("graph", serde_json::json!({
                "nodes": {
                    "1":{"id":1,"pos":{"x":40,"y":20},"size":{"x":120,"y":70},"data":"Start",
                         "node_type":"default","parent":null,"selected":false,"draggable":true,
                         "handles":[{"id":"out","kind":"Source","side":"Right","offset":0.5}]},
                    "2":{"id":2,"pos":{"x":220,"y":20},"size":{"x":120,"y":70},"data":"Finish",
                         "node_type":"default","parent":null,"selected":false,"draggable":true,
                         "handles":[{"id":"in","kind":"Target","side":"Left","offset":0.5}]}
                },
                "edges":{"1":{"id":1,"source":{"node":1,"handle":"out"},"target":{"node":2,"handle":"in"},
                              "edge_type":"Bezier","label":"then","animated":false,"marker_start":"None","marker_end":"ArrowClosed"}}
            }))
            .bind("graph", "flow-graph")
            .prop("background", "cross")
            .prop("minimap", false)
            .prop("controls", true)
            .prop("toolbar", false)
            .prop("snap-grid", 10)
            .prop("fit-on-init", false)
            .prop("allow-self", false)
            .prop("allow-parallel", true)
            .prop("allow-cycles", false)
            .prop("pan-x", 10)
            .prop("pan-y", 20)
            .prop("zoom", 1.5)
            .prop("layout", serde_json::json!({
                "engine": "layered", "direction": "top-to-bottom", "routing": "spline",
                "spacing": {"x": 52, "y": 32}, "pinned": [1], "affected": [1, 2],
                "constraints": [{"kind": "above", "first": 1, "second": 2}],
                "order": [{"node": 2, "priority": -1}], "crossingWeight": 2
            }))
            .on("change", "change")
            .on("viewport-change", "viewport")
            .on("selection-change", "selection")
            .on("error", "error"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let template = Arc::new(Template::compile(&doc, &registry).expect("flow document"));
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
fn ui_macro_accepts_a_native_flow_canvas() {
    use guiraffe_flow::{FlowCanvas, FlowState};
    fn example(flow: &Entity<FlowState>) -> impl IntoElement {
        ui! { FlowCanvas(flow) }
    }
    let _ = example;
}

#[gpui_kit::test]
fn dock_document_validates_and_mounts_native_panels(cx: &mut TestAppContext) {
    cx.update(guiraffe_dnd::init);
    let doc = Document::new(
        Node::new("dock")
            .prop(
                "panels",
                serde_json::json!([
                    {"title":"Workspace","text":"Main panel","closable":false,"zoomable":false},
                    {"title":"Tools","placement":"left","text":"Side panel","visible":true}
                ]),
            )
            .prop("version", 2)
            .prop("native-drag", false)
            .prop("persist-key", "document-dock")
            .prop("left-size", 220)
            .prop("right-size", 180)
            .prop("bottom-size", 160)
            .on("layout-change", "layout")
            .on("error", "error"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let template = Arc::new(Template::compile(&doc, &registry).expect("dock document"));
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

#[gpui_kit::test]
fn gantt_document_drives_native_tasks_and_edit_options(cx: &mut TestAppContext) {
    cx.update(guiraffe_timeline::init);
    let doc = Document::new(
        Node::new("gantt")
            .prop(
                "tasks",
                serde_json::json!([
                    {"id":1,"name":"Design","notBefore":1,"duration":3},
                    {"id":2,"name":"Build","duration":5,"after":[1]}
                ]),
            )
            .prop("time-format", "frames")
            .prop("frames-per-second", 24)
            .prop("resize-edges", "end")
            .prop("editable", true)
            .prop("focusable", true)
            .prop("key-step", 1.5)
            .prop("lock-style", "hatched")
            .prop("global-lock-style", "dimmed")
            .prop("locked", serde_json::json!([1]))
            .prop("selected", 2)
            .on("reschedule", "move-task")
            .on("resize", "resize-task"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("gantt document"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_a_native_gantt_view() {
    use guiraffe_timeline::{GanttView, Task, TimeFormat};

    fn example(cx: &mut gpui_kit::App) -> impl IntoElement {
        let tasks = vec![Task {
            id: 1,
            name: "Assemble".into(),
            not_before: 0.0,
            duration: 1.0,
            after: vec![],
        }];
        ui! { div { {GanttView::new(tasks, TimeFormat::Days, cx)} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn document_surface_and_typography_components_use_native_builders(cx: &mut TestAppContext) {
    let doc = Document::new(
        Node::new("paper")
            .prop("shadow", "sm")
            .prop("radius", "lg")
            .prop("padding", "xs")
            .prop("no-padding", true)
            .prop("with-border", true)
            .child(
                Node::new("blockquote")
                    .prop("color", "teal")
                    .prop("cite", "Engineering team")
                    .child(Node::new("mark").prop("text", "Saved")),
            )
            .child(
                Node::new("kbd-group")
                    .prop("keys", serde_json::json!(["g", "s"]))
                    .prop("separator", "then")
                    .prop("size", "sm")
                    .prop("color", "orange")
                    .prop("variant", "outline"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    assert!(compile(&doc).diagnostics().is_empty());
    let _ = mount(cx, &doc);
}

#[test]
fn ui_macro_accepts_surface_and_typography_component_builders() {
    use guiraffe_components::{Blockquote, KbdGroup, KbdSeparator, Mark, Paper};
    use guiraffe_core::{ColorName, Size, Variant};

    let _ = ui! {
        Paper { shadow: Size::Sm; radius: Size::Lg; p: Size::Xs; p_0; with_border: true;
            Blockquote { color: ColorName::Teal; cite: "Team";
                Mark("Saved")
            }
            KbdGroup(["g", "s"]) {
                separator: KbdSeparator::Then;
                size: Size::Sm;
                color: ColorName::Orange;
                variant: Variant::Outline
            }
        }
    };
}

#[gpui_kit::test]
fn document_toggles_accept_native_focus_accessibility_and_tooltip_options(cx: &mut TestAppContext) {
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("checkbox")
                    .prop("label", "Confirm")
                    .prop("size", "xl")
                    .prop("accessibility-label", "Confirm selection")
                    .prop("tooltip", "Use space to toggle")
                    .prop("tooltip-show-delay-ms", 500)
                    .prop("tab-stop", false)
                    .prop("tab-index", -1)
                    .prop("focus-ring", false)
                    .prop("role", "radio")
                    .on("change", "confirm"),
            )
            .child(
                Node::new("switch")
                    .prop("label", "Enable")
                    .prop("size", "sm")
                    .prop("accessibility-label", "Enable notifications")
                    .prop("tooltip", "Toggle notifications")
                    .prop("tooltip-show-delay-ms", 300)
                    .prop("tab-stop", true)
                    .prop("tab-index", 2)
                    .prop("focus-ring", true)
                    .prop("color", "green")
                    .on("change", "enable"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    assert!(compile(&doc).diagnostics().is_empty());
    let _ = mount(cx, &doc);
}

#[test]
fn ui_macro_accepts_native_checkbox_and_switch_options() {
    use gpui_kit::component::{FocusableExt, Sizable, checkbox::Checkbox, switch::Switch};
    use gpui_kit::{Role, px};
    use std::time::Duration;

    let _ = ui! {
        div {
            Checkbox("confirm") {
                with_size: px(42.); accessibility_label: "Confirm selection";
                tooltip: "Confirm"; tooltip_show_delay: Duration::from_millis(500);
                tab_stop: false; tab_index: -1; focus_ring: false; role: Role::RadioButton
            }
            Switch("enable") {
                with_size: px(36.); accessibility_label: "Enable notifications";
                tooltip: "Enable"; tooltip_show_delay: Duration::from_millis(300);
                tab_stop: true; tab_index: 2; focus_ring: true
            }
        }
    };
}

#[gpui_kit::test]
fn native_indicator_swatch_and_avatar_documents_render(cx: &mut TestAppContext) {
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("indicator")
                    .prop("label", "3")
                    .prop("color", "green")
                    .prop("size", "lg")
                    .prop("position", "bottom-start")
                    .prop("disabled", false)
                    .child(Node::new("text").prop("text", "Inbox")),
            )
            .child(
                Node::new("color-swatch")
                    .prop("color", "green")
                    .prop("rgb", 0x88_cc_aa)
                    .prop("label", "Green")
                    .prop("size", "sm")
                    .prop("selected", "check")
                    .prop("disabled", false)
                    .prop("focusable", true)
                    .on("click", "choose-color"),
            )
            .child(
                Node::new("avatar-group")
                    .prop(
                        "people",
                        serde_json::json!([
                            {"name":"Ada Lovelace"}, {"name":"Grace Hopper"}
                        ]),
                    )
                    .prop("limit", 1)
                    .prop("color", "violet")
                    .prop("size", "xs")
                    .child(Node::new("text").prop("text", "+1")),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    assert!(compile(&doc).diagnostics().is_empty());
    let _ = mount(cx, &doc);
}

#[test]
fn ui_macro_accepts_native_indicator_swatch_and_avatar_builders() {
    use guiraffe_components::{
        AvatarGroup, ColorSwatch, Indicator, IndicatorPosition, Person, SwatchSelected,
    };
    use guiraffe_core::{ColorName, Rgb, Size};

    let _ = ui! {
        div {
            Indicator("Inbox") {
                label: "3"; color: ColorName::Green; size: Size::Lg;
                position: IndicatorPosition::BottomStart; disabled: false
            }
            ColorSwatch("green", Rgb::hex(0x88_cc_aa)) {
                label: "Green"; size: Size::Sm; selected: SwatchSelected::Check;
                disabled: false; focusable: true
            }
            AvatarGroup([Person::new("Ada"), Person::new("Grace")]) {
                id: "team"; color: ColorName::Violet; limit: 1; size: Size::Xs; more: "+1"
            }
        }
    };
}

#[gpui_kit::test]
fn timeline_editors_accept_structured_channels_and_bar_values(cx: &mut TestAppContext) {
    cx.update(guiraffe_timeline::init);
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("dope-sheet")
                    .prop(
                        "channels",
                        serde_json::json!([
                            {"name":"Motion","group":true,"children":[
                                {"name":"Position","locked":true,"color":"teal","keyframes":[
                                    {"time":0,"value":1,"interpolation":"bezier"},
                                    {"time":4,"value":0,"interpolation":"constant"}
                                ]},
                                {"name":"Opacity","visible":true,"muted":false,"keys":[1,2]}
                            ]}
                        ]),
                    )
                    .prop("time-format", "frames")
                    .prop("frames-per-second", 25)
                    .prop("length", 11)
                    .prop("editable", false)
                    .prop("focusable", false)
                    .prop("lock-style", "hatched")
                    .prop("playhead", 2)
                    .on("key-retime", "retime")
                    .on("selection-change", "select")
                    .on("change", "update-keys")
                    .on("channel-change", "update-channel"),
            )
            .child(
                Node::new("bar-curve")
                    .prop("values", serde_json::json!([0.25, 0.5, -0.5]))
                    .prop("minimum", -1)
                    .prop("maximum", 1)
                    .prop("steps", 8)
                    .prop("bipolar", true)
                    .prop("editable", true)
                    .prop("focusable", true)
                    .on("change", "edit-bars"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let template = Arc::new(Template::compile(&doc, &registry).expect("timeline editors"));
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
fn ui_macro_accepts_dope_sheet_and_bar_curve_views() {
    use guiraffe_timeline::{BarCurve, BarCurveView, Channel, DopeSheet, TimeFormat};

    fn example(cx: &mut gpui_kit::App) -> impl IntoElement {
        let sheet = DopeSheet::new(
            vec![Channel::new("Position", vec![1.0, 2.0])],
            TimeFormat::Frames(24.0),
            10.0,
            cx,
        );
        let mut model = BarCurve::new(4, -1.0, 1.0);
        model.set_steps(Some(8));
        model.bipolar = true;
        ui! { div { {sheet} {BarCurveView::new(model, cx)} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn curve_editor_document_drives_series_tangents_and_selection(cx: &mut TestAppContext) {
    cx.update(guiraffe_timeline::init);
    let doc = Document::new(
        Node::new("curve-editor")
            .prop(
                "curves",
                serde_json::json!([
                    {"name":"Move","color":"orange","visible":true,"keys":[
                        {"time":0,"value":0,"interpolation":"bezier","outHandle":[0.3,0.2]},
                        {"time":3,"value":1,"interpolation":"linear","inHandle":[-0.3,-0.2]}
                    ]},
                    {"name":"Opacity","visible":false,"keys":[{"time":1,"value":0.4}]}
                ]),
            )
            .prop("time-start", -1)
            .prop("time-end", 8)
            .prop("minimum", -0.25)
            .prop("maximum", 1.25)
            .prop("focusable", true)
            .prop("editable", true)
            .prop("tangent-mode", "mirrored")
            .prop("selection", serde_json::json!([0, 1]))
            .prop("fit", "normalize")
            .on("key-move", "move-key")
            .on("change", "update-series")
            .on("selection-change", "select-key"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx).expect("curves"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_the_native_curve_editor() {
    use guiraffe_timeline::{Curve, CurveSeries, CurveView, Interp, Key, TangentMode};

    fn example(cx: &mut gpui_kit::App) -> impl IntoElement {
        let mut curve = Curve::default();
        curve.set(Key::new(0.0, 0.0, Interp::Linear));
        let view = CurveView::new(curve, 0.0, 10.0, 0.0, 1.0, cx);
        view.update(cx, |view, cx| {
            view.add_curve(
                CurveSeries::new("Extra", Curve::default(), guiraffe_core::ColorName::Teal),
                cx,
            );
            view.set_tangent_mode(TangentMode::Mirrored);
            view.set_editable(true, cx);
            view.set_focusable(Some(true), cx);
        });
        ui! { div { {view} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn native_navigation_loader_and_activity_documents_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("burger")
                    .prop("value", true)
                    .prop("animate-changes", false)
                    .prop("size", "lg")
                    .prop("color", "blue")
                    .prop("focusable", true)
                    .prop("closed-label", "Open tools")
                    .prop("open-label", "Close tools")
                    .on("change", "toggle-tools"),
            )
            .child(
                Node::new("segmented-control")
                    .prop("items", serde_json::json!(["Current", "All"]))
                    .prop("value", 1)
                    .prop("size", "sm")
                    .prop("full-width", true)
                    .prop("disabled", false)
                    .prop("focusable", true)
                    .on("change", "scope"),
            )
            .child(
                Node::new("loader")
                    .prop("kind", "dots")
                    .prop("label", "Loading updates")
                    .prop("count", 4)
                    .prop("period-ms", 800)
                    .prop("size", "lg")
                    .prop("color", "grape"),
            )
            .child(
                Node::new("activity-timeline")
                    .prop("items", serde_json::json!([
                        {"title":"Started","description":"The work began.","color":"teal","bullet":"A"},
                        {"title":"Finished"}
                    ]))
                    .prop("active", 1)
                    .prop("size", "sm")
                    .prop("color", "grape")
                    .prop("bullet-size", 24)
                    .prop("line-width", 2),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let template = Arc::new(Template::compile(&doc, &registry).expect("native controls"));
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
fn ui_macro_accepts_activity_and_navigation_component_builders() {
    use guiraffe_components::{
        Burger, Loader, LoaderType, SegmentedControl, Timeline, TimelineItem,
    };
    use guiraffe_core::Size;

    fn example(cx: &mut gpui_kit::App) -> impl IntoElement {
        let burger = Burger::new(cx);
        burger.update(cx, |burger, cx| burger.set_open_instant(true, cx));
        let loader = Loader::new(cx);
        loader.update(cx, |loader, cx| loader.set_kind(LoaderType::Dots, cx));
        let scope = SegmentedControl::new(["Current", "All"], cx);
        let items = [TimelineItem::new("Started")
            .description("The work began.")
            .bullet("A")];
        ui! {
            div {
                {burger}
                {loader}
                {scope}
                Timeline(items) { active: 0; size: Size::Sm; bullet_size: 24.; line_width: 2. }
            }
        }
    }
    let _ = example;
}

#[gpui_kit::test]
fn native_disclosure_documents_drive_open_values_and_body_options(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let items = serde_json::json!([
        {"title":"Details","body":"Notes for the selected entry"},
        {"title":"History","body":"Earlier entries","disabled":true}
    ]);
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("accordion")
                    .prop("items", items.clone())
                    .prop("value", serde_json::json!([0]))
                    .prop("default-open", serde_json::json!([0]))
                    .prop("variant", "separated")
                    .prop("chevron", "start")
                    .prop("multiple", true)
                    .prop("focusable", true)
                    .on("change", "update-disclosure"),
            )
            .child(
                Node::new("expanding-list")
                    .prop("items", items)
                    .prop("value", 0)
                    .prop("body-height", 120)
                    .prop("focusable", true)
                    .on("change", "open-detail"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("native disclosure"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_native_disclosure_controls() {
    use guiraffe_components::{
        Accordion, AccordionOptions, AccordionVariant, ChevronPosition, DisclosureItem,
        ExpandingList,
    };

    fn example(cx: &mut gpui_kit::App) -> impl IntoElement {
        let items = || {
            vec![DisclosureItem::new("Details", || {
                "Content".into_any_element()
            })]
        };
        let accordion = Accordion::new(
            items(),
            AccordionOptions {
                variant: AccordionVariant::Separated,
                chevron: ChevronPosition::Start,
                multiple: true,
                focusable: Some(true),
            },
            vec![0],
            cx,
        );
        let list = ExpandingList::new(items(), px(120.), cx);
        ui! { div { {accordion} {list} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn spotlight_document_activates_a_native_search_result(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct Seen {
        activated: Vec<Value>,
        selections: Vec<Value>,
        queries: Vec<Value>,
    }
    struct SearchLogic {
        events: [Option<EventId>; 3],
        seen: Rc<RefCell<Seen>>,
    }
    impl Logic for SearchLogic {
        fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
            self.events = [
                names.event("run-command"),
                names.event("highlight"),
                names.event("search"),
            ];
        }
        fn on_event(&mut self, event: EventId, payload: &Value, _: &mut PatchBuf) {
            let mut seen = self.seen.borrow_mut();
            let list = match self.events.iter().position(|id| *id == Some(event)) {
                Some(0) => &mut seen.activated,
                Some(1) => &mut seen.selections,
                Some(2) => &mut seen.queries,
                _ => return,
            };
            list.push(payload.clone());
        }
    }

    cx.update(guiraffe_overlays::init);
    let doc = Document::new(
        Node::new("spotlight")
            .prop(
                "actions",
                serde_json::json!([
                    {"id":"validate","title":"Validate screen","keywords":"check linter",
                     "group":"Screen","description":"Run the linter","shortcut":"Ctrl+Shift+V",
                     "icon":"check"},
                    {"id":"publish","title":"Publish screen","group":"Screen","disabled":true},
                    {"id":"open","title":"Open file","icon":"icons/folder-open.svg"}
                ]),
            )
            .prop("pinned", serde_json::json!(["open"]))
            .prop("recents", serde_json::json!([]))
            .prop("recent-limit", 3)
            .prop("limit", 20)
            .prop("placeholder", "Find a command")
            .prop(
                "labels",
                serde_json::json!({"pinned":"Favourites","noResultsTitle":"Nothing"}),
            )
            .prop("loading", false)
            .prop("width", 480)
            .prop("max-list-height", 320)
            .on("activate", "run-command")
            .on("selection-change", "highlight")
            .on("query-change", "search"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let seen = Rc::default();
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let template = Arc::new(Template::compile(&doc, &registry).expect("command search"));
        assert!(
            template.diagnostics().is_empty(),
            "{:?}",
            template.diagnostics()
        );
        Shell {
            ui: DataUi::from_template(
                template,
                registry,
                SearchLogic {
                    events: [None; 3],
                    seen: Rc::clone(&seen),
                },
                cx,
            ),
        }
    });
    window.run_until_parked();
    // Rows: "Favourites" header, Open file, "Screen" header, Validate, Publish (disabled).
    window.update(|window, cx| {
        assert!(window.find(("spotlight-row", 1usize)).visible());
        window.click(("spotlight-row", 4usize), cx);
    });
    window.run_until_parked();
    assert!(
        seen.borrow().activated.is_empty(),
        "a disabled action never runs"
    );
    window.update(|window, cx| window.click(("spotlight-row", 3usize), cx));
    window.run_until_parked();
    assert_eq!(seen.borrow().activated, [Value::from("validate")]);
    assert!(
        seen.borrow().selections.contains(&Value::from("validate")),
        "{:?}",
        seen.borrow().selections
    );
    // The palette keeps its own recents across document renders: Validate moves under
    // "Recent", so a third section header appears.
    window.update(|window, _| {
        assert!(window.find(("spotlight-group", 2usize)).visible());
    });
}

#[gpui_kit::test]
fn picker_wrapper_and_event_timeline_documents_validate_and_mount(cx: &mut TestAppContext) {
    use gpui_kit::AppContext as _;
    use std::{cell::RefCell, rc::Rc};

    struct ErrorLogic {
        error: Option<EventId>,
        seen: Rc<RefCell<Vec<Value>>>,
    }
    impl Logic for ErrorLogic {
        fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
            self.error = names.event("timeline-error");
        }
        fn on_event(&mut self, event: EventId, payload: &Value, _: &mut PatchBuf) {
            if Some(event) == self.error {
                self.seen.borrow_mut().push(payload.clone());
            }
        }
    }

    init_document_widgets(cx);
    cx.update(guiraffe_timeline::init);
    let document = |track: u64| {
        Document::new(
            Node::new("stack")
                .child(
                    Node::new("input-wrapper")
                        .prop("label", "Notes")
                        .prop("description", "Shown to reviewers")
                        .prop("error", "Required")
                        .prop("required", true)
                        .prop("size", "md")
                        .child(Node::new("text").prop("text", "Body")),
                )
                .child(
                    Node::new("picker")
                        .prop("mode", "date-range")
                        .prop("value", serde_json::json!(["2026-11-01", "2026-11-07"]))
                        .prop("label", "Period")
                        .prop("clearable", true)
                        .prop("locale", "en-GB")
                        .prop("number-of-months", 2)
                        .prop("exclude-date", serde_json::json!(["2026-11-03"]))
                        .prop(
                            "presets",
                            serde_json::json!([
                                {"label":"Last 7 days","range":"last-days","days":7},
                                {"label":"Q4","range":"fixed","start":"2026-10-01","end":"2026-12-31"}
                            ]),
                        )
                        .on("change", "period-changed")
                        .on("open-change", "period-open"),
                )
                .child(
                    Node::new("picker")
                        .prop("mode", "date-time")
                        .prop("default-value", "2026-11-21T09:30")
                        .prop("hour-format", "h12")
                        .prop("with-seconds", true),
                )
                .child(
                    Node::new("event-timeline")
                        .prop(
                            "tracks",
                            serde_json::json!([
                                {"id":1,"label":"main"},
                                {"id":2,"label":"feature","color":"teal"}
                            ]),
                        )
                        .prop(
                            "events",
                            serde_json::json!([
                                {"id":10,"track":1,"position":0,"label":"init"},
                                {"id":11,"track":track,"position":1,"label":"branch",
                                 "content":"Start the feature","tags":["wip"]}
                            ]),
                        )
                        .prop(
                            "links",
                            serde_json::json!([{"source":10,"destination":11,"kind":"branch"}]),
                        )
                        .prop("orientation", "horizontal")
                        .prop("label", "History")
                        .on("select", "commit-selected")
                        .on("error", "timeline-error"),
                ),
        )
    };
    for (track, errors) in [(2, 0), (9, 1)] {
        let doc = document(track);
        let issues = validation::Validation::default().check(&doc);
        assert!(issues.is_empty(), "{issues:?}");
        let seen = Rc::new(RefCell::new(Vec::new()));
        let logic_seen = seen.clone();
        cx.update(|cx| {
            cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                let registry = Arc::new(Registry::for_app(cx));
                let template = Arc::new(Template::compile(&doc, &registry).expect("document"));
                let ui = DataUi::from_template(
                    template,
                    registry,
                    ErrorLogic {
                        error: None,
                        seen: logic_seen,
                    },
                    cx,
                );
                let view = cx.new(|_| Shell { ui });
                guiraffe_overlays::root(view, window, cx)
            })
            .expect("window")
        });
        cx.run_until_parked();
        assert_eq!(seen.borrow().len(), errors, "{:?}", seen.borrow());
    }

    // Picker values must be ISO dates, times or date-times.
    let bad = Document::new(Node::new("picker").prop("value", "21/11/2026"));
    assert_eq!(validation::Validation::default().check(&bad).len(), 1);
}

#[test]
fn ui_macro_accepts_the_native_spotlight_entity() {
    use gpui_kit::{AppContext, Context, Window};
    use guiraffe_overlays::{Spotlight, SpotlightAction, SpotlightState};

    fn example(window: &mut Window, cx: &mut Context<Shell>) -> impl IntoElement {
        let search = cx.new(|cx| {
            SpotlightState::new(window, cx)
                .actions([SpotlightAction::new("validate", "Validate screen")])
                .on_activate(|_, _, _| {})
        });
        let inline = Spotlight::new(&search);
        ui! { div { {search} {inline} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn native_commands_icons_steps_and_splitter_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    cx.update(guiraffe_layout::init);
    cx.update(|cx| {
        let _ = guiraffe_commands::init(cx);
    });
    let doc = Document::new(
        Node::new("command-scope").child(
            Node::new("splitter")
                .prop("axis", "vertical")
                .prop("pane-sizes", serde_json::json!([120, null]))
                .prop("first-size", 160)
                .prop("min-size", 48)
                .prop("max-size", 600)
                .prop("persist-key", "example")
                .on("resize", "save-pane-sizes")
                .child(
                    Node::new("command-menu")
                        .prop("commands", serde_json::json!([{
                            "id": "file.save", "title": "Save", "category": "File",
                            "keywords": "write", "keybinding": "Control+S",
                            "when": "!dialog", "enabled": true
                        }]))
                        .prop("show-categories", true)
                        .prop("show-keybindings", true)
                        .on("invoke", "run-command")
                        .on("error", "report-command-error"),
                )
                .child(
                    Node::new("stack")
                        .child(Node::new("theme-icon").prop("icon", "icons/check.svg").prop("size", "sm").prop("variant", "outline"))
                        .child(Node::new("action-icon").prop("icon", "icons/search.svg").prop("label", "Search").prop("focusable", true).on("click", "search"))
                        .child(Node::new("close-button").prop("label", "Close panel").on("click", "close-panel"))
                        .child(Node::new("copy-button").prop("value", "Deploy").prop("focusable", true).on("copy", "copied"))
                        .child(Node::new("stepper").prop("steps", serde_json::json!([
                            {"label": "Start", "description": "Create an account", "icon": "icons/check.svg"},
                            {"label": "Finish"}
                        ])).prop("active", 0).on("step-click", "select-step")),
                ),
        ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("native commands and splitter"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_native_commands_icons_steps_and_splitter_panes() {
    use gpui_kit::{Entity, ParentElement, assets::IconName, div, px};
    use guiraffe_commands::{CommandScope, Commands, Menu};
    use guiraffe_components::{ActionIcon, Step, Stepper, ThemeIcon};
    use guiraffe_core::{ColorName, Size, Variant};
    use guiraffe_layout::{Splitter, SplitterAxis, SplitterState};
    use guiraffe_ui::ui;

    fn example(commands: &Commands, state: &Entity<SplitterState>) -> impl IntoElement {
        let menu = ui! {
            Menu("commands", commands) { show_categories: false; show_keybindings: true }
        };
        let controls = ui! {
            div {
                ThemeIcon(IconName::Check) { radius: Size::Xs; variant: Variant::Outline }
                ActionIcon("search", IconName::Check) { label: "Search"; color: ColorName::Teal; focusable: true }
                Stepper([Step::new("Start")]) { active: 0; size: Size::Sm; focusable: true }
            }
        };
        let split = Splitter::new(state)
            .axis(SplitterAxis::Vertical)
            .first_size(px(120.))
            .first(menu)
            .second(controls);
        ui! { CommandScope(commands) { {split} } }
    }

    let _ = example;
}

#[gpui_kit::test]
fn responsive_document_layouts_validate_and_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let doc = Document::new(
        Node::new("app-shell")
            .prop(
                "slots",
                serde_json::json!(["header", "navbar", "main", "aside", "footer"]),
            )
            .prop("header-height", 50)
            .prop("footer-height", 30)
            .prop("padding", "md")
            .prop("border", true)
            .prop("collapse-below", "sm")
            .prop("collapse-aside-below", "md")
            .prop("resizable-navbar", true)
            .prop("resizable-aside", true)
            .prop("navbar-collapsed", false)
            .prop("aside-width", 250)
            .on("navbar-collapsed", "hide-navigation")
            .on("aside-resized", "resize-aside")
            .child(Node::new("text").prop("text", "Header"))
            .child(Node::new("text").prop("text", "Navigation"))
            .child(
                Node::new("responsive-grid")
                    .prop("columns", 12)
                    .prop("gutter", "xs")
                    .prop("gutter-px", 8)
                    .prop("justify", "center")
                    .prop("grow", true)
                    .prop("width-source", "viewport")
                    .prop("width-px", 900)
                    .prop(
                        "placements",
                        serde_json::json!([
                            {"span": {"base": "full", "sm": 3}, "offset": {"base": 0, "sm": 1}},
                            {"span": 6, "offset": 2}
                        ]),
                    )
                    .child(Node::new("text").prop("text", "First column"))
                    .child(Node::new("text").prop("text", "Second column")),
            )
            .child(Node::new("text").prop("text", "Aside"))
            .child(Node::new("text").prop("text", "Footer")),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("native application shell and responsive grid"),
    });
    window.run_until_parked();
}

#[gpui_kit::test]
fn native_spoiler_speed_dial_and_card_sections_validate_and_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let doc = Document::new(
        Node::new("card")
            .prop("section-borders", serde_json::json!([true, null, false]))
            .child(
                Node::new("spoiler")
                    .prop("text", "A short preview with more content available.")
                    .prop("expanded", true)
                    .prop("collapsed-lines", 2)
                    .prop("duration-ms", 0)
                    .prop("easing", "ease-in")
                    .prop("focusable", true)
                    .prop("color", "violet")
                    .prop("size", "sm")
                    .on("change", "update-expanded"),
            )
            .child(
                Node::new("card-section").prop("with-border", true).child(
                    Node::new("button")
                        .prop("label", "Continue")
                        .prop("inherit-focus-scope", true),
                ),
            )
            .child(
                Node::new("speed-dial")
                    .prop("primary-icon", "icons/check.svg")
                    .prop("label", "File commands")
                    .prop("layout", "radial")
                    .prop("radius", 80)
                    .prop("clockwise", false)
                    .prop("arc", "half")
                    .prop("start-degrees", 180)
                    .prop(
                        "actions",
                        serde_json::json!([{"label":"Confirm","icon":"icons/check.svg"}]),
                    )
                    .prop("show-labels", true)
                    .prop("focusable", true)
                    .on("change", "toggle-actions")
                    .on("action", "run-action"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
            .expect("native disclosure and card components"),
    });
    window.run_until_parked();
}

#[test]
fn ui_macro_accepts_native_responsive_grid_card_and_scoped_button() {
    use gpui_kit::{Entity, ParentElement, component::button::Button, px};
    use guiraffe_components::{Card, CardSection, ScopedButton};
    use guiraffe_layout::{AppShell, AppShellState, Breakpoint, Col, Grid, Justify, Responsive};
    use guiraffe_ui::ui;

    fn example(state: &Entity<AppShellState>) -> impl IntoElement {
        let button = ui! { ScopedButton(Button::new("next")) { focusable: true } };
        let card = ui! { Card { with_border: true; .section(CardSection::new().with_border(true).child(button)); } };
        let column = Col::span(Responsive::base(12).at(Breakpoint::Sm, 6)).offset(1);
        let columns = ui! { Grid { columns: 12; justify: Justify::Center; grow: true; .child(column.child(card)); } };
        ui! {
            AppShell(state) {
                header_height: px(48.);
                collapse_below: Breakpoint::Sm;
                border;
                .header(columns);
            }
        }
    }

    let _ = example;
}

#[gpui_kit::test]
fn modal_and_toast_documents_use_the_native_window_overlay_stack(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, WindowOptions, test::TestWindowExt};
    use guiraffe_overlays::{OverlayHost, stack::OverlayKind, toast::ToastPosition};
    use std::{cell::Cell, rc::Rc};

    struct DialogLogic {
        ok: Option<EventId>,
        accepted: Rc<Cell<bool>>,
        show_drawer: Option<SlotId>,
    }

    impl Logic for DialogLogic {
        fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
            self.ok = names.event("apply");
            self.show_drawer = names.slot("show-drawer");
        }

        fn on_event(&mut self, event: EventId, _: &Value, patch: &mut PatchBuf) {
            if self.ok == Some(event) {
                self.accepted.set(true);
                if let Some(slot) = self.show_drawer {
                    patch.set(slot, true);
                }
            }
        }
    }

    cx.update(guiraffe_overlays::init);
    let doc = Document::new(
        Node::new("toast-tray")
            .prop("toast-limit", 3)
            .prop(
                "toasts",
                serde_json::json!([
                    {"id":"accepted","message":"Ready","position":"bottom-left","persistent":true}
                ]),
            )
            .child(
                Node::new("modal")
                    .prop("open", true)
                    .prop("title", "Apply settings?")
                    .prop("message", "The current document will be updated.")
                    .prop("ok-label", "Apply")
                    .prop("cancel-label", "Cancel")
                    .prop("width", 420)
                    .prop("offset-top", 64)
                    .prop("accessibility-label", "Apply settings")
                    .prop("overlay-closable", false)
                    .on("ok", "apply"),
            )
            .child(
                Node::new("drawer")
                    .bind("open", "show-drawer")
                    .prop("title", "Results")
                    .prop("position", "left")
                    .prop("size", "sm")
                    .prop("close-on-escape", false)
                    .prop("resizable", false)
                    .on("close", "close-results"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let accepted = Rc::new(Cell::new(false));
    let handle = cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            let registry = Arc::new(Registry::for_app(cx));
            let logic = DialogLogic {
                ok: None,
                show_drawer: None,
                accepted: accepted.clone(),
            };
            let ui = DataUi::new(&doc, registry, logic, cx).expect("overlay services");
            let screen = cx.new(|_| Shell { ui });
            guiraffe_overlays::root(screen, window, cx)
        })
        .expect("window with overlay root")
    });
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        let host = OverlayHost::entity(window, cx).expect("installed host");
        let state = host.read(cx);
        assert_eq!(state.toasts().limit(), 3);
        let visible = state.toasts().visible();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].message, "Ready");
        assert_eq!(visible[0].position, ToastPosition::BottomLeft);
        let top = state.stack().top().expect("native dialog");
        assert_eq!(top.kind, OverlayKind::Modal);
        assert!(!top.policy.outside);
        window.click("document-modal-ok", cx);
    })
    .expect("live window");
    cx.run_until_parked();
    assert!(accepted.get());
    cx.update_window(handle.into(), |_, window, cx| {
        let host = OverlayHost::entity(window, cx).expect("installed host");
        let top = host.read(cx).stack().top().expect("native sheet");
        assert_eq!(top.kind, OverlayKind::Drawer);
        assert!(!top.policy.escape);
    })
    .expect("live window");
}

#[test]
fn ui_macro_accepts_tree_board_flame_graph_and_disclosure_state() {
    use gpui_kit::{App, AppContext as _, SharedString, assets::IconName};
    use guiraffe_components::{DialAction, DialLayout, SpeedDial, Spoiler};
    use guiraffe_dnd::{Card, SortableBoard};
    use guiraffe_flamegraph::{FlameGraph, FlameLayout};
    use guiraffe_tree::{PropertyGrid, RowStyle, TreeData, TreeList};
    use std::rc::Rc;

    fn example(
        layout: Arc<FlameLayout>,
        tree: Rc<dyn TreeData>,
        columns: Vec<(SharedString, Vec<Card>)>,
        cx: &mut App,
    ) -> impl IntoElement {
        let properties = cx.new(|cx| {
            ui! {
                PropertyGrid(tree.clone(), cx) { split_ratio: 0.55; focusable: true }
            }
        });
        let list = cx.new(|cx| {
            ui! {
                TreeList(tree, cx) { focusable: false; row_style: RowStyle::default() }
            }
        });
        let board = cx.new(|cx| ui! { SortableBoard(columns, cx) { with_grid: 3 } });
        let flame = cx.new(|_| ui! { FlameGraph(layout) });
        let spoiler = ui! { Spoiler(cx) };
        spoiler.update(cx, |spoiler, cx| {
            spoiler.set_collapsed_lines(2, cx);
            spoiler.set_focusable(Some(true), cx);
        });
        let dial = ui! {
            SpeedDial(
                IconName::Plus, "Create",
                vec![DialAction { icon: IconName::Check, label: "Finish".into() }],
                DialLayout::Vertical { spacing: 40. }, cx
            )
        };
        dial.update(cx, |dial, cx| dial.set_show_labels(true, cx));
        ui! { div { {properties} {list} {board} {flame} {spoiler} {dial} } }
    }

    let _ = example;
}

#[gpui_kit::test]
fn kit_overlay_documents_validate_and_mount(cx: &mut TestAppContext) {
    use gpui_kit::AppContext as _;
    cx.update(guiraffe_overlays::init);
    let items = serde_json::json!([
        {"id":"save","label":"Save","icon":"icons/check.svg","checked":true},
        {"kind":"label","label":"Recent"},
        {"kind":"separator"},
        {"kind":"submenu","label":"Options","items":[{"id":"undo","label":"Undo","disabled":true}]}
    ]);
    let doc = Document::new(
        Node::new("stack")
            .child(
                Node::new("popover")
                    .prop("trigger-label", "Open details")
                    .prop("arrow", true)
                    .prop("offset", 6)
                    .prop("default-open", true)
                    .prop("anchor", "bottom-left")
                    .prop("trigger-full-width", true)
                    .on("change", "details-changed")
                    .child(Node::new("text").prop("text", "Popover content")),
            )
            .child(
                Node::new("hover-card")
                    .prop("open-delay-ms", 10)
                    .prop("close-delay-ms", 25)
                    .prop("anchor", "right-center")
                    .on("change", "hover-changed")
                    .child(Node::new("text").prop("text", "Hover content")),
            )
            .child(
                Node::new("tooltip")
                    .prop("text", "Save all changes")
                    .prop("key-binding", "ctrl-s")
                    .child(Node::new("text").prop("text", "Save")),
            )
            .child(
                Node::new("popup-menu")
                    .prop("items", items.clone())
                    .prop("min-width", 140)
                    .prop("max-width", 220)
                    .prop("max-height", 260)
                    .prop("scrollable", true)
                    .prop("check-side", "right")
                    .prop("external-link-icon", false)
                    .on("select", "choose-popup"),
            )
            .child(
                Node::new("context-menu")
                    .prop("items", items.clone())
                    .on("select", "choose-context")
                    .child(Node::new("text").prop("text", "Right-click target")),
            )
            .child(
                Node::new("dropdown-menu")
                    .prop("items", items)
                    .prop("trigger-label", "File")
                    .prop("anchor", "top-right")
                    .on("select", "choose-file")
                    .on("change", "file-open"),
            ),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    cx.update(|cx| {
        cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
            let ui = DataUi::new(&doc, Arc::new(Registry::for_app(cx)), NoLogic, cx)
                .expect("native Kit overlay document");
            let screen = cx.new(|_| Shell { ui });
            guiraffe_overlays::root(screen, window, cx)
        })
        .expect("window with overlay root")
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn calendar_document_validates_and_mounts_records(cx: &mut TestAppContext) {
    cx.update(guiraffe_calendar::init);
    let calendar = |items: serde_json::Value| {
        Document::new(
            Node::new("calendar")
                .prop("items", items)
                .prop(
                    "sources",
                    serde_json::json!([{"id":1,"name":"Work","color":"teal"}]),
                )
                .prop("view", "week")
                .prop("date", "2026-11-21")
                .prop("now", "2026-11-21T11:00:00Z")
                .on("edit-request", "edit"),
        )
    };
    let bad = calendar(serde_json::json!([{"id":1,"title":"No source or time"}]));
    assert_eq!(validation::Validation::default().check(&bad).len(), 1);
    let document = calendar(serde_json::json!([
        {"id":1,"source":1,"title":"Review","start":"2026-11-21T09:00:00Z","end":"2026-11-21T10:00:00Z"},
        {"id":2,"source":1,"title":"Release","start":"2026-11-21","end":"2026-11-22","allDay":true}
    ]));
    let issues = validation::Validation::default().check(&document);
    assert!(issues.is_empty(), "{issues:?}");
    let registry = cx.update(|cx| Arc::new(Registry::for_app(cx)));
    let template = Arc::new(Template::compile(&document, &registry).expect("calendar document"));
    let (_, window) = cx.add_window_view(|_, cx| Shell {
        ui: DataUi::from_template(template, registry, NoLogic, cx),
    });
    window.run_until_parked();
    assert!(window.debug_bounds("calendar-day-2026-11-21").is_some());
}

#[test]
fn ui_macro_calls_reexported_overlay_builders() {
    use gpui_kit::{Anchor, component::button::Button};
    use guiraffe_overlays::{ContextMenuExt, DropdownMenu, HoverCard, Popover, Tooltip};
    use std::time::Duration;
    fn example() -> impl IntoElement {
        let popover = ui! {
            Popover("editing") {
                trigger: Button::new("open").label("Open");
                anchor: Anchor::TopRight;
                arrow: true;
                "Popover"
            }
        };
        let hover = ui! {
            HoverCard("about") {
                trigger: "Details";
                open_delay: Duration::from_millis(150);
                "More details"
            }
        };
        let context =
            ui! { div { .context_menu(|popup, _, _| popup.label("Context")) "Right-click" } };
        let dropdown = ui! { Button("choices") { label: "Choices"; .dropdown_menu(|popup, _, _| popup.label("Choice")) } };
        let _tooltip = ui! { Tooltip("Save") { key_binding: None } };
        ui! { div { {popover} {hover} {context} {dropdown} } }
    }
    let _ = example;
}

#[gpui_kit::test]
fn every_catalogue_property_validates_and_mounts_a_literal(cx: &mut TestAppContext) {
    use gpui_kit::AppContext as _;
    use serde_json::{Value as Json, json};

    fn literal(schema: &Json) -> Json {
        if let Some(value) = schema["enum"].as_array().and_then(|items| items.first()) {
            return value.clone();
        }
        match schema["type"].as_str() {
            Some("string") => {
                let value = if schema["pattern"].as_str().is_some() {
                    "2026-01-01"
                } else {
                    "sample"
                };
                json!(value)
            }
            Some("boolean") => json!(true),
            Some("integer") => json!(schema["minimum"].as_f64().unwrap_or_default().ceil() as u64),
            Some("number") => {
                let minimum = schema["minimum"].as_f64().unwrap_or(0.);
                let maximum = schema["maximum"].as_f64().unwrap_or(1.);
                json!(1_f64.clamp(minimum, maximum.max(minimum)))
            }
            Some("array") => json!([]),
            Some("object") => {
                let required = schema["required"].as_array().into_iter().flatten();
                Json::Object(
                    required
                        .filter_map(Json::as_str)
                        .map(|field| (field.to_owned(), literal(&schema["properties"][field])))
                        .collect(),
                )
            }
            _ => json!("2026-01-01"),
        }
    }

    init_document_widgets(cx);
    let registry = cx.update(|cx| Arc::new(Registry::for_app(cx)));
    for spec in catalog::all_widgets() {
        let mut root = Node::new(spec.id);
        for prop in spec.props {
            let default: Json =
                serde_json::from_str(prop.default_json).expect("descriptor default");
            let value = if !default.is_null() {
                default
            } else {
                match (spec.id, prop.name) {
                    ("date-picker", "years") => json!([2020, 2030]),
                    ("date-picker", "date-format") => json!("%Y-%m-%d"),
                    ("picker", "min-time") => json!("08:00"),
                    ("picker", "max-time") => json!("18:30"),
                    ("picker", "value-format") => json!("YYYY-MM-DD"),
                    ("dock", "layout") => json!({
                        "version":1,"center":{"panel_name":"StackPanel","children":[],"info":{}}
                    }),
                    (_, "icon" | "loading-icon") => json!("icons/check.svg"),
                    _ => literal(&prop.schema()),
                }
            };
            root = root.prop(prop.name, value);
        }
        for event in spec.events {
            root = root.on(event.id, event.id);
        }
        let document = Document::new(root);
        let issues = validation::Validation::default().check(&document);
        assert!(issues.is_empty(), "{}: {issues:?}", spec.id);
        let template = Arc::new(Template::compile(&document, &registry).expect(spec.id));
        assert!(
            template.diagnostics().is_empty(),
            "{}: {:?}",
            spec.id,
            template.diagnostics()
        );
        let registry = registry.clone();
        cx.update(|cx| {
            cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                let ui = DataUi::from_template(template, registry, NoLogic, cx);
                let view = cx.new(|_| Shell { ui });
                guiraffe_overlays::root(view, window, cx)
            })
            .expect(spec.id)
        });
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn sheet_document_applies_options_and_reports_user_edits(cx: &mut TestAppContext) {
    #[derive(Default)]
    struct SheetLogic(
        Option<EventId>,
        std::rc::Rc<std::cell::RefCell<Option<Value>>>,
    );
    impl Logic for SheetLogic {
        fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
            self.0 = names.event("sheet-edited");
        }
        fn on_event(&mut self, event: EventId, payload: &Value, _: &mut PatchBuf) {
            if self.0 == Some(event) {
                self.1.replace(Some(payload.clone()));
            }
        }
    }

    cx.update(guiraffe_sheet::init);
    let doc = Document::new(
        Node::new("sheet")
            .prop("rows", 50)
            .prop("cols", 8)
            .prop(
                "cells",
                serde_json::json!([
                    {"cell": "A1", "text": "2"},
                    {"cell": "B1", "text": "=A1*3", "format": "number", "decimals": 1, "align": "right"}
                ]),
            )
            .prop("read-only", false)
            .prop("frozen-rows", 1)
            .prop("frozen-cols", 1)
            .prop("height", 240)
            .on("edit", "sheet-edited"),
    );
    let issues = validation::Validation::default().check(&doc);
    assert!(issues.is_empty(), "{issues:?}");
    let bad = Document::new(Node::new("sheet").prop("cells", serde_json::json!([{"text": "x"}])));
    assert!(!validation::Validation::default().check(&bad).is_empty());
    let payload = std::rc::Rc::default();
    let (_, window) = cx.add_window_view(|_, cx| {
        let registry = Arc::new(Registry::for_app(cx));
        let ui = DataUi::new(
            &doc,
            registry,
            SheetLogic(None, std::rc::Rc::clone(&payload)),
            cx,
        )
        .expect("compile sheet");
        Shell { ui }
    });
    window.run_until_parked();
    let origin = window.update(|window, _| {
        use gpui_kit::test::TestQueryExt as _;
        window.find_by_label("Spreadsheet").bounds().origin
    });
    // Past the row-number gutter and the header: cell A1.
    window.simulate_click(
        origin + gpui_kit::point(px(90.), px(40.)),
        Default::default(),
    );
    window.simulate_keystrokes("9 enter");
    window.run_until_parked();
    let expected = Value::List(
        vec![Value::Record(Arc::new(
            [("cell".into(), "A1".into()), ("text".into(), "9".into())].into(),
        ))]
        .into(),
    );
    assert_eq!(*payload.borrow(), Some(expected));
}

#[test]
fn ui_macro_accepts_sheet_builders() {
    use gpui_kit::{App, AppContext as _};
    use guiraffe_sheet::{Sheet, SheetModel, SheetState};

    fn example(cx: &mut App) -> impl IntoElement {
        let state = cx.new(|cx| {
            ui! {
                SheetState(SheetModel::new(100, 10), cx) {
                    read_only: true; frozen_rows: 1; frozen_cols: 2
                }
            }
        });
        ui! { div { Sheet(&state) } }
    }
    let _ = example;
}

/// Pointer input in a registered document and a script action by path produce the same
/// logic run and the same view data, and both pass through bus interceptors.
#[gpui_kit::test]
fn registered_document_events_flow_over_the_action_bus(cx: &mut TestAppContext) {
    use guiraffe_core::protocol::{Action, ActionBus, Recorder, Source};
    let (ui, vcx) = mount(cx, &doc());
    let recorder = Recorder::new();
    let id = vcx.update(|window, cx| {
        ActionBus::add_interceptor(recorder.clone(), window, cx);
        DataUi::register(&ui, "inbox/doc", window, cx)
    });
    let center = vcx
        .debug_bounds("refresh")
        .map(|b| b.center())
        .unwrap_or_else(|| panic!("button addressable by its document id"));
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(status(&ui, vcx), "refreshed 1×");
    vcx.update(|window, cx| {
        let action = Action {
            name: "refresh".into(),
            payload: guiraffe_core::widget_value::Value::Null,
        };
        assert!(ActionBus::dispatch(id, &action, Source::Script, window, cx).is_ok());
        let unknown = Action {
            name: "nope".into(),
            payload: guiraffe_core::widget_value::Value::Null,
        };
        assert!(ActionBus::dispatch(id, &unknown, Source::Script, window, cx).is_err());
    });
    vcx.run_until_parked();
    assert_eq!(status(&ui, vcx), "refreshed 2×");
    let log = recorder.take();
    assert_eq!(
        log.iter()
            .map(|r| (
                r.path.as_ref().to_owned(),
                r.action.name.to_string(),
                r.source
            ))
            .collect::<Vec<_>>(),
        [
            ("inbox/doc".to_owned(), "refresh".to_owned(), Source::DataUi),
            ("inbox/doc".to_owned(), "refresh".to_owned(), Source::Script),
        ]
    );
}
