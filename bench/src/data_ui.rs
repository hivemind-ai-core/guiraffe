//! Native host workloads. TLS fixture setup and initial rendering are warmed before timing.
//! Instantiation creates a real DataUi entity from a compiled template (lazy first render
//! excluded). Patch/splice exercise mounted hosts and targeted invalidation, excluding draw.
use gpui_kit::{Entity, HeadlessAppContext, WindowHandle, px, size};
use guiraffe_ui::{
    DataUi, DataUiHandle, Document, Logic, Names, NoLogic, Node, PatchBuf, Registry, Template,
    Value,
};
use std::{cell::RefCell, hint::black_box, sync::Arc};
struct Initial;
impl Logic for Initial {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        if let Some(slot) = names.slot("items") {
            out.set(
                slot,
                Value::List(
                    (0..10_000)
                        .map(|i| {
                            Value::Record(Arc::new(
                                [
                                    ("id".into(), Value::Str(format!("item-{i}").into())),
                                    ("title".into(), Value::from("row")),
                                ]
                                .into_iter()
                                .collect(),
                            ))
                        })
                        .collect(),
                ),
            );
        }
        if let Some(slot) = names.slot("tone") {
            out.set(slot, "blue");
        }
    }
}
struct Fixture {
    cx: HeadlessAppContext,
    registry: Arc<Registry>,
    template: Arc<Template>,
    handle: DataUiHandle,
    _window: WindowHandle<DataUi>,
    _host: Entity<DataUi>,
    toggle: bool,
    replacements: [Arc<[Value]>; 2],
}
impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let registry = Arc::new(Registry::with_builtins());
        let doc = Document::new(
            Node::new("stack")
                .child(
                    Node::new("text")
                        .prop("text", "Bound")
                        .bind("color", "tone"),
                )
                .child(
                    Node::new("stack").each("items").child(
                        Node::new("text")
                            .prop("text", "@item.title")
                            .prop("single-line", true),
                    ),
                ),
        );
        let template = Arc::new(Template::compile(&doc, &registry)?);
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(guiraffe_components::init);
        let mut host = None;
        let window = cx.open_window(
            size(
                px(guiraffe_core::Size::Md.container_width()),
                px(guiraffe_core::Size::Md.control_height() * 16.),
            ),
            |_, cx| {
                let ui = DataUi::from_template(template.clone(), registry.clone(), Initial, cx);
                host = Some(ui.clone());
                ui
            },
        );
        let window = window?;
        for _ in 0..2 {
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })?;
            cx.run_until_parked();
        }
        let host = host.ok_or_else(|| anyhow::anyhow!("benchmark host was not instantiated"))?;
        let handle = cx.update(|cx| host.read(cx).handle());
        let replacements = ["A", "B"].map(|title| {
            Arc::from([Value::Record(Arc::new(
                [
                    ("id".into(), Value::from("item-5000")),
                    ("title".into(), Value::from(title)),
                ]
                .into_iter()
                .collect(),
            ))])
        });
        Ok(Self {
            cx,
            registry,
            template,
            handle,
            _window: window,
            _host: host,
            toggle: false,
            replacements,
        })
    }
}
thread_local! { static FIXTURE: RefCell<Fixture> = RefCell::new(match Fixture::new() { Ok(fixture)=>fixture, Err(error)=>panic!("data UI benchmark setup failed: {error:#}") }); }
pub fn instantiate() {
    FIXTURE.with(|fixture| {
        let mut fixture = fixture.borrow_mut();
        let template = fixture.template.clone();
        let registry = fixture.registry.clone();
        fixture.cx.update(|cx| {
            black_box(DataUi::from_template(template, registry, NoLogic, cx));
        });
    });
}
pub fn patch() {
    FIXTURE.with(|fixture| {
        let mut fixture = fixture.borrow_mut();
        fixture.toggle = !fixture.toggle;
        let color = if fixture.toggle { "blue" } else { "red" };
        let handle = fixture.handle.clone();
        fixture.cx.update(|cx| {
            handle.patch(
                |names, out| {
                    if let Some(slot) = names.slot("tone") {
                        out.set(slot, color);
                    }
                },
                cx,
            )
        });
    });
}
pub fn splice() {
    FIXTURE.with(|fixture| {
        let mut fixture = fixture.borrow_mut();
        fixture.toggle = !fixture.toggle;
        let values = fixture.replacements[usize::from(fixture.toggle)].clone();
        let handle = fixture.handle.clone();
        fixture.cx.update(|cx| {
            handle.patch(
                |names, out| {
                    if let Some(slot) = names.slot("items") {
                        out.splice(slot, 5000..5001, values);
                    }
                },
                cx,
            )
        });
    });
}
