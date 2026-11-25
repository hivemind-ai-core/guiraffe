//! Every interactive control of the tool is a registered bus control: pointer, keyboard
//! shortcuts, Spotlight, MCP and scripts all apply the same actions, so they produce the
//! same store events and state.
//!
//! The controls are thin: a kind, the store, and a mapping from protocol actions to store
//! events. Snapshots read the store.
use crate::review::{
    Decide, Decision, Inspect, Mode, Review, Select, SetMode, SetReason, SetZoom, Step, ZoomStep,
};
use gpui_kit::{App, AppContext, Context, Entity, Window};
use guiraffe_core::{
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source, names,
    },
    widget_value::Value,
};
use guiraffe_state::Store;
use std::sync::Arc;

/// Control paths. Stable: shortcuts, scripts and recordings address them.
pub mod paths {
    pub const LIST: &str = "goldens/list";
    pub const SUMMARY: &str = "goldens/summary";
    pub const MODE: &str = "goldens/mode";
    pub const ZOOM: &str = "goldens/zoom";
    pub const INSPECT: &str = "goldens/inspect";
    pub const REASON: &str = "goldens/reason";
    pub const ACCEPT: &str = "goldens/accept";
    pub const REJECT: &str = "goldens/reject";
    pub const EXPORT: &str = "goldens/export";
    /// The detachable compare panel (pop out = `open`, dock back = `close`).
    pub const COMPARE: &str = "goldens/compare";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `select` (payload: index), `increment`/`decrement` (next/previous).
    List,
    /// Read-only: the diff summary of the selected golden.
    Summary,
    /// `set_value` (payload: "before" | "after" | "diff" | "side-by-side").
    Mode,
    /// `set_value` (payload: zoom factor), `increment`/`decrement` (double/halve).
    Zoom,
    /// `set_value` (payload: `[x, y]` or null).
    Inspect,
    /// `set_value` (payload: text).
    Reason,
    /// `activate` (payload: an optional reason overriding the typed one).
    Accept,
    Reject,
    /// `activate`: answers the commit-message summary as the outcome.
    Export,
}

impl Kind {
    pub const ALL: [(Kind, &'static str); 9] = [
        (Kind::List, paths::LIST),
        (Kind::Summary, paths::SUMMARY),
        (Kind::Mode, paths::MODE),
        (Kind::Zoom, paths::ZOOM),
        (Kind::Inspect, paths::INSPECT),
        (Kind::Reason, paths::REASON),
        (Kind::Accept, paths::ACCEPT),
        (Kind::Reject, paths::REJECT),
        (Kind::Export, paths::EXPORT),
    ];
    fn capabilities(self) -> Capabilities {
        match self {
            Kind::List => Capabilities::SELECT | Capabilities::INCREMENT | Capabilities::DECREMENT,
            Kind::Summary => Capabilities::empty(),
            Kind::Mode | Kind::Inspect | Kind::Reason => Capabilities::SET_VALUE,
            Kind::Zoom => {
                Capabilities::SET_VALUE | Capabilities::INCREMENT | Capabilities::DECREMENT
            }
            Kind::Accept | Kind::Reject | Kind::Export => Capabilities::ACTIVATE,
        }
    }
    fn wire(self) -> &'static str {
        match self {
            Kind::List => "list",
            Kind::Summary => "summary",
            Kind::Mode => "segmented",
            Kind::Zoom => "stepper",
            Kind::Inspect => "pixel-inspector",
            Kind::Reason => "text-input",
            Kind::Accept | Kind::Reject | Kind::Export => "button",
        }
    }
}

pub struct ReviewControl {
    pub kind: Kind,
    store: Store<Review>,
}

/// Decoded actions; each is one store event.
pub enum Op {
    Select(usize),
    Step(i64),
    Mode(Mode),
    Zoom(f32),
    ZoomStep(i32),
    Inspect(Option<(u32, u32)>),
    Reason(Arc<str>),
    Decide(Decision, Option<Arc<str>>),
    Export,
}

fn num(value: &Value) -> Option<f64> {
    match value {
        Value::Num(n) => Some(*n),
        Value::List(items) => items.first().and_then(num),
        _ => None,
    }
}

fn text(value: &Value) -> Option<Arc<str>> {
    match value {
        Value::Str(s) => Some(s.as_ref().into()),
        _ => None,
    }
}

impl ReviewControl {
    fn decode_for(kind: Kind, action: &Action) -> Result<Op, ActionError> {
        let name = action.name.as_ref();
        let payload = &action.payload;
        let invalid = ActionError::Invalid;
        Ok(match (kind, name) {
            (Kind::List, names::SELECT) => {
                let index = num(payload)
                    .filter(|n| *n >= 0.)
                    .ok_or(invalid("select needs an index"))?;
                Op::Select(index as usize)
            }
            (Kind::List, names::INCREMENT) => Op::Step(1),
            (Kind::List, names::DECREMENT) => Op::Step(-1),
            (Kind::Mode, names::SET_VALUE) => Op::Mode(
                text(payload)
                    .and_then(|t| Mode::parse(&t))
                    .ok_or(invalid("unknown mode"))?,
            ),
            (Kind::Zoom, names::SET_VALUE) => {
                Op::Zoom(num(payload).ok_or(invalid("zoom needs a number"))? as f32)
            }
            (Kind::Zoom, names::INCREMENT) => Op::ZoomStep(1),
            (Kind::Zoom, names::DECREMENT) => Op::ZoomStep(-1),
            (Kind::Inspect, names::SET_VALUE) => match payload {
                Value::Null => Op::Inspect(None),
                Value::List(items) => {
                    match (items.first().and_then(num), items.get(1).and_then(num)) {
                        (Some(x), Some(y)) if x >= 0. && y >= 0. => {
                            Op::Inspect(Some((x as u32, y as u32)))
                        }
                        _ => return Err(invalid("inspect needs [x, y]")),
                    }
                }
                _ => return Err(invalid("inspect needs [x, y] or null")),
            },
            (Kind::Reason, names::SET_VALUE) => {
                Op::Reason(text(payload).ok_or(invalid("reason needs text"))?)
            }
            (Kind::Accept, names::ACTIVATE) => Op::Decide(Decision::Accept, text(payload)),
            (Kind::Reject, names::ACTIVATE) => Op::Decide(Decision::Reject, text(payload)),
            (Kind::Export, names::ACTIVATE) => Op::Export,
            _ => return Err(ActionError::Unsupported),
        })
    }
}

/// The payload of `query(goldens/list)`: every changed golden with its verdict.
pub fn list_value(state: &Review) -> Value {
    Value::item_from_json(&golden_rows(state)).unwrap_or(Value::Null)
}

fn golden_rows(state: &Review) -> serde_json::Value {
    serde_json::Value::Array(
        state
            .goldens
            .iter()
            .map(|g| {
                let verdict = state.verdict(g);
                serde_json::json!({
                    "path": &*g.path,
                    "name": &*g.name,
                    "backend": g.backend.label(),
                    "theme": g.theme.label(),
                    "change": g.change.label(),
                    "verdict": verdict.map(|v| match v.decision { Decision::Accept => "accepted", Decision::Reject => "rejected" }),
                    "reason": verdict.map(|v| &*v.reason),
                })
            })
            .collect(),
    )
}

/// The payload of `query(goldens/summary)`: the selected golden's diff.
pub fn summary_json(state: &Review) -> serde_json::Value {
    let Some(golden) = state.current() else {
        return serde_json::Value::Null;
    };
    match state.current_loaded() {
        Some(loaded) => {
            let s = loaded.summary;
            serde_json::json!({
                "path": &*golden.path, "loaded": true,
                "changed": s.changed, "total": s.total, "max_delta": s.max_delta,
                "size_changed": s.size_changed(),
                "before": [s.before.0, s.before.1], "after": [s.after.0, s.after.1],
            })
        }
        None => serde_json::json!({ "path": &*golden.path, "loaded": false }),
    }
}

impl Observe for ReviewControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        let state = self.store.state(cx);
        let label = |s: &str| Some(Rc::<str>::from(s));
        use std::rc::Rc;
        match self.kind {
            Kind::List => ControlSnapshot {
                value: Some(list_value(&state)),
                selected: (!state.goldens.is_empty()).then(|| Rc::from([state.selected])),
                label: label("Changed goldens"),
                ..Default::default()
            },
            Kind::Summary => ControlSnapshot {
                value: Value::item_from_json(&summary_json(&state)).ok(),
                label: label("Diff summary"),
                ..Default::default()
            },
            Kind::Mode => ControlSnapshot {
                value: Some(Value::Str(state.view.mode.wire().into())),
                label: label("View"),
                ..Default::default()
            },
            Kind::Zoom => ControlSnapshot {
                value: Some(Value::Num(f64::from(state.view.zoom))),
                label: label("Zoom"),
                ..Default::default()
            },
            Kind::Inspect => ControlSnapshot {
                value: Some(match state.view.inspect {
                    Some((x, y)) => {
                        Value::item_from_json(&serde_json::json!([x, y])).unwrap_or(Value::Null)
                    }
                    None => Value::Null,
                }),
                label: label("Pixel"),
                ..Default::default()
            },
            Kind::Reason => ControlSnapshot {
                value: Some(Value::Str(state.reason.as_ref().into())),
                label: label("Reason"),
                ..Default::default()
            },
            Kind::Accept | Kind::Reject => ControlSnapshot {
                disabled: Some(state.current().is_none()),
                label: label(if self.kind == Kind::Accept {
                    "Accept golden"
                } else {
                    "Reject golden"
                }),
                ..Default::default()
            },
            Kind::Export => ControlSnapshot {
                value: Some(Value::Str(state.commit_summary().into())),
                label: label("Export verdicts"),
                ..Default::default()
            },
        }
    }
}

impl Drive for ReviewControl {
    // `decode` is static, but what an action means depends on the control's kind, so the
    // erased action is decoded in `apply` (API feedback: decode needs `&self`).
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
        let op = Self::decode_for(self.kind, &action)?;
        self.run(op, cx)
    }
}

/// Create and register every review control in `window`. Keep the entities alive.
pub fn register_all(
    store: &Store<Review>,
    window: &Window,
    cx: &mut App,
) -> Vec<Entity<ReviewControl>> {
    Kind::ALL
        .into_iter()
        .map(|(kind, path)| {
            let entity = cx.new(|_| ReviewControl {
                kind,
                store: store.clone(),
            });
            ActionBus::register(path, &entity, kind.wire(), kind.capabilities(), window, cx);
            entity
        })
        .collect()
}

/// The pointer path: views dispatch the same actions with [`Source::Pointer`].
pub fn dispatch(
    path: &str,
    name: &'static str,
    payload: Value,
    source: Source,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(id) = ControlId::lookup(path, cx) {
        let action = Action {
            name: name.into(),
            payload,
        };
        let _ = ActionBus::dispatch(id, &action, source, window, cx);
    }
}

impl ReviewControl {
    fn run(&mut self, op: Op, cx: &mut App) -> Result<Outcome, ActionError> {
        let store = &self.store;
        if matches!(op, Op::Decide(..)) && store.state(cx).current().is_none() {
            return Err(ActionError::Disabled);
        }
        match op {
            Op::Select(i) => store.dispatch_now(Select(i), cx),
            Op::Step(n) => store.dispatch_now(Step(n), cx),
            Op::Mode(m) => store.dispatch_now(SetMode(m), cx),
            Op::Zoom(z) => store.dispatch_now(SetZoom(z), cx),
            Op::ZoomStep(n) => store.dispatch_now(ZoomStep(n), cx),
            Op::Inspect(p) => store.dispatch_now(Inspect(p), cx),
            Op::Reason(r) => store.dispatch_now(SetReason(r), cx),
            Op::Decide(decision, reason) => store.dispatch_now(Decide { decision, reason }, cx),
            Op::Export => {
                let summary = store.state(cx).commit_summary();
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(summary.clone()));
                return Ok(Outcome(Value::Str(summary.into())));
            }
        }
        Ok(Outcome::EMPTY)
    }
}
