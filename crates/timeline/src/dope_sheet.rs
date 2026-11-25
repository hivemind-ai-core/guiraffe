//! Hierarchical, frame-snapped dope sheet with viewport-culling and explicit selection.
#[path = "dope_sheet_button.rs"]
mod button;
#[path = "dope_sheet_strip.rs"]
mod strip;
use crate::plot::append_diamond;
use crate::{
    Axis, LockStyle, Playhead, PlotSurface, SelectionBox, TimeFormat, Zebra, dope_model::DopeModel,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Theme, tooltip::Tooltip};
use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Render, Role, Styled,
    TestSupportExt, Window, canvas, div, prelude::*, px,
};
use guiraffe_core::{A11yDisabledExt, ColorName, GuiraffeTheme, StateKind, Variant};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub const ROW_H: f32 = 28.;
pub const NAME_W: f32 = 230.;
pub const KEY_GRAB: f64 = 7.;
pub const RULER_H: f32 = 28.;
pub use crate::channels::{Channel, KeyShape, snap_time};
#[derive(Debug, Clone, PartialEq)]
pub enum DopeEvent {
    KeyRetimed { channel: usize, from: f64, to: f64 },
    SelectionChanged,
    Changed,
    ChannelChanged { channel: usize },
}
#[derive(Clone)]
enum Gesture {
    Keys {
        last: f64,
    },
    Box {
        start: (f64, f64),
        end: (f64, f32),
        add: bool,
        base: std::collections::BTreeSet<(usize, u64)>,
    },
}
pub struct DopeSheet {
    model: DopeModel,
    strip: Rc<RefCell<strip::DiamondStrip>>,
    surface: PlotSurface,
    format: TimeFormat,
    drag: Option<Gesture>,
    body: Rc<Cell<Bounds<Pixels>>>,
    pending_fit: Option<(f64, f64)>,
    offset: f64,
    editable: bool,
    focusable: Option<bool>,
    focus: FocusHandle,
    playhead: f64,
    lock_style: LockStyle,
    header_column: Option<Entity<HeaderColumn>>,
    header_offset: f64,
    first_header_row: usize,
    headers: Vec<ChannelHeader>,
}
impl EventEmitter<DopeEvent> for DopeSheet {}
impl Focusable for DopeSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl DopeSheet {
    pub fn new(
        channels: Vec<Channel>,
        format: TimeFormat,
        length: f64,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self {
            strip: {
                let strip = Rc::new(RefCell::new(strip::DiamondStrip::default()));
                let owned = strip.clone();
                cx.on_release(move |_, cx| owned.borrow_mut().release(cx))
                    .detach();
                strip
            },
            model: DopeModel::new(channels),
            surface: PlotSurface::timeline(Axis::new(0., length.max(1e-3), 800.), format),
            format,
            drag: None,
            body: Rc::default(),
            pending_fit: Some((0., length.max(1e-3))),
            offset: 0.,
            editable: true,
            focusable: None,
            focus: cx.focus_handle(),
            playhead: 0.,
            lock_style: LockStyle::Badge,
            header_column: None,
            header_offset: 0.,
            first_header_row: 0,
            headers: Vec::new(),
        })
    }
    pub fn channels(&self) -> &[Channel] {
        &self.model.channels
    }
    pub fn axis(&self) -> &Axis {
        &self.surface.axis
    }
    pub fn model(&self) -> &DopeModel {
        &self.model
    }
    pub fn add_layer(&mut self, layer: impl crate::Layer, cx: &mut Context<Self>) {
        self.surface.add_layer(layer);
        cx.notify();
    }
    pub fn set_focusable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.focusable = Some(on);
        cx.notify();
    }
    pub fn set_editable(&mut self, on: bool, cx: &mut Context<Self>) {
        self.editable = on;
        self.drag = None;
        cx.notify();
    }
    pub fn set_locked(&mut self, channel: usize, locked: bool, cx: &mut Context<Self>) {
        if let Some(c) = self.model.channels.get_mut(channel) {
            c.locked = locked;
            self.drag = None;
            cx.notify();
        }
    }
    pub fn set_lock_style(&mut self, style: LockStyle, cx: &mut Context<Self>) {
        self.lock_style = style;
        cx.notify();
    }
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.model.select_all();
        cx.emit(DopeEvent::SelectionChanged);
        cx.notify();
    }
    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if self.editable && self.model.delete_selected() {
            cx.emit(DopeEvent::Changed);
            cx.notify();
        }
    }
    pub fn nudge(&mut self, frames: f64, cx: &mut Context<Self>) {
        if self.editable
            && self
                .model
                .translate(frames * self.frame_step(), self.format)
        {
            cx.emit(DopeEvent::Changed);
            cx.notify();
        }
    }
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        if self.editable && self.model.duplicate(self.frame_step(), self.format) {
            cx.emit(DopeEvent::Changed);
            cx.notify();
        }
    }
    fn frame_step(&self) -> f64 {
        match self.format {
            TimeFormat::Frames(fps) if fps > 0. => 1. / fps,
            _ => 1.,
        }
    }
    pub fn press(&mut self, row: usize, x: f64) -> bool {
        self.press_with(row, x, false)
    }
    pub fn press_with(&mut self, row: usize, x: f64, add: bool) -> bool {
        self.press_key(row, x, add).is_some()
    }

    // On a hit, report whether the selection needs repainting. Starting a
    // drag on an already selected key changes only the pending gesture.
    fn press_key(&mut self, row: usize, x: f64, add: bool) -> Option<bool> {
        let channel = self.model.rows().get(row).map(|r| r.channel).unwrap_or(row);
        let nearest = self
            .model
            .channels
            .get(channel)
            .filter(|c| c.visible)
            .and_then(|c| {
                let keys = if c.group {
                    self.model.summary_keys(channel)
                } else {
                    c.keys()
                };
                let axis = self.surface.axis;
                let index = keys.partition_point(|time| axis.x(*time) < x);
                [
                    index.checked_sub(1).and_then(|i| keys.get(i)),
                    keys.get(index),
                ]
                .into_iter()
                .flatten()
                .copied()
                .map(|t| (t, (axis.x(t) - x).abs()))
                .filter(|(_, distance)| *distance <= KEY_GRAB)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            });
        if let Some((t, _)) = nearest {
            let changed = add || !self.model.selected.contains(&(channel, t.to_bits()));
            if changed {
                if self.model.channels[channel].group {
                    self.model.select_summary(channel, t, add);
                } else {
                    self.model.select(channel, t, add);
                }
            }
            self.drag = (self.editable && self.model.editable(channel)).then_some(Gesture::Keys {
                last: self.surface.axis.t(x),
            });
            Some(changed)
        } else {
            None
        }
    }
    pub fn drag_to(&mut self, x: f64, cx: &mut Context<Self>) {
        let Some(Gesture::Keys { last }) = self.drag.clone() else {
            return;
        };
        if !self.editable {
            return;
        }
        let delta = snap_time(self.surface.axis.t(x) - last, self.format);
        let before = self.model.selected.clone();
        if self.model.translate(delta, self.format) {
            self.drag = Some(Gesture::Keys { last: last + delta });
            for (channel, t) in before {
                if self.model.editable(channel) {
                    let from = f64::from_bits(t);
                    cx.emit(DopeEvent::KeyRetimed {
                        channel,
                        from,
                        to: snap_time(from + delta, self.format),
                    });
                }
            }
            cx.notify();
        }
    }
    pub fn release(&mut self) {
        self.drag = None;
    }
    fn local(&self, p: gpui_kit::Point<Pixels>) -> (f64, f32) {
        let b = self.body.get();
        (
            f64::from(f32::from(p.x - b.origin.x)),
            f32::from(p.y - b.origin.y),
        )
    }
    fn toggle(&mut self, channel: usize, kind: ChannelControl, cx: &mut Context<Self>) {
        if let Some(c) = self.model.channels.get_mut(channel) {
            match kind {
                ChannelControl::Expand => c.collapsed = !c.collapsed,
                ChannelControl::Mute => c.muted = !c.muted,
                ChannelControl::Lock => c.locked = !c.locked,
                ChannelControl::Visibility => c.visible = !c.visible,
            }
            self.model.rebuild_rows();
            self.drag = None;
            cx.emit(DopeEvent::ChannelChanged { channel });
            cx.notify();
        }
    }
}
#[derive(Clone, PartialEq)]
struct ChannelHeader {
    channel: usize,
    depth: usize,
    name: gpui_kit::SharedString,
    color: ColorName,
    group: bool,
    collapsed: bool,
    muted: bool,
    locked: bool,
    visible: bool,
    owner: gpui_kit::WeakEntity<DopeSheet>,
}

#[derive(Clone, Copy)]
enum ChannelControl {
    Expand,
    Mute,
    Lock,
    Visibility,
}

impl ChannelHeader {
    fn control(&self, kind: ChannelControl, theme: &Theme) -> impl IntoElement + use<> {
        let (id, label, icon, selected, disabled) = match kind {
            ChannelControl::Expand => (
                "channel-expand",
                "Expand or collapse channel",
                if !self.group {
                    IconName::Minus
                } else if self.collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                },
                false,
                !self.group,
            ),
            ChannelControl::Mute => (
                "channel-mute",
                "Mute channel",
                if self.muted {
                    IconName::VolumeX
                } else {
                    IconName::Volume2
                },
                self.muted,
                false,
            ),
            ChannelControl::Lock => (
                "channel-lock",
                "Lock channel",
                if self.locked {
                    IconName::Lock
                } else {
                    IconName::LockOpen
                },
                self.locked,
                false,
            ),
            ChannelControl::Visibility => (
                "channel-visible",
                "Channel visibility",
                if self.visible {
                    IconName::Eye
                } else {
                    IconName::EyeOff
                },
                false,
                false,
            ),
        };
        let owner = self.owner.clone();
        let channel = self.channel;
        let foreground = if disabled {
            theme.muted_foreground.opacity(0.5)
        } else {
            theme.secondary_foreground
        };
        let hover = theme
            .tokens
            .accent
            .opacity(if theme.mode.is_dark() { 0.5 } else { 1. });
        let hover_foreground = theme.accent_foreground;
        let active = theme.tokens.button_active;
        let radius = theme.radius;
        let inner = div()
            .debug_selector(move || format!("{id}-{channel}"))
            .id((id, channel))
            .role(Role::Button)
            .aria_description(label)
            .aria_selected(selected)
            .size_5()
            .flex_shrink_0()
            .when(selected, |el| {
                el.bg(theme.tokens.secondary_active).rounded(radius)
            })
            .when(!disabled && !selected, |el| {
                el.hover(move |style| style.bg(hover).rounded(radius))
                    .active(move |style| style.bg(active).rounded(radius))
            })
            .when(!disabled, |el| {
                el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |_, _, cx| {
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |view, cx| view.toggle(channel, kind, cx));
                        }
                    })
            })
            .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            .a11y_disabled(disabled);
        button::ChannelButton {
            inner,
            icon: icon.path(),
            foreground,
            hover_foreground: (!disabled && !selected).then_some(hover_foreground),
        }
    }
}

struct HeaderColumn {
    rows: Vec<ChannelHeader>,
    top: f32,
}

impl Render for HeaderColumn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .children(self.rows.iter().enumerate().map(|(index, header)| {
                header
                    .element(cx)
                    .absolute()
                    .top(px(self.top + index as f32 * ROW_H))
                    .w(px(NAME_W))
                    .h(px(ROW_H))
            }))
    }
}

impl ChannelHeader {
    fn element(&self, cx: &App) -> gpui_kit::Div {
        let theme = GuiraffeTheme::global(cx);
        let control_theme = cx.theme();
        div()
            .size_full()
            .flex()
            .items_center()
            .pl(px(self.depth as f32 * 12.))
            .child(self.control(ChannelControl::Expand, control_theme))
            .child(
                div()
                    .w(px(4.))
                    .h(px(12.))
                    .rounded(px(2.))
                    .mr(px(4.))
                    .bg(theme
                        .variant(Some(self.color), Variant::Filled)
                        .background
                        .to_hsla()),
            )
            .child(div().flex_1().overflow_hidden().child(self.name.clone()))
            .child(self.control(ChannelControl::Mute, control_theme))
            .child(self.control(ChannelControl::Lock, control_theme))
            .child(self.control(ChannelControl::Visibility, control_theme))
    }
}

fn key_indices(
    times: &[f64],
    first: usize,
    end: usize,
    spacing: Option<f64>,
) -> impl Iterator<Item = usize> + Clone + '_ {
    std::iter::successors((first < end).then_some(first), move |&index| {
        let next = index + 1;
        // Preserve the source sequence for picking and editing. Only the
        // rendered marks skip diamonds that overlap at this zoom.
        let next = if let Some(spacing) = spacing
            && next < end
            && times[next] < times[index] + spacing
        {
            next + times[next..end].partition_point(|&t| t < times[index] + spacing)
        } else {
            next
        };
        (next < end).then_some(next)
    })
}

#[derive(Clone, Copy)]
struct KeyMark {
    x: f64,
    y: f64,
    selected: bool,
    shape: KeyShape,
    hold: Option<f64>,
    muted: bool,
    summary: bool,
    locked: bool,
}

fn viewport_rows(
    model: &DopeModel,
    axis: Axis,
    height: f64,
    offset: f64,
) -> impl Iterator<Item = KeyRow<'_>> + Clone + '_ {
    let start = ((offset / f64::from(ROW_H)).floor() as usize).min(model.rows().len());
    let end = (((offset + height) / f64::from(ROW_H)).ceil() as usize).min(model.rows().len());
    model.rows()[start..end]
        .iter()
        .enumerate()
        .filter(|(_, row)| model.channels[row.channel].visible)
        .map(move |(row_index, row)| {
            let channel = row.channel;
            let c = &model.channels[channel];
            let times = if row.summary {
                model.summary_keys(channel)
            } else {
                c.keys()
            };
            let first = times.partition_point(|t| *t < axis.t(-KEY_GRAB));
            let end = times.partition_point(|t| *t <= axis.t(axis.width + KEY_GRAB));
            let has_selection = model
                .selected
                .range((channel, 0)..=(channel, u64::MAX))
                .next()
                .is_some();
            // Check spacing where the keys actually fall. A tight cluster can
            // cover a few pixels even when most of the viewport is empty.
            let compact = !row.summary && c.decor.is_none() && !has_selection;
            let locked = !model.editable(channel);
            let y = (start + row_index) as f64 * f64::from(ROW_H) - offset;
            KeyRow {
                model,
                channel,
                axis,
                times: &times[first..end],
                first,
                summary: row.summary,
                compact,
                locked,
                muted: c.muted,
                y,
            }
        })
}

#[derive(Clone, Copy)]
struct KeyRow<'a> {
    model: &'a DopeModel,
    channel: usize,
    axis: Axis,
    times: &'a [f64],
    first: usize,
    summary: bool,
    compact: bool,
    locked: bool,
    muted: bool,
    y: f64,
}

impl<'a> KeyRow<'a> {
    fn indices(self) -> impl Iterator<Item = usize> + Clone + 'a {
        key_indices(
            self.times,
            0,
            self.times.len(),
            self.compact.then_some(4. * self.axis.scale),
        )
    }

    fn marks(self) -> impl Iterator<Item = KeyMark> + Clone + 'a {
        self.indices().map(move |index| {
            let time = self.times[index];
            let selected = if self.compact {
                false
            } else if self.summary {
                self.model.summary_selected(self.channel, time)
            } else {
                self.model
                    .selected
                    .contains(&(self.channel, time.to_bits()))
            };
            let (shape, hold) = if self.compact {
                (KeyShape::Diamond, None)
            } else {
                self.model
                    .key_display(self.channel, self.summary, self.first + index)
            };
            KeyMark {
                x: self.axis.x(time),
                y: self.y,
                selected,
                shape,
                hold: hold.map(|t| self.axis.x(t)),
                muted: self.muted,
                summary: self.summary,
                locked: self.locked,
            }
        })
    }
}

#[cfg(test)]
fn viewport_keys(
    model: &DopeModel,
    axis: Axis,
    height: f64,
    offset: f64,
) -> impl Iterator<Item = KeyMark> + '_ {
    viewport_rows(model, axis, height, offset).flat_map(KeyRow::marks)
}

impl Render for DopeSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity();
        gpui_kit::container_query(move |size, window, cx| {
            editor.update(cx, |editor, cx| editor.render_at_size(size, window, cx))
        })
    }
}

impl DopeSheet {
    fn render_at_size(
        &mut self,
        size: gpui_kit::Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let theme = GuiraffeTheme::global(cx).clone();
        let s = theme.surfaces();
        // The outer layout fixes the rail and ruler dimensions. Use the actual
        // allocation on the first frame, including inside a split or tab.
        let w = f64::from((f32::from(size.width) - NAME_W).max(0.));
        let h = f64::from((f32::from(size.height) - RULER_H).max(1.));
        if w > 0. {
            self.surface.axis.set_width(w);
            if let Some((a, b)) = self.pending_fit.take() {
                self.surface.fit(a, b, 0.03);
            }
        }
        self.offset = self.offset.clamp(
            0.,
            (self.model.rows().len() as f64 * f64::from(ROW_H) - h).max(0.),
        );
        let start = (self.offset / f64::from(ROW_H)).floor() as usize;
        let end = ((self.offset + h) / f64::from(ROW_H)).ceil() as usize;
        let rows = self
            .model
            .rows()
            .get(start..end.min(self.model.rows().len()))
            .unwrap_or(&[]);
        let owner = cx.weak_entity();
        // Retain names and control state for channels still visible after scrolling.
        // Header equality below also handles edits and changes in the row tree.
        let mut headers_changed =
            self.header_offset != self.offset || rows.len() != self.headers.len();
        self.header_offset = self.offset;
        let shift = start
            .abs_diff(self.first_header_row)
            .min(self.headers.len());
        if start > self.first_header_row {
            self.headers.rotate_left(shift);
        } else {
            self.headers.rotate_right(shift);
        }
        self.first_header_row = start;
        for (index, row) in rows.iter().enumerate() {
            let c = &self.model.channels[row.channel];
            let current = self.headers.get_mut(index);
            let name = current
                .as_ref()
                .map(|old| &old.name)
                .filter(|name| name.as_str() == c.name.as_str())
                .cloned()
                .unwrap_or_else(|| c.name.clone().into());
            let header = ChannelHeader {
                channel: row.channel,
                depth: row.depth,
                name,
                color: c.color,
                group: c.group,
                collapsed: c.collapsed,
                muted: c.muted,
                locked: c.locked,
                visible: c.visible,
                owner: owner.clone(),
            };
            if let Some(entry) = current {
                if *entry != header {
                    headers_changed = true;
                    *entry = header;
                }
            } else {
                self.headers.push(header);
            }
        }
        self.headers.truncate(rows.len());
        let top = (start as f64 * f64::from(ROW_H) - self.offset) as f32;
        let header_rows = || self.headers.iter().cloned();
        let column = match &self.header_column {
            Some(column) => {
                if headers_changed {
                    column.update(cx, |column, cx| {
                        column.rows.clear();
                        column.rows.extend(header_rows());
                        column.top = top;
                        cx.notify();
                    });
                }
                column.clone()
            }
            None => {
                let column = cx.new(|_| HeaderColumn {
                    rows: header_rows().collect(),
                    top,
                });
                self.header_column = Some(column.clone());
                column
            }
        };
        let mut background = self.surface.clone();
        background.insert_layer(
            0,
            Zebra {
                rows: self.model.rows().len(),
            },
        );
        let mut foreground = PlotSurface::new(self.surface.axis);
        foreground.add_layer(Playhead {
            time: self.playhead,
            format: self.format,
        });
        if let Some(Gesture::Box { start, end, .. }) = self.drag {
            foreground.add_layer(SelectionBox {
                start: (self.surface.axis.x(start.0), start.1 - self.offset),
                end: (end.0, f64::from(end.1)),
            });
        }
        let key_owner = cx.weak_entity();
        let body = self.body.clone();
        let key_selected = theme
            .state(StateKind::Selected, Variant::Filled, None)
            .background
            .to_hsla();
        let key_muted = s.dimmed.to_hsla();
        let key_normal = theme
            .variant(Some(ColorName::Yellow), Variant::Filled)
            .background
            .to_hsla();
        let lock_style = self.lock_style;
        let tab = guiraffe_core::tab_stop(self.focusable);
        let focus_visible = guiraffe_core::ring_visible(&self.focus, window);
        let ring = theme
            .state(StateKind::FocusRing, Variant::Default, None)
            .border;
        let root = div()
            .id("dope-sheet")
            .test_support()
            .role(Role::Tree)
            .aria_label("Dope sheet")
            .track_focus(&self.focus.clone().tab_stop(tab))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .text_size(px(12.))
            .when(tab, |el| {
                el.key_context("TimelineDope").on_key_down(cx.listener(
                    |this, e: &gpui_kit::KeyDownEvent, _, cx| {
                        let key = e.keystroke.key.as_str();
                        match key {
                            "delete" | "backspace" => this.delete_selected(cx),
                            "left" => this.nudge(-1., cx),
                            "right" => this.nudge(1., cx),
                            "a" if e.keystroke.modifiers.control
                                || e.keystroke.modifiers.platform =>
                            {
                                this.select_all(cx)
                            }
                            "d" if e.keystroke.modifiers.control
                                || e.keystroke.modifiers.platform =>
                            {
                                this.duplicate_selected(cx)
                            }
                            _ => {}
                        }
                    },
                ))
            })
            .child(
                div()
                    .h(px(RULER_H))
                    .flex_none()
                    .flex()
                    .child(div().w(px(NAME_W)).flex_none().px(px(8.)).child("Channels"))
                    .child(
                        div()
                            .id("dope-ruler")
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.surface.element(
                                theme.clone(),
                                true,
                                f64::from(ROW_H),
                                self.offset,
                            ))
                            .child(foreground.element(
                                theme.clone(),
                                true,
                                f64::from(ROW_H),
                                self.offset,
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                                    let (x, _) = this.local(e.position);
                                    this.playhead =
                                        snap_time(this.surface.axis.t(x), this.format).max(0.);
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        column.cached(
                            gpui_kit::StyleRefinement::default()
                                .w(px(NAME_W))
                                .flex_none()
                                .h_full(),
                        ),
                    )
                    .child(
                        div()
                            .id("dope-body")
                            .test_support()
                            .debug_selector(|| "dope-body".into())
                            .relative()
                            .flex_1()
                            .overflow_hidden()
                            .cursor(gpui_kit::CursorStyle::Crosshair)
                            // The plot owns its drag feedback. Suppress GPUI's generic
                            // pressed state before bubbling would refresh the whole window.
                            .capture_any_mouse_down(|event, window, _| {
                                if event.button == MouseButton::Left {
                                    window.prevent_default();
                                }
                            })
                            .on_scroll(cx.listener(|this, e: &gpui_kit::ScrollEvent, _, cx| {
                                let d = e.delta.pixel_delta(px(16.));
                                let (x, _) = this.local(e.position);
                                let changed = if e.modifiers.control
                                    || e.modifiers.platform
                                    || e.modifiers.shift
                                {
                                    this.surface.navigate(
                                        x,
                                        f64::from(f32::from(d.x)),
                                        f64::from(f32::from(d.y)),
                                        e.modifiers.control || e.modifiers.platform,
                                    )
                                } else {
                                    let height =
                                        f64::from(f32::from(this.body.get().size.height)).max(1.);
                                    let max = (this.model.rows().len() as f64 * f64::from(ROW_H)
                                        - height)
                                        .max(0.);
                                    let offset =
                                        (this.offset - f64::from(f32::from(d.y))).clamp(0., max);
                                    let changed = offset != this.offset;
                                    this.offset = offset;
                                    changed
                                };
                                if changed {
                                    cx.notify();
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                                    let (x, y) = this.local(e.position);
                                    let row = ((f64::from(y) + this.offset) / f64::from(ROW_H))
                                        .max(0.)
                                        as usize;
                                    let repaint = this
                                        .press_key(row, x, e.modifiers.shift)
                                        .unwrap_or_else(|| {
                                            let cleared = !e.modifiers.shift
                                                && !this.model.selected.is_empty();
                                            if !e.modifiers.shift {
                                                this.model.selected.clear();
                                            }
                                            this.drag = Some(Gesture::Box {
                                                start: (
                                                    this.surface.axis.t(x),
                                                    f64::from(y) + this.offset,
                                                ),
                                                end: (x, y),
                                                add: e.modifiers.shift,
                                                base: this.model.selected.clone(),
                                            });
                                            cleared
                                        });
                                    cx.emit(DopeEvent::SelectionChanged);
                                    if repaint {
                                        cx.notify();
                                    }
                                }),
                            )
                            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    let (x, y) = this.local(e.position);
                                    if let Some(Gesture::Box {
                                        start,
                                        end,
                                        add,
                                        base,
                                    }) = &mut this.drag
                                    {
                                        *end = (x, y);
                                        let (a, b) = (
                                            (start.1 / f64::from(ROW_H)).max(0.) as usize,
                                            ((f64::from(y) + this.offset) / f64::from(ROW_H))
                                                .max(0.)
                                                as usize,
                                        );
                                        if *add {
                                            this.model.selected = base.clone();
                                        }
                                        this.model.box_select(
                                            a.min(b)..=a.max(b),
                                            start.0,
                                            this.surface.axis.t(x),
                                            *add,
                                        );
                                        cx.notify();
                                    } else {
                                        this.drag_to(x, cx);
                                    }
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    let marquee = matches!(this.drag, Some(Gesture::Box { .. }));
                                    this.release();
                                    if marquee {
                                        cx.notify();
                                    }
                                }),
                            )
                            .child(
                                canvas(move |b, _, _| body.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                            .child(background.element(
                                theme.clone(),
                                false,
                                f64::from(ROW_H),
                                self.offset,
                            ))
                            .child(key_canvas(
                                key_owner,
                                key_selected,
                                key_muted,
                                key_normal,
                                lock_style,
                            ))
                            .child(foreground.element(theme, false, f64::from(ROW_H), self.offset)),
                    ),
            );
        guiraffe_core::focus_ring(root, focus_visible, px(0.), ring).into_any_element()
    }
}
fn key_canvas(
    key_owner: gpui_kit::WeakEntity<DopeSheet>,
    key_selected: gpui_kit::Hsla,
    key_muted: gpui_kit::Hsla,
    key_normal: gpui_kit::Hsla,
    lock_style: LockStyle,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, _, window, cx| {
            let Some(owner) = key_owner.upgrade() else {
                return;
            };
            let sheet = owner.read(cx);
            let height = f64::from(f32::from(b.size.height)).max(1.);
            let rows = viewport_rows(&sheet.model, sheet.surface.axis, height, sheet.offset);
            // Sparse scenes keep their original rasterization. Count
            // only up to the crossover before traversing for paint.
            let batch_diamonds = rows.clone().flat_map(KeyRow::marks).nth(32).is_some();
            let at = |x: f64, y: f64| {
                gpui_kit::point(b.origin.x + px(x as f32), b.origin.y + px(y as f32))
            };
            let mut diamonds: Option<(gpui_kit::Hsla, gpui_kit::Path<Pixels>)> = None;
            let paint_mark = |mark: KeyMark,
                              diamonds: &mut Option<(gpui_kit::Hsla, gpui_kit::Path<Pixels>)>,
                              window: &mut Window| {
                let KeyMark {
                    x,
                    y,
                    selected,
                    shape,
                    hold,
                    muted,
                    summary,
                    locked,
                    ..
                } = mark;
                let ink = if selected {
                    key_selected
                } else if muted || (locked && lock_style == LockStyle::Dimmed) {
                    key_muted
                } else {
                    key_normal
                };
                let cy = y + f64::from(ROW_H) / 2.;
                let radius = if selected || summary { 6. } else { 4. };
                if batch_diamonds
                    && shape == KeyShape::Diamond
                    && hold.is_none()
                    && !(locked && lock_style == LockStyle::Hatched)
                {
                    if diamonds.as_ref().is_some_and(|(color, _)| *color != ink)
                        && let Some((color, mesh)) = diamonds.take()
                    {
                        window.paint_path(mesh, color);
                    }
                    let (_, mesh) =
                        diamonds.get_or_insert_with(|| (ink, gpui_kit::Path::new(at(x, cy))));
                    append_diamond(
                        mesh,
                        [
                            at(x, cy - radius),
                            at(x + radius, cy),
                            at(x, cy + radius),
                            at(x - radius, cy),
                        ],
                    );
                    return;
                }
                if let Some((color, mesh)) = diamonds.take() {
                    window.paint_path(mesh, color);
                }
                if let Some(end) = hold {
                    let mut path = gpui_kit::PathBuilder::stroke(px(3.));
                    path.move_to(at(x, cy));
                    path.line_to(at(end, cy));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, ink);
                    }
                }
                let mut path = gpui_kit::PathBuilder::fill();
                if shape == KeyShape::Diamond {
                    for (i, (dx, dy)) in [(0., -radius), (radius, 0.), (0., radius), (-radius, 0.)]
                        .iter()
                        .enumerate()
                    {
                        if i == 0 {
                            path.move_to(at(x + dx, cy + dy));
                        } else {
                            path.line_to(at(x + dx, cy + dy));
                        }
                    }
                } else {
                    let sides = if shape == KeyShape::Circle { 16 } else { 4 };
                    for i in 0..sides {
                        let angle = (i as f64 + 0.5) * std::f64::consts::TAU / sides as f64;
                        let p = at(x + radius * angle.cos(), cy + radius * angle.sin());
                        if i == 0 {
                            path.move_to(p)
                        } else {
                            path.line_to(p)
                        }
                    }
                }
                path.close();
                if locked && lock_style == LockStyle::Hatched {
                    path.move_to(at(x - radius, cy + radius));
                    path.line_to(at(x + radius, cy - radius));
                }

                if let Ok(path) = path.build() {
                    window.paint_path(path, ink);
                }
            };
            let strip = sheet.strip.clone();
            let scale = window.scale_factor();
            let left = f32::from(b.left());
            let origin = (left * scale).floor();
            let mut centers = Vec::new();
            let mut previous = None;
            let mut can_replace = true;
            for row in rows {
                if row.compact
                    && row.times.len() >= 64
                    && !(row.locked && lock_style == LockStyle::Hatched)
                {
                    // Bound comparisons by the viewport, even for a lane
                    // with millions of keys at subpixel spacing.
                    let same = row.times.len() <= 4096 && previous == Some(row.times);
                    if !same {
                        previous = Some(row.times);
                        centers.clear();
                        centers
                            .extend(row.indices().map(|i| {
                                (left + row.axis.x(row.times[i]) as f32) * scale - origin
                            }));
                    }
                    if centers.len() >= 64 {
                        let ink = if row.muted || (row.locked && lock_style == LockStyle::Dimmed) {
                            key_muted
                        } else {
                            key_normal
                        };
                        let cy = f32::from(b.top()) + row.y as f32 + ROW_H / 2.;
                        if let Some((color, mesh)) = diamonds.take() {
                            window.paint_path(mesh, color);
                        }
                        if strip
                            .borrow_mut()
                            .paint(&centers, b, cy, ink, can_replace, window)
                        {
                            can_replace = false;
                            continue;
                        }
                    }
                }
                for mark in row.marks() {
                    paint_mark(mark, &mut diamonds, window);
                }
            }
            if let Some((color, mesh)) = diamonds {
                window.paint_path(mesh, color);
            }
            if let Some(old) = strip.borrow_mut().retired.take() {
                cx.drop_image(old, Some(window));
            }
        },
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_batch_matches_generic_triangle_vertices_and_bounds() {
        use gpui_kit::{Path, point};

        let origin = point(px(2.), px(7.));
        let mut batch = Path::new(origin);
        let mut reference = Path::new(origin);
        let uv = point(0., 1.);
        for x in [-4., 20., 13.] {
            let top = point(px(x), px(4.));
            let right = point(px(x + 3.), px(7.));
            let bottom = point(px(x), px(10.));
            let left = point(px(x - 3.), px(7.));
            append_diamond(&mut batch, [top, right, bottom, left]);
            reference.push_triangle((top, right, bottom), (uv, uv, uv));
            reference.push_triangle((top, bottom, left), (uv, uv, uv));
        }
        assert_eq!(batch.bounds, reference.bounds);
        assert_eq!(batch.vertices.len(), reference.vertices.len());
        for (actual, expected) in batch.vertices.iter().zip(&reference.vertices) {
            assert_eq!(actual.xy_position, expected.xy_position);
            assert_eq!(actual.st_position, expected.st_position);
            assert_eq!(actual.content_mask, expected.content_mask);
        }
    }

    #[test]
    fn viewport_visits_only_intersecting_rows_at_both_scales() {
        for count in [10_000, 100_000] {
            let model =
                DopeModel::new((0..count).map(|_| Channel::new("lane", vec![1.])).collect());
            let axis = Axis::new(0., 10., 100.);
            let row_height = f64::from(ROW_H);
            let offset = 25.5 * row_height;
            let rows: Vec<_> = viewport_keys(&model, axis, 2. * row_height, offset)
                .map(|mark| mark.y)
                .collect();
            assert_eq!(
                rows,
                [-0.5 * row_height, 0.5 * row_height, 1.5 * row_height]
            );
        }
    }

    #[test]
    fn a_cluster_in_an_otherwise_sparse_lane_retains_isolated_keys() {
        let model = DopeModel::new(vec![Channel::new("lane", vec![1., 2., 3., 5., 80.])]);
        let marks: Vec<_> = viewport_keys(&model, Axis::new(0., 100., 100.), f64::from(ROW_H), 0.)
            .map(|mark| mark.x)
            .collect();
        assert_eq!(marks, [1., 5., 80.]);
    }

    #[test]
    fn dense_plain_keys_emit_the_same_marks_across_input_scales() {
        for count in [10_000, 100_000, 10_000_000] {
            let times: Vec<_> = (0..count).map(|i| i as f64 * 100. / count as f64).collect();
            let marks: Vec<_> = key_indices(&times, 0, count, Some(4.))
                .map(|i| times[i])
                .collect();
            assert_eq!(
                marks,
                (0..25).map(|i| f64::from(i) * 4.).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn channels_sort_dedup_and_refuse_collisions() {
        let mut c = Channel::new("x", vec![2.0, 1.0, 2.0, f64::NAN]);
        assert_eq!(c.keys(), &[1.0, 2.0]);
        assert_eq!(c.retime(0, 2.0), None, "collision");
        assert_eq!(c.retime(0, 3.0), Some(1), "reorders");
        assert_eq!(c.keys(), &[2.0, 3.0]);
        assert_eq!(c.retime(9, 1.0), None);
    }

    #[test]
    fn frames_snap() {
        assert_eq!(snap_time(0.51, TimeFormat::Frames(10.0)), 0.5);
        assert_eq!(snap_time(0.51, TimeFormat::Seconds), 0.51);
    }

    #[test]
    fn dragging_retimes_with_frame_snap_and_follows_reorder() {
        let cx = gpui_kit::TestAppContext::single();
        let view = cx.update(|cx| {
            DopeSheet::new(
                vec![Channel::new("pos", vec![0.5, 1.0])],
                TimeFormat::Frames(10.0),
                2.0,
                cx,
            )
        });
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let ev = events.clone();
        let _s = cx.update(|cx| {
            cx.subscribe(&view, move |_, e: &DopeEvent, _| {
                ev.borrow_mut().push(e.clone())
            })
        });
        view.update(&mut cx.clone(), |v, cx| {
            let a = *v.axis();
            assert!(v.press(0, a.x(0.5)));
            v.drag_to(a.x(1.42), cx); // -> 1.4, now index 1
            v.drag_to(a.x(1.61), cx); // same key keeps moving -> 1.6
            v.release();
        });
        assert_eq!(
            view.read_with(&cx, |v, _| v.channels()[0].keys().to_vec()),
            vec![1.0, 1.6]
        );
        assert_eq!(events.borrow().len(), 2);
    }
}

#[cfg(test)]
mod hierarchy_regressions {
    use super::*;
    #[test]
    fn summary_drag_selects_real_children_and_duplicate_keeps_metadata() {
        let mut model = DopeModel::new(vec![Channel::group(
            "Object",
            vec![
                Channel::with_keyframes(
                    "x",
                    vec![crate::Key::new(0.5, 1., crate::Interp::Constant)],
                ),
                Channel::new("locked", vec![0.5]),
            ],
        )]);
        model.channels[2].locked = true;
        model.select_summary(0, 0.5, false);
        assert_eq!(model.selected.len(), 2);
        assert!(model.duplicate(0.1, TimeFormat::Frames(10.)));
        assert_eq!(model.channels[1].keys(), &[0.5, 0.6]);
        assert_eq!(model.channels[2].keys(), &[0.5]);
        assert_eq!(
            model.channels[1].decor,
            Some(vec![(Some(1.), KeyShape::Square); 2])
        );
    }
    #[test]
    fn adjacent_selected_keys_move_atomically() {
        let mut model = DopeModel::new(vec![Channel::new("x", vec![0., 1., 2.])]);
        model.select_all();
        assert!(model.translate(1., TimeFormat::Seconds));
        assert_eq!(model.channels[0].keys(), &[1., 2., 3.]);
        assert!(model.channels[0].decor.is_none());
    }
}

pub(crate) mod document;
