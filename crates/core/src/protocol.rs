//! Provisional control protocol. Persist paths; integer handles belong to one App.
use crate::widget_value::Value;
use gpui_kit::{App, Context, Entity, Global, Window, WindowId};
use std::{collections::HashMap, panic::Location, rc::Rc};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ControlId(u64);

#[derive(Default)]
struct Paths {
    ids: HashMap<Rc<str>, ControlId>,
    paths: Vec<Rc<str>>,
}
impl Global for Paths {}

impl ControlId {
    /// Intern at registration or re-key time.
    pub fn intern(path: &str, cx: &mut App) -> Self {
        let paths = cx.default_global::<Paths>();
        if let Some(id) = paths.ids.get(path) {
            return *id;
        }
        let id = Self(paths.paths.len() as u64);
        let path: Rc<str> = path.into();
        paths.paths.push(path.clone());
        paths.ids.insert(path, id);
        id
    }
    pub fn lookup(path: &str, cx: &App) -> Option<Self> {
        cx.try_global::<Paths>()?.ids.get(path).copied()
    }
    pub fn path(self, cx: &App) -> Option<&str> {
        cx.try_global::<Paths>()?
            .paths
            .get(self.0 as usize)
            .map(AsRef::as_ref)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ControlSnapshot {
    pub value: Option<Value>,
    pub checked: Option<bool>,
    pub selected: Option<Rc<[usize]>>,
    pub open: Option<bool>,
    pub disabled: Option<bool>,
    pub focused: Option<bool>,
    pub label: Option<Rc<str>>,
}

pub trait Observe {
    type Snapshot: Clone + PartialEq + Into<ControlSnapshot> + 'static;
    fn snapshot(&self, cx: &App) -> Self::Snapshot;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: gpui_kit::SharedString,
    pub payload: Value,
}

impl Action {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({"name": self.name.as_ref(), "payload": self.payload.to_json()})
    }
    pub fn from_json(json: &serde_json::Value) -> Result<Self, ActionError> {
        let name = json
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or(ActionError::Invalid("missing action name"))?;
        let payload = json
            .get("payload")
            .ok_or(ActionError::Invalid("missing action payload"))?;
        Ok(Self {
            name: name.to_owned().into(),
            payload: Value::item_from_json(payload).map_err(ActionError::Invalid)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    UnknownTarget,
    Unsupported,
    Disabled,
    Invalid(&'static str),
    ReadOnly,
}

pub trait Drive: Sized + 'static {
    type Action: 'static;
    fn decode(action: &Action) -> Result<Self::Action, ActionError>;
    fn apply(
        &mut self,
        action: Self::Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ActionError>;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Source {
    Pointer,
    Keyboard,
    Shortcut,
    Spotlight,
    Ribbon,
    DataUi,
    Script,
    Mcp,
    Replay,
}

pub trait Interceptor {
    fn before(&self, id: ControlId, action: &Action, source: Source) -> Result<(), ActionError>;
    fn after(
        &self,
        id: ControlId,
        action: &Action,
        source: Source,
        result: &Result<(), ActionError>,
    );
}

trait Endpoint {
    fn apply(&self, action: &Action, window: &mut Window, cx: &mut App) -> Result<(), ActionError>;
    fn snapshot(&self, cx: &App) -> Result<ControlSnapshot, ActionError>;
}
impl<T: Observe + Drive> Endpoint for gpui_kit::WeakEntity<T> {
    fn apply(&self, action: &Action, window: &mut Window, cx: &mut App) -> Result<(), ActionError> {
        let entity = self.upgrade().ok_or(ActionError::UnknownTarget)?;
        let typed = T::decode(action)?;
        entity.update(cx, |state, cx| state.apply(typed, window, cx))
    }
    fn snapshot(&self, cx: &App) -> Result<ControlSnapshot, ActionError> {
        let entity = self.upgrade().ok_or(ActionError::UnknownTarget)?;
        Ok(entity.read(cx).snapshot(cx).into())
    }
}
bitflags::bitflags! {
    /// Provisional set of shared actions offered by a registered control.
    /// Bits 10 through 63 are reserved for future shared actions.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
    pub struct Capabilities: u64 {
        const SET_VALUE = 1 << 0;
        const ACTIVATE = 1 << 1;
        const TOGGLE = 1 << 2;
        const OPEN = 1 << 3;
        const CLOSE = 1 << 4;
        const SELECT = 1 << 5;
        const FOCUS = 1 << 6;
        const SCROLL_TO = 1 << 7;
        const INCREMENT = 1 << 8;
        const DECREMENT = 1 << 9;
    }
}

impl Capabilities {
    /// Enumerate bindable shared actions using their protocol wire names.
    pub fn action_names(self) -> impl Iterator<Item = &'static str> {
        [
            (Self::SET_VALUE, names::SET_VALUE),
            (Self::ACTIVATE, names::ACTIVATE),
            (Self::TOGGLE, names::TOGGLE),
            (Self::OPEN, names::OPEN),
            (Self::CLOSE, names::CLOSE),
            (Self::SELECT, names::SELECT),
            (Self::FOCUS, names::FOCUS),
            (Self::SCROLL_TO, names::SCROLL_TO),
            (Self::INCREMENT, names::INCREMENT),
            (Self::DECREMENT, names::DECREMENT),
        ]
        .into_iter()
        .filter_map(move |(flag, name)| self.contains(flag).then_some(name))
    }
}

struct Entry {
    endpoint: Rc<dyn Endpoint>,
    kind: &'static str,
    capabilities: Capabilities,
    site: &'static Location<'static>,
    _release: gpui_kit::Subscription,
}

#[derive(Default)]
struct Buses(HashMap<WindowId, WindowBus>);
impl Global for Buses {}
#[derive(Default)]
struct WindowBus {
    entries: HashMap<ControlId, Entry>,
    hooks: Rc<Vec<Rc<dyn Interceptor>>>,
}

/// Window-scoped access to the shared registry and synchronous action bus.
pub struct ActionBus;

impl ActionBus {
    fn window_bus<'a>(window: &Window, cx: &'a mut App) -> &'a mut WindowBus {
        if !cx.has_global::<Buses>() {
            cx.on_window_closed(|cx, id| {
                cx.default_global::<Buses>().0.remove(&id);
            })
            .detach();
        }
        cx.default_global::<Buses>()
            .0
            .entry(window.window_handle().window_id())
            .or_default()
    }

    #[track_caller]
    pub fn register<T: Observe + Drive>(
        path: &str,
        entity: &Entity<T>,
        kind: &'static str,
        capabilities: Capabilities,
        window: &Window,
        cx: &mut App,
    ) -> ControlId {
        let id = ControlId::intern(path, cx);
        let wid = window.window_handle().window_id();
        let bus = Self::window_bus(window, cx);
        if let Some(existing) = bus.entries.get(&id) {
            debug_assert!(
                false,
                "duplicate control {path} at {} and {}",
                existing.site,
                Location::caller()
            );
            return id;
        }
        let release = cx.observe_release(entity, move |_, cx| {
            if let Some(bus) = cx.default_global::<Buses>().0.get_mut(&wid) {
                bus.entries.remove(&id);
            }
        });
        Self::window_bus(window, cx).entries.insert(
            id,
            Entry {
                endpoint: Rc::new(entity.downgrade()),
                kind,
                capabilities,
                site: Location::caller(),
                _release: release,
            },
        );
        id
    }

    pub fn unregister(id: ControlId, window: &Window, cx: &mut App) {
        if cx.has_global::<Buses>()
            && let Some(bus) = cx
                .default_global::<Buses>()
                .0
                .get_mut(&window.window_handle().window_id())
        {
            bus.entries.remove(&id);
        }
    }
    pub fn add_interceptor(hook: Rc<dyn Interceptor>, window: &Window, cx: &mut App) {
        let bus = Self::window_bus(window, cx);
        Rc::make_mut(&mut bus.hooks).push(hook);
    }
    fn bus<'a>(window: &Window, cx: &'a App) -> Result<&'a WindowBus, ActionError> {
        cx.try_global::<Buses>()
            .and_then(|b| b.0.get(&window.window_handle().window_id()))
            .ok_or(ActionError::UnknownTarget)
    }
    /// Enumerate this window's registered targets without taking snapshots.
    pub fn controls<'a>(
        window: &Window,
        cx: &'a App,
    ) -> impl Iterator<Item = (ControlId, &'static str, Capabilities)> + use<'a> {
        Self::bus(window, cx).into_iter().flat_map(|bus| {
            bus.entries
                .iter()
                .map(|(id, entry)| (*id, entry.kind, entry.capabilities))
        })
    }
    pub fn describe(
        id: ControlId,
        window: &Window,
        cx: &App,
    ) -> Option<(&'static str, Capabilities)> {
        let entry = Self::bus(window, cx).ok()?.entries.get(&id)?;
        Some((entry.kind, entry.capabilities))
    }
    pub fn query(id: ControlId, window: &Window, cx: &App) -> Result<ControlSnapshot, ActionError> {
        Self::bus(window, cx)?
            .entries
            .get(&id)
            .ok_or(ActionError::UnknownTarget)?
            .endpoint
            .snapshot(cx)
    }
    pub fn dispatch(
        id: ControlId,
        action: &Action,
        source: Source,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), ActionError> {
        let bus = Self::bus(window, cx)?;
        let target = bus.entries.get(&id).map(|e| e.endpoint.clone());
        let hooks = bus.hooks.clone();
        let mut called = 0;
        let result = hooks
            .iter()
            .try_for_each(|hook| {
                called += 1;
                hook.before(id, action, source)
            })
            .and_then(|()| {
                target
                    .ok_or(ActionError::UnknownTarget)?
                    .apply(action, window, cx)
            });
        for hook in hooks[..called].iter().rev() {
            hook.after(id, action, source, &result);
        }
        result
    }
}

/// Shared names on the action wire. Domain controls may define additional names.
pub mod names {
    pub const SET_VALUE: &str = "set_value";
    pub const ACTIVATE: &str = "activate";
    pub const TOGGLE: &str = "toggle";
    pub const OPEN: &str = "open";
    pub const CLOSE: &str = "close";
    pub const SELECT: &str = "select";
    pub const FOCUS: &str = "focus";
    pub const SCROLL_TO: &str = "scroll_to";
    pub const INCREMENT: &str = "increment";
    pub const DECREMENT: &str = "decrement";
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{AppContext, Empty, TestAppContext};

    #[test]
    fn erased_action_roundtrip() {
        let input = serde_json::json!({"name":"select","payload":{"indices":[1,3]}});
        assert_eq!(Action::from_json(&input).unwrap().to_json(), input);
        assert!(Action::from_json(&serde_json::json!({"name":"select"})).is_err());
    }
    #[test]
    fn flags_enumerate_only_advertised_actions() {
        let flags = Capabilities::TOGGLE | Capabilities::FOCUS;
        assert_eq!(
            flags.action_names().collect::<Vec<_>>(),
            [names::TOGGLE, names::FOCUS]
        );
        assert_eq!(Capabilities::all().action_names().count(), 10);
    }

    struct Toggle(bool);
    impl Observe for Toggle {
        type Snapshot = ControlSnapshot;
        fn snapshot(&self, _: &App) -> ControlSnapshot {
            ControlSnapshot {
                checked: Some(self.0),
                ..Default::default()
            }
        }
    }
    impl gpui_kit::EventEmitter<bool> for Toggle {}
    impl Drive for Toggle {
        type Action = ();
        fn decode(action: &Action) -> Result<(), ActionError> {
            if action.name == names::TOGGLE {
                Ok(())
            } else {
                Err(ActionError::Unsupported)
            }
        }
        fn apply(
            &mut self,
            _: (),
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> Result<(), ActionError> {
            self.0 = !self.0;
            cx.emit(self.0);
            Ok(())
        }
    }
    #[gpui_kit::test]
    fn denied_dispatch_preserves_state_and_reports_outcome(cx: &mut TestAppContext) {
        struct Deny(std::cell::Cell<usize>);
        impl Interceptor for Deny {
            fn before(&self, _: ControlId, _: &Action, _: Source) -> Result<(), ActionError> {
                Err(ActionError::ReadOnly)
            }
            fn after(&self, _: ControlId, _: &Action, _: Source, result: &Result<(), ActionError>) {
                assert_eq!(result, &Err(ActionError::ReadOnly));
                self.0.set(self.0.get() + 1);
            }
        }
        let (_, vcx) = cx.add_window_view(|_, _| Empty);
        let (entity, id) = vcx.update(|window, cx| {
            let entity = cx.new(|_| Toggle(false));
            let id = ActionBus::register(
                "panel/control",
                &entity,
                "toggle",
                Capabilities::TOGGLE,
                window,
                cx,
            );
            let deny = Rc::new(Deny(std::cell::Cell::new(0)));
            ActionBus::add_interceptor(deny.clone(), window, cx);
            let action = Action {
                name: names::TOGGLE.into(),
                payload: Value::Null,
            };
            assert_eq!(
                ActionBus::dispatch(id, &action, Source::Mcp, window, cx),
                Err(ActionError::ReadOnly)
            );
            assert_eq!(deny.0.get(), 1);
            assert!(!entity.read(cx).0);
            (entity, id)
        });
        drop(entity);
        vcx.run_until_parked();
        vcx.update(|window, cx| assert_eq!(ActionBus::describe(id, window, cx), None));
    }

    #[gpui_kit::test]
    fn every_source_reaches_the_same_control(cx: &mut TestAppContext) {
        let (_, vcx) = cx.add_window_view(|_, _| Empty);
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let entity = vcx.update(|_, cx| {
            let entity = cx.new(|_| Toggle(false));
            let events = events.clone();
            cx.subscribe(&entity, move |_, value, _| events.borrow_mut().push(*value))
                .detach();
            entity
        });
        vcx.update(|window, cx| {
            let id = ActionBus::register(
                "settings/theme",
                &entity,
                "toggle",
                Capabilities::TOGGLE,
                window,
                cx,
            );
            assert_eq!(ControlId::lookup("settings/theme", cx), Some(id));
            let action = Action {
                name: names::TOGGLE.into(),
                payload: Value::Null,
            };
            for (i, source) in [
                Source::Pointer,
                Source::Keyboard,
                Source::Shortcut,
                Source::Spotlight,
                Source::Ribbon,
                Source::DataUi,
                Source::Script,
                Source::Mcp,
                Source::Replay,
            ]
            .into_iter()
            .enumerate()
            {
                ActionBus::dispatch(id, &action, source, window, cx).unwrap();
                assert_eq!(
                    ActionBus::query(id, window, cx).unwrap().checked,
                    Some(i % 2 == 0)
                );
            }
            let invalid = Action {
                name: names::OPEN.into(),
                payload: Value::Null,
            };
            assert_eq!(
                ActionBus::dispatch(id, &invalid, Source::Mcp, window, cx),
                Err(ActionError::Unsupported)
            );
            assert_eq!(
                ActionBus::query(id, window, cx).unwrap().checked,
                Some(true)
            );
            ActionBus::unregister(id, window, cx);
            assert_eq!(
                ActionBus::dispatch(id, &action, Source::Script, window, cx),
                Err(ActionError::UnknownTarget)
            );
        });
        assert_eq!(
            *events.borrow(),
            [true, false, true, false, true, false, true, false, true]
        );
    }
}
