//! Agents and scripts drive controls on the action bus. Every source produces the same
//! events and state as the pointer path, and a recorded session replays to that state.
#![allow(clippy::unwrap_used, reason = "test assertions")]

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, IntoElement, Render, TestAppContext, TestSupportExt, VisualTestContext,
    Window, div,
};
use guiraffe_core::protocol::{
    Action as BusAction, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive,
    Observe, Recorder, Source, names, replay,
};
use guiraffe_core::widget_value::Value;
use guiraffe_test::drive::{Action, Driver};
use guiraffe_test::inspect::{EventSink, NodeId};
use guiraffe_test::transport::{Call, Server, mcp};

const PATH: &str = "doc/counter";

struct Counter {
    value: f64,
    events: EventSink,
}
impl Observe for Counter {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: Some(Value::Num(self.value)),
            label: Some("Counter".into()),
            ..Default::default()
        }
    }
}
impl Drive for Counter {
    type Action = ();
    type Output = ();
    fn decode(action: &BusAction) -> Result<(), ActionError> {
        (action.name == names::INCREMENT)
            .then_some(())
            .ok_or(ActionError::Unsupported)
    }
    fn apply(&mut self, _: (), _: &mut Window, cx: &mut Context<Self>) -> Result<(), ActionError> {
        self.value += 1.;
        self.events
            .emit("counter.changed", Some(self.value.to_string()));
        cx.notify();
        Ok(())
    }
}

/// The pointer path goes through the bus too (design section 5).
struct Host;
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("panel").test_support().child(
            div()
                .id("plus")
                .test_support()
                .on_click(|_, window, cx| {
                    if let Some(id) = ControlId::lookup(PATH, cx) {
                        let _ = ActionBus::dispatch(id, &increment(), Source::Pointer, window, cx);
                    }
                })
                .child("+"),
        )
    }
}

fn increment() -> BusAction {
    BusAction {
        name: names::INCREMENT.into(),
        payload: Value::Null,
    }
}

fn mount(cx: &mut TestAppContext, events: EventSink) -> (Entity<Counter>, &mut VisualTestContext) {
    let (_, vcx) = cx.add_window_view(|_, _| Host);
    let counter = vcx.update(|window, cx| {
        let counter = cx.new(|_| Counter { value: 0., events });
        ActionBus::register(
            PATH,
            &counter,
            "counter",
            Capabilities::INCREMENT,
            window,
            cx,
        );
        counter
    });
    vcx.run_until_parked();
    (counter, vcx)
}

#[gpui_kit::test]
fn script_and_mcp_match_the_pointer_and_replay_restores_the_state(cx: &mut TestAppContext) {
    let events = EventSink::new();
    let (_counter, vcx) = mount(cx, events.clone());
    let recorder = Recorder::new();
    vcx.update(|window, cx| ActionBus::add_interceptor(recorder.clone(), window, cx));
    let mut server = Server::new(Driver::new(vcx, events.clone()));

    let fired = |result: &guiraffe_test::drive::DriveResult| {
        result
            .events
            .iter()
            .map(|e| (e.name.clone(), e.payload.clone()))
            .collect::<Vec<_>>()
    };
    let pointer = server.driver_mut().drive(&Action::Click {
        id: NodeId::new("plus"),
    });
    assert!(pointer.ok, "{pointer:?}");
    let script = server.driver_mut().drive(&Action::Apply {
        control: PATH.into(),
        name: names::INCREMENT.into(),
        payload: serde_json::Value::Null,
    });
    assert!(script.ok, "{script:?}");
    assert_eq!(fired(&pointer)[0].0, fired(&script)[0].0);
    assert_eq!(
        fired(&script),
        [("counter.changed".to_string(), Some("2".to_string()))]
    );

    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "apply", "arguments": { "control": PATH, "action": increment().to_json() } }
    });
    let answer = mcp(&request.to_string(), &mut server).unwrap();
    assert!(answer.contains("counter.changed"), "{answer}");

    let query = server.call(Call::Query {
        control: PATH.into(),
    });
    assert!(query.ok, "{query:?}");
    assert_eq!(
        query.snapshot,
        Some(serde_json::json!({"value": 3, "label": "Counter"}))
    );
    let missing = server.call(Call::Query {
        control: "doc/nope".into(),
    });
    assert!(!missing.ok);
    let rejected = server.driver_mut().drive(&Action::Apply {
        control: PATH.into(),
        name: "open".into(),
        payload: serde_json::Value::Null,
    });
    assert!(!rejected.ok && rejected.events.is_empty(), "{rejected:?}");

    let log = recorder.take();
    assert_eq!(
        log.iter().map(|r| r.source).collect::<Vec<_>>(),
        [Source::Pointer, Source::Script, Source::Mcp]
    );
    let original = server.driver_mut().query(PATH).unwrap();
    drop(server);

    // A fresh session replays to the identical state, with the same events.
    let replay_events = EventSink::new();
    let (_counter, fresh) = mount(cx, replay_events.clone());
    fresh.update(|window, cx| replay(&log, window, cx)).unwrap();
    let mut driver = Driver::new(fresh, replay_events.clone());
    assert_eq!(driver.query(PATH).unwrap(), original);
    assert_eq!(
        replay_events
            .events()
            .iter()
            .map(|e| e.payload.clone())
            .collect::<Vec<_>>(),
        events
            .events()
            .iter()
            .map(|e| e.payload.clone())
            .collect::<Vec<_>>()
    );
}
