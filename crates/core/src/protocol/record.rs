//! Recording is an interceptor; replay is a source (design section 6).
use super::{Action, ActionBus, ActionError, ControlId, Group, Interceptor, Outcome, Source};
use gpui_kit::{App, Window};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// One applied action, addressed by its control path so it replays in a fresh app.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub path: Rc<str>,
    pub action: Action,
    pub source: Source,
    /// Actions of one `dispatch_batch` share a group number and replay as one batch.
    pub group: Option<u32>,
}

/// Records every action the bus applied successfully, except replays.
/// Install with [`ActionBus::add_interceptor`]; read with [`Recorder::take`].
#[derive(Default)]
pub struct Recorder {
    log: RefCell<Vec<Recorded>>,
    group: Cell<Option<u32>>,
    next_group: Cell<u32>,
}

impl Recorder {
    pub fn new() -> Rc<Self> {
        Rc::default()
    }
    /// The recording so far, oldest first.
    pub fn log(&self) -> Vec<Recorded> {
        self.log.borrow().clone()
    }
    pub fn take(&self) -> Vec<Recorded> {
        self.log.take()
    }
    pub fn len(&self) -> usize {
        self.log.borrow().len()
    }
    pub fn is_empty(&self) -> bool {
        self.log.borrow().is_empty()
    }
}

impl Interceptor for Recorder {
    fn before(
        &self,
        _: ControlId,
        _: &Action,
        _: Source,
        _: &Window,
        _: &App,
    ) -> Result<(), ActionError> {
        Ok(())
    }
    fn after(
        &self,
        id: ControlId,
        action: &Action,
        source: Source,
        result: &Result<Outcome, ActionError>,
        _: &Window,
        cx: &App,
    ) {
        if source == Source::Replay || result.is_err() {
            return;
        }
        let Some(path) = id.path(cx) else { return };
        self.log.borrow_mut().push(Recorded {
            path: path.into(),
            action: action.clone(),
            source,
            group: self.group.get(),
        });
    }
    fn begin_group(&self, group: Group, _: &Window, _: &App) {
        if group.source == Source::Replay {
            return;
        }
        let next = self.next_group.get();
        self.next_group.set(next.wrapping_add(1));
        self.group.set(Some(next));
    }
    fn end_group(&self, _: Group, _: &Result<(), ActionError>, _: &Window, _: &App) {
        self.group.set(None);
    }
}

/// Dispatch a recording again with [`Source::Replay`]. Stops at the first rejection and
/// returns its position. Controls are resolved by path in `window`.
pub fn replay(
    log: &[Recorded],
    window: &mut Window,
    cx: &mut App,
) -> Result<(), (usize, ActionError)> {
    let mut at = 0;
    while at < log.len() {
        let first = &log[at];
        let id = ControlId::lookup(&first.path, cx).ok_or((at, ActionError::UnknownTarget))?;
        let len = match first.group {
            Some(group) => log[at..]
                .iter()
                .take_while(|r| r.group == Some(group) && r.path == first.path)
                .count(),
            None => 1,
        };
        if first.group.is_some() {
            let actions: Vec<Action> = log[at..at + len].iter().map(|r| r.action.clone()).collect();
            ActionBus::dispatch_batch(id, &actions, Source::Replay, window, cx)
                .map_err(|e| (at, e))?;
        } else {
            ActionBus::dispatch(id, &first.action, Source::Replay, window, cx)
                .map_err(|e| (at, e))?;
        }
        at += len;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::*;
    use crate::{
        protocol::{Capabilities, ControlSnapshot, Drive, Observe, names},
        widget_value::Value,
    };
    use gpui_kit::{AppContext, Context, Empty, TestAppContext};

    /// A counter with a free-form value, so order matters on replay.
    struct Counter(f64);
    impl Observe for Counter {
        type Snapshot = ControlSnapshot;
        fn snapshot(&self, _: &App) -> ControlSnapshot {
            ControlSnapshot {
                value: Some(Value::Num(self.0)),
                ..Default::default()
            }
        }
    }
    impl Drive for Counter {
        type Action = Option<f64>;
        type Output = ();
        fn decode(action: &Action) -> Result<Option<f64>, ActionError> {
            match (action.name.as_ref(), &action.payload) {
                (names::INCREMENT, _) => Ok(None),
                (names::SET_VALUE, Value::Num(n)) => Ok(Some(*n)),
                _ => Err(ActionError::Unsupported),
            }
        }
        fn apply(
            &mut self,
            action: Option<f64>,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> Result<(), ActionError> {
            self.0 = action.unwrap_or(self.0 + 1.);
            cx.notify();
            Ok(())
        }
    }
    fn action(name: &str, payload: Value) -> Action {
        Action {
            name: name.to_owned().into(),
            payload,
        }
    }

    #[gpui_kit::test]
    fn recorded_session_replays_to_identical_state(cx: &mut TestAppContext) {
        let paths = ["doc/a", "doc/b"];
        let (_, first) = cx.add_window_view(|_, _| Empty);
        let (recorder, original, _keep) = first.update(|window, cx| {
            let recorder = Recorder::new();
            ActionBus::add_interceptor(recorder.clone(), window, cx);
            let keep: Vec<_> = paths
                .iter()
                .map(|path| {
                    let entity = cx.new(|_| Counter(0.));
                    ActionBus::register(
                        path,
                        &entity,
                        "counter",
                        Capabilities::INCREMENT | Capabilities::SET_VALUE,
                        window,
                        cx,
                    );
                    entity
                })
                .collect();
            let [a, b] = paths.map(|p| ControlId::lookup(p, cx).unwrap());
            ActionBus::dispatch(
                a,
                &action(names::SET_VALUE, Value::Num(5.)),
                Source::Pointer,
                window,
                cx,
            )
            .unwrap();
            ActionBus::dispatch(
                b,
                &action(names::INCREMENT, Value::Null),
                Source::Shortcut,
                window,
                cx,
            )
            .unwrap();
            ActionBus::dispatch_batch(
                a,
                &[
                    action(names::INCREMENT, Value::Null),
                    action(names::INCREMENT, Value::Null),
                ],
                Source::Mcp,
                window,
                cx,
            )
            .unwrap();
            // Rejected actions are not part of the session.
            assert!(
                ActionBus::dispatch(
                    b,
                    &action(names::OPEN, Value::Null),
                    Source::Script,
                    window,
                    cx
                )
                .is_err()
            );
            ActionBus::dispatch(
                b,
                &action(names::SET_VALUE, Value::Num(-2.)),
                Source::Spotlight,
                window,
                cx,
            )
            .unwrap();
            let state: Vec<_> = [a, b]
                .map(|id| ActionBus::query(id, window, cx).unwrap())
                .into();
            (recorder, state, keep)
        });
        let log = recorder.log();
        assert_eq!(log.len(), 5);
        assert_eq!(log[2].group, log[3].group);
        assert!(log[2].group.is_some());

        // A fresh window with fresh controls, as a new app session.
        let (_, second) = cx.add_window_view(|_, _| Empty);
        second.update(|window, cx| {
            let _keep: Vec<_> = paths
                .iter()
                .map(|path| {
                    let entity = cx.new(|_| Counter(0.));
                    ActionBus::register(
                        path,
                        &entity,
                        "counter",
                        Capabilities::INCREMENT | Capabilities::SET_VALUE,
                        window,
                        cx,
                    );
                    entity
                })
                .collect();
            let watch = Recorder::new();
            ActionBus::add_interceptor(watch.clone(), window, cx);
            replay(&log, window, cx).unwrap();
            let replayed: Vec<_> = paths
                .map(|p| ActionBus::query(ControlId::lookup(p, cx).unwrap(), window, cx).unwrap())
                .into();
            assert_eq!(replayed, original);
            assert!(watch.is_empty(), "replays are not recorded again");
        });
    }
}
