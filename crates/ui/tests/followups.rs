use gpui_kit::SharedString;
use guiraffe_ui::*;
use std::sync::Arc;

#[test]
fn splice_is_bounded_atomic_and_change_checked() {
    let mut table = SlotTable::with_defaults(&[Value::List(
        vec![1i64.into(), 2i64.into(), 3i64.into()].into(),
    )]);
    let mut out = PatchBuf::default();
    out.splice(SlotId(0), 1..2, Arc::from([Value::from(9i64)]));
    assert_eq!(table.apply(&mut out), 1);
    assert_eq!(table.get(SlotId(0)).list_get(1), Some(&Value::from(9i64)));
    let version = table.version(SlotId(0));
    out.splice(SlotId(0), 1..2, Arc::from([Value::from(9i64)]));
    out.splice(SlotId(0), 4..5, Arc::from([Value::Null]));
    assert_eq!(table.apply(&mut out), 0);
    assert_eq!(table.version(SlotId(0)), version);
    out.splice(SlotId(0), 0..3, Arc::from([]));
    assert_eq!(table.apply(&mut out), 1);
    assert_eq!(table.get(SlotId(0)).list_len(), Some(0));
}

#[test]
fn compile_resolves_repeat_condition_item_paths_and_implied_events() {
    let doc = Document::from_json(r#"{"version":1,"root":{"kind":"stack","each":"todos","key":"id","when":"visible","children":[{"kind":"text","props":{"text":"@item.title","size":"sm","color":"blue"}},{"kind":"checkbox","bind":{"value":"done"}}]}}"#).expect("document");
    let t = Template::compile(&doc, &Registry::with_builtins()).expect("compile");
    assert!(t.diagnostics().is_empty(), "{:?}", t.diagnostics());
    assert!(t.names().slot("todos").is_some());
    assert!(t.names().slot("visible").is_some());
    assert!(t.names().slot("item.title").is_none());
    assert!(t.names().event("done.change").is_some());
    assert!(matches!(
        t.node(1).expect("text").props.get(2),
        Some(Prop::Size(guiraffe_core::Size::Sm))
    ));
}

#[test]
fn typed_bindings_report_missing_names_and_stay_inert() {
    let t = Template::compile(
        &Document::new(Node::new("text").bind("text", "title")),
        &Registry::with_builtins(),
    )
    .expect("compile");
    let mut b = Bindings::new(t.names());
    let title: Slot<SharedString> = b.slot("title");
    let missing: Slot<bool> = b.slot("missing");
    let save = b.event("save");
    assert!(title.is_resolved());
    assert!(!missing.is_resolved());
    assert!(!save.is_resolved());
    assert_eq!(b.diagnostics().len(), 2);
    let mut table = SlotTable::with_len(1);
    let mut out = PatchBuf::default();
    out.set(title, "hello");
    out.set(missing, true);
    table.apply(&mut out);
    assert_eq!(title.get(&table).as_deref(), Some("hello"));
    assert_eq!(missing.get(&table), None);
}

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div,
};
struct Scene {
    view: gpui_kit::AnyView,
}
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}
fn mount(
    cx: &mut TestAppContext,
    doc: Document,
    logic: impl Logic,
) -> (Entity<DataUi>, DataUiHandle, &mut VisualTestContext) {
    mount_impl(cx, doc, logic, false)
}
fn mount_impl(
    cx: &mut TestAppContext,
    doc: Document,
    logic: impl Logic,
    native_root: bool,
) -> (Entity<DataUi>, DataUiHandle, &mut VisualTestContext) {
    cx.update(guiraffe_components::init);
    let mut mounted = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let ui = DataUi::new(&doc, Arc::new(Registry::with_builtins()), logic, cx).expect("host");
        mounted = Some(ui.clone());
        let view: gpui_kit::AnyView = if native_root {
            cx.new(|cx| gpui_kit::component::Root::new(ui, window, cx))
                .into()
        } else {
            ui.into()
        };
        Scene { view }
    });
    vcx.run_until_parked();
    let ui = mounted.expect("mounted");
    let handle = vcx.update(|_, cx| ui.read(cx).handle());
    (ui, handle, vcx)
}
fn item(id: &str, title: &str) -> Value {
    Value::item_from_json(&serde_json::json!({"id":id,"title":title})).expect("item")
}
struct ListLogic;
impl Logic for ListLogic {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        if let Some(slot) = names.slot("todos") {
            out.set(
                slot,
                Value::List(
                    vec![item("a", "Alpha"), item("b", "Bravo"), item("c", "Charlie")].into(),
                ),
            );
        }
        if let Some(slot) = names.slot("visible") {
            out.set(slot, true);
        }
    }
}
#[gpui_kit::test]
fn splice_retains_keyed_entities_and_unrelated_renders(cx: &mut TestAppContext) {
    let d = Document::new(
        Node::new("stack")
            .each("todos")
            .key("id")
            .child(Node::new("text").prop("text", "@item.title")),
    );
    let (ui, h, vcx) = mount(cx, d, ListLogic);
    let before_a = h.item_metrics(0, "a").expect("a");
    let before_b = h.item_metrics(0, "b").expect("b");
    let before_c = h.item_metrics(0, "c").expect("c");
    let root = vcx.update(|_, cx| ui.read(cx).node_renders(0));
    vcx.update(|_, cx| {
        h.patch(
            |names, out| {
                out.splice(
                    names.slot("todos").expect("slot"),
                    1..2,
                    Arc::from([item("b", "BRAVO")]),
                )
            },
            cx,
        )
    });
    vcx.run_until_parked();
    let after_a = h.item_metrics(0, "a").expect("a");
    let after_b = h.item_metrics(0, "b").expect("b");
    let after_c = h.item_metrics(0, "c").expect("c");
    assert_eq!(before_a.0, after_a.0);
    assert_eq!(before_b.0, after_b.0);
    assert_eq!(before_c.0, after_c.0);
    assert!(after_b.1 > before_b.1);
    if gpui_kit::GPUI_FAST {
        assert_eq!(before_a, after_a, "unchanged first row");
        assert_eq!(before_c, after_c, "unchanged last row");
    }
    vcx.update(|_, cx| {
        h.patch(
            |names, out| {
                out.splice(
                    names.slot("todos").expect("slot"),
                    1..2,
                    Arc::from([item("b", "BRAVO")]),
                )
            },
            cx,
        )
    });
    vcx.run_until_parked();
    assert_eq!(after_b, h.item_metrics(0, "b").expect("no-op"));
    assert!(vcx.update(|_, cx| ui.read(cx).node_renders(0)) >= root);
    assert_eq!(vcx.update(|_, cx| ui.read(cx).host_refreshes()), 0);
}
#[gpui_kit::test]
fn when_switches_visibility(cx: &mut TestAppContext) {
    let d = Document::new(Node::new("stack").child(Node {
        id: Some("conditional".into()),
        ..Node::new("button").when("visible").prop("label", "Here")
    }));
    let (ui, h, vcx) = mount(cx, d, ListLogic);
    assert!(vcx.debug_bounds("conditional").is_some());
    vcx.update(|_, cx| {
        h.patch(
            |names, out| out.set(names.slot("visible").expect("slot"), false),
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("conditional").is_none());
    vcx.update(|_, cx| {
        h.patch(
            |names, out| out.set(names.slot("visible").expect("slot"), true),
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("conditional").is_some());
    assert_eq!(vcx.update(|_, cx| ui.read(cx).host_refreshes()), 0);
}
struct InputLogic {
    seen: std::rc::Rc<std::cell::RefCell<Vec<Payload>>>,
    veto: bool,
}
impl Logic for InputLogic {
    fn bind(&mut self, _: &Names, _: &mut PatchBuf) {}
    fn on_payload(&mut self, _: EventId, payload: &Payload, out: &mut PatchBuf) {
        self.seen.borrow_mut().push(payload.clone());
        if self.veto {
            out.veto();
        }
    }
}
#[gpui_kit::test]
fn native_checkbox_implied_change_optimistically_updates_and_can_veto(cx: &mut TestAppContext) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let d = Document::new(Node {
        id: Some("toggle".into()),
        ..Node::new("checkbox")
            .prop("label", "Enabled")
            .prop("value", false)
            .bind("value", "enabled")
    });
    let (ui, h, vcx) = mount(
        cx,
        d,
        InputLogic {
            seen: seen.clone(),
            veto: false,
        },
    );
    let center = vcx.debug_bounds("toggle").expect("checkbox").center();
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(*seen.borrow(), vec![Payload::Bool(true)]);
    assert_eq!(h.slot_value("enabled"), Some(Value::Bool(true)));
    assert_eq!(vcx.update(|_, cx| ui.read(cx).host_refreshes()), 0);
}

#[gpui_kit::test]
fn checkbox_veto_and_switch_disabled_guard(cx: &mut TestAppContext) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let d = Document::new(
        Node::new("stack")
            .child(Node {
                id: Some("toggle".into()),
                ..Node::new("checkbox")
                    .prop("label", "Try")
                    .prop("value", false)
                    .bind("value", "enabled")
            })
            .child(Node {
                id: Some("disabled".into()),
                ..Node::new("switch")
                    .prop("label", "Disabled")
                    .prop("disabled", true)
                    .bind("value", "other")
            }),
    );
    let (ui, h, vcx) = mount(
        cx,
        d,
        InputLogic {
            seen: seen.clone(),
            veto: true,
        },
    );
    let center = vcx.debug_bounds("toggle").expect("control").center();
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(h.slot_value("enabled"), Some(Value::Bool(false)));
    assert_eq!(*seen.borrow(), vec![Payload::Bool(true)]);
    let center = vcx.debug_bounds("disabled").expect("control").center();
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(vcx.update(|_, cx| ui.read(cx).host_refreshes()), 0);
}
struct EditLogic {
    title: Slot<SharedString>,
    seen: std::rc::Rc<std::cell::RefCell<Vec<Payload>>>,
}
impl Logic for EditLogic {
    fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
        self.title = Bindings::new(names).slot("title");
    }
    fn on_payload(&mut self, _: EventId, payload: &Payload, out: &mut PatchBuf) {
        self.seen.borrow_mut().push(payload.clone());
        if let Payload::Text(text) = payload {
            if text.contains('!') {
                out.veto();
            } else {
                out.set(self.title, text.to_uppercase());
            }
        }
    }
}
#[gpui_kit::test]
fn text_input_normalises_and_veto_restores_native_state(cx: &mut TestAppContext) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let d = Document::new(Node {
        id: Some("edit".into()),
        ..Node::new("text-input")
            .prop("value", "")
            .bind("value", "title")
    });
    let (_, h, vcx) = mount(
        cx,
        d,
        EditLogic {
            title: Slot::default(),
            seen: seen.clone(),
        },
    );
    let center = vcx.debug_bounds("edit").expect("control").center();
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.simulate_input("hello");
    vcx.run_until_parked();
    assert_eq!(h.slot_value("title"), Some(Value::from("HELLO")));
    assert_eq!(
        vcx.update(|_, cx| h.native_input_value(0, cx)),
        Some(Value::from("HELLO"))
    );
    vcx.simulate_input("!");
    vcx.run_until_parked();
    assert_eq!(h.slot_value("title"), Some(Value::from("HELLO")));
    assert_eq!(
        vcx.update(|_, cx| h.native_input_value(0, cx)),
        Some(Value::from("HELLO")),
        "veto repairs the native editor even though the final prop is unchanged"
    );
    assert_eq!(
        seen.borrow().len(),
        6,
        "simulated input emits one change per character"
    );
}

#[gpui_kit::test]
fn select_uses_native_popup_and_returns_selection_payload(cx: &mut TestAppContext) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let d = Document::new(Node {
        id: Some("choose".into()),
        ..Node::new("select")
            .prop("options", serde_json::json!(["Alpha", "Bravo", "Charlie"]))
            .prop("value", "Alpha")
            .bind("value", "selected")
    });
    let (_, h, vcx) = mount_impl(
        cx,
        d,
        InputLogic {
            seen: seen.clone(),
            veto: false,
        },
        true,
    );
    let center = vcx.debug_bounds("choose").expect("select").center();
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down enter");
    vcx.run_until_parked();
    assert_eq!(h.slot_value("selected"), Some(Value::from("Bravo")));
    assert_eq!(
        *seen.borrow(),
        vec![Payload::Selection(Some("Bravo".into()))]
    );
    assert_eq!(
        vcx.update(|_, cx| h.native_input_value(0, cx)),
        Some(Value::from("Bravo"))
    );
}
#[gpui_kit::test]
fn switch_change_updates_bound_value(cx: &mut TestAppContext) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let d = Document::new(Node {
        id: Some("switch".into()),
        ..Node::new("switch")
            .prop("label", "Switch")
            .prop("value", false)
            .bind("value", "enabled")
    });
    let (_, h, vcx) = mount(
        cx,
        d,
        InputLogic {
            seen: seen.clone(),
            veto: false,
        },
    );
    let center = vcx.debug_bounds("switch").expect("switch").center();
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(h.slot_value("enabled"), Some(Value::Bool(true)));
    assert_eq!(*seen.borrow(), vec![Payload::Bool(true)]);
}
#[gpui_kit::test]
fn structural_splices_preserve_identity_and_unmoved_row_views(cx: &mut TestAppContext) {
    let d = Document::new(
        Node::new("stack")
            .each("todos")
            .child(Node::new("text").prop("text", "@item.title")),
    );
    let (_, h, vcx) = mount(cx, d, ListLogic);
    let a = h.item_metrics(0, "a").expect("a");
    let c = h.item_metrics(0, "c").expect("c");
    vcx.update(|_, cx| {
        h.patch(
            |names, out| {
                out.splice(
                    names.slot("todos").expect("slot"),
                    1..1,
                    Arc::from([item("d", "Delta")]),
                )
            },
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(h.item_metrics(0, "d").is_some());
    assert_eq!(h.item_metrics(0, "a").expect("a").0, a.0);
    assert_eq!(h.item_metrics(0, "c").expect("c").0, c.0);
    if gpui_kit::GPUI_FAST {
        assert_eq!(h.item_metrics(0, "a"), Some(a));
        assert!(h.item_metrics(0, "c").expect("moved row").1 >= c.1);
    }
    vcx.update(|_, cx| {
        h.patch(
            |names, out| {
                out.set(
                    names.slot("todos").expect("slot"),
                    Value::List(vec![item("c", "Charlie"), item("a", "Alpha")].into()),
                )
            },
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(h.item_metrics(0, "d").is_none());
    assert_eq!(h.item_metrics(0, "a").expect("a").0, a.0);
    assert_eq!(h.item_metrics(0, "c").expect("c").0, c.0);
    // Reordering changes absolute row bounds. GPUI may re-layout/re-render those
    // views, while keyed identity (and its native input state) remains stable.
}

#[test]
fn compiled_style_recipes_and_bound_caches_stay_typed() {
    let doc = Document::new(
        Node::new("text")
            .prop("text", "@title")
            .prop("size", "sm")
            .prop("color", "red"),
    );
    let template = Template::compile(&doc, &Registry::with_builtins()).expect("compile");
    let node = template.node(0).expect("root");
    assert!(matches!(node.props.first(), Some(Prop::Slot(_))));
    assert!(matches!(
        node.props.get(2),
        Some(Prop::Size(guiraffe_core::Size::Sm))
    ));
    assert!(matches!(
        node.props.get(3),
        Some(Prop::Color(guiraffe_core::ColorName::Red))
    ));
    let mut table = SlotTable::with_defaults(&[Value::from("sm")]);
    assert_eq!(table.size(SlotId(0)), Some(guiraffe_core::Size::Sm));
    let mut out = PatchBuf::default();
    out.set(SlotId(0), "blue");
    table.apply(&mut out);
    assert_eq!(table.size(SlotId(0)), None);
    assert_eq!(table.color(SlotId(0)), Some(guiraffe_core::ColorName::Blue));
    let invalid = Template::compile(
        &Document::new(
            Node::new("text")
                .prop("size", "enormous")
                .prop("color", "not-a-color"),
        ),
        &Registry::with_builtins(),
    )
    .expect("compile");
    assert_eq!(invalid.diagnostics().len(), 2);
}
#[test]
fn splices_cover_insert_delete_sequential_ranges_and_unknown_slots() {
    let mut table = SlotTable::with_defaults(&[
        Value::List(vec![1i64.into(), 2i64.into(), 3i64.into()].into()),
        Value::Bool(true),
    ]);
    let mut out = PatchBuf::default();
    out.splice(
        SlotId(0),
        1..1,
        Arc::from([Value::from(4i64), Value::from(5i64)]),
    );
    out.splice(SlotId(0), 3..4, Arc::from([]));
    out.splice(SlotId(1), 0..0, Arc::from([Value::Null]));
    out.splice(SlotId(u32::MAX), 0..0, Arc::from([Value::Null]));
    assert_eq!(table.apply(&mut out), 2);
    assert_eq!(
        (0..4)
            .filter_map(|i| table.get(SlotId(0)).list_get(i).and_then(Value::as_f64))
            .collect::<Vec<_>>(),
        vec![1., 4., 5., 3.]
    );
    let before = table.clone();
    out.splice(SlotId(0), 0..0, Arc::from([]));
    assert_eq!(table.apply(&mut out), 0);
    assert_eq!(before.get(SlotId(0)), table.get(SlotId(0)));
}
