//! Every adopted control: physical input and bus input give the same events and state,
//! and a registered control's physical input passes through the bus.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::component::IconName;
use gpui_kit::{
    AnyView, App, Context, Entity, EventEmitter, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, component::select::SelectEvent, div, px,
    test::TestWindowExt,
};
use guiraffe_components::controls::*;
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, ControlId, ControlSnapshot, Interceptor, Outcome, Source, names,
};
use guiraffe_core::widget_value::Value;

struct Host(Vec<AnyView>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).flex().flex_col().children(self.0.clone())
    }
}

/// Records each dispatch's source, so a test can see physical input on the bus.
#[derive(Default)]
struct Sources(RefCell<Vec<Source>>, Option<ActionError>);
impl Interceptor for Sources {
    fn before(
        &self,
        _: ControlId,
        _: &Action,
        source: Source,
        _: &Window,
        _: &App,
    ) -> Result<(), ActionError> {
        self.0.borrow_mut().push(source);
        self.1.clone().map_or(Ok(()), Err)
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
}

type Built<T> = (Entity<T>, Entity<T>, Entity<T>);

/// Builds three identical controls: unregistered, registered for physical input and
/// registered for bus input.
fn mount<T: Render>(
    cx: &mut TestAppContext,
    make: impl Fn(Option<&'static str>, &mut Window, &mut App) -> Entity<T>,
) -> (Built<T>, &mut VisualTestContext) {
    cx.update(guiraffe_components::init);
    let mut built = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let plain = make(None, window, cx);
        let physical = make(Some("test/physical"), window, cx);
        let bus = make(Some("test/bus"), window, cx);
        let views = vec![
            plain.clone().into(),
            physical.clone().into(),
            bus.clone().into(),
        ];
        built = Some((plain, physical, bus));
        Host(views)
    });
    vcx.run_until_parked();
    let built = built.unwrap_or_else(|| unreachable!());
    (built, vcx)
}

fn record<T: EventEmitter<E>, E: Clone + 'static>(
    vcx: &mut VisualTestContext,
    entity: &Entity<T>,
) -> Rc<RefCell<Vec<E>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    vcx.update(|_, cx| {
        cx.subscribe(entity, move |_, event: &E, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach()
    });
    log
}

fn sources(vcx: &mut VisualTestContext, deny: Option<ActionError>) -> Rc<Sources> {
    let hook = Rc::new(Sources(RefCell::default(), deny));
    vcx.update(|window, cx| ActionBus::add_interceptor(hook.clone(), window, cx));
    hook
}

fn bus(
    vcx: &mut VisualTestContext,
    path: &str,
    name: &'static str,
    payload: Value,
) -> Result<Outcome, ActionError> {
    vcx.update(|window, cx| {
        let id = ControlId::lookup(path, cx).ok_or(ActionError::UnknownTarget)?;
        let action = Action {
            name: name.into(),
            payload,
        };
        ActionBus::dispatch(id, &action, Source::Mcp, window, cx)
    })
}

fn snapshot(vcx: &mut VisualTestContext, path: &str) -> Option<ControlSnapshot> {
    vcx.update(|window, cx| ActionBus::query(ControlId::lookup(path, cx)?, window, cx).ok())
}

fn click<T: 'static>(
    vcx: &mut VisualTestContext,
    entity: &Entity<T>,
    id: impl Fn(&T) -> gpui_kit::ElementId,
) {
    let id = vcx.update(|_, cx| id(entity.read(cx)));
    vcx.update(|window, cx| window.click(id, cx));
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn button_pointer_and_bus_activate_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = ButtonControl::new().label("Save");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record::<_, ButtonEvent>(vcx, &plain),
        record::<_, ButtonEvent>(vcx, &physical),
        record::<_, ButtonEvent>(vcx, &bus_side),
    ];
    click(vcx, &plain, ButtonControl::element_id);
    click(vcx, &physical, ButtonControl::element_id);
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Null),
        Ok(Outcome::EMPTY)
    );
    for (log, source) in logs
        .iter()
        .zip([Source::Pointer, Source::Pointer, Source::Mcp])
    {
        assert_eq!(*log.borrow(), [ButtonEvent::Activated(source)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
}

fn toggle_case<T>(
    cx: &mut TestAppContext,
    make: impl Fn(Option<&'static str>, &mut Window, &mut App) -> Entity<T>,
) where
    T: Render + EventEmitter<ToggleEvent>,
{
    let ((plain, physical, bus_side), vcx) = mount(cx, make);
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    for entity in [&plain, &physical] {
        let element = vcx.update(|_, cx| element_of(entity, cx));
        vcx.update(|window, cx| window.click(element, cx));
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::TOGGLE, Value::Null),
        Ok(Outcome(Value::Bool(true)))
    );
    for (log, source) in logs
        .iter()
        .zip([Source::Pointer, Source::Pointer, Source::Mcp])
    {
        assert_eq!(*log.borrow(), [ToggleEvent::Changed(true, source)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    let physical_state = snapshot(vcx, "test/physical");
    assert_eq!(physical_state.as_ref().and_then(|s| s.checked), Some(true));
    assert_eq!(physical_state, snapshot(vcx, "test/bus"));
    // Setting the current value is a no-op; an invalid payload changes nothing.
    assert_eq!(
        bus(vcx, "test/bus", names::SET_VALUE, Value::Bool(true)),
        Ok(Outcome(Value::Bool(true)))
    );
    assert!(bus(vcx, "test/bus", names::SET_VALUE, Value::Num(1.0)).is_err());
    assert_eq!(logs[2].borrow().len(), 1);
}

fn element_of<T: 'static>(entity: &Entity<T>, cx: &App) -> gpui_kit::ElementId {
    let any = entity.clone().into_any();
    if let Ok(c) = any.clone().downcast::<CheckboxControl>() {
        return c.read(cx).element_id();
    }
    if let Ok(c) = any.downcast::<SwitchControl>() {
        return c.read(cx).element_id();
    }
    unreachable!("not a toggle control")
}

#[gpui_kit::test]
fn checkbox_pointer_and_bus_toggle_identically(cx: &mut TestAppContext) {
    toggle_case(cx, |path, window, cx| {
        let b = CheckboxControl::new().label("Dark");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
}

#[gpui_kit::test]
fn switch_pointer_and_bus_toggle_identically(cx: &mut TestAppContext) {
    toggle_case(cx, |path, window, cx| {
        let b = SwitchControl::new().label("Wi-Fi");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
}

#[gpui_kit::test]
fn vetoed_pointer_input_leaves_state_unchanged(cx: &mut TestAppContext) {
    let ((_, physical, _), vcx) = mount(cx, |path, window, cx| {
        let b = CheckboxControl::new();
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, Some(ActionError::ReadOnly));
    let log = record(vcx, &physical);
    click(vcx, &physical, CheckboxControl::element_id);
    assert_eq!(*hook.0.borrow(), [Source::Pointer]);
    assert!(log.borrow().is_empty());
    assert!(!vcx.update(|_, cx| physical.read(cx).checked()));
}

#[gpui_kit::test]
fn slider_keyboard_step_and_bus_increment_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = SliderControl::new().range(0.0, 10.0).step(1.0).value(4.0);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    // The kit's arrow-key handler: one step towards the end.
    for entity in [&plain, &physical] {
        let state = vcx.update(|_, cx| entity.read(cx).state().clone());
        vcx.update(|window, cx| state.update(cx, |s, cx| s.step_thumb(false, 1.0, window, cx)));
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::INCREMENT, Value::Null),
        Ok(Outcome(Value::Num(5.0)))
    );
    for ((log, entity), source) in logs.iter().zip([&plain, &physical, &bus_side]).zip([
        Source::Pointer,
        Source::Pointer,
        Source::Mcp,
    ]) {
        assert_eq!(*log.borrow(), [SliderControlEvent::Changed(5.0, source)]);
        let (value, kit) = vcx.update(|_, cx| {
            let control = entity.read(cx);
            (control.value(), control.state().read(cx).value())
        });
        assert_eq!(value, 5.0);
        assert_eq!(kit, gpui_kit::component::slider::SliderValue::Single(5.0));
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    assert!(bus(vcx, "test/bus", names::SET_VALUE, Value::Str("x".into())).is_err());
}

#[gpui_kit::test]
fn vetoed_slider_input_snaps_the_kit_state_back(cx: &mut TestAppContext) {
    let ((_, physical, _), vcx) = mount(cx, |path, window, cx| {
        let b = SliderControl::new().range(0.0, 10.0).value(4.0);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    sources(vcx, Some(ActionError::ReadOnly));
    let state = vcx.update(|_, cx| physical.read(cx).state().clone());
    vcx.update(|window, cx| state.update(cx, |s, cx| s.step_thumb(false, 1.0, window, cx)));
    vcx.run_until_parked();
    let kit = vcx.update(|_, cx| state.read(cx).value());
    assert_eq!(kit, gpui_kit::component::slider::SliderValue::Single(4.0));
    assert_eq!(vcx.update(|_, cx| physical.read(cx).value()), 4.0);
}

#[gpui_kit::test]
fn text_input_typing_and_bus_set_value_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = TextInputControl::new().placeholder("Name");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    for entity in [&plain, &physical] {
        let input = vcx.update(|_, cx| entity.read(cx).input().expect("kit input").clone());
        vcx.update(|window, cx| {
            input.update(cx, |s, cx| s.focus(window, cx));
            window.input("Ada", cx);
        });
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::SET_VALUE, Value::Str("Ada".into())),
        Ok(Outcome(Value::Str("Ada".into())))
    );
    for ((log, entity), source) in logs.iter().zip([&plain, &physical, &bus_side]).zip([
        Source::Keyboard,
        Source::Keyboard,
        Source::Mcp,
    ]) {
        assert_eq!(
            *log.borrow(),
            [TextInputEvent::Changed("Ada".into(), source)]
        );
        let (value, kit) = vcx.update(|_, cx| {
            let control = entity.read(cx);
            (
                control.value().clone(),
                control.input().expect("kit input").read(cx).value(),
            )
        });
        assert_eq!((value.as_ref(), kit.as_ref()), ("Ada", "Ada"));
    }
    assert_eq!(*hook.0.borrow(), [Source::Keyboard, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    assert_eq!(
        bus(vcx, "test/bus", names::FOCUS, Value::Null),
        Ok(Outcome::EMPTY)
    );
}

#[gpui_kit::test]
fn select_confirm_and_bus_select_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = SelectControl::new()
            .options(["Red", "Green", "Blue"])
            .selected(0);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record::<_, SelectionEvent>(vcx, &plain),
        record::<_, SelectionEvent>(vcx, &physical),
        record::<_, SelectionEvent>(vcx, &bus_side),
    ];
    // The kit list's confirm, as a pointer click on an open dropdown emits it.
    for entity in [&plain, &physical] {
        let state = vcx.update(|_, cx| entity.read(cx).state().expect("kit state").clone());
        vcx.update(|_, cx| {
            state.update(cx, |_, cx| {
                cx.emit(SelectEvent::Confirm(Some("Blue".into())))
            })
        });
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::SELECT, Value::Num(2.0)),
        Ok(Outcome(Value::Str("Blue".into())))
    );
    for ((log, entity), source) in logs.iter().zip([&plain, &physical, &bus_side]).zip([
        Source::Pointer,
        Source::Pointer,
        Source::Mcp,
    ]) {
        assert_eq!(*log.borrow(), [SelectionEvent::Selected(2, source)]);
        assert_eq!(vcx.update(|_, cx| entity.read(cx).selected()), Some(2));
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    assert!(bus(vcx, "test/bus", names::SELECT, Value::Num(3.0)).is_err());
}

#[gpui_kit::test]
fn tabs_click_and_bus_select_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = TabsControl::new().tabs(["One", "Two", "Three"]);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    for entity in [&plain, &physical] {
        let bar = vcx.update(|_, cx| entity.read(cx).element_id());
        vcx.update(|window, cx| window.within(bar).click(1usize, cx));
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::SELECT, Value::Num(1.0)),
        Ok(Outcome(Value::Num(1.0)))
    );
    for ((log, entity), source) in logs.iter().zip([&plain, &physical, &bus_side]).zip([
        Source::Pointer,
        Source::Pointer,
        Source::Mcp,
    ]) {
        assert_eq!(*log.borrow(), [SelectionEvent::Selected(1, source)]);
        assert_eq!(vcx.update(|_, cx| entity.read(cx).selected()), 1);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
}

#[gpui_kit::test]
fn menu_click_and_bus_activate_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = MenuControl::new()
            .item("Open")
            .item("Save")
            .disabled_item("Print");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    for entity in [&plain, &physical] {
        click(vcx, entity, |menu| menu.item_id(1));
    }
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Num(1.0)),
        Ok(Outcome(Value::Num(1.0)))
    );
    for (log, source) in logs
        .iter()
        .zip([Source::Pointer, Source::Pointer, Source::Mcp])
    {
        assert_eq!(*log.borrow(), [MenuEvent::Activated(1, source)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Num(2.0)),
        Err(ActionError::Disabled)
    );
}

#[gpui_kit::test]
fn unbound_controls_register_nothing(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let views: Vec<AnyView> = vec![
            ButtonControl::new().build(window, cx).into(),
            CheckboxControl::new().build(window, cx).into(),
            SliderControl::new().build(window, cx).into(),
            TextInputControl::new().build(window, cx).into(),
            SelectControl::new().options(["a"]).build(window, cx).into(),
            TabsControl::new().tabs(["a"]).build(window, cx).into(),
            MenuControl::new().item("a").build(window, cx).into(),
        ];
        Host(views)
    });
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|window, cx| ActionBus::controls(window, cx).count()),
        0
    );
}

#[gpui_kit::test]
fn host_setters_need_no_window_and_never_emit(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let mut built = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let slider = SliderControl::new()
            .range(0.0, 10.0)
            .path("host/slider")
            .build(window, cx);
        let input = TextInputControl::new().path("host/input").build(window, cx);
        let select = SelectControl::new()
            .options(["a", "b"])
            .path("host/select")
            .build(window, cx);
        let views: Vec<AnyView> = vec![
            slider.clone().into(),
            input.clone().into(),
            select.clone().into(),
        ];
        built = Some((slider, input, select));
        Host(views)
    });
    let (slider, input, select) = built.unwrap_or_else(|| unreachable!());
    vcx.run_until_parked();
    let slider_log = record(vcx, &slider);
    let input_log = record::<_, TextInputEvent>(vcx, &input);
    let select_log = record::<_, SelectionEvent>(vcx, &select);
    // App-only context, as inside a Store watch: no window in reach.
    vcx.update(|_, cx| {
        let app: &mut App = cx;
        slider.update(app, |c, cx| {
            c.set_value(7.0, cx);
            c.set_label("Volume", cx);
            c.set_disabled(true, cx);
        });
        input.update(app, |c, cx| {
            c.set_value("typed by host", cx);
            c.set_placeholder("Name", cx);
        });
        select.update(app, |c, cx| c.set_selected(Some(1), cx));
    });
    vcx.run_until_parked();
    vcx.update(|window, _| window.refresh());
    vcx.run_until_parked();
    let (kit_slider, kit_input, kit_select) = vcx.update(|_, cx| {
        (
            slider.read(cx).state().read(cx).value(),
            input.read(cx).input().expect("kit input").read(cx).value(),
            select
                .read(cx)
                .state()
                .expect("kit state")
                .read(cx)
                .selected_index(cx),
        )
    });
    assert_eq!(
        kit_slider,
        gpui_kit::component::slider::SliderValue::Single(7.0)
    );
    assert_eq!(kit_input.as_ref(), "typed by host");
    assert_eq!(kit_select, Some(gpui_kit::component::IndexPath::new(1)));
    assert!(slider_log.borrow().is_empty());
    assert!(input_log.borrow().is_empty());
    assert!(select_log.borrow().is_empty());
    let snap = snapshot(vcx, "host/slider");
    assert_eq!(
        snap.as_ref().and_then(|s| s.label.as_deref()),
        Some("Volume")
    );
    assert_eq!(
        bus(vcx, "host/slider", names::INCREMENT, Value::Null),
        Err(ActionError::Disabled),
        "a host-disabled control rejects bus input"
    );
    assert_eq!(
        snapshot(vcx, "host/input").and_then(|s| s.label).as_deref(),
        Some("Name")
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Theme {
    Light,
    Dark,
    System,
}

#[gpui_kit::test]
fn typed_select_binds_values_by_pointer_and_bus(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = SelectControl::typed([
            (Theme::Light, "Light"),
            (Theme::Dark, "Dark"),
            (Theme::System, "System"),
        ])
        .selected_value(&Theme::Light);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record::<_, SelectedValue<Theme>>(vcx, &plain),
        record::<_, SelectedValue<Theme>>(vcx, &physical),
        record::<_, SelectedValue<Theme>>(vcx, &bus_side),
    ];
    for entity in [&plain, &physical] {
        let state = vcx.update(|_, cx| entity.read(cx).state().expect("kit state").clone());
        vcx.update(|_, cx| {
            state.update(cx, |_, cx| {
                cx.emit(SelectEvent::Confirm(Some("Dark".into())))
            })
        });
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::SELECT, Value::Num(1.0)),
        Ok(Outcome(Value::Str("Dark".into())))
    );
    for ((log, entity), source) in logs.iter().zip([&plain, &physical, &bus_side]).zip([
        Source::Pointer,
        Source::Pointer,
        Source::Mcp,
    ]) {
        assert_eq!(*log.borrow(), [SelectedValue(Theme::Dark, source)]);
        assert_eq!(
            vcx.update(|_, cx| entity.read(cx).selected_value().copied()),
            Some(Theme::Dark)
        );
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    // A host write by value selects without emitting.
    let found =
        vcx.update(|_, cx| bus_side.update(cx, |c, cx| c.set_selected_value(&Theme::System, cx)));
    assert!(found);
    vcx.run_until_parked();
    assert_eq!(logs[2].borrow().len(), 1);
    assert_eq!(vcx.update(|_, cx| bus_side.read(cx).selected()), Some(2));
}

#[gpui_kit::test]
fn one_way_disabled_blocks_pointer_and_bus(cx: &mut TestAppContext) {
    let ((_, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = ButtonControl::new().label("Go");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let logs = [
        record::<_, ButtonEvent>(vcx, &physical),
        record::<_, ButtonEvent>(vcx, &bus_side),
    ];
    vcx.update(|_, cx| {
        for entity in [&physical, &bus_side] {
            entity.update(cx, |c, cx| {
                c.set_disabled(true, cx);
                c.set_label("Stop", cx);
            });
        }
    });
    vcx.run_until_parked();
    click(vcx, &physical, ButtonControl::element_id);
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Null),
        Err(ActionError::Disabled)
    );
    for log in &logs {
        assert!(log.borrow().is_empty());
    }
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    assert_eq!(
        snapshot(vcx, "test/bus").and_then(|s| s.label).as_deref(),
        Some("Stop")
    );
}

#[gpui_kit::test]
fn events_carry_the_dispatch_source(cx: &mut TestAppContext) {
    use guiraffe_core::protocol::Drive;
    let ((plain, _, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = SwitchControl::new();
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let bus_log = record(vcx, &bus_side);
    let plain_log = record(vcx, &plain);
    vcx.update(|window, cx| {
        let id = bus_side
            .read(cx)
            .control_id()
            .unwrap_or_else(|| unreachable!());
        let toggle = Action {
            name: names::TOGGLE.into(),
            payload: Value::Null,
        };
        assert!(ActionBus::dispatch(id, &toggle, Source::Script, window, cx).is_ok());
        let batch = ActionBus::dispatch_batch(id, &[toggle], Source::Ribbon, window, cx);
        assert!(batch.into_result().is_ok());
        // A host applying a typed action itself, outside any dispatch.
        plain.update(cx, |c, cx| {
            c.apply(ToggleAction::Toggle, window, cx).map(drop)
        })
    })
    .unwrap_or_else(|e| panic!("{e:?}"));
    vcx.run_until_parked();
    assert_eq!(
        *bus_log.borrow(),
        [
            ToggleEvent::Changed(true, Source::Script),
            ToggleEvent::Changed(false, Source::Ribbon)
        ]
    );
    assert_eq!(
        *plain_log.borrow(),
        [ToggleEvent::Changed(true, Source::Host)]
    );
}

#[gpui_kit::test]
fn button_variants_survive_host_writes(cx: &mut TestAppContext) {
    let ((_, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = ButtonControl::new()
            .label("Reject")
            .variant(ButtonVariant::Danger);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let logs = [
        record::<_, ButtonEvent>(vcx, &physical),
        record::<_, ButtonEvent>(vcx, &bus_side),
    ];
    assert_eq!(
        vcx.update(|_, cx| *physical.read(cx).variant()),
        ButtonVariant::Danger
    );
    vcx.update(|_, cx| {
        physical.update(cx, |b, cx| b.set_label("Accept", cx));
        bus_side.update(cx, |b, cx| b.set_variant(ButtonVariant::Primary, cx));
    });
    vcx.run_until_parked();
    click(vcx, &physical, ButtonControl::element_id);
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Null),
        Ok(Outcome::EMPTY)
    );
    assert_eq!(*logs[0].borrow(), [ButtonEvent::Activated(Source::Pointer)]);
    assert_eq!(*logs[1].borrow(), [ButtonEvent::Activated(Source::Mcp)]);
    let (a, b) = vcx.update(|_, cx| (*physical.read(cx).variant(), *bus_side.read(cx).variant()));
    assert_eq!((a, b), (ButtonVariant::Danger, ButtonVariant::Primary));
}

#[gpui_kit::test]
fn latched_button_pointer_and_bus_toggle_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = ButtonControl::new()
            .icon(IconName::Play)
            .tooltip("Record")
            .xsmall()
            .toggled(false);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record::<_, ToggleEvent>(vcx, &plain),
        record::<_, ToggleEvent>(vcx, &physical),
        record::<_, ToggleEvent>(vcx, &bus_side),
    ];
    let activations = record::<_, ButtonEvent>(vcx, &physical);
    click(vcx, &plain, ButtonControl::element_id);
    click(vcx, &physical, ButtonControl::element_id);
    assert_eq!(
        bus(vcx, "test/bus", names::TOGGLE, Value::Null),
        Ok(Outcome(Value::Bool(true)))
    );
    for (log, source) in logs
        .iter()
        .zip([Source::Pointer, Source::Pointer, Source::Mcp])
    {
        assert_eq!(*log.borrow(), [ToggleEvent::Changed(true, source)]);
    }
    assert!(
        activations.borrow().is_empty(),
        "a latched click is a toggle"
    );
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    let snap = snapshot(vcx, "test/bus");
    assert_eq!(snap.as_ref().and_then(|s| s.checked), Some(true));
    assert_eq!(
        snap.and_then(|s| s.label).as_deref(),
        Some("Record"),
        "an icon-only button is named by its tooltip"
    );
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Null),
        Err(ActionError::Unsupported)
    );
    assert_eq!(
        bus(vcx, "test/bus", names::SET_VALUE, Value::Bool(false)),
        Ok(Outcome(Value::Bool(false)))
    );
    // A host write: no window, no event.
    vcx.update(|_, cx| {
        let app: &mut App = cx;
        bus_side.update(app, |b, cx| b.set_toggled(true, cx));
        physical.update(app, |b, cx| b.set_toggled(false, cx));
    });
    vcx.run_until_parked();
    assert_eq!(logs[2].borrow().len(), 2);
    assert_eq!(vcx.update(|_, cx| bus_side.read(cx).toggled()), Some(true));
}

#[gpui_kit::test]
fn toggle_chip_pointer_and_bus_toggle_identically(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = ToggleChipControl::new().label("M").tooltip("Mute");
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    click(vcx, &plain, ToggleChipControl::element_id);
    click(vcx, &physical, ToggleChipControl::element_id);
    assert_eq!(
        bus(vcx, "test/bus", names::SET_VALUE, Value::Bool(true)),
        Ok(Outcome(Value::Bool(true)))
    );
    for (log, source) in logs
        .iter()
        .zip([Source::Pointer, Source::Pointer, Source::Mcp])
    {
        assert_eq!(*log.borrow(), [ToggleEvent::Changed(true, source)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    assert_eq!(
        snapshot(vcx, "test/bus").and_then(|s| s.label).as_deref(),
        Some("Mute")
    );
    vcx.update(|_, cx| bus_side.update(cx, |c, cx| c.set_checked(false, cx)));
    vcx.run_until_parked();
    assert_eq!(logs[2].borrow().len(), 1);
    assert!(!vcx.update(|_, cx| bus_side.read(cx).checked()));
}

#[gpui_kit::test]
fn vertical_slider_steps_and_bus_like_horizontal(cx: &mut TestAppContext) {
    let ((plain, physical, bus_side), vcx) = mount(cx, |path, window, cx| {
        let b = SliderControl::new()
            .range(0.0, 10.0)
            .step(1.0)
            .value(4.0)
            .vertical()
            .length(120.);
        match path {
            Some(path) => b.path(path),
            None => b,
        }
        .build(window, cx)
    });
    let hook = sources(vcx, None);
    let logs = [
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    for entity in [&plain, &physical] {
        let state = vcx.update(|_, cx| entity.read(cx).state().clone());
        vcx.update(|window, cx| state.update(cx, |s, cx| s.step_thumb(false, 1.0, window, cx)));
        vcx.run_until_parked();
    }
    assert_eq!(
        bus(vcx, "test/bus", names::SET_VALUE, Value::Num(5.0)),
        Ok(Outcome(Value::Num(5.0)))
    );
    for (log, source) in logs
        .iter()
        .zip([Source::Pointer, Source::Pointer, Source::Mcp])
    {
        assert_eq!(*log.borrow(), [SliderControlEvent::Changed(5.0, source)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Mcp]);
    assert_eq!(snapshot(vcx, "test/physical"), snapshot(vcx, "test/bus"));
    let id = vcx.update(|_, cx| physical.read(cx).element_id());
    let track = vcx.update(|window, _| window.within(id).find("slider-bar-container").bounds());
    let (w, h) = (f32::from(track.size.width), f32::from(track.size.height));
    assert!(h > w, "a vertical track: {w}x{h}");
    assert!(
        (100.0..=120.0).contains(&h),
        "the fader's length bounds the track: {h}"
    );
}

#[gpui_kit::test]
fn cycle_routes_pointer_keyboard_and_bus_through_the_same_transition(cx: &mut TestAppContext) {
    use gpui_kit::{Modifiers, MouseButton, point};
    cx.update(guiraffe_components::init);
    let cycle = cx.update(|cx| {
        CycleControl::new()
            .options(["4", "8", "16"].map(|v| CycleOption::new(v, v)))
            .element_id("resolution")
            .path("grid/resolution")
            .build_later(cx)
    });
    let (_, vcx) = cx.add_window_view(|_, _| Host(vec![cycle.clone().into()]));
    vcx.run_until_parked();
    let log = record::<_, CycleEvent>(vcx, &cycle);
    click(vcx, &cycle, CycleControl::element_id);
    let pos = point(px(8.), px(8.));
    vcx.simulate_mouse_down(pos, MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(pos, MouseButton::Right, Modifiers::default());
    vcx.update(|window, cx| window.focus_next(cx));
    vcx.simulate_keystrokes("space");
    let path = "grid/resolution";
    assert_eq!(
        bus(vcx, path, names::PREVIOUS, Value::Null),
        Ok(Outcome(Value::Str("4".into())))
    );
    assert_eq!(
        bus(vcx, path, names::SET_VALUE, Value::Str("16".into())),
        Ok(Outcome(Value::Str("16".into())))
    );
    assert_eq!(
        bus(vcx, path, names::NEXT, Value::Null),
        Ok(Outcome(Value::Str("4".into())))
    );
    vcx.update(|_, cx| {
        cycle.update(cx, |cycle, cx| {
            assert!(cycle.set_value("8", cx).is_ok());
            cycle.set_disabled(true, cx);
        })
    });
    assert_eq!(
        bus(vcx, path, names::NEXT, Value::Null),
        Err(ActionError::Disabled)
    );
    assert_eq!(
        *log.borrow(),
        [
            ("8", Source::Pointer),
            ("4", Source::Pointer),
            ("8", Source::Keyboard),
            ("4", Source::Mcp),
            ("16", Source::Mcp),
            ("4", Source::Mcp)
        ]
        .map(|(v, s)| CycleEvent::Changed(v.into(), s))
    );
}

#[gpui_kit::test]
fn cycle_toolbar_labels_meet_contrast_in_both_schemes(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme;
    use guiraffe_core::{GuiraffeTheme, Rgb, Scheme};
    fn rgb(color: gpui_kit::Hsla) -> Rgb {
        let c = color.to_rgb();
        Rgb {
            r: (255. * c.r).round() as u8,
            g: (255. * c.g).round() as u8,
            b: (255. * c.b).round() as u8,
            a: (255. * c.a).round() as u8,
        }
    }
    cx.update(guiraffe_components::init);
    cx.update(|cx| {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let theme = GuiraffeTheme {
                scheme,
                ..Default::default()
            };
            let surface = theme.surfaces().body;
            guiraffe_core::apply(theme, cx);
            let kit = cx.theme();
            for (label, background) in [
                (kit.button_foreground, rgb(kit.button).over(surface)),
                (kit.secondary_foreground, surface),
            ] {
                let ratio = rgb(label).over(background).contrast(background);
                assert!(ratio >= 4.5, "{scheme:?} label contrast {ratio}");
            }
        }
    });
}
