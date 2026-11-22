use std::{rc::Rc, sync::Arc};

use chrono::{Datelike, NaiveDate, Timelike};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, TestSupportExt, Window,
    base::StyledExt,
    canvas,
    component::{
        IconName, Sizable,
        button::{Button, ButtonGroup, ButtonVariants},
    },
    div,
    prelude::*,
    px, relative,
};
use guiraffe_components::Title;
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StateKind, Variant};

use crate::state::{
    CalendarActivate, CalendarCancel, CalendarDown, CalendarLeft, CalendarPageDown, CalendarPageUp,
    CalendarPickUp, CalendarRight, CalendarShiftDown, CalendarShiftLeft, CalendarShiftRight,
    CalendarShiftUp, CalendarToday, CalendarUp, DEFAULT_VIEWPORT_MINUTES, KEY_CONTEXT,
};
use crate::{
    CalendarCursor, CalendarItem, CalendarState, CalendarViewKind, DragMode, EventId, ItemTime,
    ResizeEdge, TimedSlice, collapse_lanes, format_local_time, format_time_range, hour_label,
    layout_overlaps, month_cells, month_chip_plan, row_iso_week, seconds_in_display_day,
    time_seconds_in_display_day, visible_hour_range, week_dates,
};

/// What a custom content renderer receives for one month chip or event block.
///
/// The returned element is placed inside the calendar-owned wrapper, which keeps
/// selection, drag/resize, focus and accessibility; content cannot replace them.
pub struct ItemContent<'a> {
    pub item: &'a CalendarItem,
    /// The item color, or its source color when the item has no override.
    pub color: ColorName,
    /// Localized title as displayed (possibly shortened for narrow lanes).
    pub title: SharedString,
    /// Localized time text in the resolved 12/24-hour format.
    pub time: SharedString,
    /// The item's icon slot, if it has one.
    pub icon: Option<SharedString>,
    /// True when only a one-line compact rendering fits.
    pub compact: bool,
}

/// Custom content callback: return `None` to use the default renderer.
pub type ItemRenderer = Rc<dyn Fn(&ItemContent<'_>) -> Option<AnyElement>>;

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct CalendarProps {
    pub show_toolbar: bool,
}

impl Default for CalendarProps {
    fn default() -> Self {
        Self { show_toolbar: true }
    }
}

#[derive(IntoElement)]
pub struct Calendar {
    state: Entity<CalendarState>,
    props: CalendarProps,
    month_chip_renderer: Option<ItemRenderer>,
    event_block_renderer: Option<ItemRenderer>,
    style: StyleRefinement,
}

impl Calendar {
    pub fn new(state: &Entity<CalendarState>) -> Self {
        Self {
            state: state.clone(),
            props: CalendarProps::default(),
            month_chip_renderer: None,
            event_block_renderer: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn props(mut self, props: CalendarProps) -> Self {
        self.props = props;
        self
    }

    pub fn show_toolbar(mut self, show: bool) -> Self {
        self.props.show_toolbar = show;
        self
    }

    /// Custom content for Month chips and all-day lane chips.
    pub fn month_chip_renderer(mut self, renderer: Option<ItemRenderer>) -> Self {
        self.month_chip_renderer = renderer;
        self
    }

    /// Custom content for Week/Day timed event blocks.
    pub fn event_block_renderer(mut self, renderer: Option<ItemRenderer>) -> Self {
        self.event_block_renderer = renderer;
        self
    }
}

impl Styled for Calendar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Per-render values shared by every view.
struct Frame<'a> {
    state: &'a Entity<CalendarState>,
    /// The calendar has keyboard focus: draw the roving cursor.
    focused: bool,
    /// Grid chips and affordances are Tab stops.
    tab: bool,
    ring: Hsla,
}

impl RenderOnce for Calendar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        let view = state.view;
        let overflow = state.overflow;
        let focus = state.focus.clone();
        let tab = guiraffe_core::tab_stop(state.focusable);
        let focused = focus.is_focused(window);
        let ring_on = guiraffe_core::ring_visible(&focus, window);
        let ring_color = GuiraffeTheme::global(cx)
            .state(StateKind::FocusRing, Variant::Default, None)
            .border;
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        let (rows, columns) = match view {
            CalendarViewKind::Year => (12, 1),
            CalendarViewKind::Month => (6, 7),
            CalendarViewKind::Week => (1, 7),
            CalendarViewKind::Day => (1, 1),
        };
        let label = format!("Calendar, {}", heading(state));
        let frame = Frame {
            state: &self.state,
            focused,
            tab,
            ring: ring_color.to_hsla(),
        };
        let mut root = div()
            .id("guiraffe-calendar")
            .test_support()
            .debug_selector(|| "calendar".into())
            .role(Role::Grid)
            .aria_label(label)
            .aria_row_count(rows)
            .aria_column_count(columns)
            .track_focus(&focus.tab_stop(tab))
            .key_context(KEY_CONTEXT)
            .size_full()
            .relative()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(guiraffe_core::StyleContext::current()
                .density
                .space(Size::Sm)))
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla());
        root = bind_actions(root, &self.state).on_mouse_up(MouseButton::Left, {
            let state = self.state.clone();
            move |_, _, cx| {
                state.update(cx, |state, cx| {
                    state.commit_drag(cx);
                })
            }
        });
        if self.props.show_toolbar {
            root = root.child(toolbar(&frame, cx));
        }
        let content = match view {
            CalendarViewKind::Year => year_view(&frame, cx),
            CalendarViewKind::Month => month_view(&frame, self.month_chip_renderer.as_ref(), cx),
            CalendarViewKind::Week | CalendarViewKind::Day => timed_view(
                &frame,
                view == CalendarViewKind::Day,
                self.event_block_renderer.as_ref(),
                self.month_chip_renderer.as_ref(),
                cx,
            ),
        };
        root.child(content)
            .when_some(overflow, |root, date| {
                root.child(overflow_overlay(&frame, date, cx))
            })
            .child(pointer_capture(&self.state))
            .map(|root| guiraffe_core::focus_ring(root, ring_on, px(Size::Md.radius()), ring_color))
            .map(|root| StyledExt::refine_style(root, &self.style))
    }
}

fn heading(state: &CalendarState) -> String {
    let anchor = state.anchor;
    let locale = &state.locale;
    match state.view {
        CalendarViewKind::Year => format!("{}", anchor.year()),
        CalendarViewKind::Month => format!("{} {}", locale.month(anchor.month()), anchor.year()),
        CalendarViewKind::Week => {
            let dates = week_dates(anchor, state.first_weekday());
            match (dates.first(), dates.last()) {
                (Some(first), Some(last)) if first.month() == last.month() => format!(
                    "{} {} – {}, {}",
                    locale.month(first.month()),
                    first.day(),
                    last.day(),
                    last.year()
                ),
                (Some(first), Some(last)) if first.year() == last.year() => format!(
                    "{} {} – {} {}, {}",
                    locale.month(first.month()),
                    first.day(),
                    locale.month(last.month()),
                    last.day(),
                    last.year()
                ),
                (Some(first), Some(last)) => format!(
                    "{} {}, {} – {} {}, {}",
                    locale.month(first.month()),
                    first.day(),
                    first.year(),
                    locale.month(last.month()),
                    last.day(),
                    last.year()
                ),
                _ => String::new(),
            }
        }
        CalendarViewKind::Day => locale.date_label(anchor),
    }
}

const fn view_index(view: CalendarViewKind) -> usize {
    match view {
        CalendarViewKind::Year => 0,
        CalendarViewKind::Month => 1,
        CalendarViewKind::Week => 2,
        CalendarViewKind::Day => 3,
    }
}

fn toolbar(frame: &Frame<'_>, cx: &mut App) -> impl IntoElement {
    let state = frame.state;
    let current = state.read(cx);
    let labels = current.locale.labels;
    let index = view_index(current.view);
    let title = heading(current);
    let sources = current.model.sources().to_vec();
    let switcher = current.view_switcher.clone();
    let navigation = ButtonGroup::new("calendar-navigation")
        .outline()
        .child(navigation_button(
            state,
            "calendar-previous",
            "‹",
            labels.previous.get(index).copied().unwrap_or("Previous"),
            -1,
            frame.tab,
        ))
        .child(
            Button::new("calendar-today")
                .compact()
                .label(labels.today)
                .tab_stop(frame.tab)
                .on_click({
                    let state = state.clone();
                    move |_, _, cx| state.update(cx, |state, cx| state.today(cx))
                }),
        )
        .child(navigation_button(
            state,
            "calendar-next",
            "›",
            labels.next.get(index).copied().unwrap_or("Next"),
            1,
            frame.tab,
        ));
    let mut bar = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(Size::Sm.spacing()))
        .child(navigation)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(Title::new(title).order(4)),
        )
        .child(switcher);
    for source in sources {
        let entity = state.clone();
        let id = source.id;
        let visible = source.visible;
        // Hue is never the only cue: a visible source also carries a check mark
        // and a toggled accessibility state; a hidden one is dimmed.
        let colors = resolved_color(
            source.color,
            if visible {
                Variant::Light
            } else {
                Variant::Default
            },
            cx,
        );
        let mut button = Button::new(ElementId::Name(format!("calendar-source-{}", id.0).into()))
            .compact()
            .rounded_full()
            .label(source.name.to_string())
            .accessibility_label(format!("{} calendar", source.name))
            .toggled(visible)
            .tab_stop(frame.tab)
            .bg(colors.background.to_hsla())
            .text_color(colors.foreground.to_hsla())
            .on_click(move |_, _, cx| {
                entity.update(cx, |state, cx| state.set_source_visible(id, !visible, cx))
            });
        if visible {
            button = button.icon(IconName::Check);
        } else {
            button = button.opacity(0.7);
        }
        bar = bar.child(button);
    }
    bar
}

fn navigation_button(
    state: &Entity<CalendarState>,
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    delta: i32,
    tab: bool,
) -> Button {
    let state = state.clone();
    Button::new(id)
        .compact()
        .accessibility_label(label)
        .tooltip(label)
        .label(glyph)
        .tab_stop(tab)
        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.navigate(delta, cx)))
}

fn event_at_position(event: &MouseMoveEvent, origin_minute: i32, origin_y: f32) -> i32 {
    let y: f32 = event.position.y.into();
    origin_minute + (((y - origin_y) / default_hour_height()) * 60.0).round() as i32
}

/// The hour grid's live scroll window in minutes, read from the scroll handle
/// so wheel scrolling (which re-renders) virtualizes the right hours.
fn scroll_window(current: &CalendarState) -> (i32, i32) {
    let offset: f32 = current.scroll.offset().y.into();
    let height: f32 = current.scroll.bounds().size.height.into();
    let per_minute = default_hour_height() / 60.0;
    let scroll = ((-offset) / per_minute).round() as i32;
    let viewport = if height > 0.0 {
        (height / per_minute).round() as i32
    } else {
        current.viewport_minutes.max(DEFAULT_VIEWPORT_MINUTES)
    };
    (scroll.clamp(0, 1_440), viewport.clamp(0, 1_440))
}

fn timed_view(
    frame: &Frame<'_>,
    one_day: bool,
    renderer: Option<&ItemRenderer>,
    chip_renderer: Option<&ItemRenderer>,
    cx: &mut App,
) -> AnyElement {
    let state = frame.state;
    let current = state.read(cx);
    let zone = current.zone;
    let dates = if one_day {
        vec![current.anchor]
    } else {
        week_dates(current.anchor, current.first_weekday())
    };
    let locale = &current.locale;
    let labels = locale.labels;
    let index = current.index.clone();
    let hour_height = default_hour_height();
    let (scroll_minutes, viewport_minutes) = scroll_window(current);
    let (first_minute, last_minute) = visible_hour_range(scroll_minutes, viewport_minutes, 1);
    let hours = (first_minute / 60) as u32..=((last_minute.max(1) - 1) / 60).clamp(0, 23) as u32;
    let now = current.show_now.then(|| zone.local(current.now));
    let now_date = now.map(|time| time.date_naive());
    let now_minute = now.map(|time| (time.hour() * 60 + time.minute()) as i32);
    let today = zone.local(current.now).date_naive();
    let show_business = current.show_business_hours;
    let format = current.time_format();
    let theme = GuiraffeTheme::global(cx);
    let grid_color = theme.surfaces().border.to_hsla();
    let shade = theme.surfaces().track.to_hsla();
    let dimmed = theme.surfaces().dimmed.to_hsla();
    let today_colors = theme.variant(None, Variant::Filled);
    let max_lanes = if one_day { 8 } else { 3 };
    let day_count = dates.len();
    let weekdays = dates
        .iter()
        .map(|date| {
            let is_today = *date == today;
            let selected = *date == current.selected_date;
            let number = div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(Size::Xs.control_height()))
                .rounded_full()
                .when(is_today, |number| {
                    number
                        .bg(today_colors.background.to_hsla())
                        .text_color(today_colors.foreground.to_hsla())
                })
                .when(!is_today && selected, |number| {
                    number
                        .border_1()
                        .border_color(today_colors.background.to_hsla())
                })
                .child(day_label(date.day()));
            div()
                .flex_1()
                .min_w_0()
                .h(px(Size::Sm.control_height()))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(Size::Xs.spacing() / 2.0))
                .child(div().text_color(dimmed).child(SharedString::new_static(
                    locale.short_weekday(date.weekday()),
                )))
                .child(number)
                .into_any_element()
        })
        .collect::<Vec<_>>();
    let mut columns = Vec::with_capacity(day_count);
    let mut all_day_lanes = Vec::with_capacity(day_count);
    for (column, date) in dates.into_iter().enumerate() {
        let day_events = index.day(date);
        let layout = current.labels.day_layout((date, max_lanes), || {
            day_layout(day_events, date, zone, max_lanes)
        });
        let hidden_events = layout.hidden;
        let hidden_top = layout.hidden_top.max(first_minute * 60);
        let mut day = div()
            .id(("calendar-day-column", column))
            .debug_selector(move || format!("calendar-day-{date}"))
            .role(Role::GridCell)
            .aria_row_index(1)
            .aria_column_index(column + 1)
            .aria_label(
                current
                    .labels
                    .date("day-column", date, || {
                        format!(
                            "{}, {}",
                            locale.date_label(date),
                            locale.count_label(day_events.len())
                        )
                    })
                    .1,
            )
            .flex_1()
            .min_w_0()
            .relative()
            .h(px(1_440.0 / 60.0 * hour_height))
            .border_l_1()
            .border_color(grid_color);
        // One canvas paints shading and hour/half-hour lines for the visible
        // hours: no per-hour elements, listeners or strings.
        let shading: [(i32, i32); 2] = if show_business {
            let mut ranges = current.business_hours.non_working_minutes(date.weekday());
            [
                ranges.next().unwrap_or((0, 0)),
                ranges.next().unwrap_or((0, 0)),
            ]
        } else {
            [(0, 0), (0, 0)]
        };
        let (grid_first, grid_last) = (first_minute, last_minute);
        day = day
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let per_minute = hour_height / 60.0;
                        for (start, end) in shading {
                            let (start, end) = (start.max(grid_first), end.min(grid_last));
                            if start < end {
                                window.paint_quad(gpui_kit::fill(
                                    gpui_kit::Bounds::new(
                                        gpui_kit::point(
                                            bounds.left(),
                                            bounds.top() + px(start as f32 * per_minute),
                                        ),
                                        gpui_kit::size(
                                            bounds.size.width,
                                            px((end - start) as f32 * per_minute),
                                        ),
                                    ),
                                    shade,
                                ));
                            }
                        }
                        let mut minute = (grid_first / 30) * 30;
                        while minute <= grid_last {
                            if minute > 0 {
                                let color = if minute % 60 == 0 {
                                    grid_color
                                } else {
                                    grid_color.opacity(0.5)
                                };
                                window.paint_quad(gpui_kit::fill(
                                    gpui_kit::Bounds::new(
                                        gpui_kit::point(
                                            bounds.left(),
                                            bounds.top() + px(minute as f32 * per_minute),
                                        ),
                                        gpui_kit::size(bounds.size.width, px(1.)),
                                    ),
                                    color,
                                ));
                            }
                            minute += 30;
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, {
                let state = state.clone();
                move |event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    state.update(cx, |state, cx| {
                        let minute = minute_at(state, event.position.y.into());
                        state.cursor = CalendarCursor { date, minute };
                        state.pointer_origin_y = Some(event.position.y.into());
                        state.begin_create(date, minute, cx);
                    });
                }
            })
            .on_mouse_move({
                let state = state.clone();
                move |event: &MouseMoveEvent, _, cx| {
                    let current = state.read(cx);
                    let Some(drag) = current.drag.as_ref() else {
                        return;
                    };
                    if current.keyboard.is_some()
                        || !matches!(drag.proposed, ItemTime::Timed { .. })
                    {
                        return;
                    }
                    let origin_y = current.pointer_origin_y.unwrap_or(event.position.y.into());
                    let minute = event_at_position(event, drag.origin_minute, origin_y);
                    state.update(cx, |state, cx| {
                        state.update_timed_drag(date, minute.clamp(-1_440, 2_880), cx)
                    });
                }
            });
        // Blocks share a layer; with collapsed lanes it leaves a right gutter
        // for the "+N" count so the affordance never covers an event.
        let mut blocks = div()
            .absolute()
            .top_0()
            .left_0()
            .bottom_0()
            .right(px(if hidden_events > 0 { 32. } else { 0. }));
        for (item_index, position) in &layout.blocks {
            // Only blocks intersecting the visible hours become elements.
            if position.end_second <= first_minute * 60 || position.start_second >= last_minute * 60
            {
                continue;
            }
            let Some(item) = day_events.get(*item_index) else {
                continue;
            };
            blocks = blocks.child(event_block(
                frame, current, item, position, date, one_day, renderer, cx,
            ));
        }
        day = day.child(blocks);
        if frame.focused
            && current.drag.is_none()
            && current.cursor.date == date
            && matches!(current.view, CalendarViewKind::Week | CalendarViewKind::Day)
        {
            day = day.child(
                div()
                    .id("calendar-cursor")
                    .absolute()
                    .left_0()
                    .w_full()
                    .top(px(current.cursor.minute as f32 / 60.0 * hour_height))
                    .h(px(15.0 / 60.0 * hour_height))
                    .border_2()
                    .border_color(frame.ring)
                    .rounded(px(Size::Xs.radius())),
            );
        }
        if let Some(drag) = current.drag.as_ref()
            && let Some((start, end)) = time_seconds_in_display_day(&drag.proposed, date, zone)
            && end > first_minute * 60
            && start < last_minute * 60
        {
            let preview = resolved_color(ColorName::Blue, Variant::Light, cx);
            let label = if drag.mode == DragMode::Create {
                labels.new_event
            } else {
                labels.edit_preview
            };
            let time = match &drag.proposed {
                ItemTime::Timed { start, end } => {
                    format_time_range(zone.local(*start), zone.local(*end), format)
                }
                ItemTime::AllDay { .. } => String::new(),
            };
            day = day.child(
                div()
                    .id("calendar-drag-preview")
                    .absolute()
                    .left_0()
                    .w_full()
                    .top(px(start as f32 / 60.0))
                    .h(px(((end - start) as f32 / 60.0).max(15.0)))
                    .overflow_hidden()
                    .px(px(Size::Xs.spacing() / 2.0))
                    .rounded(px(Size::Sm.radius()))
                    .border_2()
                    .border_dashed()
                    .border_color(preview.foreground.to_hsla())
                    .bg(preview.background.to_hsla())
                    .text_color(preview.foreground.to_hsla())
                    .text_size(px(Size::Xs.font_size()))
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(format!("{label} · {time}")),
            );
        }
        if hidden_events > 0 {
            let entity = state.clone();
            day = day.child(
                div()
                    .absolute()
                    .top(px(hidden_top as f32 / 60.0 + 2.0))
                    .right(px(1.0))
                    .child(
                        Button::new(ElementId::Name(format!("overlap-overflow-{date}").into()))
                            .ghost()
                            .xsmall()
                            .label(format!("+{hidden_events}"))
                            .accessibility_label(format!("{hidden_events} more overlapping events"))
                            .tab_stop(frame.tab)
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |state, cx| state.set_overflow(Some(date), cx))
                            }),
                    ),
            );
        }
        if now_date == Some(date)
            && let Some(minute) = now_minute
            && minute >= first_minute
            && minute <= last_minute
        {
            let red = resolved_color(ColorName::Red, Variant::Filled, cx)
                .background
                .to_hsla();
            day = day.child(
                div()
                    .id("calendar-now-line")
                    .absolute()
                    .left_0()
                    .w_full()
                    .top(px(minute as f32 / 60.0 * hour_height - 1.0))
                    .h(px(2.))
                    .bg(red)
                    .child(
                        div()
                            .absolute()
                            .left(px(-4.))
                            .top(px(-3.))
                            .size(px(8.))
                            .rounded_full()
                            .bg(red),
                    ),
            );
        }
        all_day_lanes.push(all_day_lane(
            frame,
            current,
            date,
            one_day,
            chip_renderer,
            cx,
        ));
        columns.push(day.into_any_element());
    }
    let gutter_width = Size::Xl.control_height() + Size::Sm.spacing();
    let mut time_gutter = div()
        .w(px(gutter_width))
        .flex_none()
        .relative()
        .h(px(1_440.0 / 60.0 * hour_height));
    for hour in hours {
        // Labels sit just below their hour line so the first visible one is
        // never clipped by the top of the scroll viewport.
        time_gutter = time_gutter.child(
            div()
                .absolute()
                .debug_selector(move || format!("calendar-hour-label-{hour}"))
                .top(px(hour as f32 * hour_height + 2.0))
                .right_0()
                .pr(px(Size::Xs.spacing() / 2.0))
                .text_size(px(Size::Xs.font_size()))
                .text_color(dimmed)
                .child(hour_label(hour, format)),
        );
    }
    let scroll = current.scroll.clone();
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .child(div().w(px(gutter_width)).flex_none())
                .children(weekdays),
        )
        .child(
            div()
                .flex()
                .border_b_1()
                .border_color(grid_color)
                .child(
                    div()
                        .w(px(gutter_width))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .pr(px(Size::Xs.spacing() / 2.0))
                        .text_size(px(Size::Xs.font_size()))
                        .text_color(dimmed)
                        .child(labels.all_day),
                )
                .children(all_day_lanes),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .child(
                    div()
                        .id("calendar-hour-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .child(
                            div()
                                .flex()
                                .h(px(1_440.0 / 60.0 * hour_height))
                                .child(time_gutter)
                                .children(columns),
                        ),
                )
                .child(paint_sync(state)),
        )
        .into_any_element()
}

/// Overlap layout of one display day over the whole day (stable lanes while
/// scrolling), collapsed to `max_lanes`.
fn day_layout(
    day_events: &[Arc<CalendarItem>],
    date: NaiveDate,
    zone: crate::CalendarZone,
    max_lanes: usize,
) -> crate::state::DayLayout {
    let mut slices: Vec<TimedSlice> = day_events
        .iter()
        .filter_map(|item| {
            let (start, end) = seconds_in_display_day(item, date, zone)?;
            Some(TimedSlice {
                id: item.id,
                start_second: start,
                end_second: end,
            })
        })
        .collect();
    let mut positions = layout_overlaps(&mut slices);
    let hidden_top = positions
        .iter()
        .filter(|position| position.lane >= max_lanes)
        .map(|position| position.start_second)
        .min()
        .unwrap_or(0);
    let hidden = collapse_lanes(&mut positions, max_lanes);
    let order: std::collections::HashMap<EventId, usize> = day_events
        .iter()
        .enumerate()
        .map(|(index, item)| (item.id, index))
        .collect();
    let blocks = positions
        .into_iter()
        .filter_map(|position| Some((*order.get(&position.id)?, position)))
        .collect();
    crate::state::DayLayout {
        blocks,
        hidden,
        hidden_top,
    }
}

/// Wall-clock minute under window `y` in the hour grid, snapped down to
/// fifteen minutes. The content origin is the scroll viewport top plus offset.
fn minute_at(state: &CalendarState, y: f32) -> i32 {
    let top: f32 = (state.scroll.bounds().origin.y + state.scroll.offset().y).into();
    let minute = ((y - top) / default_hour_height() * 60.0).floor() as i32;
    (minute.clamp(0, 1_425) / 15) * 15
}

#[allow(clippy::too_many_arguments)]
fn event_block(
    frame: &Frame<'_>,
    current: &CalendarState,
    item: &Arc<CalendarItem>,
    position: &crate::PositionedEvent,
    date: NaiveDate,
    one_day: bool,
    renderer: Option<&ItemRenderer>,
    cx: &App,
) -> AnyElement {
    let state = frame.state;
    let zone = current.zone;
    let format = current.time_format();
    let labels = current.locale.labels;
    let height = ((position.end_second - position.start_second) as f32 / 60.0).max(15.0);
    let full_title = SharedString::from(item.title.clone());
    let title = if one_day || position.lanes <= 1 {
        full_title.clone()
    } else {
        SharedString::from(clip_label(&item.title, (14 / position.lanes).max(3)))
    };
    let compact = height < Size::Xl.control_height();
    let (continues_before, continues_after) = match item.time {
        ItemTime::Timed { start, end } => {
            let (start, end) = (zone.local(start), zone.local(end));
            (
                start.date_naive() < date,
                end.date_naive() > date
                    && !(end.date_naive() == date.succ_opt().unwrap_or(date)
                        && end.time() == chrono::NaiveTime::MIN),
            )
        }
        ItemTime::AllDay { .. } => (false, false),
    };
    let color = item_color(current, item);
    let locked = item.locked || !current.editable;
    let labels_for = current.labels.event((item.id, date), || {
        let time = match item.time {
            ItemTime::Timed { start, end } => {
                format_time_range(zone.local(start), zone.local(end), format)
            }
            ItemTime::AllDay { .. } => labels.all_day.to_string(),
        };
        let mut label = format!("{full_title}, {time}");
        for (on, text) in [
            (continues_before, labels.continues_before),
            (continues_after, labels.continues_after),
            (item.locked, labels.locked),
        ] {
            if on {
                label.push_str(", ");
                label.push_str(text);
            }
        }
        (time, label)
    });
    let time_text = labels_for.time;
    let label = labels_for.label;
    let block_id = labels_for.id;
    let content = ItemContent {
        item,
        color,
        title: title.clone(),
        time: time_text.clone(),
        icon: item
            .icon
            .as_ref()
            .map(|icon| SharedString::from(icon.to_string())),
        compact,
    };
    let content = renderer
        .and_then(|renderer| renderer(&content))
        .unwrap_or_else(|| default_item_content(&content));
    let light = resolved_color(color, Variant::Light, cx);
    let filled = resolved_color(color, Variant::Filled, cx);
    let selected = current.selected_item == Some(item.id);
    let id = item.id;
    let origin_minute = position.start_second / 60;
    let radius = px(Size::Sm.radius());
    let mut block = div()
        .id(ElementId::Name(block_id))
        .test_support()
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(selected)
        .absolute()
        .top(px(position.start_second as f32 / 60.0))
        .left(relative(
            position.lane as f32 / position.lanes.max(1) as f32,
        ))
        .w(relative(1.0 / position.lanes.max(1) as f32))
        .h(px(height))
        .overflow_hidden()
        .rounded(radius)
        .when(continues_before, |block| block.rounded_t(px(0.)))
        .when(continues_after, |block| block.rounded_b(px(0.)))
        // The source accent is the left border; selection adds a ring.
        .border_l(px(3.))
        .border_color(filled.background.to_hsla())
        .when(selected, |block| block.border_2().border_color(frame.ring))
        .bg(light
            .background
            .over(GuiraffeTheme::global(cx).surfaces().body)
            .to_hsla())
        .text_color(light.foreground.to_hsla())
        .pl(px(Size::Xs.spacing() / 2.0))
        .pr(px(2.))
        .when(compact, |block| block.py(px(1.)))
        .when(!compact, |block| block.py(px(Size::Xs.spacing() / 3.0)))
        .child(content)
        .on_click({
            let state = state.clone();
            move |_, _, cx| state.update(cx, |state, cx| state.select_item(id, cx))
        });
    if locked {
        block = block.on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        });
    } else {
        block = block
            .cursor_grab()
            .on_mouse_down(MouseButton::Left, {
                let state = state.clone();
                move |event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    state.update(cx, |state, cx| {
                        state.pointer_origin_y = Some(event.position.y.into());
                        state.begin_item_drag(id, DragMode::Move, date, origin_minute, cx);
                    });
                }
            })
            // Pointer movement is handled once by the day column (and the
            // window capture), not per block.
            // Resize handles exist only on the selected item, which keeps
            // dense weeks cheap; select (click/Enter) then drag an edge.
            .when(selected, |block| {
                block
                    .child(resize_handle(
                        state,
                        id,
                        ResizeEdge::Start,
                        date,
                        origin_minute,
                    ))
                    .child(resize_handle(
                        state,
                        id,
                        ResizeEdge::End,
                        date,
                        position.end_second / 60,
                    ))
            });
    }
    block.into_any_element()
}

fn all_day_lane(
    frame: &Frame<'_>,
    current: &CalendarState,
    date: NaiveDate,
    one_day: bool,
    renderer: Option<&ItemRenderer>,
    cx: &App,
) -> AnyElement {
    let state = frame.state;
    let labels = current.locale.labels;
    let items: Vec<&Arc<CalendarItem>> = current
        .index
        .day(date)
        .iter()
        .filter(|item| matches!(item.time, ItemTime::AllDay { .. }))
        .collect();
    let capacity = if one_day { 4 } else { 1 };
    let (shown, remaining) = if items.len() <= capacity {
        (items.len(), 0)
    } else {
        (
            capacity.saturating_sub(1).max(1),
            items.len() - capacity.saturating_sub(1).max(1),
        )
    };
    let mut lane = div()
        .flex_1()
        .min_w_0()
        .h(px(Size::Sm.control_height()))
        .px(px(2.))
        .flex()
        .items_center()
        .gap(px(2.))
        .border_l_1()
        .border_color(GuiraffeTheme::global(cx).surfaces().border.to_hsla())
        .on_mouse_down(MouseButton::Left, {
            let state = state.clone();
            move |_, _, cx| state.update(cx, |state, cx| state.begin_all_day_create(date, cx))
        })
        .on_mouse_move({
            let state = state.clone();
            move |_: &MouseMoveEvent, _, cx| {
                state.update(cx, |state, cx| state.update_day_drag(date, cx))
            }
        });
    for item in items.iter().take(shown) {
        lane = lane.child(
            div()
                .flex_1()
                .min_w_0()
                .child(month_chip(frame, item, current, date, renderer, cx)),
        );
    }
    if let Some(drag) = current.drag.as_ref()
        && let ItemTime::AllDay {
            start,
            end_exclusive,
        } = drag.proposed
        && start <= date
        && date < end_exclusive
    {
        lane = lane.child(all_day_preview(
            if drag.mode == DragMode::Create {
                labels.new_event
            } else {
                labels.edit_preview
            },
            cx,
        ));
    }
    if remaining > 0 {
        lane = lane.child(overflow_affordance(frame, current, date, remaining));
    }
    lane.into_any_element()
}

fn all_day_preview(label: &'static str, cx: &App) -> AnyElement {
    let preview = resolved_color(ColorName::Blue, Variant::Light, cx);
    div()
        .id("calendar-all-day-preview")
        .flex_1()
        .min_w_0()
        .h(px(month_chip_height()))
        .px(px(Size::Xs.spacing() / 2.0))
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .rounded(px(Size::Sm.radius()))
        .border_2()
        .border_dashed()
        .border_color(preview.foreground.to_hsla())
        .bg(preview.background.to_hsla())
        .text_color(preview.foreground.to_hsla())
        .text_size(px(Size::Xs.font_size()))
        .child(label)
        .into_any_element()
}

/// `+N more`: an operable, accessible affordance that opens the overflow list.
fn overflow_affordance(
    frame: &Frame<'_>,
    current: &CalendarState,
    date: NaiveDate,
    remaining: usize,
) -> AnyElement {
    let state = frame.state.clone();
    let expanded = current.overflow == Some(date);
    div()
        .id(ElementId::Name(format!("month-overflow-{date}").into()))
        .test_support()
        .debug_selector(move || format!("calendar-overflow-{date}"))
        .role(Role::Button)
        .aria_label(format!("{remaining} more events"))
        .aria_expanded(expanded)
        .flex_none()
        .px(px(4.))
        .rounded(px(Size::Xs.radius()))
        .text_size(px(Size::Xs.font_size()))
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|style| style.underline())
        .child(format!("+{remaining} more"))
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_click(move |_, _, cx| {
            state.update(cx, |state, cx| {
                state.set_overflow(if expanded { None } else { Some(date) }, cx)
            })
        })
        .into_any_element()
}

/// Paint-time hook over the hour viewport: syncs scroll/viewport into the state
/// and arms or stops the now-line timer. It only reads state on steady frames
/// and defers an update when something changed, so warm repaints do not allocate.
fn paint_sync(state: &Entity<CalendarState>) -> AnyElement {
    let state = state.clone();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let current = state.read(cx);
            let (scroll, viewport) = scroll_window(current);
            let on_screen = window.content_mask().bounds.intersects(&bounds)
                && bounds.size.height > px(0.)
                && bounds.size.width > px(0.);
            if current.needs_paint_sync(scroll, viewport, on_screen) {
                let state = state.clone();
                cx.defer(move |cx| {
                    state.update(cx, |state, cx| {
                        state.sync_after_paint(scroll, viewport, on_screen, cx)
                    })
                });
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

fn pointer_capture(state: &Entity<CalendarState>) -> AnyElement {
    let movement = state.clone();
    let release = state.clone();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != gpui_kit::DispatchPhase::Capture {
                    return;
                }
                let current = movement.read(cx);
                let Some(drag) = current.drag.as_ref() else {
                    return;
                };
                if current.keyboard.is_some() || !matches!(drag.proposed, ItemTime::Timed { .. }) {
                    return;
                }
                let origin_minute = drag.origin_minute;
                let date = drag.origin_date;
                let origin_y = current.pointer_origin_y.unwrap_or(event.position.y.into());
                let minute = event_at_position(event, origin_minute, origin_y);
                movement.update(cx, |state, cx| state.update_timed_drag(date, minute, cx));
            });
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == gpui_kit::DispatchPhase::Capture
                    && release.read(cx).keyboard.is_none()
                    && release.read(cx).drag.is_some()
                {
                    release.update(cx, |state, cx| {
                        state.commit_drag(cx);
                    });
                }
            });
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

fn resize_handle(
    state: &Entity<CalendarState>,
    id: EventId,
    edge: ResizeEdge,
    date: NaiveDate,
    origin_minute: i32,
) -> AnyElement {
    let down = state.clone();
    let movement = state.clone();
    let top = edge == ResizeEdge::Start;
    let side = if top { "start" } else { "end" };
    div()
        .id(ElementId::Name(
            format!("calendar-resize-{side}-{}-{date}", id.0).into(),
        ))
        .test_support()
        .absolute()
        .left_0()
        .w_full()
        .h(px(4.))
        .when(top, |handle| handle.top_0())
        .when(!top, |handle| handle.bottom_0())
        .cursor(gpui_kit::CursorStyle::ResizeUpDown)
        .on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                down.update(cx, |state, cx| {
                    state.pointer_origin_y = Some(event.position.y.into());
                    state.begin_item_drag(id, DragMode::Resize(edge), date, origin_minute, cx);
                });
            },
        )
        .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
            let current = movement.read(cx);
            if current.drag.is_none() {
                return;
            }
            let origin_y = current.pointer_origin_y.unwrap_or(event.position.y.into());
            let minute = event_at_position(event, origin_minute, origin_y);
            movement.update(cx, |state, cx| state.update_timed_drag(date, minute, cx));
        })
        .into_any_element()
}

fn day_resize_handle(
    state: &Entity<CalendarState>,
    id: EventId,
    edge: ResizeEdge,
    date: NaiveDate,
) -> AnyElement {
    let down = state.clone();
    let movement = state.clone();
    let start = edge == ResizeEdge::Start;
    let side = if start { "start" } else { "end" };
    div()
        .id(ElementId::Name(
            format!("calendar-day-resize-{side}-{}-{date}", id.0).into(),
        ))
        .test_support()
        .absolute()
        .top_0()
        .h_full()
        .w(px(4.))
        .when(start, |handle| handle.left_0())
        .when(!start, |handle| handle.right_0())
        .cursor(gpui_kit::CursorStyle::ResizeLeftRight)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            down.update(cx, |state, cx| {
                state.begin_item_drag(id, DragMode::Resize(edge), date, 0, cx)
            });
        })
        .on_mouse_move(move |_: &MouseMoveEvent, _, cx| {
            movement.update(cx, |state, cx| state.update_day_drag(date, cx));
        })
        .into_any_element()
}

fn clip_label(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut label: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        label.push('…');
    }
    label
}

fn default_item_content(content: &ItemContent<'_>) -> AnyElement {
    let timed = matches!(content.item.time, ItemTime::Timed { .. });
    // The title keeps priority; the time shrinks first. Three elements in
    // either layout, so dense weeks stay cheap.
    let title = div()
        .flex_shrink_0()
        .max_w_full()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .font_weight(gpui_kit::FontWeight::MEDIUM)
        .when_some(content.icon.clone(), |title, icon| {
            title.child(icon).child(" ")
        })
        .child(content.title.clone());
    let time = timed.then(|| {
        div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .opacity(0.8)
            .child(content.time.clone())
    });
    let body = div()
        .size_full()
        .flex()
        .overflow_hidden()
        .text_size(px(Size::Xs.font_size()))
        .line_height(px(Size::Xs.font_size() * 1.3));
    let body = if content.compact {
        body.flex_row().items_start().gap(px(4.))
    } else {
        body.flex_col()
    };
    body.child(title).children(time).into_any_element()
}

/// Day-of-month label without allocating.
fn day_label(day: u32) -> &'static str {
    const DAYS: [&str; 32] = [
        "", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
    ];
    DAYS.get(day as usize).copied().unwrap_or("")
}

pub fn default_hour_height() -> f32 {
    60.0
}

fn month_chip_height() -> f32 {
    Size::Xs.font_size() * 1.3 + 6.0
}

fn year_view(frame: &Frame<'_>, cx: &mut App) -> AnyElement {
    let state = frame.state;
    let current = state.read(cx);
    let index = current.index.clone();
    let locale = &current.locale;
    let first_weekday = current.first_weekday();
    let today = current.zone.local(current.now).date_naive();
    let selected = current.selected_date;
    let cursor = current.cursor.date;
    let iso = current.show_iso_weeks;
    let year = current.anchor.year();
    let theme = GuiraffeTheme::global(cx);
    let dimmed = theme.surfaces().dimmed.to_hsla();
    let today_colors = theme.variant(None, Variant::Filled);
    let cell = Size::Xs.control_height() - 4.0;
    let marker_color = resolved_color(ColorName::Blue, Variant::Filled, cx)
        .background
        .to_hsla();
    let mut months = Vec::with_capacity(12);
    for month in 1..=12u32 {
        let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
            continue;
        };
        let cells = month_cells(first, first_weekday);
        // Header row plus six week rows, so short months do not jump.
        let mut grid = div()
            .grid()
            .grid_cols(if iso { 8 } else { 7 })
            .content_start()
            .min_h(px(cell + 6.0 * (cell + 6.0)));
        if iso {
            grid = grid.child(div());
        }
        for date in week_dates(first, first_weekday) {
            grid = grid.child(
                div()
                    .h(px(cell))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(Size::Xs.font_size()))
                    .text_color(dimmed)
                    .child(
                        locale
                            .short_weekday(date.weekday())
                            .chars()
                            .next()
                            .map_or(SharedString::default(), |initial| {
                                SharedString::from(initial.to_string())
                            }),
                    ),
            );
        }
        let mut first_in_month = true;
        for (position, day) in cells.iter().enumerate() {
            let row_has_month = cells
                .get(position..(position + 7).min(cells.len()))
                .is_some_and(|row| row.iter().any(|cell| cell.in_month));
            if iso && position % 7 == 0 && row_has_month {
                grid = grid.child(
                    div()
                        .col_start(1)
                        .h(px(cell + 6.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(Size::Xs.font_size() - 2.0))
                        .text_color(dimmed)
                        .child(format!("W{}", row_iso_week(day.date))),
                );
            }
            if !day.in_month {
                // Unrelated-month cells are blank: no element at all; the first
                // date is placed with a column start instead.
                continue;
            }
            let column_start = (position % 7) as i16 + 1 + i16::from(iso);
            let place_first = std::mem::take(&mut first_in_month);
            let count = index.count(day.date);
            let date = day.date;
            // Bounded intensity steps; the marker row disambiguates neighbours.
            let (variant, markers) = match count {
                0 => (None, ""),
                1..=2 => (Some(Variant::Light), "•"),
                3..=5 => (Some(Variant::Light), "••"),
                _ => (Some(Variant::Filled), "•••"),
            };
            let tint = variant.map(|variant| resolved_color(ColorName::Blue, variant, cx));
            let entity = state.clone();
            let is_today = today == date;
            let (cell_id, cell_label) = current.labels.date("year-day", date, || {
                format!("{}, {}", locale.date_label(date), locale.count_label(count))
            });
            let number = div()
                .size(px(cell))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .text_size(px(Size::Xs.font_size()))
                .when_some(tint.filter(|_| !is_today), |number, tint| {
                    number
                        .bg(tint.background.to_hsla())
                        .text_color(tint.foreground.to_hsla())
                })
                .when(is_today, |number| {
                    number
                        .bg(today_colors.background.to_hsla())
                        .text_color(today_colors.foreground.to_hsla())
                        .font_weight(gpui_kit::FontWeight::BOLD)
                })
                .when(selected == date && !is_today, |number| {
                    number
                        .border_1()
                        .border_color(today_colors.background.to_hsla())
                })
                .when(frame.focused && cursor == date, |number| {
                    number.border_2().border_color(frame.ring)
                })
                .child(day_label(date.day()));
            grid = grid.child(
                div()
                    .when(place_first, |cell| cell.col_start(column_start))
                    .id(ElementId::Name(cell_id))
                    .test_support()
                    .role(Role::GridCell)
                    .aria_label(cell_label)
                    .aria_selected(selected == date)
                    .h(px(cell + 6.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .cursor_pointer()
                    .child(number)
                    .when(count > 0, |cell| {
                        cell.child(
                            div()
                                .h(px(6.0))
                                .text_size(px(8.0))
                                .line_height(px(6.0))
                                .text_color(marker_color)
                                .child(markers),
                        )
                    })
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |state, cx| {
                            state.select_date(date, cx);
                            state.set_view(CalendarViewKind::Day, cx);
                        })
                    }),
            );
        }
        let entity = state.clone();
        months.push(
            div()
                .id(("year-month-block", month as usize))
                .role(Role::Group)
                .aria_label(format!("{} {year}", locale.month(month)))
                .flex()
                .flex_col()
                .gap(px(2.))
                .p(px(Size::Xs.spacing() / 2.0))
                .child(
                    Button::new(("year-month", month as usize))
                        .ghost()
                        .compact()
                        .self_start()
                        .tab_stop(frame.tab)
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .label(locale.month(month).to_string())
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |state, cx| {
                                state.select_date(first, cx);
                                state.set_view(CalendarViewKind::Month, cx);
                            })
                        }),
                )
                .child(grid)
                .into_any_element(),
        );
    }
    div()
        .id("calendar-year-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(
            div()
                .grid()
                .grid_cols(4)
                .gap(px(Size::Sm.spacing()))
                .children(months),
        )
        .into_any_element()
}

fn month_view(frame: &Frame<'_>, renderer: Option<&ItemRenderer>, cx: &mut App) -> AnyElement {
    let state = frame.state;
    let current = state.read(cx);
    let index = current.index.clone();
    let first_weekday = current.first_weekday();
    let today = current.zone.local(current.now).date_naive();
    let selected = current.selected_date;
    let cursor = current.cursor.date;
    let iso = current.show_iso_weeks;
    let locale = &current.locale;
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    let grid_color = surfaces.border.to_hsla();
    let today_colors = theme.variant(None, Variant::Filled);
    let columns = if iso { 8 } else { 7 };
    let mut header = div().grid().grid_cols(columns);
    if iso {
        header = header.child(div());
    }
    for date in week_dates(current.anchor, first_weekday) {
        header = header.child(
            div()
                .flex()
                .justify_center()
                .py(px(4.))
                .text_size(px(Size::Xs.font_size()))
                .text_color(surfaces.dimmed.to_hsla())
                .child(SharedString::new_static(
                    locale.short_weekday(date.weekday()),
                )),
        );
    }
    let (cell_height, number_height, chip_height, gap, pad) = month_geometry();
    let mut grid = div()
        .grid()
        .grid_cols(columns)
        .border_t_1()
        .border_l_1()
        .border_color(grid_color);
    for (position, cell) in month_cells(current.anchor, first_weekday)
        .into_iter()
        .enumerate()
    {
        let (row, column) = (position / 7, position % 7);
        if iso && column == 0 {
            grid = grid.child(
                div()
                    .h(px(cell_height))
                    .pt(px(pad + 4.0))
                    .flex()
                    .justify_center()
                    .border_r_1()
                    .border_b_1()
                    .border_color(grid_color)
                    .text_size(px(Size::Xs.font_size() - 1.0))
                    .text_color(surfaces.dimmed.to_hsla())
                    .child(format!("W{}", row_iso_week(cell.date))),
            );
        }
        let date = cell.date;
        let items = index.day(date);
        let (shown, remaining) = month_chip_plan(
            items.len(),
            cell_height - pad * 2.0,
            number_height + gap,
            chip_height + gap,
            0.0,
        );
        let entity = state.clone();
        let is_today = today == date;
        let (cell_id, cell_label) = current.labels.date("month-cell", date, || {
            format!(
                "{}, {}",
                locale.date_label(date),
                locale.count_label(items.len())
            )
        });
        let mut column_el = div()
            .id(ElementId::Name(cell_id))
            .test_support()
            .debug_selector(move || format!("calendar-month-cell-{date}"))
            .role(Role::GridCell)
            .aria_row_index(row + 1)
            .aria_column_index(column + 1)
            .aria_selected(selected == date)
            .aria_label(cell_label)
            .relative()
            .h(px(cell_height))
            .min_w_0()
            .overflow_hidden()
            .p(px(pad))
            .flex()
            .flex_col()
            .gap(px(gap))
            .border_r_1()
            .border_b_1()
            .border_color(grid_color)
            .bg(if cell.in_month {
                surfaces.surface.to_hsla()
            } else {
                surfaces.body.to_hsla()
            })
            .when(frame.focused && cursor == date, |cell| {
                cell.border_2().border_color(frame.ring)
            })
            .on_mouse_down(MouseButton::Left, {
                let state = entity.clone();
                move |_, _, cx| state.update(cx, |state, cx| state.begin_all_day_create(date, cx))
            })
            .on_mouse_move({
                let state = entity.clone();
                move |_: &MouseMoveEvent, _, cx| {
                    state.update(cx, |state, cx| state.update_day_drag(date, cx))
                }
            });
        let number = div()
            .id(("month-day", position))
            .size(px(number_height))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .text_size(px(Size::Xs.font_size()))
            .text_color(if cell.in_month {
                surfaces.text.to_hsla()
            } else {
                surfaces.dimmed.to_hsla()
            })
            .when(is_today, |number| {
                number
                    .bg(today_colors.background.to_hsla())
                    .text_color(today_colors.foreground.to_hsla())
                    .font_weight(gpui_kit::FontWeight::BOLD)
            })
            .when(!is_today && selected == date, |number| {
                number
                    .border_1()
                    .border_color(today_colors.background.to_hsla())
            })
            .child(day_label(date.day()))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click({
                let state = entity.clone();
                move |_, _, cx| state.update(cx, |state, cx| state.select_date(date, cx))
            });
        let mut top = div()
            .h(px(number_height))
            .flex()
            .items_center()
            .justify_between();
        top = if remaining > 0 {
            top.child(overflow_affordance(frame, current, date, remaining))
        } else {
            top.child(div())
        };
        column_el = column_el.child(top.child(number));
        for item in items.iter().take(shown) {
            column_el = column_el.child(month_chip(frame, item, current, date, renderer, cx));
        }
        if let Some(drag) = current.drag.as_ref()
            && let ItemTime::AllDay {
                start,
                end_exclusive,
            } = drag.proposed
            && start <= date
            && date < end_exclusive
        {
            column_el = column_el.child(
                div()
                    .absolute()
                    .bottom(px(pad))
                    .left(px(pad))
                    .right(px(pad))
                    .child(all_day_preview(
                        if drag.mode == DragMode::Create {
                            locale.labels.new_event
                        } else {
                            locale.labels.edit_preview
                        },
                        cx,
                    )),
            );
        }
        grid = grid.child(column_el);
    }
    // The six fixed rows never spill out of a short host: the grid scrolls.
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(header)
        .child(
            div()
                .id("calendar-month-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(grid),
        )
        .into_any_element()
}

/// Month cell geometry: (cell height, day-number size, chip height, gap, padding).
fn month_geometry() -> (f32, f32, f32, f32, f32) {
    (
        Size::Xl.control_height() * 2.0,
        Size::Xs.control_height() - 6.0,
        month_chip_height(),
        2.0,
        4.0,
    )
}

fn month_chip(
    frame: &Frame<'_>,
    item: &Arc<CalendarItem>,
    current: &CalendarState,
    date: NaiveDate,
    renderer: Option<&ItemRenderer>,
    cx: &App,
) -> AnyElement {
    let state = frame.state;
    let id = item.id;
    let format = current.time_format();
    let title = SharedString::from(item.title.to_string());
    let time = SharedString::from(match &item.time {
        ItemTime::Timed { start, .. } => format_local_time(current.zone.local(*start), format),
        ItemTime::AllDay { .. } => current.locale.labels.all_day.to_string(),
    });
    let color = item_color(current, item);
    let timed = matches!(item.time, ItemTime::Timed { .. });
    let content = ItemContent {
        item,
        color,
        title: title.clone(),
        time: time.clone(),
        icon: item
            .icon
            .as_ref()
            .map(|icon| SharedString::from(icon.to_string())),
        compact: true,
    };
    let colors = resolved_color(color, Variant::Light, cx);
    let filled = resolved_color(color, Variant::Filled, cx);
    let content = renderer
        .and_then(|renderer| renderer(&content))
        .unwrap_or_else(|| {
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(4.))
                .overflow_hidden()
                .whitespace_nowrap()
                .when(timed, |line| {
                    line.child(
                        div()
                            .flex_none()
                            .size(px(6.))
                            .rounded_full()
                            .bg(filled.background.to_hsla()),
                    )
                    .child(div().flex_none().opacity(0.8).child(time.clone()))
                })
                .when_some(content.icon.clone(), |line, icon| line.child(icon))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                        .child(title.clone()),
                )
                .into_any_element()
        });
    let selected = current.selected_item == Some(id);
    let editable = !item.locked && current.editable;
    let mut label = format!("{title}, {time}");
    if item.locked {
        label.push_str(", ");
        label.push_str(current.locale.labels.locked);
    }
    let entity = state.clone();
    let chip = div()
        .id(ElementId::Name(format!("month-event-{}", id.0).into()))
        .test_support()
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(selected)
        .w_full()
        .h(px(month_chip_height()))
        .px(px(4.))
        .flex()
        .items_center()
        .overflow_hidden()
        .rounded(px(Size::Sm.radius()))
        .text_size(px(Size::Xs.font_size()))
        .when(!timed, |chip| {
            chip.bg(colors.background.to_hsla())
                .text_color(colors.foreground.to_hsla())
        })
        .when(selected, |chip| chip.border_2().border_color(frame.ring))
        .when(editable, |chip| chip.cursor_grab())
        .hover(|style| style.opacity(0.85))
        .child(content)
        .on_mouse_down(MouseButton::Left, {
            let entity = state.clone();
            move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                if editable {
                    entity.update(cx, |state, cx| {
                        state.begin_item_drag(id, DragMode::Move, date, 0, cx)
                    });
                }
            }
        })
        .on_click(move |_, _, cx| entity.update(cx, |state, cx| state.select_item(id, cx)));
    let mut wrapper = div().relative().w_full().min_w_0().child(chip);
    if editable && !timed {
        wrapper = wrapper
            .child(day_resize_handle(state, id, ResizeEdge::Start, date))
            .child(day_resize_handle(state, id, ResizeEdge::End, date));
    }
    wrapper.into_any_element()
}

fn overflow_overlay(frame: &Frame<'_>, date: NaiveDate, cx: &App) -> AnyElement {
    let state = frame.state;
    let current = state.read(cx);
    let labels = current.locale.labels;
    let format = current.time_format();
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    let items: Vec<AnyElement> = current
        .index
        .day(date)
        .iter()
        .enumerate()
        .map(|(position, item)| {
            let id = item.id;
            let time = match &item.time {
                ItemTime::Timed { start, end } => {
                    format_time_range(current.zone.local(*start), current.zone.local(*end), format)
                }
                ItemTime::AllDay { .. } => labels.all_day.to_string(),
            };
            let color = item_color(current, item);
            let highlighted = position == current.overflow_cursor;
            let entity = state.clone();
            div()
                .id(("calendar-overflow-item", id.0 as usize))
                .test_support()
                .role(Role::Button)
                .aria_label(format!("{}, {time}", item.title))
                .aria_selected(current.selected_item == Some(id))
                .aria_position_in_set(position + 1)
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(6.))
                .py(px(3.))
                .rounded(px(Size::Sm.radius()))
                .cursor_pointer()
                .when(highlighted, |row| {
                    row.bg(resolved_color(ColorName::Blue, Variant::Light, cx)
                        .background
                        .to_hsla())
                })
                .hover(|style| style.opacity(0.85))
                .child(
                    div()
                        .flex_none()
                        .size(px(8.))
                        .rounded_full()
                        .bg(resolved_color(color, Variant::Filled, cx)
                            .background
                            .to_hsla()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(Size::Xs.font_size()))
                        .text_color(surfaces.dimmed.to_hsla())
                        .child(time),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(Size::Sm.font_size()))
                        .child(item.title.to_string()),
                )
                .on_click(move |_, window, cx| {
                    entity.update(cx, |state, cx| {
                        state.set_overflow(None, cx);
                        state.select_item(id, cx);
                    });
                    let focus = entity.read(cx).focus.clone();
                    if entity.read(cx).focusable != Some(false) {
                        window.focus(&focus, cx);
                    }
                })
                .into_any_element()
        })
        .collect();
    let count = items.len();
    let close = state.clone();
    div()
        .id("calendar-overflow")
        .test_support()
        .debug_selector(|| "calendar-overflow".into())
        .role(Role::Dialog)
        .aria_label(format!(
            "{} {}",
            labels.events_for,
            current.locale.date_label(date)
        ))
        .aria_size_of_set(count)
        .occlude()
        .absolute()
        .top(px(Size::Xl.control_height()))
        .right(px(Size::Md.spacing()))
        .w(px(300.))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(Size::Sm.spacing()))
        .flex()
        .flex_col()
        .gap(px(2.))
        .bg(surfaces.surface.to_hsla())
        .text_color(surfaces.text.to_hsla())
        .shadow_lg()
        .rounded(px(Size::Md.radius()))
        .border_1()
        .border_color(surfaces.border.to_hsla())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .pb(px(4.))
                .child(
                    div()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(current.locale.date_label(date)),
                )
                .child(
                    Button::new("close-overflow")
                        .ghost()
                        .compact()
                        .icon(IconName::Close)
                        .accessibility_label(labels.close)
                        .tab_stop(frame.tab)
                        .on_click(move |_, window, cx| {
                            close.update(cx, |state, cx| state.set_overflow(None, cx));
                            let focus = close.read(cx).focus.clone();
                            window.focus(&focus, cx);
                        }),
                ),
        )
        .children(items)
        .into_any_element()
}

fn item_color(state: &CalendarState, item: &CalendarItem) -> ColorName {
    item.color
        .or_else(|| state.model.source(item.source).map(|source| source.color))
        .unwrap_or_default()
}

fn resolved_color(color: ColorName, variant: Variant, cx: &App) -> guiraffe_core::StateColors {
    GuiraffeTheme::global(cx).state(StateKind::Rest, variant, Some(color))
}

fn bind_actions<E: InteractiveElement>(root: E, state: &Entity<CalendarState>) -> E {
    fn on<A: gpui_kit::Action, E: InteractiveElement>(
        root: E,
        state: &Entity<CalendarState>,
        run: fn(&mut CalendarState, &mut gpui_kit::Context<CalendarState>),
    ) -> E {
        let state = state.clone();
        root.on_action(move |_: &A, _, cx| state.update(cx, run))
    }
    let root = on::<CalendarToday, _>(root, state, |s, cx| s.cursor_to_today(cx));
    let root = on::<CalendarLeft, _>(root, state, |s, cx| s.arrow(-1, 0, false, cx));
    let root = on::<CalendarRight, _>(root, state, |s, cx| s.arrow(1, 0, false, cx));
    let root = on::<CalendarUp, _>(root, state, |s, cx| s.arrow(0, -1, false, cx));
    let root = on::<CalendarDown, _>(root, state, |s, cx| s.arrow(0, 1, false, cx));
    let root = on::<CalendarShiftLeft, _>(root, state, |s, cx| s.arrow(-1, 0, true, cx));
    let root = on::<CalendarShiftRight, _>(root, state, |s, cx| s.arrow(1, 0, true, cx));
    let root = on::<CalendarShiftUp, _>(root, state, |s, cx| s.arrow(0, -1, true, cx));
    let root = on::<CalendarShiftDown, _>(root, state, |s, cx| s.arrow(0, 1, true, cx));
    let root = on::<CalendarPageUp, _>(root, state, |s, cx| s.navigate(-1, cx));
    let root = on::<CalendarPageDown, _>(root, state, |s, cx| s.navigate(1, cx));
    let root = on::<CalendarActivate, _>(root, state, |s, cx| s.activate(cx));
    let root = on::<CalendarPickUp, _>(root, state, |s, cx| {
        s.pick_up(cx);
    });
    on::<CalendarCancel, _>(root, state, |s, cx| s.cancel(cx))
}
