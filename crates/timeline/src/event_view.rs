//! Virtualized timeline control for parallel event histories.
//!
//! ```
//! use guiraffe_timeline::{EventModel, EventTimeline, EventTimelineState};
//! use gpui_kit::{App, AppContext, IntoElement};
//!
//! fn timeline(cx: &mut App) -> Result<impl IntoElement, guiraffe_timeline::EventModelError> {
//!     let model = EventModel::new(vec![], vec![], vec![])?;
//!     let state = cx.new(|cx| EventTimelineState::new(model, cx));
//!     Ok(EventTimeline::new(&state))
//! }
//! ```

use std::{collections::HashSet, rc::Rc};

use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyBinding, ParentElement, PathBuilder, Pixels, Point, Render, RenderOnce, Role, ScrollEvent,
    SharedString, StyleRefinement, Styled, TestSupportExt, Window, canvas, div, point, prelude::*,
    px,
};
use guiraffe_core::{
    ColorName, GuiraffeTheme, Press, PressColors, PressableExt, Size, Surfaces, Variant,
    focus_ring, ring_visible,
};

use crate::event_layout::{
    EventModel, LinkKind, Placement, TimelineEvent, clip_segment, visible_events, visible_range,
};
use crate::measure::{self, BodyCell};

/// Direction of chronology. Text stays upright in both orientations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventOrientation {
    #[default]
    Vertical,
    Horizontal,
}

impl EventOrientation {
    fn xy<T>(self, time: T, lanes: T) -> (T, T) {
        match self {
            Self::Vertical => (lanes, time),
            Self::Horizontal => (time, lanes),
        }
    }
}

/// User actions report the original stable event ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventTimelineEvent {
    Selected(u64),
    Activated(u64),
}

gpui_kit::actions!(
    event_timeline,
    [Left, Right, Up, Down, First, Last, Activate]
);
const KEY_CONTEXT: &str = "GrEventTimeline";

pub(crate) fn init(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("home", First, context),
        KeyBinding::new("end", Last, context),
        KeyBinding::new("enter", Activate, context),
        KeyBinding::new("space", Activate, context),
    ]);
}

type Content = Rc<dyn Fn(&TimelineEvent, &mut Window, &mut App) -> AnyElement>;

/// Owns the model, stable cursor, selection and viewport.
pub struct EventTimelineState {
    model: Rc<EventModel>,
    orientation: EventOrientation,
    color: Option<ColorName>,
    size: Option<Size>,
    label: SharedString,
    focusable: Option<bool>,
    focus: FocusHandle,
    item_focus: FocusHandle,
    selected: Option<u64>,
    cursor: Option<Placement>,
    offset: (f64, f64),
    pitch: (f64, f64),
    header: f64,
    body: BodyCell,
    content: Option<Content>,
    theme_cache: Option<Rc<TimelineTheme>>,
    style: StyleRefinement,
}

impl EventEmitter<EventTimelineEvent> for EventTimelineState {}

impl Focusable for EventTimelineState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Styled for EventTimelineState {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl EventTimelineState {
    /// Construct with theme defaults and pointer selection enabled.
    pub fn new(model: EventModel, cx: &mut Context<Self>) -> Self {
        let cursor = model.lanes.iter().find_map(|lane| lane.first().copied());
        Self {
            model: Rc::new(model),
            orientation: EventOrientation::Vertical,
            color: None,
            size: None,
            label: "Event timeline".into(),
            focusable: None,
            focus: cx.focus_handle(),
            item_focus: cx.focus_handle(),
            selected: None,
            cursor,
            offset: (0.0, 0.0),
            pitch: (1.0, 1.0),
            header: 0.0,
            body: BodyCell::default(),
            content: None,
            theme_cache: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn orientation(mut self, orientation: EventOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = Some(size);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = Some(focusable);
        self
    }

    /// Render custom content below the label in the fixed event slot. The region clips overflow.
    pub fn item_content(
        mut self,
        content: impl Fn(&TimelineEvent, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(content));
        self
    }

    /// Display another validated model, keeping cursor and selection by identity.
    pub fn replace(&mut self, model: EventModel, cx: &mut Context<Self>) {
        self.cursor = self
            .cursor
            .and_then(|current| {
                let id = self.model.events.get(current.event)?.id;
                model
                    .locate(id)
                    .or_else(|| nearest(model.lanes.get(current.track)?, current.slot))
            })
            .or_else(|| model.lanes.iter().find_map(|lane| lane.first().copied()));
        self.selected = self.selected.filter(|id| model.locate(*id).is_some());
        self.model = Rc::new(model);
        self.clamp();
        cx.notify();
    }

    /// Current selection by stable ID.
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    fn viewport(&self) -> (f64, f64) {
        let bounds = self.body.get();
        let (time, cross) = self.orientation.xy(
            f64::from(f32::from(bounds.size.width)),
            f64::from(f32::from(bounds.size.height)),
        );
        ((time - self.header).max(0.0), cross)
    }

    fn clamp(&mut self) {
        let (time, cross) = self.viewport();
        self.offset.0 = self.offset.0.clamp(
            0.0,
            (self.pitch.0 * self.model.slots as f64 - time).max(0.0),
        );
        self.offset.1 = self.offset.1.clamp(
            0.0,
            (self.pitch.1 * self.model.tracks.len() as f64 - cross).max(0.0),
        );
    }

    fn select(&mut self, placement: Placement, cx: &mut Context<Self>) {
        self.cursor = Some(placement);
        if let Some(item) = self.model.events.get(placement.event)
            && self.selected != Some(item.id)
        {
            self.selected = Some(item.id);
            cx.emit(EventTimelineEvent::Selected(item.id));
            cx.notify();
        }
    }

    fn activate(&mut self, cx: &mut Context<Self>) {
        if let Some(cursor) = self.cursor {
            self.select(cursor, cx);
            if let Some(item) = self.model.events.get(cursor.event) {
                cx.emit(EventTimelineEvent::Activated(item.id));
            }
        }
    }

    fn move_cursor(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let Some(current) = self.cursor else { return };
        let (time, cross) = self.viewport();
        let Some(next) = direction.target(current, &self.model, self.orientation) else {
            return;
        };
        self.offset.0 = reveal(
            self.offset.0,
            next.slot as f64 * self.pitch.0,
            self.pitch.0,
            time,
        );
        self.offset.1 = reveal(
            self.offset.1,
            next.track as f64 * self.pitch.1,
            self.pitch.1,
            cross,
        );
        self.select(next, cx);
        self.clamp();
        cx.notify();
    }
}

fn reveal(offset: f64, start: f64, extent: f64, viewport: f64) -> f64 {
    offset.clamp((start + extent.min(viewport) - viewport).max(0.0), start)
}

fn nearest(lane: &[Placement], slot: usize) -> Option<Placement> {
    let row = lane.partition_point(|item| item.slot < slot);
    let next = lane.get(row);
    let prev = row.checked_sub(1).and_then(|row| lane.get(row));
    prev.into_iter()
        .chain(next)
        .min_by_key(|item| item.slot.abs_diff(slot))
        .copied()
}

#[derive(Clone, Copy)]
enum Direction {
    Screen(isize, isize),
    First,
    Last,
}

impl Direction {
    fn target(
        self,
        item: Placement,
        model: &EventModel,
        orientation: EventOrientation,
    ) -> Option<Placement> {
        let lane = model.lanes.get(item.track)?;
        match self {
            Self::First => lane.first().copied(),
            Self::Last => lane.last().copied(),
            Self::Screen(x, y) => {
                let (time, cross) = orientation.xy(x, y);
                if time != 0 {
                    return lane.get(item.row.checked_add_signed(time)?).copied();
                }
                let mut index = item.track;
                while let Some(next) = index
                    .checked_add_signed(cross)
                    .filter(|index| *index < model.lanes.len())
                {
                    if let Some(found) = nearest(model.lanes.get(next)?, item.slot) {
                        return Some(found);
                    }
                    index = next;
                }
                None
            }
        }
    }
}

/// Element adapter for an event timeline state entity.
#[derive(IntoElement)]
pub struct EventTimeline(Entity<EventTimelineState>);

impl EventTimeline {
    pub fn new(state: &Entity<EventTimelineState>) -> Self {
        Self(state.clone())
    }
}

impl RenderOnce for EventTimeline {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

#[derive(Clone)]
struct Projection {
    orientation: EventOrientation,
    pitch: (f64, f64),
    offset: (f64, f64),
    inset: f64,
    header: f64,
}

impl Projection {
    fn coordinates(&self, time: f64, track: f64) -> (Pixels, Pixels) {
        self.orientation.xy(
            px((time * self.pitch.0 - self.offset.0) as f32),
            px((self.inset + track * self.pitch.1 - self.offset.1) as f32),
        )
    }

    fn position(&self, origin: Point<Pixels>, time: f64, track: f64) -> Point<Pixels> {
        let (x, y) = self.coordinates(time, track);
        origin + point(x, y)
    }
}

fn stroke(
    window: &mut Window,
    points: impl IntoIterator<Item = Point<Pixels>>,
    width: Pixels,
    color: gpui_kit::Hsla,
) {
    let mut points = points.into_iter();
    let Some(first) = points.next() else { return };
    let mut builder = PathBuilder::stroke(width);
    builder.move_to(first);
    for point in points {
        builder.line_to(point);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[derive(Clone, Copy)]
struct TrackColors {
    dot: gpui_kit::Hsla,
    rail: gpui_kit::Hsla,
    badge_background: gpui_kit::Hsla,
    badge_text: gpui_kit::Hsla,
    press: PressColors,
}

impl TrackColors {
    fn new(theme: &GuiraffeTheme, color: ColorName) -> Self {
        let filled = theme.variant(Some(color), Variant::Filled);
        let light = theme.variant(Some(color), Variant::Light);
        Self {
            dot: filled.background.to_hsla(),
            rail: light.foreground.to_hsla(),
            badge_background: light.background.to_hsla(),
            badge_text: light.foreground.to_hsla(),
            press: theme.press_colors(Variant::Subtle, Some(color)),
        }
    }
}

struct TimelineTheme {
    theme: GuiraffeTheme,
    surfaces: Surfaces,
    colors: [TrackColors; ColorName::ALL.len()],
    primary: TrackColors,
}

impl TimelineTheme {
    fn new(theme: &GuiraffeTheme) -> Self {
        let colors = ColorName::ALL.map(|color| TrackColors::new(theme, color));
        Self {
            theme: theme.clone(),
            surfaces: theme.surfaces(),
            primary: colors
                .get(theme.primary as usize)
                .copied()
                .unwrap_or_else(|| TrackColors::new(theme, theme.primary)),
            colors,
        }
    }

    fn color(&self, color: Option<ColorName>) -> TrackColors {
        color
            .and_then(|color| self.colors.get(color as usize))
            .copied()
            .unwrap_or(self.primary)
    }
}

impl Render for EventTimelineState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let resolved = match &self.theme_cache {
            Some(cached) if &cached.theme == theme => cached.clone(),
            _ => {
                let resolved = Rc::new(TimelineTheme::new(theme));
                self.theme_cache = Some(resolved.clone());
                resolved
            }
        };
        let surfaces = resolved.surfaces;
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let scale = if theme.scale.is_finite() && theme.scale > 0.0 {
            theme.scale
        } else {
            1.0
        };
        let spacing = size.spacing() * scale;
        let unit = f64::from(spacing);
        self.pitch = match self.orientation {
            EventOrientation::Vertical => (unit * 4.0, unit * 15.0),
            EventOrientation::Horizontal => (unit * 14.0, unit * 5.0),
        };
        self.header = match self.orientation {
            EventOrientation::Vertical => unit * 2.25,
            EventOrientation::Horizontal => unit * 9.6,
        };
        self.clamp();
        let projection = Projection {
            orientation: self.orientation,
            pitch: self.pitch,
            offset: self.offset,
            inset: unit * 0.75,
            header: self.header,
        };
        let (mut chronological, mut cross) = self.viewport();
        if cross == 0.0 {
            (chronological, cross) = self.orientation.xy(
                f64::from(f32::from(window.viewport_size().width)),
                f64::from(f32::from(window.viewport_size().height)),
            );
        }
        let slots = visible_range(
            self.offset.0,
            chronological + self.pitch.0,
            self.pitch.0,
            self.model.slots,
        );
        let lanes = visible_range(
            self.offset.1,
            cross + self.pitch.1,
            self.pitch.1,
            self.model.tracks.len(),
        );
        let model = self.model.clone();
        let paint_model = model.clone();
        let paint_projection = projection.clone();
        let paint_lanes = lanes.clone();
        let paint_slots = slots.clone();
        let paint_theme = resolved.clone();
        let default_color = self.color;
        let body = self.body.clone();
        let owner = cx.entity_id();
        let headers = lanes
            .clone()
            .filter_map(|track| {
                let lane = model.tracks.get(track)?;
                let lane_offset = track as f64 * projection.pitch.1 - projection.offset.1;
                let (left, top) = projection.orientation.xy(0.0, lane_offset);
                let (w, h) = projection
                    .orientation
                    .xy(projection.header, projection.pitch.1);
                let color = resolved.color(lane.color.or(default_color)).dot;
                Some(
                    div()
                        .absolute()
                        .left(px(left as f32))
                        .top(px(top as f32))
                        .w(px(w as f32))
                        .h(px(h as f32))
                        .flex()
                        .items_center()
                        .gap(px(spacing / 2.0))
                        .px(px(spacing / 2.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .size(px(spacing / 2.0))
                                .flex_none()
                                .rounded_full()
                                .bg(color),
                        )
                        .child(
                            div()
                                .truncate()
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child(lane.label.clone()),
                        ),
                )
            })
            .collect::<Vec<_>>();
        let event_owner = cx.weak_entity();
        let mut items = Vec::new();
        for track in lanes {
            let (Some(lane), Some(track_data)) = (model.lanes.get(track), model.tracks.get(track))
            else {
                continue;
            };
            for &placement in visible_events(lane, slots.clone()) {
                let Some(item) = model.events.get(placement.event) else {
                    continue;
                };
                let chosen = self.selected == Some(item.id);
                let current = self.cursor == Some(placement);
                let color = item.color.or(track_data.color).or(default_color);
                let colors = resolved.color(color);
                let (x, y) = projection.coordinates(placement.slot as f64, track as f64);
                let (w, h) = self.orientation.xy(self.pitch.0, self.pitch.1);
                let (dot_x, dot_y) = self.orientation.xy(
                    px((self.pitch.0 * crate::event_layout::DOT_OFFSET) as f32),
                    px(0.0),
                );
                let dot = px(spacing * 0.55);
                let label: SharedString = if window.is_a11y_active() {
                    let [branches, merges] = model.relations(item.id);
                    let tags = item.tags.join(", ");
                    format!(
                        "{}, {}, position {}, {} branches, {} merges. {} {}",
                        item.label,
                        track_data.label,
                        item.position,
                        branches,
                        merges,
                        tags,
                        item.content.as_deref().unwrap_or_default()
                    )
                    .into()
                } else {
                    item.label.clone().into()
                };
                let press = Press::new(colors.press)
                    .role(Role::ListBoxOption)
                    .focusable(Some(false))
                    .label(label)
                    .selected(chosen);
                let listener = event_owner.clone();
                let title = div()
                    .id(("event-timeline-item", item.id))
                    .test_support()
                    .when(current, |item| item.aria_active_descendant())
                    .aria_position_in_set(placement.row + 1)
                    .aria_size_of_set(lane.len())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(spacing / 3.0))
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .truncate()
                            .font_weight(if chosen {
                                gpui_kit::FontWeight::BOLD
                            } else {
                                gpui_kit::FontWeight::SEMIBOLD
                            })
                            .child(item.label.clone()),
                    )
                    .children(item.tags.iter().take(4).map(|tag| {
                        div()
                            .flex_none()
                            .rounded_sm()
                            .px(px(spacing / 4.0))
                            .bg(colors.badge_background)
                            .text_color(colors.badge_text)
                            .child(tag.clone())
                    }))
                    .press(&press, &self.item_focus, window)
                    .on_press(false, move |_, _, cx| {
                        let _ = listener.update(cx, |this, cx| this.select(placement, cx));
                    });
                let content = self
                    .content
                    .as_ref()
                    .map(|render| render(item, window, cx))
                    .or_else(|| {
                        item.content.as_ref().map(|text| {
                            div()
                                .truncate()
                                .text_color(surfaces.dimmed.to_hsla())
                                .child(text.clone())
                                .into_any_element()
                        })
                    });
                let card = div()
                    .absolute()
                    .flex()
                    .flex_col()
                    .gap(px(spacing / 5.0))
                    .overflow_hidden()
                    .top(match self.orientation {
                        EventOrientation::Vertical => dot_y - px(spacing * 0.75),
                        EventOrientation::Horizontal => px(spacing * 0.8),
                    })
                    .left(match self.orientation {
                        EventOrientation::Vertical => px(spacing * 0.8),
                        EventOrientation::Horizontal => px(spacing * 1.6),
                    })
                    .right(px(spacing * 1.15))
                    .bottom(px(spacing * 0.2))
                    .bg(surfaces.body.to_hsla())
                    .child(title)
                    .when_some(content, |card, content| card.child(content));
                items.push(
                    div()
                        .absolute()
                        .left(x)
                        .top(y)
                        .w(px(w as f32))
                        .h(px(h as f32))
                        .child(
                            div()
                                .absolute()
                                .left(dot_x - dot / 2.0)
                                .top(dot_y - dot / 2.0)
                                .size(dot)
                                .rounded_full()
                                .bg(colors.dot),
                        )
                        .child(card),
                );
            }
        }
        let (header_w, header_h) = self.orientation.xy(px(self.header as f32), px(0.0));
        let focused = ring_visible(&self.focus, window);
        let (left, top) = self.orientation.xy(px(self.header as f32), px(0.0));
        let mut root = div()
            .id("event-timeline")
            .test_support()
            .role(Role::ListBox)
            .aria_label(self.label.clone())
            .track_focus(
                &self
                    .focus
                    .clone()
                    .tab_stop(guiraffe_core::tab_stop(self.focusable)),
            )
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &Left, window, cx| {
                if this.focus.is_focused(window) {
                    this.move_cursor(Direction::Screen(-1, 0), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Right, window, cx| {
                if this.focus.is_focused(window) {
                    this.move_cursor(Direction::Screen(1, 0), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Up, window, cx| {
                if this.focus.is_focused(window) {
                    this.move_cursor(Direction::Screen(0, -1), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Down, window, cx| {
                if this.focus.is_focused(window) {
                    this.move_cursor(Direction::Screen(0, 1), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &First, window, cx| {
                if this.focus.is_focused(window) {
                    this.move_cursor(Direction::First, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Last, window, cx| {
                if this.focus.is_focused(window) {
                    this.move_cursor(Direction::Last, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Activate, window, cx| {
                if this.focus.is_focused(window) {
                    this.activate(cx);
                }
            }))
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(spacing));
                let (time, lane) = this.orientation.xy(f32::from(delta.x), f32::from(delta.y));
                let previous = this.offset;
                if event.modifiers.shift {
                    this.offset.1 -= f64::from(if lane == 0.0 { time } else { lane });
                } else if this.orientation == EventOrientation::Horizontal && time == 0.0 {
                    this.offset.0 -= f64::from(lane);
                } else {
                    this.offset.0 -= f64::from(time);
                    this.offset.1 -= f64::from(lane);
                }
                this.clamp();
                if this.offset != previous {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .text_size(px(size.font_size() * scale))
            .line_height(px(size.font_size() * scale * 1.12))
            .child(
                canvas(
                    move |bounds, _, cx| measure::record(&body, bounds, cx, owner),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(left)
                    .top(top)
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .overflow_hidden()
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, (), window, _| {
                                let length = paint_projection
                                    .orientation
                                    .xy(bounds.size.width, bounds.size.height)
                                    .0;
                                let extent = f64::from(f32::from(length));
                                for lane in paint_lanes.clone() {
                                    let Some(track) = paint_model.tracks.get(lane) else {
                                        continue;
                                    };
                                    let color =
                                        paint_theme.color(track.color.or(default_color)).rail;
                                    let start = paint_projection.position(
                                        bounds.origin,
                                        paint_projection.offset.0 / paint_projection.pitch.0,
                                        lane as f64,
                                    );
                                    let end = paint_projection.position(
                                        bounds.origin,
                                        (paint_projection.offset.0 + extent)
                                            / paint_projection.pitch.0,
                                        lane as f64,
                                    );
                                    stroke(window, [start, end], px(spacing / 12.0), color);
                                }
                                let mut seen = HashSet::new();
                                let min = paint_projection.offset.0 / paint_projection.pitch.0;
                                let max =
                                    (paint_projection.offset.0 + extent) / paint_projection.pitch.0;
                                paint_model.links(paint_slots.clone(), |route| {
                                    let cross = route.from.track.min(route.to.track)
                                        ..=route.from.track.max(route.to.track);
                                    if *cross.end() < paint_lanes.start
                                        || *cross.start() >= paint_lanes.end
                                    {
                                        return;
                                    }
                                    let color = route.link.color.or(match route.link.kind {
                                        LinkKind::Branch => Some(ColorName::Blue),
                                        LinkKind::Merge => Some(ColorName::Teal),
                                    });
                                    let color = paint_theme.color(color).dot;
                                    let width = if route.link.kind == LinkKind::Merge {
                                        spacing / 6.0
                                    } else {
                                        spacing / 10.0
                                    };
                                    let [from, turn, exit, to] = route.points();
                                    for (start, end) in [(from, turn), (turn, exit), (exit, to)] {
                                        let Some(segment) = clip_segment(start, end, min..max)
                                        else {
                                            continue;
                                        };
                                        let key = segment
                                            .map(|(slot, track)| (slot.to_bits(), track.to_bits()));
                                        if seen.insert((key, route.link.kind, route.link.color)) {
                                            let points = segment.map(|(slot, track)| {
                                                paint_projection.position(
                                                    bounds.origin,
                                                    slot,
                                                    track,
                                                )
                                            });
                                            stroke(window, points, px(width), color);
                                        }
                                    }
                                    if (min..max).contains(&to.0)
                                        && paint_lanes.contains(&route.to.track)
                                    {
                                        let tip =
                                            paint_projection.position(bounds.origin, to.0, to.1);
                                        let direction = (to.0 - exit.0).signum() as f32;
                                        let back = px(-spacing * 0.48 * direction);
                                        let wing = px(spacing / 4.0);
                                        let (x, y) = paint_projection.orientation.xy(back, wing);
                                        let (other_x, other_y) =
                                            paint_projection.orientation.xy(back, -wing);
                                        stroke(
                                            window,
                                            [tip + point(x, y), tip, tip + point(other_x, other_y)],
                                            px(width),
                                            color,
                                        );
                                    }
                                });
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .children(items)
                    .when(model.events.is_empty(), |body| {
                        body.child(
                            div()
                                .p(px(spacing))
                                .text_color(surfaces.dimmed.to_hsla())
                                .child("No events"),
                        )
                    }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .overflow_hidden()
                    .when(self.orientation == EventOrientation::Vertical, |header| {
                        header.right(px(0.0)).h(header_h)
                    })
                    .when(self.orientation == EventOrientation::Horizontal, |header| {
                        header.bottom(px(0.0)).w(header_w)
                    })
                    .bg(surfaces.body.to_hsla())
                    .children(headers),
            );
        *root.style() = {
            let mut style = root.style().clone();
            use gpui_kit::Refineable;
            style.refine(&self.style);
            style
        };
        focus_ring(root, focused, px(0.), resolved.primary.press.ring)
    }
}
