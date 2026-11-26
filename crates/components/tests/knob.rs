//! Knob and number field: pointer and bus give the same values, a drag is one batch, typed
//! entry parses through the mapping, host writes never emit, and a log slider steps by travel.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AnyView, App, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement, Render,
    Styled, TestAppContext, VisualTestContext, Window, div, point, px,
};
use guiraffe_components::controls::*;
use guiraffe_components::mapping::{Curve, Unit, ValueMap, ValueMapping};
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, ControlId, Group, Interceptor, Outcome, Source, names,
};
use guiraffe_core::widget_value::Value;

struct Host(Vec<AnyView>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Each control in a 60 px column, so the first knob's dial sits at (20, 20).
        div().w(px(60.)).flex().flex_col().children(self.0.clone())
    }
}

#[derive(Default)]
struct Log {
    sources: RefCell<Vec<Source>>,
    groups: RefCell<usize>,
}

impl Interceptor for Log {
    fn before(
        &self,
        _: ControlId,
        _: &Action,
        source: Source,
        _: &Window,
        _: &App,
    ) -> Result<(), ActionError> {
        self.sources.borrow_mut().push(source);
        Ok(())
    }
    fn after(
        &self,
        _: ControlId,
        _: &Action,
        _: Source,
        _: &Result<Outcome, ActionError>,
        _: &Window,
        _: &App,
    ) {
    }
    fn begin_group(&self, _: Group, _: &Window, _: &App) {
        *self.groups.borrow_mut() += 1;
    }
}

fn cutoff() -> ValueMap {
    ValueMap::new(20.0, 20_000.0)
        .curve(Curve::Log)
        .unit(Unit::Hertz)
        .default_to(1_000.0)
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    Entity<KnobControl>,
    Entity<NumberFieldControl>,
    Rc<Log>,
    &mut VisualTestContext,
) {
    cx.update(guiraffe_components::init);
    let mut built = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let knob = KnobControl::new()
            .mapping(cutoff())
            .label("Cutoff")
            .path("synth/cutoff")
            .build(window, cx);
        let field = NumberFieldControl::new()
            .mapping(cutoff())
            .label("Cutoff")
            .path("synth/cutoff-field")
            .build(window, cx);
        built = Some((knob.clone(), field.clone()));
        Host(vec![knob.into(), field.into()])
    });
    let (knob, field) = built.expect("built");
    let log = Rc::new(Log::default());
    vcx.update(|window, cx| ActionBus::add_interceptor(log.clone(), window, cx));
    vcx.run_until_parked();
    (knob, field, log, vcx)
}

fn events<T: gpui_kit::EventEmitter<ParamEvent> + 'static>(
    vcx: &mut VisualTestContext,
    control: &Entity<T>,
) -> Rc<RefCell<Vec<ParamEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    vcx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(control, move |_, event: &ParamEvent, _| {
            seen.borrow_mut().push(*event)
        })
        .detach();
    });
    seen
}

fn bus(vcx: &mut VisualTestContext, path: &str, name: &'static str, payload: Value) {
    vcx.update(|window, cx| {
        let id = ControlId::lookup(path, cx).expect("registered");
        let action = Action {
            name: name.into(),
            payload,
        };
        let _ = ActionBus::dispatch(id, &action, Source::Mcp, window, cx);
    });
}

#[gpui_kit::test]
fn keyboard_and_bus_steps_agree(cx: &mut TestAppContext) {
    let (knob, _, log, vcx) = mount(cx);
    let seen = events(vcx, &knob);
    vcx.update(|window, cx| {
        window.focus(&gpui_kit::Focusable::focus_handle(knob.read(cx), cx), cx)
    });
    vcx.simulate_keystrokes("up");
    let by_key = vcx.update(|_, cx| knob.read(cx).value());
    bus(vcx, "synth/cutoff", names::SET_VALUE, Value::Num(1_000.0));
    bus(vcx, "synth/cutoff", names::INCREMENT, Value::Null);
    let by_bus = vcx.update(|_, cx| knob.read(cx).value());
    assert!((by_key - by_bus).abs() < 1e-9, "{by_key} vs {by_bus}");
    assert!(by_key > 1_000.0);
    let sources: Vec<_> = seen
        .borrow()
        .iter()
        .map(|ParamEvent::Changed(_, s)| *s)
        .collect();
    assert_eq!(sources, [Source::Keyboard, Source::Mcp, Source::Mcp]);
    assert_eq!(
        log.sources.borrow()[0],
        Source::Keyboard,
        "physical input goes through the bus"
    );
    vcx.update(|_, cx| {
        assert_eq!(knob.read(cx).formatted(), cutoff().format(by_bus));
    });
}

#[gpui_kit::test]
fn a_drag_is_one_batch_and_matches_the_bus(cx: &mut TestAppContext) {
    let (knob, _, log, vcx) = mount(cx);
    let seen = events(vcx, &knob);
    let none = Modifiers::default();
    vcx.simulate_mouse_down(point(px(20.), px(20.)), MouseButton::Left, none);
    vcx.run_until_parked();
    for y in [15., 5., -10., -30.] {
        vcx.simulate_mouse_move(point(px(20.), px(y)), MouseButton::Left, none);
        vcx.run_until_parked();
    }
    assert!(seen.borrow().is_empty(), "a drag previews without emitting");
    vcx.simulate_mouse_up(point(px(20.), px(-30.)), MouseButton::Left, none);
    vcx.run_until_parked();
    assert_eq!(
        *log.groups.borrow(),
        1,
        "the drag commits as one batch (one undo step)"
    );
    assert_eq!(seen.borrow().len(), 1);
    let dragged = vcx.update(|_, cx| knob.read(cx).value());
    // 50 px up from 1 kHz: a quarter of the travel.
    let map = cutoff();
    let expected = map.from_normal(map.to_normal(1_000.0) + 50.0 / 200.0);
    assert!((dragged - expected).abs() < 1e-6, "{dragged} vs {expected}");
    // The same value over the bus lands identically.
    bus(vcx, "synth/cutoff", names::SET_VALUE, Value::Num(1_000.0));
    bus(vcx, "synth/cutoff", names::SET_VALUE, Value::Num(expected));
    assert_eq!(vcx.update(|_, cx| knob.read(cx).value()), dragged);
}

#[gpui_kit::test]
fn double_click_resets_to_the_default(cx: &mut TestAppContext) {
    let (knob, _, _, vcx) = mount(cx);
    bus(vcx, "synth/cutoff", names::SET_VALUE, Value::Num(5_000.0));
    vcx.simulate_event(gpui_kit::MouseDownEvent {
        position: point(px(20.), px(20.)),
        modifiers: Modifiers::default(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| knob.read(cx).value()), 1_000.0);
}

#[gpui_kit::test]
fn typed_entry_parses_through_the_mapping(cx: &mut TestAppContext) {
    let (_, field, _, vcx) = mount(cx);
    let seen = events(vcx, &field);
    vcx.update(|window, cx| field.update(cx, |f, cx| f.begin_entry(window, cx)));
    vcx.run_until_parked();
    let entry = vcx.update(|_, cx| field.read(cx).entry().clone());
    vcx.update(|window, cx| entry.update(cx, |e, cx| e.set_value("2.5k", window, cx)));
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(field.read(cx).value(), 2_500.0);
        assert!(!field.read(cx).editing());
        assert_eq!(field.read(cx).formatted(), "2.5 kHz");
    });
    assert_eq!(
        *seen.borrow(),
        [ParamEvent::Changed(2_500.0, Source::Keyboard)]
    );
    // Nonsense keeps entry open with the parse error, and changes nothing.
    vcx.update(|window, cx| field.update(cx, |f, cx| f.begin_entry(window, cx)));
    vcx.update(|window, cx| entry.update(cx, |e, cx| e.set_value("loud", window, cx)));
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(field.read(cx).value(), 2_500.0);
        assert!(field.read(cx).editing());
        assert_eq!(
            field.read(cx).error().map(|e| e.to_string()),
            Some("not a number".into())
        );
    });
}

#[gpui_kit::test]
fn host_writes_never_emit(cx: &mut TestAppContext) {
    let (knob, field, _, vcx) = mount(cx);
    let knob_events = events(vcx, &knob);
    let field_events = events(vcx, &field);
    vcx.update(|_, cx| {
        knob.update(cx, |k, cx| {
            k.set_value(440.0, cx);
            k.set_modulation(Some((200.0, 900.0)), cx);
        });
        field.update(cx, |f, cx| f.set_value(9e9, cx));
    });
    vcx.run_until_parked();
    assert!(knob_events.borrow().is_empty() && field_events.borrow().is_empty());
    vcx.update(|window, cx| {
        assert_eq!(knob.read(cx).value(), 440.0);
        assert_eq!(field.read(cx).value(), 20_000.0, "clamped");
        let id = ControlId::lookup("synth/cutoff", cx).expect("registered");
        let snapshot = ActionBus::query(id, window, cx).expect("snapshot");
        assert_eq!(snapshot.value, Some(Value::Num(440.0)));
        assert_eq!(
            snapshot.label.as_deref(),
            Some("Cutoff: 440 Hz"),
            "accessible value with units"
        );
    });
}

#[gpui_kit::test]
fn a_log_slider_steps_by_travel_like_a_knob(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let mut built = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let slider = SliderControl::new()
            .mapping(cutoff())
            .path("synth/slider")
            .build(window, cx);
        let knob = KnobControl::new()
            .mapping(cutoff())
            .path("synth/knob")
            .build(window, cx);
        built = Some((slider.clone(), knob.clone()));
        Host(vec![slider.into(), knob.into()])
    });
    let (slider, knob) = built.expect("built");
    bus(vcx, "synth/slider", names::INCREMENT, Value::Null);
    bus(vcx, "synth/knob", names::INCREMENT, Value::Null);
    vcx.update(|_, cx| {
        assert_eq!(
            slider.read(cx).value_f64(),
            knob.read(cx).value(),
            "Hz controls step the same"
        );
        assert_eq!(slider.read(cx).formatted(), knob.read(cx).formatted());
    });
}
