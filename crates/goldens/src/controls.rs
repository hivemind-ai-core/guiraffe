//! Every interactive control of the tool is registered on the action bus. Most are the
//! protocol control entities from `guiraffe_components::controls`, bound to the store in
//! `ui!` (reason, view mode, zoom) or wired to store events (accept, reject, export). This
//! module holds the three the library has no control for: the golden list, the read-only
//! diff summary and the pixel inspector.
use crate::review::{Decision, Inspect, Review, Select, Step};
use gpui_kit::{App, AppContext, Context, Entity, Window};
use guiraffe_core::{
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source, names,
    },
    widget_value::Value,
};
use guiraffe_state::Store;
use std::rc::Rc;

/// Control paths. Stable: shortcuts, scripts and recordings address them.
pub mod paths {
    /// Custom: `select` (payload: key), `increment`/`decrement`.
    pub const LIST: &str = "goldens/list";
    /// Custom, read-only: the selected golden's diff, plus the commit-message summary.
    pub const SUMMARY: &str = "goldens/summary";
    /// Custom: `set_value` (payload: `[x, y]` or null).
    pub const INSPECT: &str = "goldens/inspect";
    /// `TextInputControl`: `set_value` (text).
    pub const REASON: &str = "goldens/reason";
    /// `SelectControl<Mode>`: `select` (index: before, after, diff, side by side).
    pub const MODE: &str = "goldens/mode";
    /// `SliderControl`: `set_value`, `increment`, `decrement` (steps of 25%).
    pub const ZOOM: &str = "goldens/zoom";
    /// `ButtonControl`s: `activate`.
    pub const ACCEPT: &str = "goldens/accept";
    pub const REJECT: &str = "goldens/reject";
    pub const EXPORT: &str = "goldens/export";
    /// The detachable compare panel (pop out = `open`, dock back = `close`).
    pub const COMPARE: &str = "goldens/compare";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    List,
    Summary,
    Inspect,
}

pub struct ReviewControl {
    kind: Kind,
    store: Store<Review>,
}

pub enum Op {
    Select(u32),
    Step(i64),
    Inspect(Option<(u32, u32)>),
}

fn num(value: &Value) -> Option<f64> {
    match value {
        Value::Num(n) => Some(*n),
        Value::List(items) => items.first().and_then(num),
        _ => None,
    }
}

fn decode(kind: Kind, action: &Action) -> Result<Op, ActionError> {
    let invalid = ActionError::Invalid;
    Ok(match (kind, action.name.as_ref()) {
        (Kind::List, names::SELECT) => {
            let key = num(&action.payload)
                .filter(|n| *n >= 0.)
                .ok_or(invalid("select needs a key"))?;
            Op::Select(key as u32)
        }
        (Kind::List, names::INCREMENT) => Op::Step(1),
        (Kind::List, names::DECREMENT) => Op::Step(-1),
        (Kind::Inspect, names::SET_VALUE) => match &action.payload {
            Value::Null => Op::Inspect(None),
            Value::List(items) => match (items.first().and_then(num), items.get(1).and_then(num)) {
                (Some(x), Some(y)) if x >= 0. && y >= 0. => Op::Inspect(Some((x as u32, y as u32))),
                _ => return Err(invalid("inspect needs [x, y]")),
            },
            _ => return Err(invalid("inspect needs [x, y] or null")),
        },
        _ => return Err(ActionError::Unsupported),
    })
}

fn json(value: serde_json::Value) -> Option<Value> {
    Value::item_from_json(&value).ok()
}

/// `query(goldens/list)`: every changed golden with its key and verdict.
pub fn list_json(state: &Review) -> serde_json::Value {
    serde_json::Value::Array(
        state
            .goldens
            .iter()
            .map(|(key, row)| {
                let g = &row.golden;
                serde_json::json!({
                    "key": key,
                    "path": &*g.path,
                    "name": &*g.name,
                    "backend": g.backend.label(),
                    "theme": g.theme.label(),
                    "change": g.change.label(),
                    "verdict": row.verdict.as_ref().map(|v| match v.decision {
                        Decision::Accept => "accepted",
                        Decision::Reject => "rejected",
                    }),
                    "reason": row.verdict.as_ref().map(|v| v.reason.as_ref()),
                })
            })
            .collect(),
    )
}

/// `query(goldens/summary)`: the selected golden's diff and the verdict summary.
pub fn summary_json(state: &Review) -> serde_json::Value {
    let commit = state.commit_summary();
    let Some(row) = state.current() else {
        return serde_json::json!({ "commit_message": commit });
    };
    match state.current_loaded() {
        Some(loaded) => {
            let s = loaded.summary;
            serde_json::json!({
                "path": &*row.golden.path, "loaded": true,
                "changed": s.changed, "total": s.total, "max_delta": s.max_delta,
                "size_changed": s.size_changed(),
                "before": [s.before.0, s.before.1], "after": [s.after.0, s.after.1],
                "commit_message": commit,
            })
        }
        None => {
            serde_json::json!({ "path": &*row.golden.path, "loaded": false, "commit_message": commit })
        }
    }
}

impl Observe for ReviewControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        let state = self.store.state(cx);
        let label = |s: &str| Some(Rc::<str>::from(s));
        match self.kind {
            Kind::List => ControlSnapshot {
                value: json(list_json(&state)),
                selected: (!state.goldens.is_empty()).then(|| Rc::from([state.selected as usize])),
                label: label("Changed goldens"),
                ..Default::default()
            },
            Kind::Summary => ControlSnapshot {
                value: json(summary_json(&state)),
                label: label("Diff summary"),
                ..Default::default()
            },
            Kind::Inspect => ControlSnapshot {
                value: Some(match state.inspect {
                    Some((x, y)) => json(serde_json::json!([x, y])).unwrap_or(Value::Null),
                    None => Value::Null,
                }),
                label: label("Pixel"),
                ..Default::default()
            },
        }
    }
}

impl Drive for ReviewControl {
    // `decode` is static, but what an action means depends on the control's kind, so the
    // erased action is decoded in `apply` (API feedback: decode cannot see the instance).
    type Action = Action;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<Action, ActionError> {
        Ok(action.clone())
    }
    fn apply(
        &mut self,
        action: Action,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        match decode(self.kind, &action)? {
            Op::Select(key) => self.store.dispatch_now(Select(key), cx),
            Op::Step(n) => self.store.dispatch_now(Step(n), cx),
            Op::Inspect(p) => self.store.dispatch_now(Inspect(p), cx),
        }
        Ok(Outcome::EMPTY)
    }
}

/// Create and register the custom controls in `window`. Keep the entities alive.
pub fn register_custom(
    store: &Store<Review>,
    window: &Window,
    cx: &mut App,
) -> Vec<Entity<ReviewControl>> {
    [
        (
            Kind::List,
            paths::LIST,
            "list",
            Capabilities::SELECT | Capabilities::INCREMENT | Capabilities::DECREMENT,
        ),
        (
            Kind::Summary,
            paths::SUMMARY,
            "summary",
            Capabilities::empty(),
        ),
        (
            Kind::Inspect,
            paths::INSPECT,
            "pixel-inspector",
            Capabilities::SET_VALUE,
        ),
    ]
    .into_iter()
    .map(|(kind, path, wire, capabilities)| {
        let entity = cx.new(|_| ReviewControl {
            kind,
            store: store.clone(),
        });
        ActionBus::register(path, &entity, wire, capabilities, window, cx);
        entity
    })
    .collect()
}

/// The pointer path for elements that are not controls themselves (list rows, the image
/// area, the pop-out buttons): dispatch to the registered control with `Source::Pointer`.
pub fn dispatch(path: &str, name: &'static str, payload: Value, window: &mut Window, cx: &mut App) {
    if let Some(id) = ControlId::lookup(path, cx) {
        let action = Action {
            name: name.into(),
            payload,
        };
        let _ = ActionBus::dispatch(id, &action, Source::Pointer, window, cx);
    }
}
