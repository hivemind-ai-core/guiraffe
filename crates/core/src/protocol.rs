//! Provisional control protocol. Persist paths; integer handles belong to one App.
use crate::widget_value::Value;
use gpui_kit::{App, Context, Entity, Global, Window, WindowId};
use std::{collections::HashMap, panic::Location, rc::Rc};

mod record;
pub use record::{Recorded, Recorder, replay};

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
    fn intern(path: &str, cx: &mut App) -> Self {
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

impl ControlSnapshot {
    /// Wire form for the inspect/drive API, MCP and scripts. Absent fields are omitted.
    pub fn to_json(&self) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        if let Some(value) = &self.value {
            out.insert("value".into(), value.to_json());
        }
        for (key, flag) in [
            ("checked", self.checked),
            ("open", self.open),
            ("disabled", self.disabled),
            ("focused", self.focused),
        ] {
            if let Some(flag) = flag {
                out.insert(key.into(), flag.into());
            }
        }
        if let Some(selected) = &self.selected {
            out.insert(
                "selected".into(),
                selected.iter().copied().collect::<Vec<_>>().into(),
            );
        }
        if let Some(label) = &self.label {
            out.insert("label".into(), label.as_ref().into());
        }
        serde_json::Value::Object(out)
    }
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

/// Reasons the bus or a control rejects an action. More variants may follow.
/// Erased result of an applied action, such as `KeyInserted { t }`.
/// Empty (`Value::Null`) by default, which costs no allocation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome(pub Value);

impl Outcome {
    pub const EMPTY: Self = Self(Value::Null);
    pub fn is_empty(&self) -> bool {
        self.0 == Value::Null
    }
}
impl From<()> for Outcome {
    fn from((): ()) -> Self {
        Self::EMPTY
    }
}
impl From<Value> for Outcome {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActionError {
    UnknownTarget,
    Unsupported,
    Disabled,
    Invalid(&'static str),
    ReadOnly,
}

pub trait Drive: Sized + 'static {
    type Action: 'static;
    /// Typed result; `()` for actions with nothing to report.
    type Output: Into<Outcome>;
    fn decode(action: &Action) -> Result<Self::Action, ActionError>;
    fn apply(
        &mut self,
        action: Self::Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self::Output, ActionError>;
}

/// Where an action came from. More sources may follow.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[non_exhaustive]
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
    /// The application itself, outside any user gesture.
    Host,
    Devtools,
    Test,
}

/// Outcomes of a batch, in action order. Batches of up to eight stay inline.
pub type Outcomes = smallvec::SmallVec<[Outcome; 8]>;

/// The action at `index` was rejected and the batch stopped there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchError {
    pub index: usize,
    pub error: ActionError,
}

/// Result of `dispatch_batch`: the applied prefix's outcomes and, if the batch
/// stopped early, why.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct Batch {
    pub outcomes: Outcomes,
    pub error: Option<BatchError>,
}

impl Batch {
    /// All outcomes when every action applied; the rejection otherwise.
    pub fn into_result(self) -> Result<Outcomes, BatchError> {
        match self.error {
            None => Ok(self.outcomes),
            Some(error) => Err(error),
        }
    }
}

/// A run of actions on one target that is one undo step, such as a drag-paint.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Group {
    pub id: ControlId,
    pub source: Source,
    pub len: usize,
}

/// Observes and may veto dispatch. Hooks get read access to the window and app,
/// so they can consult the store for permission checks or recording.
pub trait Interceptor {
    fn before(
        &self,
        id: ControlId,
        action: &Action,
        source: Source,
        window: &Window,
        cx: &App,
    ) -> Result<(), ActionError>;
    fn after(
        &self,
        id: ControlId,
        action: &Action,
        source: Source,
        result: &Result<Outcome, ActionError>,
        window: &Window,
        cx: &App,
    );
    /// A `dispatch_batch` starts; its actions follow as ordinary `before`/`after` calls.
    fn begin_group(&self, _group: Group, _window: &Window, _cx: &App) {}
    /// The batch ended, after its last applied or rejected action.
    fn end_group(
        &self,
        _group: Group,
        _result: &Result<(), ActionError>,
        _window: &Window,
        _cx: &App,
    ) {
    }
}

trait Endpoint {
    fn apply(
        &self,
        action: &Action,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Outcome, ActionError>;
    fn snapshot(&self, cx: &App) -> Result<ControlSnapshot, ActionError>;
}
impl<T: Observe + Drive> Endpoint for gpui_kit::WeakEntity<T> {
    fn apply(
        &self,
        action: &Action,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Outcome, ActionError> {
        let entity = self.upgrade().ok_or(ActionError::UnknownTarget)?;
        let typed = T::decode(action)?;
        entity
            .update(cx, |state, cx| state.apply(typed, window, cx))
            .map(Into::into)
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
    owner: gpui_kit::EntityId,
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
                owner: entity.entity_id(),
                endpoint: Rc::new(entity.downgrade()),
                kind,
                capabilities,
                site: Location::caller(),
                _release: release,
            },
        );
        id
    }

    /// Release this entity's registration before re-keying or moving windows.
    /// A stale owner cannot remove the current occupant of the same path.
    pub fn unregister<T: 'static>(
        id: ControlId,
        owner: &Entity<T>,
        window: &Window,
        cx: &mut App,
    ) -> bool {
        if Self::bus(window, cx)
            .ok()
            .and_then(|bus| bus.entries.get(&id))
            .is_none_or(|entry| entry.owner != owner.entity_id())
        {
            return false;
        }
        Self::window_bus(window, cx).entries.remove(&id);
        true
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
    ) -> Result<Outcome, ActionError> {
        let bus = Self::bus(window, cx)?;
        let target = bus.entries.get(&id).map(|e| e.endpoint.clone());
        if bus.hooks.is_empty() {
            // The empty interceptor list costs a branch (design section 8.5).
            let target = target.ok_or(ActionError::UnknownTarget)?;
            return with_source(source, || target.apply(action, window, cx));
        }
        let hooks = bus.hooks.clone();
        Self::run(id, target.as_deref(), &hooks, action, source, window, cx)
    }

    /// Apply `actions` to one target as one group: interceptors see `begin_group`,
    /// each action, then `end_group`, and store history records one undo step.
    /// Returns each action's outcome, or stops at the first rejected action.
    /// Up to eight actions allocate nothing.
    pub fn dispatch_batch(
        id: ControlId,
        actions: &[Action],
        source: Source,
        window: &mut Window,
        cx: &mut App,
    ) -> Batch {
        let bus = match Self::bus(window, cx) {
            Ok(bus) => bus,
            Err(error) => {
                return Batch {
                    outcomes: Outcomes::new(),
                    error: Some(BatchError { index: 0, error }),
                };
            }
        };
        let target = bus.entries.get(&id).map(|e| e.endpoint.clone());
        let hooks = bus.hooks.clone();
        let group = Group {
            id,
            source,
            len: actions.len(),
        };
        for hook in hooks.iter() {
            hook.begin_group(group, window, cx);
        }
        let mut outcomes = Outcomes::new();
        let mut failed = None;
        for (index, action) in actions.iter().enumerate() {
            match Self::run(id, target.as_deref(), &hooks, action, source, window, cx) {
                Ok(outcome) => outcomes.push(outcome),
                Err(error) => {
                    failed = Some((index, error));
                    break;
                }
            }
        }
        let result = match &failed {
            None => Ok(()),
            Some((_, error)) => Err(error.clone()),
        };
        for hook in hooks.iter().rev() {
            hook.end_group(group, &result, window, cx);
        }
        Batch {
            outcomes,
            error: failed.map(|(index, error)| BatchError { index, error }),
        }
    }

    fn run(
        id: ControlId,
        target: Option<&dyn Endpoint>,
        hooks: &[Rc<dyn Interceptor>],
        action: &Action,
        source: Source,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Outcome, ActionError> {
        let mut called = 0;
        let result = hooks
            .iter()
            .try_for_each(|hook| {
                called += 1;
                hook.before(id, action, source, window, cx)
            })
            .and_then(|()| {
                let target = target.ok_or(ActionError::UnknownTarget)?;
                with_source(source, || target.apply(action, window, cx))
            });
        for hook in hooks[..called].iter().rev() {
            hook.after(id, action, source, &result, window, cx);
        }
        result
    }
}

thread_local! {
    static CURRENT_SOURCE: std::cell::Cell<Option<Source>> = const { std::cell::Cell::new(None) };
}

/// Restores the enclosing dispatch's source, also when `apply` unwinds.
struct RestoreSource(Option<Source>);

impl Drop for RestoreSource {
    fn drop(&mut self) {
        CURRENT_SOURCE.with(|current| current.set(self.0));
    }
}

fn with_source<R>(source: Source, apply: impl FnOnce() -> R) -> R {
    let _restore = RestoreSource(CURRENT_SOURCE.with(|current| current.replace(Some(source))));
    apply()
}

impl ActionBus {
    /// The source of the action being applied, while a control's `Drive::apply` runs
    /// under [`ActionBus::dispatch`] or [`ActionBus::dispatch_batch`]. `None` outside a
    /// dispatch. Nested dispatches see their own source and restore the outer one.
    pub fn current_source(_cx: &App) -> Option<Source> {
        CURRENT_SOURCE.with(std::cell::Cell::get)
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
        assert_eq!(
            Action::from_json(&input).map(|action| action.to_json()),
            Ok(input)
        );
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
    /// A typed outcome with its erased conversion.
    struct Toggled(bool);
    impl From<Toggled> for Outcome {
        fn from(Toggled(on): Toggled) -> Self {
            Self(Value::Bool(on))
        }
    }
    impl Drive for Toggle {
        type Action = ();
        type Output = Toggled;
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
        ) -> Result<Toggled, ActionError> {
            self.0 = !self.0;
            cx.emit(self.0);
            Ok(Toggled(self.0))
        }
    }
    #[gpui_kit::test]
    fn windows_share_path_identity_and_resolve_their_own_targets(cx: &mut TestAppContext) {
        let (_, first) = cx.add_window_view(|_, _| Empty);
        let (first_id, _keep_first) = first.update(|window, cx| {
            let entity = cx.new(|_| Toggle(true));
            let id = ActionBus::register(
                "panel/theme",
                &entity,
                "toggle",
                Capabilities::TOGGLE,
                window,
                cx,
            );
            (id, entity)
        });
        let (_, second) = cx.add_window_view(|_, _| Empty);
        second.update(|window, cx| {
            assert_eq!(ControlId::lookup("panel/theme", cx), Some(first_id));
            assert_eq!(
                ActionBus::query(first_id, window, cx),
                Err(ActionError::UnknownTarget)
            );
            let entity = cx.new(|_| Toggle(false));
            let id = ActionBus::register(
                "panel/theme",
                &entity,
                "toggle",
                Capabilities::TOGGLE,
                window,
                cx,
            );
            assert_eq!(first_id, id);
            assert_eq!(id.path(cx), Some("panel/theme"));
            assert_eq!(
                ActionBus::query(id, window, cx).map(|s| s.checked),
                Ok(Some(false))
            );
            assert_eq!(
                ActionBus::controls(window, cx).collect::<Vec<_>>(),
                [(id, "toggle", Capabilities::TOGGLE)]
            );
        });
    }

    #[gpui_kit::test]
    fn denied_dispatch_preserves_state_and_reports_outcome(cx: &mut TestAppContext) {
        struct Deny(std::cell::Cell<usize>);
        impl Interceptor for Deny {
            fn before(
                &self,
                _: ControlId,
                _: &Action,
                _: Source,
                _: &Window,
                _: &App,
            ) -> Result<(), ActionError> {
                Err(ActionError::ReadOnly)
            }
            fn after(
                &self,
                _: ControlId,
                _: &Action,
                _: Source,
                result: &Result<Outcome, ActionError>,
                _: &Window,
                _: &App,
            ) {
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
        vcx.update(|_, _| drop(entity));
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
                Source::Host,
                Source::Devtools,
                Source::Test,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(
                    ActionBus::dispatch(id, &action, source, window, cx),
                    Ok(Outcome(Value::Bool(i % 2 == 0)))
                );
                assert_eq!(
                    ActionBus::query(id, window, cx).map(|s| s.checked),
                    Ok(Some(i % 2 == 0))
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
                ActionBus::query(id, window, cx).map(|s| s.checked),
                Ok(Some(false))
            );
            let stranger = cx.new(|_| Toggle(false));
            assert!(!ActionBus::unregister(id, &stranger, window, cx));
            assert!(ActionBus::unregister(id, &entity, window, cx));
            assert_eq!(
                ActionBus::dispatch(id, &action, Source::Script, window, cx),
                Err(ActionError::UnknownTarget)
            );
        });
        assert_eq!(
            *events.borrow(),
            [
                true, false, true, false, true, false, true, false, true, false, true, false
            ]
        );
    }

    #[gpui_kit::test]
    fn batch_is_one_group_and_hooks_see_outcomes_and_state(cx: &mut TestAppContext) {
        use std::cell::RefCell;
        #[derive(Default)]
        struct Record {
            target: RefCell<Option<Entity<Toggle>>>,
            log: RefCell<Vec<String>>,
        }
        impl Interceptor for Record {
            fn before(
                &self,
                _: ControlId,
                action: &Action,
                _: Source,
                _: &Window,
                cx: &App,
            ) -> Result<(), ActionError> {
                // Read access: a permission check can consult current state.
                let on = self.target.borrow().as_ref().map(|e| e.read(cx).0);
                self.log
                    .borrow_mut()
                    .push(format!("before {} {on:?}", action.name));
                if action.name == names::OPEN {
                    return Err(ActionError::ReadOnly);
                }
                Ok(())
            }
            fn after(
                &self,
                _: ControlId,
                _: &Action,
                _: Source,
                result: &Result<Outcome, ActionError>,
                _: &Window,
                _: &App,
            ) {
                self.log.borrow_mut().push(format!("after {result:?}"));
            }
            fn begin_group(&self, group: Group, _: &Window, _: &App) {
                self.log
                    .borrow_mut()
                    .push(format!("begin {} {:?}", group.len, group.source));
            }
            fn end_group(&self, _: Group, result: &Result<(), ActionError>, _: &Window, _: &App) {
                self.log.borrow_mut().push(format!("end {result:?}"));
            }
        }
        let (_, vcx) = cx.add_window_view(|_, _| Empty);
        vcx.update(|window, cx| {
            let entity = cx.new(|_| Toggle(false));
            let id = ActionBus::register(
                "grid/steps",
                &entity,
                "toggle",
                Capabilities::TOGGLE,
                window,
                cx,
            );
            let hook = Rc::new(Record::default());
            *hook.target.borrow_mut() = Some(entity.clone());
            ActionBus::add_interceptor(hook.clone(), window, cx);
            let toggle = Action {
                name: names::TOGGLE.into(),
                payload: Value::Null,
            };
            let open = Action {
                name: names::OPEN.into(),
                payload: Value::Null,
            };
            assert_eq!(
                ActionBus::dispatch_batch(
                    id,
                    &[toggle.clone(), toggle.clone()],
                    Source::Pointer,
                    window,
                    cx
                ),
                Batch {
                    error: None,
                    outcomes: Outcomes::from_iter([
                        Outcome(Value::Bool(true)),
                        Outcome(Value::Bool(false))
                    ]),
                }
            );
            assert_eq!(
                ActionBus::dispatch_batch(
                    id,
                    &[toggle.clone(), open, toggle],
                    Source::Pointer,
                    window,
                    cx
                ),
                Batch {
                    outcomes: Outcomes::from_iter([Outcome(Value::Bool(true))]),
                    error: Some(BatchError {
                        index: 1,
                        error: ActionError::ReadOnly,
                    }),
                }
            );
            assert!(entity.read(cx).0, "the rejected action stops the batch");
            assert_eq!(
                *hook.log.borrow(),
                [
                    "begin 2 Pointer",
                    "before toggle Some(false)",
                    "after Ok(Outcome(Bool(true)))",
                    "before toggle Some(true)",
                    "after Ok(Outcome(Bool(false)))",
                    "end Ok(())",
                    "begin 3 Pointer",
                    "before toggle Some(false)",
                    "after Ok(Outcome(Bool(true)))",
                    "before open Some(true)",
                    "after Err(ReadOnly)",
                    "end Err(ReadOnly)",
                ]
            );
            drop(hook.target.borrow_mut().take());
        });
    }

    #[gpui_kit::test]
    fn apply_sees_its_own_source_and_nested_dispatch_restores_it(cx: &mut TestAppContext) {
        use std::cell::RefCell;
        struct Probe {
            inner: Option<ControlId>,
            seen: Rc<RefCell<Vec<Option<Source>>>>,
        }
        impl Observe for Probe {
            type Snapshot = ControlSnapshot;
            fn snapshot(&self, _: &App) -> ControlSnapshot {
                ControlSnapshot::default()
            }
        }
        impl Drive for Probe {
            type Action = ();
            type Output = ();
            fn decode(_: &Action) -> Result<(), ActionError> {
                Ok(())
            }
            fn apply(
                &mut self,
                _: (),
                window: &mut Window,
                cx: &mut Context<Self>,
            ) -> Result<(), ActionError> {
                self.seen.borrow_mut().push(ActionBus::current_source(cx));
                if let Some(inner) = self.inner {
                    let action = Action {
                        name: names::ACTIVATE.into(),
                        payload: Value::Null,
                    };
                    ActionBus::dispatch(inner, &action, Source::Script, window, cx)?;
                    self.seen.borrow_mut().push(ActionBus::current_source(cx));
                }
                Ok(())
            }
        }
        let (_, vcx) = cx.add_window_view(|_, _| Empty);
        let seen = Rc::new(RefCell::new(Vec::new()));
        vcx.update(|window, cx| {
            let inner = cx.new(|_| Probe {
                inner: None,
                seen: seen.clone(),
            });
            let inner_id = ActionBus::register(
                "nest/inner",
                &inner,
                "probe",
                Capabilities::ACTIVATE,
                window,
                cx,
            );
            let outer = cx.new(|_| Probe {
                inner: Some(inner_id),
                seen: seen.clone(),
            });
            let outer_id = ActionBus::register(
                "nest/outer",
                &outer,
                "probe",
                Capabilities::ACTIVATE,
                window,
                cx,
            );
            let action = Action {
                name: names::ACTIVATE.into(),
                payload: Value::Null,
            };
            assert_eq!(ActionBus::current_source(cx), None);
            assert!(ActionBus::dispatch(outer_id, &action, Source::Mcp, window, cx).is_ok());
            assert_eq!(ActionBus::current_source(cx), None);
            assert!(
                ActionBus::dispatch_batch(outer_id, &[action], Source::Ribbon, window, cx)
                    .error
                    .is_none()
            );
            assert_eq!(ActionBus::current_source(cx), None);
            drop((inner, outer));
        });
        assert_eq!(
            *seen.borrow(),
            [
                Some(Source::Mcp),
                Some(Source::Script),
                Some(Source::Mcp),
                Some(Source::Ribbon),
                Some(Source::Script),
                Some(Source::Ribbon),
            ]
        );
    }
}
