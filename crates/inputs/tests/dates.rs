//! Date & time picker interaction tests (spec/00 §12): open, pick, range pick, presets,
//! clear, disabled/excluded suppression, keyboard operation, controlled values, time
//! editing, focus on click, focus-ring layout stability and render counts.
use std::{cell::RefCell, rc::Rc, time::Duration};

use chrono::{Datelike, NaiveDate, NaiveTime, Weekday};
use gpui_kit::{
    AnyView, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, Styled, TestAppContext, VisualTestContext, Window,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_core::focus_scope;
use guiraffe_inputs::dates::*;
use guiraffe_test::{
    layout::{assert_same_bounds, capture},
    render::RenderCounter,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
}
fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default()
}
/// Wednesday 11 March 2026: every picker here pins "today" so the calendar is stable.
fn today() -> NaiveDate {
    d(2026, 3, 11)
}

struct Host {
    input: Entity<InputState>,
    picker: AnyView,
    scoped: bool,
    renders: RenderCounter,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        let picker = div()
            .debug_selector(|| "picker".into())
            .w(px(260.))
            .child(self.picker.clone());
        let body = div().flex().flex_col().gap(px(8.)).child(picker).child(
            div()
                .debug_selector(|| "below".into())
                .h(px(20.))
                .w(px(40.)),
        );
        div()
            .size_full()
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().w(px(200.)).child(Input::new(&self.input)))
            .map(|el| {
                if self.scoped {
                    el.child(focus_scope(body))
                } else {
                    el.child(body)
                }
            })
    }
}

use gpui_kit::prelude::FluentBuilder as _;

struct Mounted<'a, V: PickerValue> {
    picker: Entity<Picker<V>>,
    vcx: &'a mut VisualTestContext,
    input: FocusHandle,
    events: Rc<RefCell<Vec<PickerEvent<V>>>>,
    renders: RenderCounter,
}

fn mount<V: PickerValue>(
    cx: &mut TestAppContext,
    scoped: bool,
    configure: impl FnOnce(PickerProps<V>) -> PickerProps<V> + 'static,
) -> Mounted<'_, V> {
    cx.update(guiraffe_inputs::init);
    let renders = RenderCounter::new();
    let r2 = renders.clone();
    let mut picker = None;
    let (host, vcx) = cx.add_window_view(|window, cx| {
        let p = Picker::<V>::new(window, cx, |p| configure(p.today(today())));
        picker = Some(p.clone());
        Host {
            input: cx.new(|cx| InputState::new(window, cx)),
            picker: p.into(),
            scoped,
            renders: r2,
        }
    });
    let picker = picker.unwrap_or_else(|| unreachable!());
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&picker, move |_, e: &PickerEvent<V>, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach()
    });
    // Focus events (blur, focus-out) only fire in an active window.
    vcx.update(|w, _| w.activate_window());
    vcx.run_until_parked();
    let input = vcx.update(|window, cx| {
        let h = host.read(cx).input.focus_handle(cx);
        window.focus(&h, cx);
        h
    });
    vcx.run_until_parked();
    Mounted {
        picker,
        vcx,
        input,
        events,
        renders,
    }
}

/// Click the element with accessible name `label` (calendar days are named by date).
fn click_label(vcx: &mut VisualTestContext, label: &str) {
    let center = vcx.update(|window, cx| {
        window.render_frame(cx);
        window.find_by_label(label).bounds().center()
    });
    vcx.simulate_click(center, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
}

fn click(vcx: &mut VisualTestContext, id: impl Into<gpui_kit::ElementId>) {
    let id = id.into();
    vcx.update(|window, cx| window.click(id, cx));
    vcx.run_until_parked();
}

fn keyboard_focus_next(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.focus_next(cx);
        window.press("f13", cx);
    });
    vcx.run_until_parked();
}

fn frames(vcx: &mut VisualTestContext, n: usize) {
    for _ in 0..n {
        vcx.executor().advance_clock(Duration::from_millis(16));
        vcx.update(|w, cx| w.simulate_next_frame(cx));
        vcx.run_until_parked();
    }
}

fn changes<V: PickerValue>(events: &Rc<RefCell<Vec<PickerEvent<V>>>>) -> Vec<Option<V>> {
    events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            PickerEvent::Change(v) => Some(*v),
            PickerEvent::OpenChange(_) => None,
        })
        .collect()
}

fn is_open<V: PickerValue>(m: &mut Mounted<'_, V>) -> bool {
    let p = m.picker.clone();
    m.vcx.update(|_, cx| p.read(cx).is_open())
}
fn value<V: PickerValue>(m: &mut Mounted<'_, V>) -> Option<V> {
    let p = m.picker.clone();
    m.vcx.update(|_, cx| p.read(cx).value())
}

#[gpui_kit::test]
fn pointer_opens_and_picks_without_taking_focus(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, false, |p| p.label("Due date"));
    click(m.vcx, "gr-picker-trigger");
    assert!(is_open(&mut m));
    let input = m.input.clone();
    m.vcx
        .update(|window, _| assert!(input.is_focused(window), "opening stole focus"));
    // The calendar opens on today's month; days are named in the locale's long form.
    click_label(m.vcx, "Monday, March 16, 2026");
    assert_eq!(value(&mut m), Some(d(2026, 3, 16)));
    assert!(!is_open(&mut m), "a date pick closes the popover");
    m.vcx
        .update(|window, _| assert!(input.is_focused(window), "picking stole focus"));
    assert_eq!(changes(&m.events), vec![Some(d(2026, 3, 16))]);
    assert_eq!(
        *m.events
            .borrow()
            .first()
            .unwrap_or(&PickerEvent::Change(None)),
        PickerEvent::OpenChange(true)
    );
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        let combo = window.find_by_label("Due date");
        assert_eq!(combo.role(), Some(Role::ComboBox));
        assert_eq!(combo.value(), Some("Mar 16, 2026"));
    });
}

#[gpui_kit::test]
fn range_pick_orders_and_emits_once(cx: &mut TestAppContext) {
    let mut m = mount::<DateRange>(cx, false, |p| p.label("Stay"));
    click(m.vcx, "gr-picker-trigger");
    click_label(m.vcx, "Tuesday, March 10, 2026");
    assert!(is_open(&mut m), "half a range keeps the popover open");
    assert!(
        changes(&m.events).is_empty(),
        "an open range is not a value"
    );
    let picker = m.picker.clone();
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Stay").value(),
            Some("Mar 10, 2026 – …")
        );
        let _ = picker;
    });
    click_label(m.vcx, "Saturday, March 14, 2026");
    let range = DateRange::new(d(2026, 3, 10), d(2026, 3, 14));
    assert_eq!(value(&mut m), Some(range));
    assert!(!is_open(&mut m));
    assert_eq!(changes(&m.events), vec![Some(range)]);

    // A pick before the start restarts the range; the result is ordered.
    click(m.vcx, "gr-picker-trigger");
    click_label(m.vcx, "Friday, March 20, 2026");
    click_label(m.vcx, "Thursday, March 5, 2026");
    click_label(m.vcx, "Sunday, March 8, 2026");
    let next = DateRange::new(d(2026, 3, 5), d(2026, 3, 8));
    assert_eq!(value(&mut m), Some(next));
}

#[gpui_kit::test]
fn presets_pick_ranges_and_custom_keeps_open(cx: &mut TestAppContext) {
    let mut m = mount::<DateRange>(cx, false, |p| {
        let presets = DatePreset::standard_ranges(&p.locale);
        p.presets(presets).min_date(d(2026, 2, 1))
    });
    click(m.vcx, "gr-picker-trigger");
    click(m.vcx, ("gr-preset", 1usize)); // Last 7 days
    assert_eq!(
        value(&mut m),
        Some(DateRange::new(d(2026, 3, 5), d(2026, 3, 11)))
    );
    assert!(!is_open(&mut m));
    click(m.vcx, "gr-picker-trigger");
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        // The active preset is marked selected; Last month (February) is in bounds.
        assert_eq!(window.find(("gr-preset", 1usize)).checked(), Some(true));
        assert_ne!(window.find(("gr-preset", 4usize)).disabled(), Some(true));
    });
    click(m.vcx, ("gr-preset", 5usize)); // Custom
    assert!(is_open(&mut m), "Custom leaves the calendar open");
    click_label(m.vcx, "Monday, March 2, 2026");
    click_label(m.vcx, "Wednesday, March 4, 2026");
    assert_eq!(
        value(&mut m),
        Some(DateRange::new(d(2026, 3, 2), d(2026, 3, 4)))
    );
}

#[gpui_kit::test]
fn presets_outside_the_bounds_are_disabled(cx: &mut TestAppContext) {
    let mut m = mount::<DateRange>(cx, false, |p| {
        let presets = DatePreset::standard_ranges(&p.locale);
        p.presets(presets).min_date(d(2026, 3, 9))
    });
    click(m.vcx, "gr-picker-trigger");
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(("gr-preset", 4usize)).disabled(), Some(true));
    });
    click(m.vcx, ("gr-preset", 4usize));
    assert_eq!(value(&mut m), None);
    click(m.vcx, ("gr-preset", 2usize)); // Last 30 days, clamped to min
    assert_eq!(
        value(&mut m),
        Some(DateRange::new(d(2026, 3, 9), d(2026, 3, 11)))
    );
}

#[gpui_kit::test]
fn clear_button_clears_without_opening(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, false, |p| {
        p.default_value(d(2026, 3, 3)).clearable(true)
    });
    click(m.vcx, "gr-picker-clear");
    assert_eq!(value(&mut m), None);
    assert!(
        !is_open(&mut m),
        "the clear press does not open the calendar"
    );
    assert_eq!(changes(&m.events), vec![None]);
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("gr-picker-clear").is_none(),
            "no value, no clear"
        );
    });
}

#[gpui_kit::test]
fn disabled_read_only_and_excluded_suppress_input(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, true, |p| {
        p.default_value(d(2026, 3, 3))
            .clearable(true)
            .disabled(true)
            .label("Off")
    });
    click(m.vcx, "gr-picker-trigger");
    assert!(!is_open(&mut m));
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_by_label("Off").disabled(), Some(true));
        assert!(window.try_find("gr-picker-clear").is_none());
    });
    keyboard_focus_next(m.vcx);
    let picker = m.picker.clone();
    let focused = m
        .vcx
        .update(|w, cx| picker.read(cx).focus_handle(cx).is_focused(w));
    assert!(!focused, "a disabled picker is no tab stop");

    // Read-only: shows the value, does not open or clear.
    m.vcx.update(|window, cx| {
        picker.update(cx, |p, cx| {
            p.update_props(
                |p| {
                    p.disabled = false;
                    p.read_only = true;
                },
                window,
                cx,
            )
        })
    });
    click(m.vcx, "gr-picker-trigger");
    assert!(!is_open(&mut m));

    // Excluded days (weekends here) cannot be clicked.
    m.vcx.update(|window, cx| {
        picker.update(cx, |p, cx| {
            p.update_props(
                |p| {
                    p.read_only = false;
                    p.exclude_date = Some(std::sync::Arc::new(|d: NaiveDate| {
                        matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
                    }));
                },
                window,
                cx,
            )
        })
    });
    click(m.vcx, "gr-picker-trigger");
    click_label(m.vcx, "Saturday, March 7, 2026");
    assert_eq!(value(&mut m), Some(d(2026, 3, 3)), "excluded day ignored");
    assert!(is_open(&mut m));
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Saturday, March 7, 2026").disabled(),
            None,
            "Kit gives calendar days no disabled node; the day is styled and inert"
        );
    });
    assert!(changes(&m.events).is_empty());
}

#[gpui_kit::test]
fn keyboard_opens_navigates_picks_and_closes(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, true, |p| {
        p.label("When")
            .clearable(true)
            .first_day_of_week(Weekday::Mon)
    });
    keyboard_focus_next(m.vcx);
    let picker = m.picker.clone();
    assert!(
        m.vcx
            .update(|w, cx| picker.read(cx).focus_handle(cx).is_focused(w))
    );
    assert!(
        m.vcx.debug_bounds("focus-ring").is_some(),
        "keyboard focus shows a ring"
    );
    m.vcx.simulate_keystrokes("down");
    assert!(is_open(&mut m), "ArrowDown opens");
    assert_eq!(m.vcx.update(|_, cx| picker.read(cx).cursor()), today());
    m.vcx.simulate_keystrokes("right right down");
    assert_eq!(
        m.vcx.update(|_, cx| picker.read(cx).cursor()),
        d(2026, 3, 20)
    );
    m.vcx.simulate_keystrokes("home");
    assert_eq!(
        m.vcx.update(|_, cx| picker.read(cx).cursor()),
        d(2026, 3, 16),
        "Home goes to Monday, the locale's week start here"
    );
    m.vcx.simulate_keystrokes("pagedown");
    assert_eq!(
        m.vcx.update(|_, cx| picker.read(cx).cursor()),
        d(2026, 4, 16)
    );
    let shown = m.vcx.update(|_, cx| {
        let c = picker.read(cx).calendar_for_test();
        let c = c.read(cx);
        (c.current_year(), c.current_month())
    });
    assert_eq!(shown, (2026, 4), "the calendar follows the cursor");
    m.vcx.simulate_keystrokes("enter");
    assert_eq!(value(&mut m), Some(d(2026, 4, 16)));
    assert!(!is_open(&mut m), "Enter picks and closes");
    m.vcx.simulate_keystrokes("space");
    assert!(is_open(&mut m), "Space opens");
    m.vcx.simulate_keystrokes("escape");
    assert!(!is_open(&mut m), "Escape closes");
    m.vcx.simulate_keystrokes("enter");
    assert!(is_open(&mut m), "Enter opens");
    m.vcx.simulate_keystrokes("escape backspace");
    assert_eq!(value(&mut m), None, "Backspace clears a clearable picker");
}

#[gpui_kit::test]
fn keyboard_range_pick_and_bounds(cx: &mut TestAppContext) {
    let mut m = mount::<DateRange>(cx, true, |p| {
        p.min_date(d(2026, 3, 9)).max_date(d(2026, 3, 20))
    });
    keyboard_focus_next(m.vcx);
    m.vcx.simulate_keystrokes("enter");
    m.vcx.simulate_keystrokes("up up");
    let picker = m.picker.clone();
    assert_eq!(
        m.vcx.update(|_, cx| picker.read(cx).cursor()),
        d(2026, 3, 9),
        "clamped to min"
    );
    m.vcx.simulate_keystrokes("space pagedown space");
    assert_eq!(
        value(&mut m),
        Some(DateRange::new(d(2026, 3, 9), d(2026, 3, 20))),
        "PageDown clamps to max"
    );
}

#[gpui_kit::test]
fn month_navigation_stays_in_bounds(cx: &mut TestAppContext) {
    let m = mount::<NaiveDate>(cx, false, |p| p.min_date(d(2026, 3, 1)));
    click(m.vcx, "gr-picker-trigger");
    click(m.vcx, "calendar-prev");
    let picker = m.picker.clone();
    let shown = m.vcx.update(|_, cx| {
        let c = picker.read(cx).calendar_for_test();
        let c = c.read(cx);
        (c.current_year(), c.current_month())
    });
    assert_eq!(shown, (2026, 3), "Kit's previous button cannot leave min");
    click(m.vcx, "calendar-next");
    let shown = m.vcx.update(|_, cx| {
        let c = picker.read(cx).calendar_for_test();
        c.read(cx).current_month()
    });
    assert_eq!(shown, 4);
}

#[gpui_kit::test]
fn out_of_range_values_are_clamped(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, false, |p| {
        p.default_value(d(2026, 1, 1))
            .min_date(d(2026, 3, 2))
            .max_date(d(2026, 3, 27))
    });
    assert_eq!(value(&mut m), Some(d(2026, 3, 2)));
    let picker = m.picker.clone();
    m.vcx.update(|window, cx| {
        picker.update(cx, |p, cx| p.set_value(Some(d(2027, 5, 5)), window, cx))
    });
    assert_eq!(value(&mut m), Some(d(2026, 3, 27)));
    assert!(changes(&m.events).is_empty(), "set_value does not emit");
}

#[gpui_kit::test]
fn controlled_value_changes_only_through_the_owner(cx: &mut TestAppContext) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s2 = seen.clone();
    let mut m = mount::<NaiveDate>(cx, false, move |p| {
        p.value(Some(d(2026, 3, 2)))
            .on_change(move |v, _, _| s2.borrow_mut().push(*v))
    });
    click(m.vcx, "gr-picker-trigger");
    click_label(m.vcx, "Thursday, March 19, 2026");
    assert_eq!(*seen.borrow(), vec![Some(d(2026, 3, 19))]);
    assert_eq!(
        value(&mut m),
        Some(d(2026, 3, 2)),
        "the owner has not accepted it"
    );
    let picker = m.picker.clone();
    m.vcx.update(|window, cx| {
        picker.update(cx, |p, cx| p.set_value(Some(d(2026, 3, 19)), window, cx))
    });
    assert_eq!(value(&mut m), Some(d(2026, 3, 19)));
}

#[gpui_kit::test]
fn controlled_open_asks_the_owner(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, false, |p| p.open(false));
    click(m.vcx, "gr-picker-trigger");
    assert!(!is_open(&mut m), "open is controlled");
    assert_eq!(*m.events.borrow(), vec![PickerEvent::OpenChange(true)]);
    let picker = m.picker.clone();
    m.vcx
        .update(|window, cx| picker.update(cx, |p, cx| p.set_open(true, window, cx)));
    assert!(is_open(&mut m));
}

#[gpui_kit::test]
fn date_time_picker_keeps_open_for_the_time(cx: &mut TestAppContext) {
    let mut m = mount::<chrono::NaiveDateTime>(cx, true, |p| {
        p.default_time(t(9, 0)).hour_format(HourFormat::H24)
    });
    click(m.vcx, "gr-picker-trigger");
    click_label(m.vcx, "Friday, March 13, 2026");
    assert!(is_open(&mut m), "the time is edited next");
    assert_eq!(value(&mut m), Some(d(2026, 3, 13).and_time(t(9, 0))));
    // The time field is text-like: a click focuses it; ↑ steps the hour.
    click(m.vcx, "hour");
    let picker = m.picker.clone();
    let (start, _) = m.vcx.update(|_, cx| picker.read(cx).time_fields_for_test());
    assert!(
        m.vcx
            .update(|w, cx| start.read(cx).focus_handle(cx).is_focused(w))
    );
    m.vcx.simulate_keystrokes("up");
    assert_eq!(value(&mut m), Some(d(2026, 3, 13).and_time(t(10, 0))));
    m.vcx.simulate_keystrokes("enter");
    assert!(!is_open(&mut m), "Enter in the time field closes");
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("gr-picker-trigger").bounds().size.height,
            px(guiraffe_core::Size::Sm.control_height())
        );
    });
    assert_eq!(
        changes(&m.events),
        vec![
            Some(d(2026, 3, 13).and_time(t(9, 0))),
            Some(d(2026, 3, 13).and_time(t(10, 0)))
        ]
    );
}

#[gpui_kit::test]
fn date_time_range_edits_both_times(cx: &mut TestAppContext) {
    let mut m = mount::<DateTimeRange>(cx, false, |p| {
        p.default_time(t(9, 0)).hour_format(HourFormat::H24)
    });
    click(m.vcx, "gr-picker-trigger");
    click_label(m.vcx, "Monday, March 9, 2026");
    click_label(m.vcx, "Wednesday, March 11, 2026");
    assert!(is_open(&mut m));
    let picker = m.picker.clone();
    let (_, end) = m.vcx.update(|_, cx| picker.read(cx).time_fields_for_test());
    m.vcx
        .update(|window, cx| end.read(cx).focus_handle(cx).focus(window, cx));
    m.vcx.simulate_keystrokes("1 7");
    assert_eq!(
        value(&mut m),
        Some(DateTimeRange::new(
            d(2026, 3, 9).and_time(t(9, 0)),
            d(2026, 3, 11).and_time(t(17, 0))
        ))
    );
    click(m.vcx, "gr-done");
    assert!(!is_open(&mut m));
}

#[gpui_kit::test]
fn time_picker_types_clamps_and_clears(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveTime>(cx, false, |p| {
        p.clearable(true)
            .hour_format(HourFormat::H24)
            .min_time(t(8, 0))
            .label("Alarm")
    });
    assert_eq!(value(&mut m), None);
    click(m.vcx, "hour");
    let picker = m.picker.clone();
    assert!(
        m.vcx
            .update(|w, cx| picker.read(cx).focus_handle(cx).is_focused(w))
    );
    m.vcx.simulate_keystrokes("1 0 3 0");
    assert_eq!(value(&mut m), Some(t(10, 30)));
    // Typing 07 makes 07:30, which clamps to the 08:00 minimum; leaving the field shows it.
    click(m.vcx, "hour");
    m.vcx.simulate_keystrokes("0 7");
    assert_eq!(value(&mut m), Some(t(8, 0)));
    let input = m.input.clone();
    m.vcx.update(|w, cx| {
        w.focus(&input, cx);
        w.render_frame(cx);
    });
    m.vcx.run_until_parked();
    let picker = m.picker.clone();
    let (field, _) = m.vcx.update(|_, cx| picker.read(cx).time_fields_for_test());
    assert_eq!(m.vcx.update(|_, cx| field.read(cx).time()), t(8, 0));
    click(m.vcx, "gr-picker-clear");
    assert_eq!(value(&mut m), None);
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all_by_role(Role::TimeInput).len(), 1);
    });
}

#[gpui_kit::test]
fn focus_ring_and_popover_never_move_layout(cx: &mut TestAppContext) {
    let sels = ["picker", "below"];
    let mut m = mount::<DateRange>(cx, true, |p| {
        let presets = DatePreset::standard_ranges(&p.locale);
        p.label("Period").presets(presets).clearable(true)
    });
    let base = capture(m.vcx, &sels);
    click(m.vcx, "gr-picker-trigger");
    assert!(is_open(&mut m));
    assert!(m.vcx.debug_bounds("picker-popover").is_some());
    assert_same_bounds(m.vcx, &sels, &base, "the popover is open");
    let input = m.input.clone();
    m.vcx
        .update(|window, _| assert!(input.is_focused(window), "popover click stole focus"));
    click(m.vcx, ("gr-preset", 0usize));
    assert_same_bounds(m.vcx, &sels, &base, "a value is shown with a clear button");
    keyboard_focus_next(m.vcx);
    assert!(m.vcx.debug_bounds("focus-ring").is_some());
    assert_same_bounds(m.vcx, &sels, &base, "the picker has keyboard focus");
    m.vcx.simulate_keystrokes("down");
    assert!(is_open(&mut m));
    assert_same_bounds(m.vcx, &sels, &base, "keyboard-opened with a day cursor");
}

#[gpui_kit::test]
fn an_open_popover_settles_and_closing_renders_little(cx: &mut TestAppContext) {
    let m = mount::<DateRange>(cx, false, |p| p.number_of_months(2));
    click(m.vcx, "gr-picker-trigger");
    frames(m.vcx, 5);
    let before = m.renders.get();
    frames(m.vcx, 10);
    assert_eq!(
        m.renders.get(),
        before,
        "an idle open popover must not re-render every frame"
    );
    m.vcx.update(|w, cx| {
        w.dispatch_action(Box::new(guiraffe_inputs::dates::picker::Dismiss), cx);
    });
    let _ = &m.picker;
    if gpui_kit::GPUI_FAST {
        frames(m.vcx, 2);
        let host = m.renders.get();
        click(m.vcx, "gr-picker-trigger");
        frames(m.vcx, 3);
        assert_eq!(
            m.renders.get(),
            host,
            "opening re-renders the picker only, not its host (gpui-fast)"
        );
    }
}

#[gpui_kit::test]
fn locale_changes_names_and_week_start(cx: &mut TestAppContext) {
    let mut m = mount::<NaiveDate>(cx, false, |p| {
        p.locale(DateLocale::de()).default_value(d(2026, 3, 9))
    });
    click(m.vcx, "gr-picker-trigger");
    m.vcx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find_by_label("Montag, 9. März 2026").is_some());
        assert!(
            window.try_find_by_label("März").is_some(),
            "month toggle in German"
        );
        assert_eq!(
            window.find_by_label("Datum wählen").value(),
            Some("09.03.2026")
        );
    });
    let _ = value(&mut m);
}
