//! Every adopted control: physical input and bus input give the same events and state,
//! and a registered control's physical input passes through the bus.
use std::{cell::RefCell, rc::Rc};

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
        ActionBus::dispatch(id, &action, Source::Test, window, cx)
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
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    click(vcx, &plain, ButtonControl::element_id);
    click(vcx, &physical, ButtonControl::element_id);
    assert_eq!(
        bus(vcx, "test/bus", names::ACTIVATE, Value::Null),
        Ok(Outcome::EMPTY)
    );
    for log in &logs {
        assert_eq!(*log.borrow(), [ButtonEvent::Activated]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Test]);
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
    for log in &logs {
        assert_eq!(*log.borrow(), [ToggleEvent::Changed(true)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Test]);
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
    for (log, entity) in logs.iter().zip([&plain, &physical, &bus_side]) {
        assert_eq!(*log.borrow(), [SliderControlEvent::Changed(5.0)]);
        let (value, kit) = vcx.update(|_, cx| {
            let control = entity.read(cx);
            (control.value(), control.state().read(cx).value())
        });
        assert_eq!(value, 5.0);
        assert_eq!(kit, gpui_kit::component::slider::SliderValue::Single(5.0));
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Test]);
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
        let input = vcx.update(|_, cx| entity.read(cx).input().clone());
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
    for (log, entity) in logs.iter().zip([&plain, &physical, &bus_side]) {
        assert_eq!(*log.borrow(), [TextInputEvent::Changed("Ada".into())]);
        let (value, kit) = vcx.update(|_, cx| {
            let control = entity.read(cx);
            (control.value().clone(), control.input().read(cx).value())
        });
        assert_eq!((value.as_ref(), kit.as_ref()), ("Ada", "Ada"));
    }
    assert_eq!(*hook.0.borrow(), [Source::Keyboard, Source::Test]);
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
        record(vcx, &plain),
        record(vcx, &physical),
        record(vcx, &bus_side),
    ];
    // The kit list's confirm, as a pointer click on an open dropdown emits it.
    for entity in [&plain, &physical] {
        let state = vcx.update(|_, cx| entity.read(cx).state().clone());
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
    for (log, entity) in logs.iter().zip([&plain, &physical, &bus_side]) {
        assert_eq!(*log.borrow(), [SelectionEvent::Selected(2)]);
        assert_eq!(vcx.update(|_, cx| entity.read(cx).selected()), Some(2));
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Test]);
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
    for (log, entity) in logs.iter().zip([&plain, &physical, &bus_side]) {
        assert_eq!(*log.borrow(), [SelectionEvent::Selected(1)]);
        assert_eq!(vcx.update(|_, cx| entity.read(cx).selected()), 1);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Test]);
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
    for log in &logs {
        assert_eq!(*log.borrow(), [MenuEvent::Activated(1)]);
    }
    assert_eq!(*hook.0.borrow(), [Source::Pointer, Source::Test]);
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
