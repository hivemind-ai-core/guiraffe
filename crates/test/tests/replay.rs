//! The replay: a recorded interaction script the agent can run
//! again after a fix.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, IntoElement, Render, Role, TestAppContext, TestSupportExt,
    VisualTestContext, Window, div, px,
};
use guiraffe_test::drive::{Action, Driver};
use guiraffe_test::inspect::{EventSink, NodeId};
use guiraffe_test::replay::{Assertion, Script, Step};

struct App {
    events: EventSink,
    clicks: Rc<Cell<usize>>,
    focus: FocusHandle,
}

impl Render for App {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        let clicks = self.clicks.clone();
        div().id("panel").test_support().child(
            div()
                .id("save")
                .test_support()
                .track_focus(&self.focus)
                .focusable()
                .role(Role::Button)
                .aria_label("Save")
                .w(px(120.))
                .h(px(40.))
                .on_click(move |_, _, _| {
                    clicks.set(clicks.get() + 1);
                    events.emit("save.clicked", None);
                })
                .child("Save"),
        )
    }
}

fn mount(cx: &mut TestAppContext) -> (EventSink, Rc<Cell<usize>>, &mut VisualTestContext) {
    let events = EventSink::new();
    let clicks = Rc::new(Cell::new(0));
    let host_events = events.clone();
    let host_clicks = clicks.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        events: host_events,
        clicks: host_clicks,
        focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    (events, clicks, vcx)
}

fn script() -> Script {
    Script::new(
        "settings-save",
        vec![Step {
            action: Action::Click {
                id: NodeId::new("save"),
            },
            assert: vec![
                Assertion::Present {
                    id: NodeId::new("save"),
                },
                Assertion::Events {
                    names: vec!["save.clicked".into()],
                },
            ],
        }],
    )
}

#[gpui_kit::test]
fn a_script_runs_again_and_its_assertions_hold(cx: &mut TestAppContext) {
    let (events, clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let result = guiraffe_test::replay::run(&mut driver, &script());
    assert!(result.ok(), "{result:?}");
    assert_eq!(clicks.get(), 1, "the action ran");
    assert_eq!(result.steps.len(), 1);
}

#[gpui_kit::test]
fn a_script_that_breaks_fails_with_the_reason(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let broken = Script::new(
        "broken",
        vec![Step {
            action: Action::Click {
                id: NodeId::new("save"),
            },
            assert: vec![Assertion::Events {
                names: vec!["something.else".into()],
            }],
        }],
    );
    let result = guiraffe_test::replay::run(&mut driver, &broken);
    assert!(!result.ok());
    assert!(
        result
            .failures
            .iter()
            .any(|failure| failure.contains("events")),
        "{:?}",
        result.failures
    );
}

#[gpui_kit::test]
fn a_script_is_a_json_document(cx: &mut TestAppContext) {
    let (events, _clicks, vcx) = mount(cx);
    let mut driver = Driver::new(vcx, events);
    let json = script().to_json().expect("the script is JSON");
    assert!(json.contains("save.clicked") || json.contains("click"));
    let parsed = Script::from_json(&json).expect("the script parses");
    let result = guiraffe_test::replay::run(&mut driver, &parsed);
    assert!(result.ok(), "{result:?}");
}
