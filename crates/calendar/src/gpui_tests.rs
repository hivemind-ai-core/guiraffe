use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, test::TestWindowExt,
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
    let bounds = vcx
        .debug_bounds("calendar-hour-2026-11-21-8")
        .unwrap_or_else(|| panic!("Saturday 08:00 hour target must be laid out"));
    let start = bounds.center();
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
    assert_eq!((*start, *end), (instant(8, 0), instant(9, 30)));
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
            assert!(state.keyboard_move_selected(15, None, cx).is_none());
        })
    });
}

#[gpui_kit::test]
fn drag_auto_scrolls_and_now_timer_stops_when_hidden(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, CalendarViewKind::Week);
    vcx.update(|_, cx| {
        state.update(cx, |state, cx| {
            state.set_live_now(true, cx);
            assert!(
                state.now_task.is_some(),
                "visible live now-line must have a minute timer"
            );
            state.set_scroll_minutes(0, cx);
            assert!(
                state.now_task.is_none(),
                "scrolling away must stop the timer"
            );
            state.set_scroll_minutes(480, cx);
            assert!(
                state.now_task.is_some(),
                "returning to the now-line restarts one timer"
            );
            state.set_show_now(false, cx);
            assert!(
                state.now_task.is_none(),
                "hiding the now-line must stop the timer"
            );
            state.begin_create(date(), 480, cx);
            state.update_timed_drag(date(), -1, cx);
            assert_eq!(
                state.scroll_minutes, 465,
                "dragging above the viewport scrolls up one snap"
            );
        })
    });
}

#[gpui_kit::test]
fn custom_renderer_and_overflow_run_through_real_month_view(cx: &mut TestAppContext) {
    let rendered = Rc::new(Cell::new(0));
    let counter = rendered.clone();
    let renderer: ItemRenderer = Rc::new(move |item, _, _| {
        counter.set(counter.get() + 1);
        Some(
            gpui_kit::div()
                .child(SharedString::from(format!("Custom: {}", item.title)))
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
