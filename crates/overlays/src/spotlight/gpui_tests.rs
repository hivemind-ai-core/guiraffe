//! Interaction tests for the command bar (spec/00 §12, spec/04 dismissal and focus contract).
use super::*;
use crate::gpui_tests::{open as open_window, open_with_content};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{Entity, FocusHandle, Focusable, TestAppContext, WindowHandle};
use std::cell::RefCell;

type Log = Rc<RefCell<Vec<String>>>;

fn actions() -> Vec<SpotlightAction> {
    vec![
        SpotlightAction::new("open", "Open file")
            .group("File")
            .shortcut("Ctrl+O"),
        SpotlightAction::new("close", "Close file").group("File"),
        SpotlightAction::new("archived", "Archive file")
            .group("File")
            .disabled(true),
        SpotlightAction::new("commit", "Commit")
            .group("Git")
            .description("Record staged changes"),
        SpotlightAction::new("push", "Push").group("Git"),
    ]
}

/// A palette whose activations and events are logged.
fn palette(window: &mut Window, cx: &mut App, log: &Log) -> Entity<SpotlightState> {
    let ran = log.clone();
    let state = cx.new(|cx| {
        SpotlightState::new(window, cx)
            .actions(actions())
            .on_activate(move |id, _, _| ran.borrow_mut().push(format!("run {id}")))
    });
    let events = log.clone();
    cx.subscribe(&state, move |_, event: &SpotlightEvent, _| {
        events.borrow_mut().push(match event {
            SpotlightEvent::Opened => "opened".into(),
            SpotlightEvent::Closed => "closed".into(),
            SpotlightEvent::QueryChanged(q) => format!("query {q}"),
            SpotlightEvent::SelectionChanged(id) => {
                format!("select {}", id.as_ref().map_or("-", |s| s.as_ref()))
            }
            SpotlightEvent::Activated(id) => format!("activated {id}"),
        })
    })
    .detach();
    state
}

fn selected(state: &Entity<SpotlightState>, cx: &App) -> Option<String> {
    state.read(cx).selected_action().map(|a| a.id.to_string())
}

fn input_focus(state: &Entity<SpotlightState>, cx: &App) -> FocusHandle {
    state.read(cx).input().read(cx).focus_handle(cx)
}

#[gpui_kit::test]
fn open_type_enter_runs_once_and_restores_focus(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let handle = open_window(cx);
    let log = Log::default();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        let before = window.focused(cx);
        let state = palette(window, cx, &log);
        SpotlightState::open(&state, window, cx).expect("open");
        window.render_frame(cx);
        assert!(
            input_focus(&state, cx).is_focused(window),
            "input owns focus"
        );
        window.input("cl", cx);
        window.render_frame(cx);
        window.press("enter", cx);
        assert_eq!(window.focused(cx), before, "focus returns to the opener");
        assert!(!state.read(cx).is_open());
        assert_eq!(state.read(cx).recent_ids()[0].as_ref(), "close");
    })
    .expect("window");
    cx.run_until_parked();
    let log = log.borrow();
    assert_eq!(
        log.iter()
            .filter(|e| e.starts_with("run"))
            .collect::<Vec<_>>(),
        ["run close"],
        "one Enter runs exactly one action"
    );
    let order: Vec<&str> = log
        .iter()
        .map(String::as_str)
        .filter(|e| matches!(*e, "opened" | "closed" | "activated close" | "run close"))
        .collect();
    assert_eq!(order, ["opened", "closed", "activated close", "run close"]);
    assert!(log.iter().any(|e| e == "query cl"));
}

#[gpui_kit::test]
fn escape_closes_and_restores_focus_without_running(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let handle = open_window(cx);
    let log = Log::default();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        let before = window.focused(cx);
        let state = palette(window, cx, &log);
        let id = SpotlightState::open(&state, window, cx).expect("open");
        assert_eq!(
            SpotlightState::open(&state, window, cx),
            Ok(id),
            "idempotent"
        );
        window.render_frame(cx);
        window.input("pu", cx);
        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(window.focused(cx), before);
        assert!(!state.read(cx).is_open());
        // Reopening starts from an empty query.
        SpotlightState::open(&state, window, cx).expect("reopen");
        window.render_frame(cx);
        assert_eq!(state.read(cx).query(), "");
        assert_eq!(state.read(cx).input().read(cx).value().as_ref(), "");
    })
    .expect("window");
    cx.run_until_parked();
    let log = log.borrow();
    assert!(!log.iter().any(|e| e.starts_with("run")));
    assert_eq!(log.iter().filter(|e| *e == "closed").count(), 1);
}

#[gpui_kit::test]
fn arrows_skip_disabled_tab_cycles_groups(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let handle = open_window(cx);
    let log = Log::default();
    cx.update_window(handle.into(), |_, window, cx| {
        let state = palette(window, cx, &log);
        SpotlightState::open(&state, window, cx).expect("open");
        window.render_frame(cx);
        assert_eq!(selected(&state, cx).as_deref(), Some("open"));
        window.press("down", cx);
        assert_eq!(selected(&state, cx).as_deref(), Some("close"));
        window.press("down", cx);
        assert_eq!(
            selected(&state, cx).as_deref(),
            Some("commit"),
            "disabled skipped"
        );
        window.press("up", cx);
        window.press("tab", cx);
        assert_eq!(
            selected(&state, cx).as_deref(),
            Some("commit"),
            "next group"
        );
        window.press("tab", cx);
        assert_eq!(selected(&state, cx).as_deref(), Some("open"), "wraps");
        window.press("shift-tab", cx);
        assert_eq!(selected(&state, cx).as_deref(), Some("commit"));
        assert!(
            input_focus(&state, cx).is_focused(window),
            "Tab cycles groups instead of moving focus out of the input"
        );
        window.press("ctrl-n", cx);
        assert_eq!(selected(&state, cx).as_deref(), Some("push"));
        window.press("enter", cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(log.borrow().iter().any(|e| e == "run push"));
    assert!(log.borrow().iter().any(|e| e == "select close"));
}

#[gpui_kit::test]
fn rows_expose_listbox_semantics(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let handle = open_window(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = palette(window, cx, &Log::default());
        SpotlightState::open(&state, window, cx).expect("open");
        window.render_frame(cx);
        let options = window.find_all_by_role(gpui_kit::Role::ListBoxOption);
        assert_eq!(options.len(), 5);
        // Rows: 0 "File" header, 1 open, 2 close, 3 archived, 4 "Git", 5 commit, 6 push.
        let row = |window: &Window, i: usize| window.find(("spotlight-row", i));
        assert_eq!(row(window, 1).label(), Some("Open file"));
        assert_eq!(row(window, 1).selected(), Some(true));
        assert_eq!(row(window, 2).selected(), Some(false));
        assert_eq!(row(window, 3).disabled(), Some(true));
        assert_eq!(
            window.find(("spotlight-group", 1_usize)).label(),
            Some("Git")
        );
        assert_eq!(
            window.find("spotlight-results").role(),
            Some(gpui_kit::Role::ListBox)
        );
        window.press("down", cx);
        assert_eq!(row(window, 1).selected(), Some(false));
        assert_eq!(row(window, 2).selected(), Some(true));
    })
    .expect("window");
}

#[gpui_kit::test]
fn pointer_hover_and_click_keep_input_focus(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let handle = open_window(cx);
    let log = Log::default();
    cx.update_window(handle.into(), |_, window, cx| {
        let state = palette(window, cx, &log);
        SpotlightState::open(&state, window, cx).expect("open");
        window.render_frame(cx);
        let input = input_focus(&state, cx);
        window.hover(("spotlight-row", 6_usize), cx);
        window.render_frame(cx);
        assert!(input.is_focused(window), "hover never takes focus");
        assert_eq!(selected(&state, cx).as_deref(), Some("open"), "nor selects");
        // A disabled row ignores clicks and keeps the palette open.
        window.click(("spotlight-row", 3_usize), cx);
        assert!(state.read(cx).is_open());
        assert!(input.is_focused(window));
        window.click(("spotlight-row", 6_usize), cx);
        assert!(!state.read(cx).is_open());
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| e.starts_with("run"))
            .collect::<Vec<_>>(),
        ["run push"]
    );
}

struct Inline {
    state: Entity<SpotlightState>,
}
impl Render for Inline {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(Spotlight::new(&self.state))
    }
}

fn open_inline(
    cx: &mut TestAppContext,
    build: impl FnOnce(SpotlightState) -> SpotlightState + 'static,
) -> (WindowHandle<gpui_kit::base::Root>, Entity<SpotlightState>) {
    cx.update(crate::init);
    let mut slot = None;
    let handle = cx
        .update(|cx| {
            cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                let state = cx.new(|cx| build(SpotlightState::new(window, cx)));
                slot = Some(state.clone());
                let view = cx.new(|_| Inline { state });
                crate::root(view, window, cx)
            })
        })
        .expect("window");
    (handle, slot.expect("state"))
}

#[gpui_kit::test]
fn inline_click_runs_without_moving_focus(cx: &mut TestAppContext) {
    let log = Log::default();
    let ran = log.clone();
    let (handle, state) = open_inline(cx, move |s| {
        s.actions(actions())
            .on_activate(move |id, _, _| ran.borrow_mut().push(id.to_string()))
    });
    cx.update_window(handle.into(), |_, window, cx| {
        let input = input_focus(&state, cx);
        input.focus(window, cx);
        window.render_frame(cx);
        window.click(("spotlight-row", 5_usize), cx);
        assert!(
            input.is_focused(window),
            "a row click keeps the input focused"
        );
        window.press("enter", cx);
    })
    .expect("window");
    cx.run_until_parked();
    // Inline: the click and the Enter on the clicked (now selected) row both run it.
    assert_eq!(*log.borrow(), ["commit", "commit"]);
}

#[gpui_kit::test]
fn empty_no_results_and_loading_states(cx: &mut TestAppContext) {
    let (handle, state) = open_inline(cx, |s| s);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("spotlight-empty").is_some());
        state.update(cx, |s, cx| s.set_actions(actions(), cx));
        window.render_frame(cx);
        assert!(window.try_find("spotlight-empty").is_none());
        state.update(cx, |s, cx| s.set_query("zzz", window, cx));
        window.render_frame(cx);
        assert!(window.try_find("spotlight-no-results").is_some());
        assert_eq!(selected(&state, cx), None);
        state.update(cx, |s, cx| s.set_query("", window, cx));
        state.update(cx, |s, cx| {
            s.load(
                async { vec![SpotlightAction::new("late", "Arrived late")] },
                cx,
            )
        });
        state.update(cx, |s, cx| s.set_actions(Vec::new(), cx));
        window.render_frame(cx);
        assert!(window.try_find("spotlight-loading").is_some());
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("spotlight-loading").is_none());
        assert_eq!(selected(&state, cx).as_deref(), Some("late"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn typing_prepares_nothing_and_submit_does_not_rescore(cx: &mut TestAppContext) {
    let (handle, state) = open_inline(cx, |s| s.actions(actions()));
    cx.update_window(handle.into(), |_, window, cx| {
        input_focus(&state, cx).focus(window, cx);
        window.render_frame(cx);
    })
    .expect("window");
    let prepared = crate::search::normalizations();
    cx.update_window(handle.into(), |_, window, cx| window.input("co", cx))
        .expect("type");
    // Highlighting prepares only the few visible titles; the index is not rebuilt.
    assert!(crate::search::normalizations() - prepared <= 8);
    let scored = state.read_with(cx, |s, _| s.model().scored);
    cx.update_window(handle.into(), |_, window, cx| window.press("enter", cx))
        .expect("submit");
    assert_eq!(state.read_with(cx, |s, _| s.model().scored), scored);
}

#[gpui_kit::test]
fn typing_rerenders_only_the_palette(cx: &mut TestAppContext) {
    cx.update(crate::init);
    let (handle, content) = open_with_content(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = palette(window, cx, &Log::default());
        SpotlightState::open(&state, window, cx).expect("open");
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    let renders = content.read_with(cx, |c, _| c.renders.get());
    cx.update_window(handle.into(), |_, window, cx| {
        for key in ["p", "u", "s"] {
            window.input(key, cx);
            window.render_frame(cx);
        }
    })
    .expect("window");
    let after = content.read_with(cx, |c, _| c.renders.get());
    // Upstream GPUI re-renders the uncached children of a notified view's ancestors
    // (spec/00 §6), so only gpui-fast can promise an untouched page.
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            after, renders,
            "the page behind the palette never re-renders"
        );
    }
}

#[gpui_kit::test]
fn keyboard_navigation_scrolls_the_virtualised_list(cx: &mut TestAppContext) {
    let many: Vec<_> = (0..500)
        .map(|i| SpotlightAction::new(format!("a{i}"), format!("Action {i:03}")))
        .collect();
    let (handle, state) = open_inline(cx, move |s| s.actions(many));
    cx.update_window(handle.into(), |_, window, cx| {
        input_focus(&state, cx).focus(window, cx);
        window.render_frame(cx);
        let rendered = window.find_all_by_role(gpui_kit::Role::ListBoxOption).len();
        assert!(
            rendered < 60,
            "only rows near the viewport mount: {rendered}"
        );
        window.press("up", cx);
        window.render_frame(cx);
        assert_eq!(
            selected(&state, cx).as_deref(),
            Some("a499"),
            "up wraps to the end"
        );
        assert!(window.find(("spotlight-row", 499_usize)).visible());
        window.press("pageup", cx);
        window.render_frame(cx);
        assert_eq!(selected(&state, cx).as_deref(), Some("a491"));
    })
    .expect("window");
}
