use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext,
    Window, div, point, px,
};
use guiraffe_ui::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};
struct Scene {
    ui: Entity<DataUi>,
}
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.ui.clone())
    }
}
fn mount(
    cx: &mut TestAppContext,
    doc: Document,
    logic: impl Logic,
) -> (Entity<DataUi>, DataUiHandle, &mut VisualTestContext) {
    cx.update(guiraffe_components::init);
    let mut host = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let ui = DataUi::new(&doc, Arc::new(Registry::with_builtins()), logic, cx).expect("host");
        host = Some(ui.clone());
        Scene { ui }
    });
    vcx.run_until_parked();
    let ui = host.expect("mounted");
    let handle = vcx.update(|_, cx| ui.read(cx).handle());
    (ui, handle, vcx)
}
fn record(key: &str, title: &str) -> Value {
    Value::item_from_json(&serde_json::json!({"uuid":key,"title":title,"done":false}))
        .expect("record")
}
fn document() -> Document {
    Document::from_json(r#"{"version":1,"root":{"kind":"stack","each":"todos","key":"uuid","children":[{"kind":"stack","children":[{"kind":"text-input","id":"edit","bind":{"value":"@item.title"}},{"kind":"checkbox","id":"done","props":{"label":"Done"},"bind":{"value":"@item.done"}},{"kind":"button","id":"delete","props":{"label":"Delete"},"on":{"click":"delete"}}]}]}}"#).expect("document")
}
struct Todos {
    events: Rc<RefCell<Vec<Payload>>>,
    normalise: bool,
    delete: Event,
}
impl Logic for Todos {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        self.delete = Bindings::new(names).event("delete");
        out.set(
            names.slot("todos").expect("list"),
            Value::List(
                vec![
                    record("a", "Alpha"),
                    record("b", "Bravo"),
                    record("c", "Charlie"),
                ]
                .into(),
            ),
        );
    }
    fn on_payload(&mut self, event: EventId, payload: &Payload, out: &mut PatchBuf) {
        self.events.borrow_mut().push(payload.clone());
        let item = payload.item().expect("scoped event");
        if self.delete.matches(event) {
            out.splice(item.slot, item.index..item.index + 1, Arc::from([]));
        }
        if self.normalise
            && let Payload::Text(text) = payload.input()
        {
            out.item(
                payload
                    .item_change()
                    .expect("write-back")
                    .with_value(text.to_uppercase()),
            );
            if text.contains('!') {
                out.veto();
            }
        }
    }
}
fn click(vcx: &mut VisualTestContext, selector: &'static str, left: bool) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    let position = if left {
        point(bounds.left() + px(8.), bounds.center().y)
    } else {
        bounds.center()
    };
    vcx.simulate_click(position, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
}
fn rows(h: &DataUiHandle) -> Value {
    h.slot_value("todos").expect("rows")
}
fn field(h: &DataUiHandle, index: usize, name: &str) -> Value {
    rows(h)
        .list_get(index)
        .expect("row")
        .field(&[name.to_owned().into()])
        .clone()
}
#[gpui_kit::test]
fn per_row_checkbox_and_delete_target_only_the_live_item(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let (_, h, vcx) = mount(
        cx,
        document(),
        Todos {
            events: events.clone(),
            normalise: false,
            delete: Event::default(),
        },
    );
    click(vcx, "done[b]", true);
    assert_eq!(field(&h, 0, "done"), Value::Bool(false));
    assert_eq!(field(&h, 1, "done"), Value::Bool(true));
    assert_eq!(field(&h, 2, "done"), Value::Bool(false));
    let event = events.borrow().last().cloned().expect("change");
    let item = event.item().expect("item");
    assert_eq!(item.key, Value::from("b"));
    assert_eq!(item.index, 1);
    assert_eq!(event.input(), &Payload::Bool(true));
    click(vcx, "delete[a]", false);
    assert_eq!(rows(&h).list_len(), Some(2));
    assert_eq!(field(&h, 0, "uuid"), Value::from("b"));
    assert_eq!(field(&h, 0, "done"), Value::Bool(true));
    click(vcx, "done[b]", true);
    assert_eq!(field(&h, 0, "done"), Value::Bool(false));
    assert_eq!(
        events
            .borrow()
            .last()
            .and_then(Payload::item)
            .map(|item| item.index),
        Some(0)
    );
}
#[gpui_kit::test]
fn row_typing_normalisation_and_veto_persist_through_reorder(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let (ui, h, vcx) = mount(
        cx,
        document(),
        Todos {
            events: events.clone(),
            normalise: true,
            delete: Event::default(),
        },
    );
    let identity = h.item_metrics(0, "b").expect("custom key metrics").0;
    click(vcx, "edit[b]", false);
    vcx.simulate_input("x");
    vcx.run_until_parked();
    assert_eq!(field(&h, 1, "title"), Value::from("BRAVOX"));
    let old = rows(&h);
    let reordered = Value::List(
        [1, 0, 2]
            .into_iter()
            .filter_map(|i| old.list_get(i).cloned())
            .collect(),
    );
    vcx.update(|_, cx| {
        h.patch(
            |names, out| out.set(names.slot("todos").expect("slot"), reordered),
            cx,
        )
    });
    vcx.run_until_parked();
    assert_eq!(h.item_metrics(0, "b").expect("retained").0, identity);
    vcx.simulate_input("y");
    vcx.run_until_parked();
    assert_eq!(field(&h, 0, "title"), Value::from("BRAVOXY"));
    assert_eq!(field(&h, 1, "title"), Value::from("Alpha"));
    assert_eq!(
        events
            .borrow()
            .last()
            .and_then(Payload::item)
            .map(|item| item.index),
        Some(0)
    );
    vcx.simulate_input("!");
    vcx.run_until_parked();
    assert_eq!(
        field(&h, 0, "title"),
        Value::from("BRAVOXY"),
        "veto discards normalization"
    );
    let edit = vcx.update(|_, cx| ui.read(cx).template().find("edit").expect("edit"));
    assert_eq!(
        vcx.update(|_, cx| h.native_input_value_in(edit, identity, cx)),
        Some(Value::from("BRAVOXY")),
        "native editor also rolled back"
    );
}
#[gpui_kit::test]
fn unknown_input_event_is_inert_before_optimistic_write(cx: &mut TestAppContext) {
    let d = Document::new(
        Node::new("checkbox")
            .prop("value", false)
            .bind("value", "enabled"),
    );
    let (ui, h, vcx) = mount(cx, d, NoLogic);
    let before = vcx.update(|_, cx| ui.read(cx).host_renders());
    assert!(before > 0, "the real host was rendered");
    vcx.update(|_, cx| {
        h.dispatch_input_for_test(
            EventId(u32::MAX),
            ui.read(cx)
                .template()
                .names()
                .slot("enabled")
                .expect("slot"),
            Payload::Bool(true),
            cx,
        )
    });
    vcx.run_until_parked();
    assert_eq!(h.slot_value("enabled"), Some(Value::Bool(false)));
    assert_eq!(vcx.update(|_, cx| ui.read(cx).events_handled()), 0);
    assert_eq!(vcx.update(|_, cx| ui.read(cx).host_renders()), before);
}
#[gpui_kit::test]
fn a_layout_neutral_event_does_not_write_the_host(cx: &mut TestAppContext) {
    let d = Document::new(
        Node::new("stack")
            .child(Node::new("text").prop("text", "Sibling"))
            .child(
                Node::new("checkbox")
                    .prop("value", false)
                    .bind("value", "enabled"),
            ),
    );
    let (ui, h, vcx) = mount(cx, d, NoLogic);
    let before = vcx.update(|_, cx| ui.read(cx).host_renders());
    assert!(before > 0, "the real host was rendered");
    let names = vcx.update(|_, cx| ui.read(cx).template());
    let event = names.names().event("enabled.change").expect("event");
    let slot = names.names().slot("enabled").expect("slot");
    vcx.update(|_, cx| h.dispatch_input_for_test(event, slot, Payload::Bool(true), cx));
    vcx.run_until_parked();
    assert_eq!(h.slot_value("enabled"), Some(Value::Bool(true)));
    let after = vcx.update(|_, cx| ui.read(cx).host_renders());
    if gpui_kit::GPUI_FAST {
        assert_eq!(after, before, "real host render count");
    }
    vcx.update(|_, cx| h.dispatch_input_for_test(event, slot, Payload::Bool(true), cx));
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| ui.read(cx).host_renders()),
        after,
        "no-op event leaves host retained on both backends"
    );
}

#[test]
fn pure_item_writes_are_guarded_and_preserve_snapshots() {
    let original = record("a", "Alpha");
    let mut table = SlotTable::with_defaults(&[Value::List(Arc::from([original.clone()]))]);
    let snapshot = table.clone();
    let context = ItemContext {
        slot: SlotId(0),
        key: Value::from("a"),
        key_path: Arc::from(["uuid".into()]),
        index: 0,
        identity: 17,
        repeat: 0,
        parent: None,
    };
    let mut out = PatchBuf::default();
    let change = ItemChange {
        item: context.clone(),
        path: Arc::from(["title".into()]),
        value: Value::from("Updated"),
    };
    out.item(change.clone());
    assert_eq!(table.apply(&mut out), 1);
    assert_eq!(
        table
            .get(SlotId(0))
            .list_get(0)
            .expect("row")
            .field(&["title".into()]),
        &Value::from("Updated")
    );
    assert_eq!(snapshot.get(SlotId(0)).list_get(0), Some(&original));
    out.item(change);
    assert_eq!(table.apply(&mut out), 0);
    let invalid = ItemChange {
        item: ItemContext {
            key: Value::from("other"),
            ..context
        },
        path: Arc::from(["title".into()]),
        value: Value::from("wrong row"),
    };
    out.item(invalid);
    assert_eq!(table.apply(&mut out), 0);
}
#[test]
fn virtualisation_requires_a_provably_equal_height_template() {
    let compile = |child: Node| {
        Template::compile(
            &Document::new(Node::new("stack").each("rows").child(child)),
            &Registry::with_builtins(),
        )
        .expect("compile")
    };
    assert!(
        !compile(Node::new("text").bind("text", "@item.title"))
            .node(0)
            .expect("root")
            .uniform_rows
    );
    assert!(
        compile(
            Node::new("text")
                .prop("single-line", true)
                .bind("text", "@item.title")
        )
        .node(0)
        .expect("root")
        .uniform_rows
    );
    assert!(
        !compile(Node::new("card").child(Node::new("text").prop("text", "multi\nline")))
            .node(0)
            .expect("root")
            .uniform_rows
    );
    assert!(
        compile(Node::new("text-input").bind("value", "@item.title"))
            .node(0)
            .expect("root")
            .uniform_rows
    );
}
struct LayoutLogic {
    count: usize,
}
impl Logic for LayoutLogic {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        out.set(
            names.slot("rows").expect("rows"),
            Value::List(
                (0..self.count)
                    .map(|i| record(&i.to_string(), "Line one\nLine two\nLine three"))
                    .collect(),
            ),
        );
    }
}
fn row_layout(single: bool) -> Document {
    let text = Node::new("text")
        .bind("text", "@item.title")
        .prop("single-line", single);
    Document::new(
        Node::new("stack")
            .child(Node {
                id: Some("list-box".into()),
                ..Node::new("card").each("rows").key("uuid").child(text)
            })
            .child(Node {
                id: Some("after-list".into()),
                ..Node::new("button").prop("label", "After")
            }),
    )
}
#[gpui_kit::test]
fn short_lists_use_content_height_and_complex_rows_are_not_clipped(cx: &mut TestAppContext) {
    let (_, _, vcx) = mount(cx, row_layout(false), LayoutLogic { count: 2 });
    let full = vcx.debug_bounds("after-list").expect("after").top();
    // Two multiline rows must consume more than two single-line row heights.
    assert!(full > px(guiraffe_core::Size::Xl.control_height() * 2.));
    assert!(
        full < px(guiraffe_core::Size::Xl.control_height() * 8.),
        "no fixed eight-row viewport for a short list"
    );
}
#[gpui_kit::test]
fn short_equal_height_list_has_no_eight_row_minimum(cx: &mut TestAppContext) {
    let (_, _, vcx) = mount(cx, row_layout(true), LayoutLogic { count: 1 });
    let top = vcx.debug_bounds("after-list").expect("after").top();
    assert!(top < px(guiraffe_core::Size::Xl.control_height() * 3.));
}

#[gpui_kit::test]
fn a_deleted_items_delayed_field_patch_cannot_write_its_successor(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let (_, h, vcx) = mount(
        cx,
        document(),
        Todos {
            events: events.clone(),
            normalise: false,
            delete: Event::default(),
        },
    );
    click(vcx, "done[b]", true);
    let delayed = events
        .borrow()
        .last()
        .and_then(Payload::item_change)
        .expect("change");
    click(vcx, "delete[b]", false);
    let before = rows(&h);
    vcx.update(|_, cx| h.patch(|_, out| out.item(delayed.with_value(false)), cx));
    vcx.run_until_parked();
    assert_eq!(rows(&h), before);
    assert_eq!(field(&h, 1, "uuid"), Value::from("c"));
}
struct DeleteThenNormalise;
impl Logic for DeleteThenNormalise {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        Todos {
            events: Rc::default(),
            normalise: false,
            delete: Event::default(),
        }
        .bind(names, out);
    }
    fn on_payload(&mut self, _: EventId, payload: &Payload, out: &mut PatchBuf) {
        let item = payload.item().expect("item");
        assert_eq!(item.key, Value::from("b"));
        assert_eq!(item.index, 1);
        out.splice(item.slot, 0..1, Arc::from([]));
        out.item(
            payload
                .item_change()
                .expect("field")
                .with_value("NORMALISED"),
        );
    }
}
#[gpui_kit::test]
fn item_patches_rebase_after_preceding_structural_edits_in_the_same_response(
    cx: &mut TestAppContext,
) {
    let (_, h, vcx) = mount(cx, document(), DeleteThenNormalise);
    click(vcx, "edit[b]", false);
    vcx.simulate_input("x");
    vcx.run_until_parked();
    assert_eq!(rows(&h).list_len(), Some(2));
    assert_eq!(field(&h, 0, "uuid"), Value::from("b"));
    assert_eq!(field(&h, 0, "title"), Value::from("NORMALISED"));
    assert_eq!(field(&h, 1, "title"), Value::from("Charlie"));
}
#[test]
fn record_field_updates_preserve_other_fields_and_reject_invalid_intermediates() {
    let value =
        Value::item_from_json(&serde_json::json!({"id":"a","nested":{"label":"before","count":4}}))
            .expect("record");
    let changed = value
        .with_field(&["nested".into(), "label".into()], Value::from("after"))
        .expect("field");
    assert_eq!(
        changed.field(&["nested".into(), "label".into()]),
        &Value::from("after")
    );
    assert_eq!(
        changed.field(&["nested".into(), "count".into()]),
        &Value::from(4i64)
    );
    assert_eq!(
        value.field(&["nested".into(), "label".into()]),
        &Value::from("before")
    );
    assert!(
        value
            .with_field(&["id".into(), "label".into()], Value::Null)
            .is_none()
    );
}
#[test]
fn scoped_messages_do_not_inflate_common_scalar_payloads_or_patches() {
    assert!(
        std::mem::size_of::<Payload>()
            <= std::mem::size_of::<Value>() + 2 * std::mem::size_of::<usize>()
    );
    assert!(
        std::mem::size_of::<Patch>()
            <= std::mem::size_of::<Value>() + 4 * std::mem::size_of::<usize>()
    );
}
#[test]
fn overriding_a_builtin_name_does_not_inherit_its_uniform_height_proof() {
    static CUSTOM: Schema = Schema {
        props: &["text", "dimmed", "size", "color", "single-line"],
        events: &[],
    };
    let mut registry = Registry::with_builtins();
    registry.register(
        "text",
        widget(&CUSTOM, |_, _, _, _| div().h(px(500.)).into_any_element()),
    );
    let doc = Document::new(
        Node::new("stack")
            .each("rows")
            .child(Node::new("text").prop("single-line", true)),
    );
    let template = Template::compile(&doc, &registry).expect("template");
    assert!(!template.node(0).expect("root").uniform_rows);
}
#[gpui_kit::test]
fn custom_value_change_handlers_get_scoped_implied_writeback(cx: &mut TestAppContext) {
    use gpui_kit::InteractiveElement;
    static CUSTOM: Schema = Schema {
        props: &["value"],
        events: &["change"],
    };
    let mut registry = Registry::with_builtins();
    registry.register(
        "custom-toggle",
        widget(&CUSTOM, |node, _, _, _| {
            let value = !node.bool(0);
            let handler = node.handler(0);
            div()
                .debug_selector(|| "custom-change".to_owned())
                .child(
                    gpui_kit::component::button::Button::new(node.element_id())
                        .label("Toggle")
                        .on_click(move |_, window, cx| {
                            if let Some(handler) = &handler {
                                handler(Value::Bool(value), window, cx);
                            }
                        }),
                )
                .into_any_element()
        }),
    );
    let doc = Document::new(
        Node::new("stack")
            .each("rows")
            .key("uuid")
            .child(Node::new("custom-toggle").bind("value", "@item.done")),
    );
    cx.update(guiraffe_components::init);
    let mut host = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let ui = DataUi::new(&doc, Arc::new(registry), LayoutLogic { count: 1 }, cx).expect("host");
        host = Some(ui.clone());
        Scene { ui }
    });
    vcx.run_until_parked();
    let ui = host.expect("host");
    let h = vcx.update(|_, cx| ui.read(cx).handle());
    click(vcx, "custom-change", false);
    assert_eq!(
        h.slot_value("rows")
            .expect("rows")
            .list_get(0)
            .expect("row")
            .field(&["done".into()]),
        &Value::Bool(true)
    );
    assert_eq!(vcx.update(|_, cx| ui.read(cx).events_handled()), 1);
}
