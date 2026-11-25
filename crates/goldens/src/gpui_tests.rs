#![allow(clippy::unwrap_used, reason = "test assertions")]
use crate::{
    controls::paths,
    review::{Decision, Mode, Review, SetGoldens},
    view::ReviewApp,
};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, test::TestWindowExt};
use guiraffe_core::protocol::{Action, ActionBus, ControlId, Recorder, Source, names};
use guiraffe_state::{EventRecord, Interceptor};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Store events, without the background loads (their order depends on the executor).
#[derive(Clone, Default)]
struct Events(Rc<RefCell<Vec<String>>>);
impl Interceptor<Review> for Events {
    fn after(&mut self, record: &EventRecord, _: &Arc<Review>, _: &Arc<Review>) {
        if !record.description.starts_with("LoadedGolden")
            && !record.description.starts_with("Failed")
        {
            self.0.borrow_mut().push(record.description.clone());
        }
    }
}

fn mount(
    cx: &mut TestAppContext,
    goldens: Option<Vec<crate::discover::Golden>>,
) -> (Entity<ReviewApp>, Events, &mut VisualTestContext) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        guiraffe_commands::init(cx);
    });
    let (source, demo) = crate::demo();
    let events = Events::default();
    let recorder = events.clone();
    let mut app = None;
    let window = cx
        .update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let store = crate::store(Arc::new(source), cx);
                store.add_interceptor(recorder, cx);
                store.dispatch_now(SetGoldens(goldens.unwrap_or(demo)), cx);
                let view = cx.new(|cx| ReviewApp::new(store, window, cx));
                app = Some(view.clone());
                guiraffe_overlays::root(view, window, cx)
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
    vcx.run_until_parked();
    (app.unwrap(), events, vcx)
}

fn state(app: &Entity<ReviewApp>, vcx: &mut VisualTestContext) -> Arc<Review> {
    vcx.update(|_, cx| app.read(cx).store().state(cx))
}

/// The review-relevant part of the state (loaded images are compared by identity).
fn reviewed(s: &Review) -> (usize, Vec<(String, Decision, String)>, Mode, String) {
    (
        s.selected,
        s.verdicts
            .iter()
            .map(|(k, v)| (k.to_string(), v.decision, v.reason.to_string()))
            .collect(),
        s.view.mode,
        s.reason.to_string(),
    )
}

#[gpui_kit::test]
fn pointer_and_mcp_produce_the_same_events_and_state(cx: &mut TestAppContext) {
    // Pointer: click a row, a view mode, Accept; type a reason; Reject.
    let (pointer_app, pointer_events, vcx) = mount(cx, None);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click(("golden-row", 2usize), cx);
        window.click("mode-side-by-side", cx);
        window.click("golden-accept", cx);
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        let focus = pointer_app.read(cx).list().read(cx).reason_focus(cx);
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.input("clipped", cx);
        let value = pointer_app.read(cx).list().read(cx).reason_value(cx);
        eprintln!("DIAG value={value:?}");
    });
    vcx.run_until_parked();
    assert_eq!(
        &*state(&pointer_app, vcx).reason,
        "clipped",
        "typing reaches the store"
    );
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click("golden-reject", cx);
    });
    vcx.run_until_parked();
    let by_pointer = state(&pointer_app, vcx);

    // MCP: the same actions as tool calls, no simulated input.
    let (mcp_app, mcp_events, vcx) = mount(cx, None);
    let recorder = Recorder::new();
    vcx.update(|window, cx| ActionBus::add_interceptor(recorder.clone(), window, cx));
    let mut server = guiraffe_test::transport::Server::new(guiraffe_test::drive::Driver::new(
        vcx,
        guiraffe_test::inspect::EventSink::new(),
    ));
    let call = |server: &mut guiraffe_test::transport::Server<'_, VisualTestContext>,
                name: &str,
                args: serde_json::Value| {
        let request = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}});
        let answer = guiraffe_test::transport::mcp(&request.to_string(), server).unwrap();
        assert!(!answer.contains("\"isError\":true"), "{answer}");
        answer
    };
    let apply = |server: &mut _, control: &str, name: &str, payload: serde_json::Value| {
        call(
            server,
            "apply",
            serde_json::json!({"control": control, "action": {"name": name, "payload": payload}}),
        )
    };
    // An agent lists the changed goldens and reads a diff summary first.
    let listed = call(
        &mut server,
        "query",
        serde_json::json!({"control": paths::LIST}),
    );
    assert!(
        listed.contains("button/dark") && listed.contains("gpui-fast"),
        "{listed}"
    );
    apply(
        &mut server,
        paths::LIST,
        names::SELECT,
        serde_json::json!(2),
    );
    let summary = call(
        &mut server,
        "query",
        serde_json::json!({"control": paths::SUMMARY}),
    );
    assert!(
        summary.contains("max_delta") && summary.contains("changed"),
        "{summary}"
    );
    apply(
        &mut server,
        paths::MODE,
        names::SET_VALUE,
        serde_json::json!("side-by-side"),
    );
    apply(
        &mut server,
        paths::ACCEPT,
        names::ACTIVATE,
        serde_json::Value::Null,
    );
    apply(
        &mut server,
        paths::REASON,
        names::SET_VALUE,
        serde_json::json!("clipped"),
    );
    apply(
        &mut server,
        paths::REJECT,
        names::ACTIVATE,
        serde_json::Value::Null,
    );
    let after = call(
        &mut server,
        "query",
        serde_json::json!({"control": paths::LIST}),
    );
    assert!(
        after.contains("accepted") && after.contains("rejected") && after.contains("clipped"),
        "{after}"
    );
    drop(server);
    let by_mcp = state(&mcp_app, vcx);

    assert_eq!(reviewed(&by_mcp), reviewed(&by_pointer));
    assert_eq!(by_mcp.verdicts.len(), 2);
    assert_eq!(*mcp_events.0.borrow(), *pointer_events.0.borrow());
    assert!(recorder.take().iter().all(|r| r.source == Source::Mcp));
}

#[gpui_kit::test]
fn keyboard_only_review(cx: &mut TestAppContext) {
    let (app, _, vcx) = mount(cx, None);
    let recorder = Recorder::new();
    vcx.update(|window, cx| ActionBus::add_interceptor(recorder.clone(), window, cx));
    for key in [
        "down", "alt-a", "alt-r", "alt-2", "alt-i", "alt-i", "up", "alt-e",
    ] {
        vcx.update(|window, cx| window.press(key, cx));
        vcx.run_until_parked();
    }
    let s = state(&app, vcx);
    assert_eq!(s.verdicts.len(), 2, "{:?}", s.verdicts);
    assert_eq!(s.view.mode, Mode::After);
    assert_eq!(s.view.zoom, 4.);
    let log = recorder.take();
    assert_eq!(log.len(), 8);
    assert!(log.iter().all(|r| r.source == Source::Shortcut), "{log:?}");
    assert_eq!(&*log[7].path, paths::EXPORT);
    // The export also lands on the clipboard.
    let clip = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert!(
        clip.unwrap_or_default()
            .starts_with("Review goldens: 1 accepted, 1 rejected")
    );
}

#[gpui_kit::test]
fn compare_pops_out_and_docks_back_over_the_bus(cx: &mut TestAppContext) {
    let (app, _, vcx) = mount(cx, None);
    let windows = vcx.update(|_, cx| cx.windows().len());
    vcx.update(|window, cx| {
        let id = ControlId::lookup(paths::COMPARE, cx).unwrap();
        let open = Action {
            name: names::OPEN.into(),
            payload: guiraffe_core::widget_value::Value::Null,
        };
        ActionBus::dispatch(id, &open, Source::Script, window, cx).unwrap();
    });
    vcx.run_until_parked();
    assert!(vcx.update(|_, cx| app.read(cx).compare().is_detached()));
    assert_eq!(vcx.update(|_, cx| cx.windows().len()), windows + 1);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.compare().dock_back(window, cx))
            .unwrap()
    });
    vcx.run_until_parked();
    assert!(!vcx.update(|_, cx| app.read(cx).compare().is_detached()));
    assert_eq!(vcx.update(|_, cx| cx.windows().len()), windows);
}

#[gpui_kit::test]
fn the_golden_list_is_virtualised(cx: &mut TestAppContext) {
    let many: Vec<_> = (0..10_000)
        .filter_map(|i| {
            crate::discover::classify(
                &format!("c/tests/snapshots/upstream/g{i:05}-light.png"),
                crate::discover::Change::Modified,
            )
        })
        .collect();
    let (_, _, vcx) = mount(cx, Some(many));
    vcx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("golden-row", 0usize)).is_some());
        assert!(
            window.try_find(("golden-row", 9_000usize)).is_none(),
            "off-screen rows are not built"
        );
    });
}

#[gpui_kit::test]
fn pixel_inspection_and_loads_stay_off_the_ui_thread(cx: &mut TestAppContext) {
    let (app, _, vcx) = mount(cx, None);
    let s = state(&app, vcx);
    assert!(s.current_loaded().is_some(), "the background load finished");
    vcx.update(|window, cx| {
        let id = ControlId::lookup(paths::INSPECT, cx).unwrap();
        let at = Action {
            name: names::SET_VALUE.into(),
            payload: guiraffe_core::widget_value::Value::item_from_json(&serde_json::json!([
                9, 12
            ]))
            .unwrap(),
        };
        ActionBus::dispatch(id, &at, Source::Script, window, cx).unwrap();
        let snapshot = ActionBus::query(id, window, cx).unwrap();
        assert_eq!(snapshot.to_json()["value"], serde_json::json!([9, 12]));
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.render_frame(cx);
        let _ = window.find("pixel-readout");
    });
    assert_eq!(state(&app, vcx).view.inspect, Some((9, 12)));
}
