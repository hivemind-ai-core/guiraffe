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

#[gpui_kit::test]
fn installed_widget_metadata_matches_compiler_schemas(cx: &mut TestAppContext) {
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
    cx.update(guiraffe_tree::init);
    cx.update(guiraffe_timeline::init);
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
    let view = ui! { MermaidView("example") { source: "flowchart LR\n  A --> B" } };
    let _ = view.into_element();
}

#[gpui_kit::test]
fn code_document_drives_view_options_and_annotations(cx: &mut TestAppContext) {
    cx.update(guiraffe_code::init);
    let doc = Document::new(
        Node::new("code-viewer")
            .prop("source", "let x = 42;")
            .bind("source", "code")
            .prop("language", "rust")
            .prop("size", "sm")
            .prop("line-numbers", false)
            .prop("minimap", true)
            .prop("margin-chars", 20)
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

#[gpui_kit::test]
fn tree_documents_validate_and_render_property_grid_and_tree_list(cx: &mut TestAppContext) {
    cx.update(guiraffe_tree::init);
    let nodes = serde_json::json!([
        {"id":11,"label":"Group","height":40},
        {"id":12,"depth":1,"label":"Width","checked":true,"badge":"Px","icon":"#",
         "property":{"type":"number","value":24,"default":16,"min":1,"max":100,"step":1,"units":"px"}}
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
