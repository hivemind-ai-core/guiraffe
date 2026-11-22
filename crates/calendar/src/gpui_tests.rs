use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, prelude::FluentBuilder, test::TestWindowExt,
};
use guiraffe_core::ColorName;
use guiraffe_test::render::RenderCounter;

use crate::*;

struct Host {
    state: Entity<CalendarState>,
}

struct Root {
    host: Entity<Host>,
    sibling: Entity<Sibling>,
}

struct Sibling(RenderCounter);

impl Render for Sibling {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.hit();
        gpui_kit::div().size_full().child("Unrelated content")
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::div()
            .size_full()
            .child(Calendar::new(&self.state))
    }
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::div()
            .size_full()
            .child(
                self.host.clone().cached(
                    gpui_kit::StyleRefinement::default()
                        .w_full()
                        .h(gpui_kit::px(400.)),
                ),
            )
            .child(
                gpui_kit::AnyView::from(self.sibling.clone()).cached(
                    gpui_kit::StyleRefinement::default()
                        .w_full()
                        .h(gpui_kit::px(20.)),
                ),
            )
    }
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or(NaiveDate::MIN)
}

fn instant(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, hour, minute, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn fixture() -> CalendarModel {
    let source = CalendarSource::new(SourceId(1), "Work", ColorName::Blue);
    let all_day = CalendarItem::all_day(
        EventId(1),
        SourceId(1),
        "Release",
        date(),
        date().succ_opt().unwrap_or(NaiveDate::MIN),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let mut items = vec![
        Arc::new(all_day),
        Arc::new(
            CalendarItem::timed(
                EventId(2),
                SourceId(1),
                "Review",
                instant(9, 0),
                instant(10, 0),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ),
    ];
    for index in 0..4 {
        items.push(Arc::new(
            CalendarItem::all_day(
                EventId(3 + index),
                SourceId(1),
                format!("Item {index}"),
                date(),
                date().succ_opt().unwrap_or(NaiveDate::MIN),
            )
            .unwrap_or_else(|error| panic!("{error}")),
        ));
    }
    CalendarModel::new(items, [source]).unwrap_or_else(|error| panic!("{error}"))
}

fn open(
    cx: &mut TestAppContext,
    view: CalendarViewKind,
) -> (Entity<CalendarState>, RenderCounter, &mut VisualTestContext) {
    cx.update(init);
    let model = fixture();
    let mut scene = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| {
            let mut state = CalendarState::new(model, date(), CalendarZone::Utc, cx);
            state.live_now = false;
            state.now = instant(11, 30);
            state.view = view;
            state
        });
        let host = cx.new(|_| Host { state });
        let sibling = cx.new(|_| Sibling(RenderCounter::new()));
        let root = cx.new(|_| Root { host, sibling });
        scene = Some(root.clone());
        gpui_kit::base::Root::new(root, window, cx)
    });
    let root = scene.unwrap_or_else(|| panic!("calendar test root missing"));
    let (state, counter) = vcx.update(|_, cx| {
        let root = root.read(cx);
        (
            root.host.read(cx).state.clone(),
            root.sibling.read(cx).0.clone(),
        )
    });
    vcx.run_until_parked();
    (state, counter, vcx)
}

#[gpui_kit::test]
fn calendar_navigation_does_not_rerender_siblings(cx: &mut TestAppContext) {
    let (state, sibling, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|window, cx| {
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    let initial_sibling_renders = sibling.get();
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| state.navigate(1, cx));
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(state.read(cx).anchor().month(), 12);
        assert_eq!(
            sibling.get(),
            initial_sibling_renders,
            "the calendar state must not rerender its sibling"
        );
    });
}

#[gpui_kit::test]
fn focus_does_not_change_calendar_layout(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    let before = vcx
        .debug_bounds("calendar")
        .unwrap_or_else(|| panic!("calendar bounds"));
    vcx.update(|window, cx| {
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    let after = vcx
        .debug_bounds("calendar")
        .unwrap_or_else(|| panic!("focused calendar bounds"));
    assert_eq!(before, after, "focus ring must not change layout");
}

#[gpui_kit::test]
fn page_down_navigates_the_real_focused_calendar(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|window, cx| {
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.press("pagedown", cx));
    vcx.run_until_parked();
    vcx.update(|_, cx| assert_eq!(state.read(cx).anchor().month(), 12));
}

#[gpui_kit::test]
fn native_pointer_drag_creates_a_snapped_event(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let captured = changes.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&state, move |_, event: &CalendarEvent, _| {
            if let CalendarEvent::EditRequested(change) = event {
                captured.borrow_mut().push(change.clone());
            }
        })
        .detach();
    });
    let column = vcx
        .debug_bounds("calendar-day-2026-11-21")
        .unwrap_or_else(|| panic!("Saturday column must be laid out"));
    // 08:30 in the column: one hour is 60 px tall from the column's top.
    let start = gpui_kit::point(
        column.center().x,
        column.top() + gpui_kit::px(8.5 * 60.0 + 1.0),
    );
    let focus = vcx.update(|window, cx| {
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
        focus
    });
    vcx.run_until_parked();
    let end = start + gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(30.));
    vcx.simulate_mouse_down(start, gpui_kit::MouseButton::Left, Default::default());
    vcx.simulate_mouse_move(end, Some(gpui_kit::MouseButton::Left), Default::default());
    vcx.simulate_mouse_up(end, gpui_kit::MouseButton::Left, Default::default());
    vcx.run_until_parked();
    vcx.update(|window, _| {
        assert!(
            focus.is_focused(window),
            "pointer drag stole keyboard focus"
        )
    });
    let changes = changes.borrow();
    assert_eq!(
        changes.len(),
        1,
        "one native gesture emits one edit request"
    );
    assert_eq!(changes[0].kind, ChangeKind::Create);
    let ItemTime::Timed { start, end } = &changes[0].time else {
        panic!("timed create")
    };
    assert_eq!(
        (*start, *end),
        (instant(8, 30), instant(10, 0)),
        "create snaps the press to 15 minutes and the drag extends the end"
    );
}

#[gpui_kit::test]
fn create_move_resize_commit_and_escape_follow_fifteen_minute_snap(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let captured = changes.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&state, move |_, event: &CalendarEvent, _| {
            if let CalendarEvent::EditRequested(change) = event {
                captured.borrow_mut().push(change.clone());
            }
        })
        .detach();
        state.update(cx, |state, cx| {
            state.begin_create(date(), 480, cx);
            state.update_timed_drag(date(), 548, cx);
            assert!(state.commit_drag(cx).is_some());
            state.begin_item_drag(EventId(2), DragMode::Move, date(), 540, cx);
            state.update_timed_drag(date(), 615, cx);
            assert!(state.commit_drag(cx).is_some());
            state.begin_item_drag(
                EventId(2),
                DragMode::Resize(ResizeEdge::End),
                date(),
                600,
                cx,
            );
            state.cancel(cx);
            assert!(state.drag().is_none());
        });
    });
    let changes = changes.borrow();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].kind, ChangeKind::Create);
    let ItemTime::Timed { start, end } = &changes[0].time else {
        panic!("timed create")
    };
    assert_eq!((*start, *end), (instant(8, 0), instant(10, 15)));
    let ItemTime::Timed { start, end } = &changes[1].time else {
        panic!("timed move")
    };
    assert_eq!((*start, *end), (instant(10, 15), instant(11, 15)));
}

#[gpui_kit::test]
fn source_visibility_and_read_only_rules_apply_to_visible_model(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_source_visible(SourceId(1), false, cx);
            assert_eq!(state.index.count(date()), 0);
            state.set_source_visible(SourceId(1), true, cx);
            assert_eq!(state.index.count(date()), 6);
            state.set_editable(false, cx);
            state.begin_create(date(), 480, cx);
            assert!(state.drag().is_none());
            state.select_item(EventId(2), cx);
            assert!(state.pick_up(cx).is_none());
            assert!(
                !state.keyboard_editing(),
                "read-only suppresses keyboard pick-up"
            );
        })
    });
}

#[gpui_kit::test]
fn drag_auto_scrolls_one_snap_at_the_viewport_edge(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_scroll_minutes(480, cx);
            state.begin_create(date(), 480, cx);
            state.update_timed_drag(date(), -1, cx);
            assert_eq!(
                state.scroll_minutes, 465,
                "dragging above the viewport scrolls up one snap"
            );
        })
    });
}

thread_local! {
    static CLOCK: Cell<i64> = const { Cell::new(0) };
}

fn test_clock() -> DateTime<Utc> {
    DateTime::from_timestamp(CLOCK.with(Cell::get), 0).unwrap_or(DateTime::UNIX_EPOCH)
}

struct Toggle {
    state: Entity<CalendarState>,
    shown: bool,
    renders: RenderCounter,
}

impl Render for Toggle {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        gpui_kit::div()
            .w_full()
            .h(gpui_kit::px(400.))
            .when(self.shown, |root| root.child(Calendar::new(&self.state)))
    }
}

#[gpui_kit::test]
fn now_line_timer_runs_only_while_painted_and_visible(cx: &mut TestAppContext) {
    CLOCK.with(|clock| clock.set(instant(11, 30).timestamp()));
    cx.update(init);
    let model = fixture();
    let (toggle, vcx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|cx| {
            let mut state = CalendarState::new(model, date(), CalendarZone::Utc, cx);
            state.set_clock(test_clock, cx);
            state.view = CalendarViewKind::Week;
            state
        });
        Toggle {
            state,
            shown: true,
            renders: RenderCounter::new(),
        }
    });
    let state = vcx.update(|_, cx| toggle.read(cx).state.clone());
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_scroll_minutes(600, cx)));
    vcx.run_until_parked();
    let active =
        |vcx: &mut VisualTestContext| vcx.update(|_, cx| state.read(cx).now_timer_active());
    assert!(
        active(vcx),
        "a painted, visible now-line arms one minute timer"
    );

    // One minute passes: the tick refreshes now, repaints and re-arms.
    CLOCK.with(|clock| clock.set(instant(11, 31).timestamp()));
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(61));
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| state.read(cx).now()), instant(11, 31));
    assert!(active(vcx), "a visible calendar keeps ticking");

    // Hidden: the timer is dropped with the first frame that omits the calendar.
    vcx.update(|_, cx| {
        toggle.update(cx, |toggle, cx| {
            toggle.shown = false;
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert!(
        !active(vcx),
        "hiding must stop the timer without waiting for a tick"
    );
    CLOCK.with(|clock| clock.set(instant(11, 32).timestamp()));
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(61));
    vcx.run_until_parked();
    assert!(
        !active(vcx),
        "an unpainted calendar must not re-arm its timer"
    );
    let renders = vcx.update(|_, cx| toggle.read(cx).renders.get());
    let now = vcx.update(|_, cx| state.read(cx).now());
    CLOCK.with(|clock| clock.set(instant(11, 50).timestamp()));
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(20 * 60));
    vcx.run_until_parked();
    assert!(!active(vcx));
    assert_eq!(
        vcx.update(|_, cx| state.read(cx).now()),
        now,
        "no idle wake-ups while hidden"
    );
    assert_eq!(
        vcx.update(|_, cx| toggle.read(cx).renders.get()),
        renders,
        "no idle repaints while hidden"
    );

    // Shown again: catch up once and re-arm.
    vcx.update(|_, cx| {
        toggle.update(cx, |toggle, cx| {
            toggle.shown = true;
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert!(active(vcx));
    assert_eq!(vcx.update(|_, cx| state.read(cx).now()), instant(11, 50));

    // Scrolled away from the now-line, switched view, or injected now: idle.
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_scroll_minutes(0, cx)));
    vcx.run_until_parked();
    assert!(
        !active(vcx),
        "scrolling the now-line out of view stops the timer"
    );
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_scroll_minutes(600, cx)));
    vcx.run_until_parked();
    assert!(active(vcx), "scrolling back re-arms");
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_view(CalendarViewKind::Month, cx)));
    vcx.run_until_parked();
    assert!(!active(vcx), "Month view has no now-line");
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_view(CalendarViewKind::Day, cx)));
    vcx.run_until_parked();
    assert!(active(vcx), "Day view of today re-arms");
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_show_now(false, cx)));
    vcx.run_until_parked();
    assert!(!active(vcx), "a disabled now-line has no timer");
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_show_now(true, cx);
            state.set_now(instant(11, 0), cx);
        })
    });
    vcx.run_until_parked();
    assert!(!active(vcx), "an injected now never starts the live timer");
}

#[gpui_kit::test]
fn wheel_scroll_virtualizes_the_hours_it_reveals(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    assert!(
        vcx.debug_bounds("calendar-hour-label-23").is_none(),
        "late hours are virtualized away at the 08:00 initial scroll"
    );
    let anchor = vcx
        .debug_bounds("calendar-hour-label-9")
        .unwrap_or_else(|| panic!("09:00 is laid out"));
    vcx.simulate_event(gpui_kit::ScrollEvent {
        position: anchor.center(),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
            gpui_kit::px(0.),
            gpui_kit::px(-900.),
        )),
        ..Default::default()
    });
    vcx.run_until_parked();
    assert!(
        vcx.debug_bounds("calendar-hour-label-22").is_some(),
        "hours revealed by wheel scrolling must be drawn"
    );
    let minutes = vcx.update(|_, cx| state.read(cx).scroll_minutes());
    assert!(
        minutes > 900,
        "the painted scroll position syncs into the state: {minutes}"
    );
}

#[gpui_kit::test]
fn keyboard_follows_the_spec_table(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|window, cx| {
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    let cursor = |vcx: &mut VisualTestContext| vcx.update(|_, cx| state.read(cx).cursor());
    let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap_or(NaiveDate::MIN);
    vcx.update(|window, cx| window.press("right", cx));
    assert_eq!(cursor(vcx).date, day(22), "Right: next date in Month");
    vcx.update(|window, cx| window.press("up", cx));
    assert_eq!(cursor(vcx).date, day(15), "Up: same weekday, previous week");
    vcx.update(|window, cx| window.press("shift-left", cx));
    assert_eq!(cursor(vcx).date, day(14), "Shift+Left: previous date");
    vcx.update(|window, cx| window.press("home", cx));
    assert_eq!(cursor(vcx).date, date(), "Home: today, same view");
    assert_eq!(
        vcx.update(|_, cx| state.read(cx).view()),
        CalendarViewKind::Month
    );
    vcx.update(|window, cx| window.press("pageup", cx));
    assert_eq!(vcx.update(|_, cx| state.read(cx).anchor().month()), 10);
    vcx.update(|window, cx| window.press("pagedown", cx));
    vcx.update(|window, cx| window.press("home", cx));
    // Enter on a date with events reveals them; arrows choose; Enter selects.
    vcx.update(|window, cx| window.press("enter", cx));
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| state.read(cx).overflow()), Some(date()));
    vcx.update(|window, cx| window.press("down", cx));
    vcx.update(|window, cx| window.press("enter", cx));
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        let state = state.read(cx);
        assert_eq!(state.overflow(), None);
        assert!(
            state.selected_item().is_some(),
            "Enter selects the highlighted item"
        );
    });
    // Week: arrows step fifteen minutes; Shift+arrows step dates.
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_view(CalendarViewKind::Week, cx)));
    let before = cursor(vcx);
    vcx.update(|window, cx| window.press("down", cx));
    vcx.update(|window, cx| window.press("right", cx));
    assert_eq!(cursor(vcx).minute, before.minute + 30);
    assert_eq!(cursor(vcx).date, before.date);
    vcx.update(|window, cx| window.press("shift-right", cx));
    assert_eq!(
        cursor(vcx).date,
        before.date.succ_opt().unwrap_or(before.date)
    );
    // Escape with nothing open is harmless and focus stays.
    vcx.update(|window, cx| window.press("escape", cx));
    vcx.update(|window, cx| {
        assert!(state.read(cx).focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn space_picks_up_moves_resizes_and_drops_one_change(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let captured = changes.clone();
    vcx.update(|window, cx| {
        cx.subscribe(&state, move |_, event: &CalendarEvent, _| {
            if let CalendarEvent::EditRequested(change) = event {
                captured.borrow_mut().push(change.clone());
            }
        })
        .detach();
        state.update(cx, |state, cx| state.select_item(EventId(2), cx));
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    for key in ["space", "down", "down", "shift-down", "up"] {
        vcx.update(|window, cx| window.press(key, cx));
    }
    vcx.update(|_, cx| {
        let state = state.read(cx);
        assert!(state.keyboard_editing());
        let preview = state.drag().map(|drag| drag.proposed.clone());
        assert_eq!(
            preview,
            Some(ItemTime::Timed {
                start: instant(9, 15),
                end: instant(10, 30)
            }),
            "two steps down, one back up, end extended fifteen minutes"
        );
    });
    assert!(
        changes.borrow().is_empty(),
        "arrows preview; nothing is emitted yet"
    );
    vcx.update(|window, cx| window.press("space", cx));
    assert_eq!(changes.borrow().len(), 1, "drop emits exactly one change");
    assert_eq!(changes.borrow()[0].kind, ChangeKind::Move);
    // Escape restores: pick up, move, cancel, no change.
    for key in ["space", "down", "escape"] {
        vcx.update(|window, cx| window.press(key, cx));
    }
    assert_eq!(changes.borrow().len(), 1, "cancel never emits");
    vcx.update(|_, cx| assert!(state.read(cx).drag().is_none()));
    // Keyboard create at the cursor in Week: Space starts, Enter commits.
    vcx.update(|_, cx| {
        state.update(cx, |state, _| {
            state.selected_item = None;
            state.cursor = CalendarCursor {
                date: date(),
                minute: 13 * 60,
            };
        })
    });
    for key in ["space", "shift-down", "enter"] {
        vcx.update(|window, cx| window.press(key, cx));
    }
    let changes = changes.borrow();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[1].kind, ChangeKind::Create);
    assert_eq!(
        changes[1].time,
        ItemTime::Timed {
            start: instant(13, 0),
            end: instant(14, 15)
        }
    );
}

#[gpui_kit::test]
fn clicks_without_travel_select_instead_of_editing(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let captured = changes.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&state, move |_, event: &CalendarEvent, _| {
            if let CalendarEvent::EditRequested(change) = event {
                captured.borrow_mut().push(change.clone());
            }
        })
        .detach();
    });
    let day = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap_or(NaiveDate::MIN);
    let cell = vcx
        .debug_bounds("calendar-month-cell-2026-11-10")
        .unwrap_or_else(|| panic!("month cell is laid out"));
    let point = cell.bottom_left() + gpui_kit::point(gpui_kit::px(20.), gpui_kit::px(-10.));
    vcx.simulate_click(point, Default::default());
    vcx.run_until_parked();
    assert!(
        changes.borrow().is_empty(),
        "a click on a month date never creates"
    );
    assert_eq!(vcx.update(|_, cx| state.read(cx).selected_date()), day);
    // An unchanged pointer move of an event is a click, not an edit.
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.begin_item_drag(EventId(2), DragMode::Move, date(), 540, cx);
            assert!(state.commit_drag(cx).is_none());
        })
    });
    assert!(changes.borrow().is_empty());
}

#[gpui_kit::test]
fn grid_exposes_cells_sources_and_labels_to_accessibility(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    let report = guiraffe_test::inspect::a11y_report(vcx);
    let named = |needle: &str| {
        report
            .nodes
            .iter()
            .find(|node| {
                node.a11y
                    .name
                    .as_deref()
                    .is_some_and(|name| name.contains(needle))
            })
            .cloned()
    };
    let cell = named("Saturday, November 21, 2026, 6 events")
        .unwrap_or_else(|| panic!("today's cell has a localized date and count name"));
    assert_eq!(cell.a11y.state.get("selected"), Some(&true));
    assert!(
        named("Previous month").is_some(),
        "navigation names include the unit"
    );
    assert!(named("Next month").is_some());
    assert!(
        named("Work calendar").is_some(),
        "source toggle is named: {:?}",
        report
            .nodes
            .iter()
            .filter(|node| node.a11y.name.is_some())
            .map(|node| (node.path.last(), &node.a11y.name, &node.a11y.role))
            .collect::<Vec<_>>()
    );
    assert!(
        named("more events").is_some(),
        "+N more is an accessible button"
    );
    // Grid dates are not Tab stops: one roving cursor (spec §Keyboard).
    assert!(
        report
            .focus_order
            .iter()
            .all(|id| !format!("{id:?}").contains("month-cell")),
        "month cells must not be Tab stops"
    );
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_view(CalendarViewKind::Week, cx)));
    vcx.run_until_parked();
    let report = guiraffe_test::inspect::a11y_report(vcx);
    assert!(
        report.nodes.iter().any(|node| node
            .a11y
            .name
            .as_deref()
            .is_some_and(|name| name.starts_with("Review, 9:00 AM – 10:00 AM"))),
        "timed blocks announce title and start–end time"
    );
}

#[gpui_kit::test]
fn custom_renderer_and_overflow_run_through_real_month_view(cx: &mut TestAppContext) {
    let rendered = Rc::new(Cell::new(0));
    let counter = rendered.clone();
    let renderer: ItemRenderer = Rc::new(move |content: &ItemContent<'_>| {
        counter.set(counter.get() + 1);
        assert_eq!(
            content.color,
            ColorName::Blue,
            "source color reaches the renderer"
        );
        Some(
            gpui_kit::div()
                .child(SharedString::from(format!(
                    "Custom: {}",
                    content.item.title
                )))
                .into_any_element(),
        )
    });
    // Render the exact public custom-content builder once through a second host.
    cx.update(init);
    let renderer_for_host = renderer.clone();
    let model = fixture();
    let (host, custom_vcx) = cx.add_window_view(move |_, cx| {
        let state = cx.new(|cx| CalendarState::new(model, date(), CalendarZone::Utc, cx));
        struct Custom {
            state: Entity<CalendarState>,
            renderer: ItemRenderer,
        }
        impl Render for Custom {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                Calendar::new(&self.state).month_chip_renderer(Some(self.renderer.clone()))
            }
        }
        Custom {
            state,
            renderer: renderer_for_host,
        }
    });
    custom_vcx.run_until_parked();
    assert!(rendered.get() > 0, "custom content never rendered");
    custom_vcx.update(|_, cx| {
        let state = host.read(cx).state.clone();
        state.update(cx, |state, cx| state.set_overflow(Some(date()), cx));
    });
    custom_vcx.run_until_parked();
    custom_vcx.update(|_, cx| assert_eq!(host.read(cx).state.read(cx).overflow(), Some(date())));
}

#[gpui_kit::test]
fn two_view_changes_in_one_update_settle_on_the_last(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_view(CalendarViewKind::Week, cx);
            state.set_view(CalendarViewKind::Day, cx);
        })
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| assert_eq!(state.read(cx).view(), CalendarViewKind::Day));
}

#[gpui_kit::test]
fn pointer_activation_does_not_capture_focus(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    let cell = vcx
        .debug_bounds("calendar-month-cell-2026-11-10")
        .unwrap_or_else(|| panic!("month cell"));
    vcx.simulate_click(cell.center(), Default::default());
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        assert!(
            !state.read(cx).focus.is_focused(window),
            "a click must not capture focus by default"
        );
    });
}

#[gpui_kit::test]
fn enter_on_a_focused_toolbar_button_presses_the_button(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_focusable(Some(true), cx);
            state.navigate(1, cx);
        })
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        let focus = state.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    // Grid, Previous, Today.
    for _ in 0..2 {
        vcx.update(|window, cx| window.focus_next(cx));
        vcx.run_until_parked();
    }
    vcx.simulate_keystrokes("enter");
    vcx.simulate_event(gpui_kit::KeyUpEvent {
        keystroke: gpui_kit::Keystroke::parse("enter").unwrap_or_else(|error| panic!("{error}")),
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(
            state.read(cx).anchor(),
            date(),
            "Today must run; the grid's Enter binding must not swallow it"
        );
    });
}

#[gpui_kit::test]
fn a_click_does_not_commit_a_keyboard_edit(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.pick_up(cx);
        })
    });
    vcx.run_until_parked();
    let cell = vcx
        .debug_bounds("calendar-month-cell-2026-11-10")
        .unwrap_or_else(|| panic!("month cell"));
    vcx.simulate_click(cell.center(), Default::default());
    vcx.run_until_parked();
    vcx.update(|_, cx| assert!(state.read(cx).keyboard_editing()));
}

#[gpui_kit::test]
fn pointer_at_the_viewport_edge_scrolls_and_keeps_the_time_under_it(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    let viewport = vcx.update(|_, cx| state.read(cx).scroll.bounds());
    let column = vcx
        .debug_bounds("calendar-day-2026-11-21")
        .unwrap_or_else(|| panic!("day column"));
    let x = column.center().x;
    let start = gpui_kit::point(x, viewport.top() + gpui_kit::px(40.));
    let edge = gpui_kit::point(x, viewport.bottom() - gpui_kit::px(3.));
    let left = Some(gpui_kit::MouseButton::Left);
    vcx.simulate_mouse_down(start, gpui_kit::MouseButton::Left, Default::default());
    for nudge in 0..4 {
        let y = gpui_kit::px(nudge as f32 * 0.5);
        vcx.simulate_mouse_move(
            edge - gpui_kit::point(gpui_kit::px(0.), y),
            left,
            Default::default(),
        );
        vcx.run_until_parked();
    }
    vcx.update(|_, cx| {
        let state = state.read(cx);
        let scrolled: f32 = (-state.scroll.offset().y).into();
        assert!(
            scrolled > 480.,
            "the bottom edge must scroll down: {scrolled}"
        );
        let Some(ItemTime::Timed { end, .. }) = state.drag().map(|drag| drag.proposed.clone())
        else {
            panic!("create preview")
        };
        let travel: f32 = (edge.y - start.y).into();
        let expected = 8. * 60. + 30. + 60. + travel + (scrolled - 480.);
        let proposed = crate::minute_of_day(CalendarZone::Utc.local(end)) as f32;
        assert!(
            (proposed - expected).abs() <= 32.,
            "the preview follows the content under the pointer: {proposed} vs {expected}"
        );
    });
}

#[gpui_kit::test]
fn live_clock_never_serves_a_stale_today(cx: &mut TestAppContext) {
    CLOCK.with(|clock| clock.set(instant(11, 30).timestamp()));
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_clock(test_clock, cx);
            state.set_live_now(true, cx);
        })
    });
    vcx.run_until_parked();
    // Month runs no timer; two days pass.
    let later = instant(11, 30) + chrono::Duration::days(2);
    CLOCK.with(|clock| clock.set(later.timestamp()));
    vcx.update(|_, cx| state.update(cx, |state, cx| state.today(cx)));
    vcx.update(|_, cx| {
        assert_eq!(
            state.read(cx).anchor(),
            later.date_naive(),
            "Today must read the clock"
        );
    });
}

#[gpui_kit::test]
fn scrolling_to_the_current_hour_refreshes_a_stale_now_line(cx: &mut TestAppContext) {
    CLOCK.with(|clock| clock.set(instant(2, 0).timestamp()));
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_clock(test_clock, cx);
            state.set_live_now(true, cx);
        })
    });
    vcx.run_until_parked();
    // The line at 02:00 is out of view, so no timer runs. Nine hours pass.
    CLOCK.with(|clock| clock.set(instant(11, 0).timestamp()));
    vcx.update(|_, cx| state.update(cx, |state, cx| state.set_scroll_minutes(600, cx)));
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        let state = state.read(cx);
        assert_eq!(state.now(), instant(11, 0));
        assert!(state.now_timer_active());
    });
}

struct Clipped {
    state: Entity<CalendarState>,
    offscreen: bool,
}

impl Render for Clipped {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui_kit::div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(
                gpui_kit::div()
                    .absolute()
                    .top(gpui_kit::px(if self.offscreen { 5_000. } else { 0. }))
                    .w_full()
                    .h(gpui_kit::px(400.))
                    .child(Calendar::new(&self.state)),
            )
    }
}

#[gpui_kit::test]
fn now_timer_stops_when_clipped_offscreen(cx: &mut TestAppContext) {
    CLOCK.with(|clock| clock.set(instant(11, 30).timestamp()));
    cx.update(init);
    let model = fixture();
    let (clipped, vcx) = cx.add_window_view(|_, cx| {
        let state = cx.new(|cx| {
            let mut state = CalendarState::new(model, date(), CalendarZone::Utc, cx);
            state.set_clock(test_clock, cx);
            state.view = CalendarViewKind::Week;
            state.set_scroll_minutes(600, cx);
            state
        });
        Clipped {
            state,
            offscreen: false,
        }
    });
    vcx.run_until_parked();
    let state = vcx.update(|_, cx| clipped.read(cx).state.clone());
    assert!(vcx.update(|_, cx| state.read(cx).now_timer_active()));
    vcx.update(|_, cx| {
        clipped.update(cx, |clipped, cx| {
            clipped.offscreen = true;
            cx.notify();
        })
    });
    vcx.run_until_parked();
    assert!(!vcx.update(|_, cx| state.read(cx).now_timer_active()));
}

#[gpui_kit::test]
fn zero_size_viewport_recovers(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    vcx.simulate_resize(gpui_kit::size(gpui_kit::px(0.), gpui_kit::px(0.)));
    vcx.run_until_parked();
    vcx.simulate_resize(gpui_kit::size(gpui_kit::px(1_000.), gpui_kit::px(700.)));
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("calendar-day-2026-11-21").is_some());
    vcx.update(|_, cx| assert!(state.read(cx).viewport_minutes > 0));
}

#[gpui_kit::test]
fn drag_movement_does_not_rerender_siblings(cx: &mut TestAppContext) {
    let (state, sibling, vcx) = open(cx, CalendarViewKind::Week);
    let before = sibling.get();
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.begin_item_drag(EventId(2), DragMode::Move, date(), 540, cx)
        })
    });
    for minute in [555, 570, 585, 600] {
        vcx.update(|_, cx| {
            state.update(cx, |state, cx| state.update_timed_drag(date(), minute, cx))
        });
        vcx.run_until_parked();
    }
    assert_eq!(sibling.get(), before);
}

#[gpui_kit::test]
fn state_changes_repaint_inside_a_cached_host(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Month);
    vcx.update(|_, cx| state.update(cx, |state, cx| state.navigate(1, cx)));
    vcx.run_until_parked();
    assert!(
        vcx.debug_bounds("calendar-month-cell-2026-12-31").is_some(),
        "December must be painted after navigating"
    );
}
