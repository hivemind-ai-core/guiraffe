//! GPUI widgets over the pure logic in [`crate::logic`].

use std::rc::Rc;

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable,
    IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels,
    Render, RenderOnce, SharedString, Styled, Subscription, Window, actions, canvas,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px,
};
use guiraffe_components::{Pill, Text};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StyleContext, Variant};

use crate::logic::{
    ChipSelection, Range2, TagRejected, TagRules, Thumb, accept_tag, accepts, password_strength,
    split_input,
};

fn ctx_size(explicit: Option<Size>) -> Size {
    explicit.unwrap_or(StyleContext::current().control_size)
}

// ------------------------------------------------------------------ ChipGroup

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChipEvent {
    Changed(Vec<usize>),
}

/// A group of toggleable chips (single or multiple selection).
pub struct ChipGroup {
    labels: Vec<SharedString>,
    selection: ChipSelection,
    color: Option<ColorName>,
}

impl EventEmitter<ChipEvent> for ChipGroup {}

impl ChipGroup {
    pub fn new(
        labels: impl IntoIterator<Item = impl Into<SharedString>>,
        multiple: bool,
        cx: &mut App,
    ) -> Entity<Self> {
        let labels = labels.into_iter().map(Into::into).collect();
        cx.new(|_| Self {
            labels,
            selection: ChipSelection {
                multiple,
                ..Default::default()
            },
            color: None,
        })
    }
    pub fn selected(&self) -> Vec<usize> {
        self.selection.selected.iter().copied().collect()
    }
    pub fn toggle(&mut self, i: usize, cx: &mut Context<Self>) {
        if i < self.labels.len() && self.selection.toggle(i) {
            cx.emit(ChipEvent::Changed(self.selected()));
            cx.notify();
        }
    }
    pub fn set_color(&mut self, c: ColorName, cx: &mut Context<Self>) {
        self.color = Some(c);
        cx.notify();
    }
}

impl Render for ChipGroup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let size = ctx_size(None);
        let (on, off) = (
            t.variant(self.color, Variant::Light),
            t.variant(Some(ColorName::Gray), Variant::Outline),
        );
        let fs = px(size.font_size());
        div()
            .flex()
            .flex_wrap()
            .gap(px((size.spacing() / 2.0).round()))
            .children(self.labels.iter().enumerate().map(|(i, label)| {
                let checked = self.selection.selected.contains(&i);
                let v = if checked { on } else { off };
                div()
                    .id(("chip", i))
                    .h(px((size.control_height() * 0.8).round()))
                    .px(px(size.spacing()))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .border_1()
                    .cursor_pointer()
                    .text_size(fs)
                    .bg(v.background.to_hsla())
                    .border_color(if checked {
                        v.foreground.to_hsla()
                    } else {
                        v.border.to_hsla()
                    })
                    .text_color(v.foreground.to_hsla())
                    .child(if checked {
                        format!("✓ {label}")
                    } else {
                        label.to_string()
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle(i, cx)))
            }))
    }
}

// ------------------------------------------------------------------ TagsInput

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagsEvent {
    Changed(Vec<String>),
    Rejected(String, TagRejected),
}

/// Free-form tags: type and press Enter or a split character; Backspace on an empty input
/// removes the last tag; pasted lists are split.
pub struct TagsInput {
    tags: Vec<String>,
    rules: TagRules,
    input: Entity<InputState>,
    _sub: Subscription,
}

impl EventEmitter<TagsEvent> for TagsInput {}

impl TagsInput {
    pub fn new(
        rules: TagRules,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let placeholder = placeholder.into();
        cx.new(|cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            let sub = cx.subscribe_in(
                &input,
                window,
                |this: &mut Self, _, e: &InputEvent, window, cx| match e {
                    InputEvent::Change => this.on_change(window, cx),
                    InputEvent::PressEnter { .. } => this.commit_rest(window, cx),
                    _ => {}
                },
            );
            Self {
                tags: Vec::new(),
                rules,
                input,
                _sub: sub,
            }
        })
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    #[doc(hidden)]
    pub fn input_for_test(&self) -> Entity<InputState> {
        self.input.clone()
    }

    /// Try to add `raw`; emits Changed or Rejected.
    pub fn add(&mut self, raw: &str, cx: &mut Context<Self>) -> bool {
        match accept_tag(&self.tags, raw, &self.rules) {
            Ok(tag) => {
                self.tags.push(tag);
                cx.emit(TagsEvent::Changed(self.tags.clone()));
                cx.notify();
                true
            }
            Err(TagRejected::Empty) => false,
            Err(why) => {
                cx.emit(TagsEvent::Rejected(raw.trim().to_owned(), why));
                false
            }
        }
    }

    pub fn remove(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.tags.len() {
            self.tags.remove(index);
            cx.emit(TagsEvent::Changed(self.tags.clone()));
            cx.notify();
        }
    }

    fn on_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        let (done, rest) = split_input(&text, &self.rules);
        if done.is_empty() {
            return;
        }
        let rest = rest.to_owned();
        for t in done {
            self.add(t, cx);
        }
        self.input.update(cx, |i, cx| i.set_value(rest, window, cx));
    }

    fn commit_rest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if self.add(&text, cx) {
            self.input.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }

    fn backspace(&mut self, cx: &mut Context<Self>) {
        if self.input.read(cx).value().is_empty()
            && let Some(last) = self.tags.len().checked_sub(1)
        {
            self.remove(last, cx);
        }
    }
}

impl Render for TagsInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let entity = cx.entity();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .p(px(4.))
            .rounded(px(GuiraffeTheme::global(cx).default_radius.radius()))
            .border_1()
            .border_color(s.border.to_hsla())
            .children(self.tags.iter().enumerate().map(|(i, t)| {
                let e = entity.clone();
                Pill::new(("tag", i), t.clone())
                    .on_remove(move |_, _, cx| e.update(cx, |this, cx| this.remove(i, cx)))
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(80.))
                    .on_key_down(cx.listener(|this, e: &gpui_kit::KeyDownEvent, _, cx| {
                        if e.keystroke.key == "backspace" {
                            this.backspace(cx);
                        }
                    }))
                    .child(Input::new(&self.input).appearance(false)),
            )
    }
}

// ------------------------------------------------------------------ RangeSlider

actions!(guiraffe_range, [LowDown, LowUp, HighDown, HighUp]);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RangeEvent {
    Changed(f32, f32),
}

/// A two-thumb slider. Drag either thumb (the nearer one is grabbed); keyboard: ←/→ move
/// the low thumb, Shift+←/→ the high thumb.
pub struct RangeSlider {
    range: Range2,
    dragging: Option<Thumb>,
    track: Rc<std::cell::Cell<Bounds<Pixels>>>,
    focus: FocusHandle,
    color: Option<ColorName>,
}

impl EventEmitter<RangeEvent> for RangeSlider {}

impl Focusable for RangeSlider {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

const RANGE_CONTEXT: &str = "GrRangeSlider";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", LowDown, Some(RANGE_CONTEXT)),
        KeyBinding::new("right", LowUp, Some(RANGE_CONTEXT)),
        KeyBinding::new("shift-left", HighDown, Some(RANGE_CONTEXT)),
        KeyBinding::new("shift-right", HighUp, Some(RANGE_CONTEXT)),
    ]);
}

impl RangeSlider {
    pub fn new(range: Range2, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            range,
            dragging: None,
            track: Rc::default(),
            focus: cx.focus_handle(),
            color: None,
        })
    }
    pub fn values(&self) -> (f32, f32) {
        (self.range.low, self.range.high)
    }
    pub fn set(&mut self, thumb: Thumb, v: f32, cx: &mut Context<Self>) {
        if self.range.set(thumb, v) {
            cx.emit(RangeEvent::Changed(self.range.low, self.range.high));
            cx.notify();
        }
    }
    fn value_at_x(&self, x: Pixels) -> f32 {
        let b = self.track.get();
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return self.range.low;
        }
        self.range
            .value_at((f32::from(x) - f32::from(b.origin.x)) / w)
    }
    fn step(&mut self, thumb: Thumb, dir: f32, cx: &mut Context<Self>) {
        let cur = if thumb == Thumb::Low {
            self.range.low
        } else {
            self.range.high
        };
        self.set(thumb, cur + dir * self.range.step, cx);
    }
}

impl Render for RangeSlider {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let fill = t.variant(self.color, Variant::Filled).background.to_hsla();
        let size = ctx_size(None);
        let thumb = (size.control_height() * 0.45).round();
        let (lo, hi) = (
            self.range.fraction(self.range.low),
            self.range.fraction(self.range.high),
        );
        let track = self.track.clone();
        let focused = self.focus.is_focused(window);
        let thumb_el = |frac: f32| {
            div()
                .absolute()
                .top(px(-(thumb - 4.0) / 2.0))
                .left(gpui_kit::relative(frac))
                .ml(px(-thumb / 2.0))
                .size(px(thumb))
                .rounded_full()
                .bg(fill)
                .border_2()
                .border_color(s.body.to_hsla())
                .when(focused, |el| {
                    let mut halo = fill;
                    halo.a = 0.35;
                    el.shadow(vec![
                        gpui_kit::BoxShadow::new(px(0.), px(0.), halo).spread_radius(px(3.)),
                    ])
                })
        };
        div()
            .id("range")
            .track_focus(&self.focus)
            .key_context(RANGE_CONTEXT)
            .on_action(cx.listener(|this, _: &LowDown, _, cx| this.step(Thumb::Low, -1.0, cx)))
            .on_action(cx.listener(|this, _: &LowUp, _, cx| this.step(Thumb::Low, 1.0, cx)))
            .on_action(cx.listener(|this, _: &HighDown, _, cx| this.step(Thumb::High, -1.0, cx)))
            .on_action(cx.listener(|this, _: &HighUp, _, cx| this.step(Thumb::High, 1.0, cx)))
            .py(px(thumb / 2.0))
            .w_full()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    let v = this.value_at_x(e.position.x);
                    let thumb = this.range.nearest(v);
                    this.dragging = Some(thumb);
                    this.set(thumb, v, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if let (Some(thumb), Some(MouseButton::Left)) = (this.dragging, e.pressed_button) {
                    let v = this.value_at_x(e.position.x);
                    this.set(thumb, v, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = None),
            )
            .child(
                div()
                    .relative()
                    .h(px(4.))
                    .rounded_full()
                    .bg(s.track.to_hsla())
                    .child(
                        canvas(move |b, _, _| track.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .h_full()
                            .left(gpui_kit::relative(lo))
                            .w(gpui_kit::relative(hi - lo))
                            .rounded_full()
                            .bg(fill),
                    )
                    .child(thumb_el(lo))
                    .child(thumb_el(hi)),
            )
    }
}

// ------------------------------------------------------------------ PasswordStrength

/// Four-segment strength meter (pair with a Controls password input).
#[derive(IntoElement)]
pub struct PasswordStrength {
    strength: u8,
}

impl PasswordStrength {
    pub fn new(password: &str) -> Self {
        Self {
            strength: password_strength(password),
        }
    }
    pub fn strength(&self) -> u8 {
        self.strength
    }
}

impl RenderOnce for PasswordStrength {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let color = match self.strength {
            0 | 1 => ColorName::Red,
            2 => ColorName::Orange,
            3 => ColorName::Yellow,
            _ => ColorName::Teal,
        };
        let fill = t.variant(Some(color), Variant::Filled).background.to_hsla();
        let track = t.surfaces().track.to_hsla();
        let label = ["Very weak", "Weak", "Fair", "Good", "Strong"][self.strength.min(4) as usize];
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(div().flex().gap(px(4.)).children((0..4u8).map(|i| {
                div()
                    .flex_1()
                    .h(px(4.))
                    .rounded_full()
                    .bg(if i < self.strength { fill } else { track })
            })))
            .child(Text::new(label).size(Size::Xs).dimmed(true))
    }
}

// ------------------------------------------------------------------ Dropzone

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropEvent {
    Accepted(Vec<std::path::PathBuf>),
    Rejected(Vec<std::path::PathBuf>),
}

/// A file drop target with an accept filter (`[".png", "image/*"]`).
pub struct Dropzone {
    accept: Vec<&'static str>,
    label: SharedString,
}

impl EventEmitter<DropEvent> for Dropzone {}

impl Dropzone {
    pub fn new(
        accept: Vec<&'static str>,
        label: impl Into<SharedString>,
        cx: &mut App,
    ) -> Entity<Self> {
        let label = label.into();
        cx.new(|_| Self { accept, label })
    }

    /// Split dropped paths into accepted and rejected (testable without a real drop).
    pub fn receive(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        let (ok, bad): (Vec<_>, Vec<_>) = paths.iter().cloned().partition(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| accepts(&self.accept, n))
        });
        if !ok.is_empty() {
            cx.emit(DropEvent::Accepted(ok));
        }
        if !bad.is_empty() {
            cx.emit(DropEvent::Rejected(bad));
        }
    }
}

impl Render for Dropzone {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let hover = t.variant(None, Variant::Light);
        div()
            .id("dropzone")
            .p(px(Size::Xl.spacing()))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(t.default_radius.radius()))
            .border_2()
            .border_dashed()
            .border_color(s.border.to_hsla())
            .text_color(s.dimmed.to_hsla())
            .drag_over::<ExternalPaths>(move |st, _, _, _| {
                st.bg(hover.background.to_hsla())
                    .border_color(hover.foreground.to_hsla())
            })
            .on_drop(
                cx.listener(|this, paths: &ExternalPaths, _, cx| this.receive(paths.paths(), cx)),
            )
            .child(self.label.clone())
    }
}
