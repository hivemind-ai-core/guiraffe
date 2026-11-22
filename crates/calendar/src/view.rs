use std::{rc::Rc, sync::Arc};

use chrono::{Datelike, NaiveDate, Timelike};
use gpui_kit::{
    AnyElement, App, ElementId, Entity, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, TestSupportExt, Window,
    base::StyledExt,
    canvas,
    component::{
        Selectable,
        button::{Button, ButtonGroup, ButtonVariants},
    },
    div,
    prelude::*,
    px, relative,
};
use guiraffe_components::{Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StateKind, Variant};

use crate::state::{
    CalendarActivate, CalendarCancel, CalendarDown, CalendarLeft, CalendarNext, CalendarPageDown,
    CalendarPageUp, CalendarPrevious, CalendarResizeEnd, CalendarResizeStart, CalendarRight,
    CalendarToday, CalendarUp, KEY_CONTEXT,
};
use crate::{
    CalendarCursor, CalendarEvent, CalendarItem, CalendarState, CalendarViewKind, DragMode,
    EventId, ItemTime, ResizeEdge, TimedSlice, format_hour, format_local_time, layout_overlaps,
    month_cells, seconds_in_display_day, time_seconds_in_display_day, week_dates,
};

pub type ItemRenderer = Rc<dyn Fn(&CalendarItem, SharedString, SharedString) -> Option<AnyElement>>;

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

    pub fn month_chip_renderer(mut self, renderer: Option<ItemRenderer>) -> Self {
        self.month_chip_renderer = renderer;
        self
    }

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

impl RenderOnce for Calendar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        let view = state.view;
        let overflow = state.overflow;
        let focus = state.focus.clone();
        let focusable = guiraffe_core::tab_stop(state.focusable);
        let ring_on = guiraffe_core::ring_visible(&focus, window);
        let ring_color = GuiraffeTheme::global(cx)
            .state(StateKind::FocusRing, Variant::Default, None)
            .border;
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        let mut root = div()
            .id("guiraffe-calendar")
            .test_support()
            .debug_selector(|| "calendar".into())
            .role(Role::Grid)
            .track_focus(&focus.tab_stop(focusable))
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
        root = root
            .on_action(action_previous(self.state.clone()))
            .on_action(action_next(self.state.clone()))
            .on_action(action_today(self.state.clone()));
        root = root
            .on_action(action_left(self.state.clone()))
            .on_action(action_right(self.state.clone()))
            .on_action(action_up(self.state.clone()))
            .on_action(action_down(self.state.clone()))
            .on_action(action_page_up(self.state.clone()))
            .on_action(action_page_down(self.state.clone()))
            .on_action(action_activate(self.state.clone()))
            .on_action(action_pick_up(self.state.clone()))
            .on_action(action_cancel(self.state.clone()))
            .on_action(action_resize_start(self.state.clone()))
            .on_action(action_resize_end(self.state.clone()))
            .on_mouse_up(MouseButton::Left, {
                let state = self.state.clone();
                move |_, _, cx| {
                    state.update(cx, |state, cx| {
                        state.commit_drag(cx);
                    })
                }
            });
        if self.props.show_toolbar {
            root = root.child(toolbar(&self.state, cx));
        }
        let content = match view {
            CalendarViewKind::Year => year_view(&self.state, cx),
            CalendarViewKind::Month => {
                month_view(&self.state, self.month_chip_renderer.as_ref(), cx)
            }
            CalendarViewKind::Week => {
                timed_view(&self.state, false, self.event_block_renderer.as_ref(), cx)
            }
            CalendarViewKind::Day => {
                timed_view(&self.state, true, self.event_block_renderer.as_ref(), cx)
            }
        };
        root.child(content)
            .when_some(overflow, |root, date| {
                root.child(overflow_overlay(&self.state, date, cx))
            })
            .child(pointer_capture(&self.state))
            .map(|root| guiraffe_core::focus_ring(root, ring_on, px(Size::Md.radius()), ring_color))
            .map(|root| StyledExt::refine_style(root, &self.style))
    }
}

fn toolbar(state: &Entity<CalendarState>, cx: &mut App) -> impl IntoElement {
    let current = state.read(cx);
    let anchor = current.anchor;
    let kind = current.view;
    let locale = current.locale.clone();
    let sources = current.model.sources().to_vec();
    let heading = match kind {
        CalendarViewKind::Year => format!("{}", anchor.year()),
        CalendarViewKind::Month => format!("{} {}", locale.month(anchor.month()), anchor.year()),
        CalendarViewKind::Week => week_dates(anchor, current.first_weekday())
            .first()
            .map(|first| {
                format!(
                    "{} {}, {}",
                    locale.month(first.month()),
                    first.day(),
                    first.year()
                )
            })
            .unwrap_or_default(),
        CalendarViewKind::Day => format!(
            "{} {}, {}",
            locale.month(anchor.month()),
            anchor.day(),
            anchor.year()
        ),
    };
    let navigation = ButtonGroup::new("calendar-navigation")
        .outline()
        .child(navigation_button(state, "calendar-previous", "‹", -1))
        .child(today_button(state))
        .child(navigation_button(state, "calendar-next", "›", 1));
    let mut bar = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(Size::Sm.spacing()))
        .child(navigation)
        .child(div().flex_1().child(Title::new(heading).order(2)))
        .child(current.view_switcher.clone());
    for source in sources {
        let entity = state.clone();
        let id = source.id;
        let visible = source.visible;
        let colors = resolved_color(
            source.color,
            if visible {
                Variant::Light
            } else {
                Variant::Subtle
            },
            cx,
        );
        bar = bar.child(
            Button::new(("calendar-source", id.0 as usize))
                .compact()
                .rounded_full()
                .label(source.name.to_string())
                .selected(visible)
                .bg(colors.background.to_hsla())
                .text_color(colors.foreground.to_hsla())
                .on_click(move |_, _, cx| {
                    entity.update(cx, |state, cx| state.set_source_visible(id, !visible, cx))
                }),
        );
    }
    bar
}

fn navigation_button(
    state: &Entity<CalendarState>,
    id: &'static str,
    label: &'static str,
    delta: i32,
) -> Button {
    let state = state.clone();
    Button::new(id)
        .compact()
        .accessibility_label(if delta < 0 { "Previous" } else { "Next" })
        .label(label)
        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.navigate(delta, cx)))
}

fn today_button(state: &Entity<CalendarState>) -> Button {
    let state = state.clone();
    Button::new("calendar-today")
        .compact()
        .label("Today")
        .on_click(move |_, _, cx| state.update(cx, |state, cx| state.today(cx)))
}

fn normalized_minute_position(
    self_position: f32,
    origin: f32,
    origin_minute: i32,
    hour_height: f32,
) -> i32 {
    origin_minute + (((self_position - origin) / hour_height) * 60.0).round() as i32
}

fn drag_origin_minute(drag: &crate::DragPreview) -> i32 {
    drag.origin_minute
}

fn event_at_position(events: &MouseMoveEvent, origin_minute: i32, origin_y: f32) -> i32 {
    normalized_minute_position(
        events.position.y.into(),
        origin_y,
        origin_minute,
        default_hour_height(),
    )
}

fn timed_view(
    state: &Entity<CalendarState>,
    one_day: bool,
    renderer: Option<&ItemRenderer>,
    cx: &mut App,
) -> AnyElement {
    let current = state.read(cx);
    let zone = current.zone;
    let dates = if one_day {
        vec![current.anchor]
    } else {
        week_dates(current.anchor, current.first_weekday())
    };
    let locale = current.locale.clone();
    let index = current.index.clone();
    let hour_height = default_hour_height();
    let scroll_minutes = current.scroll_minutes;
    let now = current.show_now.then(|| zone.local(current.now));
    let now_date = now.map(|time| time.date_naive());
    let now_minute = now.map(|time| (time.hour() * 60 + time.minute()) as i32);
    let business = current.business_hours.clone();
    let show_business = current.show_business_hours;
    let format = current.time_format();
    let weekdays = dates
        .iter()
        .map(|date| {
            let is_now = now_date == Some(*date);
            let weekday: String = locale.weekday(date.weekday()).chars().take(3).collect();
            let label = format!("{weekday} {}", date.day());
            let content = if is_now {
                let colors = GuiraffeTheme::global(cx).variant(None, Variant::Filled);
                div()
                    .rounded_full()
                    .px(px(Size::Sm.spacing()))
                    .py(px(Size::Xs.spacing()))
                    .bg(colors.background.to_hsla())
                    .text_color(colors.foreground.to_hsla())
                    .child(label)
                    .into_any_element()
            } else {
                Text::new(label).into_any_element()
            };
            div()
                .flex_1()
                .flex()
                .justify_center()
                .child(content)
                .into_any_element()
        })
        .collect::<Vec<_>>();
    let mut columns = Vec::with_capacity(dates.len());
    let mut all_day_lanes = Vec::with_capacity(dates.len());
    for date in dates {
        let day_events = index.day(date).to_vec();
        let all_day_items: Vec<Arc<CalendarItem>> = day_events
            .iter()
            .filter(|item| matches!(item.time, ItemTime::AllDay { .. }))
            .cloned()
            .collect();
        let mut sequences: Vec<TimedSlice> = day_events
            .iter()
            .filter_map(|item| {
                let (start, end) = seconds_in_display_day(item, date, zone)?;
                (end > scroll_minutes * 60 && start < (scroll_minutes + 600) * 60).then_some(
                    TimedSlice {
                        id: item.id,
                        start_second: start,
                        end_second: end,
                    },
                )
            })
            .collect();
        let positions = layout_overlaps(&mut sequences);
        let mut day = div()
            .flex_1()
            .relative()
            .h(px(1_440.0 / 60.0 * hour_height))
            .border_l_1()
            .border_color(current_grid_color(cx));
        if show_business {
            for hour in visible_hours(scroll_minutes) {
                let local = date.and_hms_opt(hour, 0, 0);
                if local.is_none_or(|local| zone.single_from_local(local).is_none()) {
                    continue;
                }
                let naive = local.map(|time| time.time());
                if naive.is_some_and(|time| !business.is_working(date.weekday(), time)) {
                    day = day.child(
                        div()
                            .absolute()
                            .top(px(hour as f32 * hour_height))
                            .left_0()
                            .w_full()
                            .h(px(hour_height))
                            .bg(GuiraffeTheme::global(cx).surfaces().track.to_hsla()),
                    );
                }
            }
        }
        for hour in visible_hours(scroll_minutes) {
            if date
                .and_hms_opt(hour, 0, 0)
                .is_none_or(|local| zone.single_from_local(local).is_none())
            {
                continue;
            }
            day = day.child(
                div()
                    .absolute()
                    .top(px(hour as f32 * hour_height))
                    .left_0()
                    .w_full()
                    .h(px(hour_height))
                    .border_t_1()
                    .border_color(current_grid_color(cx))
                    .child(
                        div()
                            .id(ElementId::Name(format!("empty-hour-{date}-{hour}").into()))
                            .test_support()
                            .debug_selector(|| format!("calendar-hour-{date}-{hour}"))
                            .size_full()
                            .on_mouse_down(MouseButton::Left, {
                                let state = state.clone();
                                move |event: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    let minute = (hour * 60) as i32;
                                    state.update(cx, |state, cx| {
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
                                    let origin_y =
                                        current.pointer_origin_y.unwrap_or(event.position.y.into());
                                    let origin = drag_origin_minute(drag);
                                    let minute = event_at_position(event, origin, origin_y);
                                    state.update(cx, |state, cx| {
                                        state.update_timed_drag(
                                            date,
                                            minute.clamp(-1_440, 2_880),
                                            cx,
                                        )
                                    });
                                }
                            }),
                    ),
            );
            day = day.child(
                div()
                    .absolute()
                    .top(px(hour as f32 * hour_height + hour_height / 2.0))
                    .left_0()
                    .w_full()
                    .border_t_1()
                    .border_color(current_grid_color(cx)),
            );
        }
        for position in positions.iter().take(64) {
            let Some(item) = day_events.iter().find(|item| item.id == position.id) else {
                continue;
            };
            let height = ((position.end_second - position.start_second) as f32 / 60.0).max(15.0);
            let full_title = SharedString::from(item.title.to_string());
            let title = if one_day {
                full_title.clone()
            } else {
                let max_chars = (12 / position.lanes.max(1)).max(2);
                SharedString::from(clip_label(&item.title, max_chars))
            };
            let compact = height < Size::Xl.control_height() || (!one_day && position.lanes > 1);
            let local_start = match item.time {
                ItemTime::Timed { start, .. } => Some(zone.local(start)),
                _ => None,
            };
            let time_text = local_start
                .map(|start| SharedString::from(format_local_time(start, format)))
                .unwrap_or_else(|| SharedString::from("All day"));
            let content = renderer
                .and_then(|renderer| renderer(item, title.clone(), time_text.clone()))
                .unwrap_or_else(|| {
                    default_item_content(item, title.clone(), time_text.clone(), compact)
                });
            let color = item_color(current, item);
            let entity = state.clone();
            let id = item.id;
            let locked = item.locked || !current.editable;
            let origin_minute = position.start_second / 60;
            let mut block = div()
                .id(("timed-event", id.0 as usize))
                .test_support()
                .role(Role::Button)
                .aria_label(full_title.clone())
                .aria_selected(current.selected_item == Some(id))
                .absolute()
                .top(px(position.start_second as f32 / 60.0))
                .left(relative(
                    position.lane as f32 / position.lanes.max(1) as f32,
                ))
                .w(relative(1.0 / position.lanes.max(1) as f32))
                .h(px(height))
                .overflow_hidden()
                .rounded(px(Size::Sm.radius()))
                .border_1()
                .border_color(resolved_color(color, Variant::Light, cx).border.to_hsla())
                .bg(resolved_color(color, Variant::Light, cx)
                    .background
                    .to_hsla())
                .text_color(
                    resolved_color(color, Variant::Light, cx)
                        .foreground
                        .to_hsla(),
                )
                .py(px(Size::Xs.spacing()))
                .pr(px(Size::Xs.spacing()))
                .pl(px(Size::Sm.spacing()))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .h_full()
                        .w(px(guiraffe_core::RING_WIDTH))
                        .bg(resolved_color(color, Variant::Filled, cx)
                            .background
                            .to_hsla()),
                )
                .child(content);
            block = block.on_click({
                let state = entity.clone();
                move |_, _, cx| state.update(cx, |state, cx| state.select_item(id, cx))
            });
            if !locked {
                block = block
                    .cursor_grab()
                    .on_mouse_down(MouseButton::Left, {
                        let state = entity.clone();
                        move |event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            state.update(cx, |state, cx| {
                                state.pointer_origin_y = Some(event.position.y.into());
                                state.begin_item_drag(id, DragMode::Move, date, origin_minute, cx);
                            });
                        }
                    })
                    .on_mouse_move({
                        let state = entity.clone();
                        move |event: &MouseMoveEvent, _, cx| {
                            let current = state.read(cx);
                            let origin_y =
                                current.pointer_origin_y.unwrap_or(event.position.y.into());
                            let minute = event_at_position(event, origin_minute, origin_y);
                            state.update(cx, |state, cx| state.update_timed_drag(date, minute, cx));
                        }
                    });
                block = block
                    .child(resize_handle(
                        &entity,
                        id,
                        ResizeEdge::Start,
                        date,
                        origin_minute,
                        true,
                    ))
                    .child(resize_handle(
                        &entity,
                        id,
                        ResizeEdge::End,
                        date,
                        position.end_second / 60,
                        false,
                    ));
            }
            day = day.child(block);
        }
        if let Some((start, end)) = current
            .drag
            .as_ref()
            .and_then(|drag| time_seconds_in_display_day(&drag.proposed, date, zone))
            && end > scroll_minutes * 60
            && start < (scroll_minutes + 600) * 60
        {
            let preview_color = resolved_color(ColorName::Blue, Variant::Light, cx);
            let label = if current
                .drag
                .as_ref()
                .is_some_and(|drag| drag.mode == DragMode::Create)
            {
                "New event"
            } else {
                "Edit preview"
            };
            day = day.child(
                div()
                    .id("calendar-drag-preview")
                    .absolute()
                    .left_0()
                    .w_full()
                    .top(px(start as f32 / 60.0))
                    .h(px(((end - start) as f32 / 60.0).max(15.0)))
                    .rounded(px(Size::Sm.radius()))
                    .border_1()
                    .border_color(preview_color.border.to_hsla())
                    .bg(preview_color.background.to_hsla())
                    .text_color(preview_color.foreground.to_hsla())
                    .child(label),
            );
        }
        let hidden_events = positions.len().saturating_sub(64);
        if hidden_events > 0 {
            let entity = state.clone();
            day = day.child(
                Button::new(ElementId::Name(format!("overlap-overflow-{date}").into()))
                    .label(format!("+{hidden_events} overlapping events"))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |state, cx| state.set_overflow(Some(date), cx))
                    }),
            );
        }
        if now_date == Some(date)
            && let Some(minute) = now_minute
        {
            day = day.child(
                div()
                    .id("calendar-now-line")
                    .absolute()
                    .left_0()
                    .w_full()
                    .top(px(minute as f32))
                    .h(px(2.))
                    .bg(resolved_color(ColorName::Red, Variant::Filled, cx)
                        .background
                        .to_hsla())
                    .child(
                        div()
                            .id("now-marker")
                            .w(px(6.))
                            .h(px(6.))
                            .rounded_full()
                            .bg(resolved_color(ColorName::Red, Variant::Filled, cx)
                                .background
                                .to_hsla()),
                    ),
            );
        }
        let mut all_day_lane = div()
            .h(px(Size::Md.control_height()))
            .p(px(Size::Xs.spacing()))
            .flex()
            .items_center()
            .gap(px(Size::Xs.spacing()))
            .border_b_1()
            .border_color(current_grid_color(cx))
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
        let all_day_capacity = if one_day { 3 } else { 1 };
        for item in all_day_items.iter().take(all_day_capacity) {
            all_day_lane = all_day_lane.child(month_chip(state, item, current, date, None, cx));
        }
        if let Some(ItemTime::AllDay {
            start,
            end_exclusive,
        }) = current.drag.as_ref().map(|drag| &drag.proposed)
            && *start <= date
            && date < *end_exclusive
        {
            all_day_lane = all_day_lane.child(Text::new("All-day edit preview"));
        }
        let remaining = all_day_items.len().saturating_sub(all_day_capacity);
        if remaining > 0 {
            let entity = state.clone();
            all_day_lane = all_day_lane.child(
                Button::new(ElementId::Name(format!("all-day-overflow-{date}").into()))
                    .compact()
                    .ghost()
                    .label(format!("+{remaining} more"))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |state, cx| state.set_overflow(Some(date), cx))
                    }),
            );
        }
        all_day_lanes.push(div().flex_1().child(all_day_lane).into_any_element());
        columns.push(div().flex_1().child(day).into_any_element());
    }
    let gutter_width = Size::Xl.control_height() + Size::Lg.spacing();
    let mut time_gutter = div()
        .w(px(gutter_width))
        .flex_none()
        .relative()
        .h(px(1_440.0 / 60.0 * hour_height));
    for hour in visible_hours(scroll_minutes) {
        time_gutter = time_gutter.child(
            div()
                .absolute()
                .top(px(hour as f32 * hour_height - Size::Xs.font_size() / 2.0))
                .right_0()
                .pr(px(Size::Sm.spacing()))
                .child(
                    Text::new(format_hour(hour, format))
                        .text_size(px(Size::Xs.font_size()))
                        .dimmed(true),
                ),
        );
    }
    let scroll = state.read(cx).scroll.clone();
    div()
        .size_full()
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
                .child(
                    div()
                        .w(px(gutter_width))
                        .flex_none()
                        .h(px(Size::Md.control_height()))
                        .flex()
                        .items_center()
                        .justify_end()
                        .pr(px(Size::Sm.spacing()))
                        .border_b_1()
                        .border_color(current_grid_color(cx))
                        .child(
                            Text::new("all-day")
                                .text_size(px(Size::Xs.font_size()))
                                .dimmed(true),
                        ),
                )
                .children(all_day_lanes),
        )
        .child(
            div()
                .id("calendar-hour-scroll")
                .flex_1()
                .min_h_0()
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
                if !matches!(drag.proposed, ItemTime::Timed { .. }) {
                    return;
                }
                let origin_minute = drag.origin_minute;
                let date = drag.origin_date;
                let origin_y = current.pointer_origin_y.unwrap_or(event.position.y.into());
                let minute = event_at_position(event, origin_minute, origin_y);
                movement.update(cx, |state, cx| state.update_timed_drag(date, minute, cx));
            });
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == gpui_kit::DispatchPhase::Capture {
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
    top: bool,
) -> AnyElement {
    let down = state.clone();
    let movement = state.clone();
    let side = if top { "start" } else { "end" };
    div()
        .id(ElementId::Name(
            format!("calendar-resize-{side}-{}-{date}", id.0).into(),
        ))
        .test_support()
        .role(Role::Button)
        .aria_label(format!("Resize event {side}"))
        .absolute()
        .left_0()
        .w_full()
        .h(px(Size::Xs.spacing()))
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
            let origin_y = movement
                .read(cx)
                .pointer_origin_y
                .unwrap_or(event.position.y.into());
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
    start: bool,
) -> AnyElement {
    let down = state.clone();
    let movement = state.clone();
    let side = if start { "start" } else { "end" };
    div()
        .id(ElementId::Name(
            format!("calendar-day-resize-{side}-{}-{date}", id.0).into(),
        ))
        .test_support()
        .role(Role::Button)
        .aria_label(format!("Resize all-day event {side}"))
        .absolute()
        .top_0()
        .h_full()
        .w(px(Size::Xs.spacing()))
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

fn default_item_content(
    item: &CalendarItem,
    title: SharedString,
    time: SharedString,
    compact: bool,
) -> AnyElement {
    let mut content = div().size_full().flex().flex_col().overflow_hidden().child(
        div()
            .w_full()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .font_weight(gpui_kit::FontWeight::MEDIUM)
            .child(title),
    );
    if matches!(item.time, ItemTime::Timed { .. }) && !compact {
        content = content.child(
            div()
                .w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(Size::Xs.font_size()))
                .child(time),
        );
    }
    if let Some(icon) = item.icon.as_ref() {
        content =
            content.child(Text::new(format!("Icon: {icon}")).text_size(px(Size::Xs.font_size())));
    }
    content.into_any_element()
}

fn visible_hours(scroll_minutes: i32) -> std::ops::RangeInclusive<u32> {
    let first = (scroll_minutes.max(0) / 60) as u32;
    let last = ((scroll_minutes.max(0) + 600) / 60).min(23) as u32;
    first..=last
}

pub fn default_hour_height() -> f32 {
    60.0
}

fn current_border_color(cx: &App) -> gpui_kit::Hsla {
    GuiraffeTheme::global(cx)
        .state(StateKind::Rest, Variant::Outline, Some(ColorName::Gray))
        .border
        .to_hsla()
}

fn current_grid_color(cx: &App) -> gpui_kit::Hsla {
    GuiraffeTheme::global(cx).surfaces().border.to_hsla()
}

fn year_view(state: &Entity<CalendarState>, cx: &mut App) -> AnyElement {
    let current = state.read(cx);
    let index = current.index.clone();
    let locale = current.locale.clone();
    let first_weekday = current.first_weekday();
    let today = current.zone.local(current.now).date_naive();
    let selected = current.selected_date;
    let iso = current.show_iso_weeks;
    let mut months = Vec::with_capacity(12);
    for month in 1..=12 {
        let Some(anchor) = current.anchor.with_month(month) else {
            continue;
        };
        let cells = month_cells(anchor, first_weekday);
        let mut grid = div()
            .grid()
            .grid_cols(if iso { 8 } else { 7 })
            .gap(px(Size::Xs.spacing()));
        if iso {
            grid = grid.child(div());
        }
        for weekday in week_dates(anchor, first_weekday) {
            let initial = locale
                .weekday(weekday.weekday())
                .chars()
                .next()
                .unwrap_or(' ')
                .to_string();
            grid = grid.child(
                div()
                    .h(px(Size::Sm.control_height()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Text::new(initial)
                            .text_size(px(Size::Xs.font_size()))
                            .dimmed(true),
                    ),
            );
        }
        let cell_height = Size::Sm.control_height() + Size::Xs.font_size() + Size::Xs.spacing();
        for day in &cells {
            if iso && day.date.weekday() == first_weekday {
                grid = grid.child(
                    Text::new(format!("W{}", day.iso_week))
                        .text_size(px(Size::Xs.font_size()))
                        .dimmed(true),
                );
            }
            if !day.in_month {
                grid = grid.child(div().h(px(cell_height)));
                continue;
            }
            let count = index.count(day.date);
            let date = day.date;
            let entity = state.clone();
            let marker = if count == 0 { " " } else { "•" };
            let intensity = if count >= 3 {
                Variant::Filled
            } else {
                Variant::Light
            };
            let tint_colors = resolved_color(ColorName::Blue, intensity, cx);
            let tint = tint_colors.background.to_hsla();
            let tint_text = tint_colors.foreground.to_hsla();
            let today_colors = GuiraffeTheme::global(cx).variant(None, Variant::Filled);
            let button = Button::new(ElementId::Name(format!("year-day-{date}").into()))
                .ghost()
                .compact()
                .rounded_full()
                .w(px(Size::Sm.control_height()))
                .h(px(Size::Sm.control_height()))
                .self_center()
                .accessibility_label(format!("{date}, {count} events"))
                .label(date.day().to_string())
                .selected(selected == date)
                .when(today == date, |button| {
                    button
                        .bg(today_colors.background.to_hsla())
                        .text_color(today_colors.foreground.to_hsla())
                })
                .when(today != date && selected == date, Button::outline)
                .when(today != date && count > 0, |button| {
                    button.bg(tint).text_color(tint_text)
                })
                .on_click(move |_, _, cx| {
                    entity.update(cx, |state, cx| {
                        state.select_date(date, cx);
                        state.set_view(CalendarViewKind::Day, cx);
                    })
                });
            grid = grid.child(
                div()
                    .h(px(cell_height))
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(button)
                    .child(Text::new(marker).text_size(px(Size::Xs.font_size()))),
            );
        }
        let entity = state.clone();
        let title = locale.month(month);
        months.push(
            div()
                .flex()
                .flex_col()
                .gap(px(Size::Xs.spacing()))
                .child(
                    Button::new(("year-month", month as usize))
                        .ghost()
                        .compact()
                        .self_start()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .label(title)
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |state, cx| {
                                if let Some(date) = state.anchor.with_month(month) {
                                    state.select_date(date, cx);
                                }
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
                .gap(px(Size::Md.spacing()))
                .children(months),
        )
        .into_any_element()
}

fn month_view(
    state: &Entity<CalendarState>,
    renderer: Option<&ItemRenderer>,
    cx: &mut App,
) -> AnyElement {
    let current = state.read(cx);
    let index = current.index.clone();
    let first_weekday = current.first_weekday();
    let today = current.zone.local(current.now).date_naive();
    let selected = current.selected_date;
    let iso = current.show_iso_weeks;
    let mut header = div().grid().grid_cols(if iso { 8 } else { 7 });
    if iso {
        header = header.child(div());
    }
    for date in week_dates(current.anchor, first_weekday) {
        let weekday: String = current
            .locale
            .weekday(date.weekday())
            .chars()
            .take(3)
            .collect();
        header = header.child(
            div()
                .flex()
                .justify_center()
                .py(px(Size::Xs.spacing()))
                .child(
                    Text::new(weekday)
                        .text_size(px(Size::Xs.font_size()))
                        .dimmed(true),
                ),
        );
    }
    let mut grid = div().grid().grid_cols(if iso { 8 } else { 7 });
    for cell in month_cells(current.anchor, first_weekday) {
        if iso && cell.date.weekday() == first_weekday {
            grid = grid.child(
                Text::new(format!("W{}", cell.iso_week))
                    .text_size(px(Size::Xs.font_size()))
                    .dimmed(true),
            );
        }
        let date = cell.date;
        let items = index.day(date);
        let entity = state.clone();
        let mut column = div()
            .relative()
            .h(px(Size::Xl.control_height() * 2.0))
            .overflow_hidden()
            .p(px(Size::Xs.spacing()))
            .flex()
            .flex_col()
            .gap(px(Size::Xs.spacing()))
            .border_1()
            .border_color(current_grid_color(cx))
            .bg(if cell.in_month {
                GuiraffeTheme::global(cx).surfaces().surface.to_hsla()
            } else {
                GuiraffeTheme::global(cx).surfaces().body.to_hsla()
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
        let today_colors = GuiraffeTheme::global(cx).variant(None, Variant::Filled);
        let number_color = if cell.in_month {
            GuiraffeTheme::global(cx).surfaces().text.to_hsla()
        } else {
            GuiraffeTheme::global(cx).surfaces().dimmed.to_hsla()
        };
        let day_number = Button::new(ElementId::Name(format!("month-day-{date}").into()))
            .ghost()
            .compact()
            .rounded_full()
            .w(px(Size::Md.control_height()))
            .h(px(Size::Md.control_height()))
            .text_color(number_color)
            .label(date.day().to_string())
            .selected(selected == date)
            .when(today == date, |button| {
                button
                    .bg(today_colors.background.to_hsla())
                    .text_color(today_colors.foreground.to_hsla())
            })
            .when(today != date && selected == date, Button::outline)
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click({
                let state = entity.clone();
                move |_, _, cx| state.update(cx, |state, cx| state.select_date(date, cx))
            });
        column = column.child(div().w_full().flex().justify_end().child(day_number));
        let chip_capacity = 1;
        for item in items.iter().take(chip_capacity) {
            column = column.child(month_chip(&entity, item, current, date, renderer, cx));
        }
        if let Some(ItemTime::AllDay {
            start,
            end_exclusive,
        }) = current.drag.as_ref().map(|drag| &drag.proposed)
            && *start <= date
            && date < *end_exclusive
        {
            column = column.child(Text::new("All-day edit preview"));
        }
        let remaining = items.len().saturating_sub(chip_capacity);
        if remaining > 0 {
            column = column.child(
                Button::new(ElementId::Name(format!("month-overflow-{date}").into()))
                    .absolute()
                    .top(px(Size::Xs.spacing()))
                    .left(px(Size::Xs.spacing()))
                    .compact()
                    .ghost()
                    .self_start()
                    .label(format!("+{remaining} more"))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click({
                        let state = entity.clone();
                        move |_, _, cx| {
                            state.update(cx, |state, cx| state.set_overflow(Some(date), cx))
                        }
                    }),
            );
        }
        grid = grid.child(column);
    }
    div()
        .flex()
        .flex_col()
        .child(header)
        .child(grid)
        .into_any_element()
}

fn month_chip(
    state: &Entity<CalendarState>,
    item: &Arc<CalendarItem>,
    current: &CalendarState,
    date: NaiveDate,
    renderer: Option<&ItemRenderer>,
    cx: &App,
) -> AnyElement {
    let id = item.id;
    let title = SharedString::from(item.title.to_string());
    let time = SharedString::from(match &item.time {
        ItemTime::Timed { start, .. } => {
            format_local_time(current.zone.local(*start), current.time_format())
        }
        ItemTime::AllDay { .. } => "All day".into(),
    });
    let content = renderer
        .and_then(|renderer| renderer(item, title.clone(), time.clone()))
        .unwrap_or_else(|| {
            let label = match &item.time {
                ItemTime::Timed { .. } => format!("{time} {title}"),
                ItemTime::AllDay { .. } => title.to_string(),
            };
            div()
                .w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label)
                .into_any_element()
        });
    let entity = state.clone();
    let colors = resolved_color(item_color(current, item), Variant::Light, cx);
    let mut chip = Button::new(("month-event", id.0 as usize))
        .compact()
        .rounded_full()
        .w_full()
        .justify_start()
        .accessibility_label(title)
        .selected(current.selected_item == Some(id))
        .bg(colors.background.to_hsla())
        .text_color(colors.foreground.to_hsla())
        .child(content)
        .on_click(move |_, _, cx| entity.update(cx, |state, cx| state.select_item(id, cx)));
    let editable = !item.locked && current.editable;
    chip = chip.on_mouse_down(MouseButton::Left, {
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
    });
    let mut wrapper = div().relative().child(chip);
    if editable {
        wrapper = wrapper
            .child(day_resize_handle(state, id, ResizeEdge::Start, date, true))
            .child(day_resize_handle(state, id, ResizeEdge::End, date, false));
    }
    wrapper.into_any_element()
}

fn overflow_overlay(state: &Entity<CalendarState>, date: NaiveDate, cx: &App) -> AnyElement {
    let close = state.clone();
    let items: Vec<AnyElement> = state
        .read(cx)
        .index
        .day(date)
        .iter()
        .map(|item| Text::new(item.title.to_string()).into_any_element())
        .collect();
    div()
        .id("calendar-overflow")
        .role(Role::Dialog)
        .aria_label(format!("Events for {date}"))
        .absolute()
        .top(px(Size::Xl.control_height()))
        .right(px(Size::Md.spacing()))
        .max_h(px(360.))
        .overflow_y_scroll()
        .p(px(Size::Md.spacing()))
        .flex()
        .flex_col()
        .gap(px(Size::Sm.spacing()))
        .bg(GuiraffeTheme::global(cx).surfaces().surface.to_hsla())
        .text_color(GuiraffeTheme::global(cx).surfaces().text.to_hsla())
        .shadow_lg()
        .rounded(px(Size::Md.radius()))
        .border_1()
        .border_color(current_border_color(cx))
        .child(Text::new(format!("Events for {date}")))
        .children(items)
        .child(
            Button::new("close-overflow")
                .label("Close")
                .on_click(move |_, _, cx| {
                    close.update(cx, |state, cx| state.set_overflow(None, cx))
                }),
        )
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

fn action_previous(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarPrevious, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.navigate(-1, cx))
}
fn action_next(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarNext, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.navigate(1, cx))
}
fn action_today(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarToday, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.today(cx))
}
fn action_left(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarLeft, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.move_cursor(-1, 0, cx))
}
fn action_right(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarRight, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.move_cursor(1, 0, cx))
}
fn action_up(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarUp, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        state.update(cx, |state, cx| {
            if state.keyboard_edit.is_some() {
                state.keyboard_move_selected(-15, None, cx);
            } else {
                state.move_cursor(0, -1, cx);
            }
        })
    }
}
fn action_down(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarDown, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        state.update(cx, |state, cx| {
            if state.keyboard_edit.is_some() {
                state.keyboard_move_selected(15, None, cx);
            } else {
                state.move_cursor(0, 1, cx);
            }
        })
    }
}
fn action_page_up(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarPageUp, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.navigate(-1, cx))
}
fn action_page_down(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarPageDown, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.navigate(1, cx))
}
fn action_activate(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarActivate, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        state.update(cx, |state, cx| {
            if state.drag.is_some() {
                state.commit_drag(cx);
            } else {
                let date = state.cursor.date;
                state.select_date(date, cx);
            }
        })
    }
}
fn action_pick_up(
    state: Entity<CalendarState>,
) -> impl Fn(&crate::state::CalendarPickUp, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        state.update(cx, |state, cx| {
            if state.keyboard_edit.take().is_some() {
                state.keyboard_edit = None;
                cx.emit(CalendarEvent::Announcement(
                    "Keyboard edit completed".into(),
                ));
                cx.notify();
            } else if let Some(id) = state.selected_item {
                state.keyboard_edit = Some((id, DragMode::Move));
                cx.notify();
            }
        })
    }
}
fn action_cancel(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarCancel, &mut Window, &mut App) + 'static {
    move |_, _, cx| state.update(cx, |state, cx| state.cancel(cx))
}
fn action_resize_start(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarResizeStart, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        state.update(cx, |state, cx| {
            state.keyboard_move_selected(-15, Some(ResizeEdge::Start), cx);
        })
    }
}
fn action_resize_end(
    state: Entity<CalendarState>,
) -> impl Fn(&CalendarResizeEnd, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        state.update(cx, |state, cx| {
            state.keyboard_move_selected(15, Some(ResizeEdge::End), cx);
        })
    }
}
