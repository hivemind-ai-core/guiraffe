//! Controls built without a window (`build_later`): the path is reserved at build, the
//! control registers in the window it first renders in and follows it to another, and
//! until then the bus answers `NotMounted`.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
    VisualTestContext, Window, div,
};
use guiraffe_components::controls::*;
use guiraffe_core::protocol::{ActionBus, ActionError, ControlId, names};
use guiraffe_test::alloc::count_allocations;
use guiraffe_test::drive::Driver;
use guiraffe_test::inspect::EventSink;
use guiraffe_test::transport::{Call, Server, mcp};

const PATH: &str = "settings/dark";

/// A window that renders `control` once `shown`.
struct Host {
    control: AnyView,
    shown: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let host = div();
        if self.shown {
            host.child(self.control.clone())
        } else {
            host
        }
    }
}

fn host(
    cx: &mut TestAppContext,
    control: impl Into<AnyView>,
    shown: bool,
) -> (Entity<Host>, &mut VisualTestContext) {
    let control = control.into();
    let (host, vcx) = cx.add_window_view(move |_, _| Host { control, shown });
    vcx.run_until_parked();
    (host, vcx)
}

fn show(host: &Entity<Host>, shown: bool, vcx: &mut VisualTestContext) {
    vcx.update(|_, cx| {
        host.update(cx, |host, cx| {
            host.shown = shown;
            cx.notify();
        })
    });
    vcx.run_until_parked();
}

fn id(cx: &App) -> ControlId {
    ControlId::lookup(PATH, cx).expect("the path is interned when reserved")
}

#[gpui_kit::test]
fn a_control_built_without_a_window_registers_at_its_first_render(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let control = cx.update(|cx| {
        CheckboxControl::new()
            .label("Dark mode")
            .path(PATH)
            .build_later(cx)
    });
    let (host, vcx) = host(cx, control.clone(), false);

    // Reserved but not rendered yet: a clear answer, not an unknown path.
    vcx.update(|window, cx| {
        assert_eq!(
            ActionBus::query(id(cx), window, cx),
            Err(ActionError::NotMounted)
        );
    });

    show(&host, true, vcx);
    vcx.update(|window, cx| {
        let snapshot = ActionBus::query(id(cx), window, cx).expect("registered once rendered");
        assert_eq!(snapshot.label.as_deref(), Some("Dark mode"));
    });

    // Driven over MCP like any registered control.
    let events = EventSink::new();
    let mut server = Server::new(Driver::new(vcx, events));
    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "apply", "arguments": {
            "control": PATH, "action": { "name": names::TOGGLE, "payload": null } } }
    });
    let answer = mcp(&request.to_string(), &mut server).expect("an answer");
    assert!(!answer.contains("\"isError\":true"), "{answer}");
    let query = server.call(Call::Query {
        control: PATH.into(),
    });
    assert!(query.ok, "{query:?}");
    drop(server);
    assert!(vcx.update(|_, cx| control.read(cx).checked()));
}

#[gpui_kit::test]
fn a_deferred_control_follows_its_view_to_another_window(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let control = cx.update(|cx| CheckboxControl::new().path(PATH).build_later(cx));
    let (first, first_cx) = host(cx, control.clone(), true);
    let first_window = first_cx.update(|window, _| window.window_handle());
    first_cx.update(|window, cx| assert!(ActionBus::query(id(cx), window, cx).is_ok()));
    show(&first, false, first_cx);

    let (_second, second_cx) = host(cx, control.clone(), true);
    second_cx.update(|window, cx| assert!(ActionBus::query(id(cx), window, cx).is_ok()));
    cx.update_window(first_window, |_, window, cx| {
        assert_eq!(
            ActionBus::query(id(cx), window, cx),
            Err(ActionError::NotMounted),
            "the registration moved: the path is reserved, but not rendered here"
        );
    })
    .expect("live window");
}

#[gpui_kit::test]
fn a_mounted_deferred_control_redraws_as_a_window_built_one(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    // Once registered, a frame costs what it costs a control built with a window.
    let redraw = |vcx: &mut VisualTestContext, control: &Entity<CheckboxControl>| {
        let mut counts: Vec<u64> = (0..12)
            .map(|_| {
                count_allocations(|| {
                    vcx.update(|_, cx| control.update(cx, |_, cx| cx.notify()));
                    vcx.run_until_parked();
                })
                .1
                .allocations
            })
            .collect();
        counts.sort_unstable();
        counts[counts.len() / 2]
    };
    let later = cx.update(|cx| CheckboxControl::new().path(PATH).build_later(cx));
    let (_host, vcx) = host(cx, later.clone(), true);
    let deferred = redraw(vcx, &later);
    let (built_host, built_cx) = cx.add_window_view(|window, cx| Host {
        control: CheckboxControl::new()
            .path("settings/built")
            .build(window, cx)
            .into(),
        shown: true,
    });
    built_cx.run_until_parked();
    let built = built_cx.update(|_, cx| {
        built_host
            .read(cx)
            .control
            .clone()
            .downcast::<CheckboxControl>()
            .expect("a checkbox")
    });
    let other_cx = built_cx;
    let direct = redraw(other_cx, &built);
    assert!(
        deferred <= direct,
        "a mounted deferred control allocates {deferred} per frame, a window-built one {direct}"
    );
}

#[cfg(debug_assertions)]
#[gpui_kit::test]
#[should_panic(expected = "duplicate control")]
fn a_path_reserved_twice_is_rejected_in_debug_builds(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    cx.update(|cx| {
        let _first = CheckboxControl::new().path(PATH).build_later(cx);
        let _second = SwitchControl::new().path(PATH).build_later(cx);
    });
}

#[gpui_kit::test]
fn every_control_builds_without_a_window(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let views: Vec<(&str, AnyView)> = cx.update(|cx| {
        vec![
            (
                "c/button",
                ButtonControl::new().path("c/button").build_later(cx).into(),
            ),
            (
                "c/checkbox",
                CheckboxControl::new()
                    .path("c/checkbox")
                    .build_later(cx)
                    .into(),
            ),
            (
                "c/switch",
                SwitchControl::new().path("c/switch").build_later(cx).into(),
            ),
            (
                "c/chip",
                ToggleChipControl::new()
                    .label("M")
                    .path("c/chip")
                    .build_later(cx)
                    .into(),
            ),
            (
                "c/slider",
                SliderControl::new().path("c/slider").build_later(cx).into(),
            ),
            (
                "c/text",
                TextInputControl::new()
                    .value("Ada")
                    .path("c/text")
                    .build_later(cx)
                    .into(),
            ),
            (
                "c/select",
                SelectControl::new()
                    .options(["One", "Two"])
                    .path("c/select")
                    .build_later(cx)
                    .into(),
            ),
            (
                "c/tabs",
                TabsControl::new()
                    .tabs(["A", "B"])
                    .path("c/tabs")
                    .build_later(cx)
                    .into(),
            ),
            (
                "c/menu",
                MenuControl::new()
                    .item("Open")
                    .path("c/menu")
                    .build_later(cx)
                    .into(),
            ),
            (
                "c/knob",
                KnobControl::new().path("c/knob").build_later(cx).into(),
            ),
            (
                "c/number",
                NumberFieldControl::new()
                    .path("c/number")
                    .build_later(cx)
                    .into(),
            ),
        ]
    });
    let (_host, vcx) =
        cx.add_window_view(move |_, _| Panel(views.iter().map(|(_, v)| v.clone()).collect()));
    vcx.run_until_parked();
    for path in [
        "c/button",
        "c/checkbox",
        "c/switch",
        "c/chip",
        "c/slider",
        "c/text",
        "c/select",
        "c/tabs",
        "c/menu",
        "c/knob",
        "c/number",
    ] {
        vcx.update(|window, cx| {
            let id = ControlId::lookup(path, cx).expect("reserved");
            assert!(ActionBus::query(id, window, cx).is_ok(), "{path}");
        });
    }
}

struct Panel(Vec<AnyView>);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().children(self.0.iter().cloned())
    }
}
