//! GPUI widgets over the pure logic in [`crate::logic`].

use std::rc::Rc;

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable,
    IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels,
    Render, RenderOnce, Role, SharedString, Styled, Subscription, TestSupportExt, Window, actions,
    canvas,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px,
};
use guiraffe_components::{Pill, Text};
use guiraffe_core::{
    A11yDisabledExt, ColorName, GuiraffeTheme, Press, PressableExt, Size, StateKind, StyleContext,
    Variant, focus_ring, keyed_focus, ring_visible,
};

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
///
/// Interaction (spec/00 §12): each chip is a toggle button (AccessKit `CheckBox` with toggled
/// state when multiple, `RadioButton` with selected state when single). A pointer press never
/// takes focus; chips are tab stops only inside a focus scope or with
/// [`ChipGroup::set_focusable`], and then Enter/Space toggle and a focus ring shows.
pub struct ChipGroup {
    labels: Vec<SharedString>,
    /// The chip text, checked and unchecked: it changes with the labels, not
    /// with every render.
    text: Vec<SharedString>,
    checked_text: Vec<SharedString>,
    selection: ChipSelection,
    color: Option<ColorName>,
    disabled: bool,
    focusable: Option<bool>,
}

impl EventEmitter<ChipEvent> for ChipGroup {}

impl ChipGroup {
    pub fn new(
        labels: impl IntoIterator<Item = impl Into<SharedString>>,
        multiple: bool,
        cx: &mut App,
    ) -> Entity<Self> {
        let labels = labels
            .into_iter()
            .map(Into::into)
            .collect::<Vec<SharedString>>();
        let text = labels.to_vec();
        let checked_text = labels
            .iter()
            .map(|label| format!("\u{2713} {label}").into())
            .collect();
        cx.new(|_| Self {
            labels,
            text,
            checked_text,
            selection: ChipSelection {
                multiple,
                ..Default::default()
            },
            color: None,
            disabled: false,
            focusable: None,
        })
    }
    pub fn set_disabled(&mut self, v: bool, cx: &mut Context<Self>) {
        self.disabled = v;
        cx.notify();
    }
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = v;
        cx.notify();
    }
    pub fn selected(&self) -> Vec<usize> {
        self.selection.selected.iter().copied().collect()
    }
    pub fn toggle(&mut self, i: usize, cx: &mut Context<Self>) {
        if !self.disabled && i < self.labels.len() && self.selection.toggle(i) {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let size = ctx_size(None);
        let (on, off) = (
            t.variant(self.color, Variant::Light),
            t.variant(Some(ColorName::Gray), Variant::Outline),
        );
        let (on_c, off_c) = (
            t.press_colors(Variant::Light, self.color),
            t.press_colors(Variant::Outline, Some(ColorName::Gray)),
        );
        let fs = px(size.font_size());
        let h = (size.control_height() * 0.8).round();
        let (disabled, focusable, multiple) =
            (self.disabled, self.focusable, self.selection.multiple);
        let chips: Vec<_> = self
            .labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let checked = self.selection.selected.contains(&i);
                let v = if checked { on } else { off };
                let mut colors = if checked { on_c } else { off_c };
                // Keep the chip's own look: checked chips outline in their text color.
                // Border width never changes, only its color (§12.2).
                colors.rest.border = if checked { v.foreground } else { v.border };
                colors.hover.border = colors.rest.border;
                let press = Press::new(colors)
                    .disabled(disabled)
                    .focusable(focusable)
                    .radius(px(h / 2.0))
                    .role(if multiple {
                        Role::CheckBox
                    } else {
                        Role::RadioButton
                    })
                    .label(label.clone());
                // Toggle state goes to AccessKit; the chip paints its own checked look
                // (colors + check mark) rather than the filled Selected token.
                let press = if multiple {
                    press.toggled(checked)
                } else {
                    press
                };
                let focus = keyed_focus(("chip-focus", i), window, cx);
                div()
                    .id(("chip", i))
                    .a11y_disabled(disabled)
                    .test_support()
                    .h(px(h))
                    .px(px(size.spacing()))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .border_1()
                    .text_size(fs)
                    .child(if checked {
                        self.checked_text[i].clone()
                    } else {
                        self.text[i].clone()
                    })
                    .press(&press, &focus, window)
                    .when(!multiple, |el| el.aria_selected(checked))
                    .on_press(
                        disabled,
                        cx.listener(move |this, _, _, cx| this.toggle(i, cx)),
                    )
            })
            .collect();
        div()
            .id("chips")
            .role(if multiple {
                Role::Group
            } else {
                Role::RadioGroup
            })
            .flex()
            .flex_wrap()
            .gap(px((size.spacing() / 2.0).round()))
            .children(chips)
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
    /// The tag labels, as the pills show them: they change with the tags, not
    /// with every render.
    tag_labels: Vec<SharedString>,
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
                tag_labels: Vec::new(),
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
                self.tag_labels
                    .push(self.tags[self.tags.len() - 1].as_str().into());
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
            self.tag_labels.remove(index);
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
            .children(self.tags.iter().enumerate().map(|(i, _)| {
                let e = entity.clone();
                Pill::new(("tag", i), self.tag_labels[i].clone())
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

actions!(
    guiraffe_range,
    [LowDown, LowUp, HighDown, HighUp, LowMin, HighMax]
);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RangeEvent {
    Changed(f32, f32),
}

/// A two-thumb slider. Drag either thumb (the nearer one is grabbed).
///
/// Interaction (spec/00 §12): a pointer press drags without taking keyboard focus. The slider
/// is a tab stop only inside a focus scope or with [`RangeSlider::set_focusable`]; focused,
/// ←/→ move the low thumb, Shift+←/→ the high thumb, Home sends the low thumb to the minimum
/// and End the high thumb to the maximum, and a focus ring shows. AccessKit: `Slider` with
/// numeric value (low), min, max, step and a "low – high" value text.
pub struct RangeSlider {
    range: Range2,
    /// The accessible value text: it changes with the range, not with every
    /// render.
    aria_value: SharedString,
    dragging: Option<Thumb>,
    track: Rc<std::cell::Cell<Bounds<Pixels>>>,
    focus: FocusHandle,
    color: Option<ColorName>,
    disabled: bool,
    focusable: Option<bool>,
    label: SharedString,
}

impl EventEmitter<RangeEvent> for RangeSlider {}

impl Focusable for RangeSlider {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

const RANGE_CONTEXT: &str = "GrRangeSlider";

pub fn init(cx: &mut App) {
    document::register(cx);
    cx.bind_keys([
        KeyBinding::new("left", LowDown, Some(RANGE_CONTEXT)),
        KeyBinding::new("right", LowUp, Some(RANGE_CONTEXT)),
        KeyBinding::new("shift-left", HighDown, Some(RANGE_CONTEXT)),
        KeyBinding::new("shift-right", HighUp, Some(RANGE_CONTEXT)),
        KeyBinding::new("home", LowMin, Some(RANGE_CONTEXT)),
        KeyBinding::new("end", HighMax, Some(RANGE_CONTEXT)),
    ]);
}

/// The accessible value of a range: "low – high".
fn aria_value(range: &Range2) -> SharedString {
    format!("{} – {}", range.low, range.high).into()
}

impl RangeSlider {
    pub fn new(range: Range2, cx: &mut App) -> Entity<Self> {
        let aria_value = aria_value(&range);
        cx.new(|cx| Self {
            range,
            aria_value,
            dragging: None,
            track: Rc::default(),
            focus: cx.focus_handle(),
            color: None,
            disabled: false,
            focusable: None,
            label: "Range".into(),
        })
    }
    /// Disabled: no dragging or keys, disabled tokens, `not-allowed` cursor.
    pub fn set_disabled(&mut self, v: bool, cx: &mut Context<Self>) {
        self.disabled = v;
        if v {
            self.dragging = None;
        }
        cx.notify();
    }
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = v;
        cx.notify();
    }
    /// Accessible name (default "Range").
    pub fn set_label(&mut self, l: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.label = l.into();
        cx.notify();
    }
    pub fn values(&self) -> (f32, f32) {
        (self.range.low, self.range.high)
    }
    pub fn set(&mut self, thumb: Thumb, v: f32, cx: &mut Context<Self>) {
        if self.range.set(thumb, v) {
            self.aria_value = aria_value(&self.range);
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
        if self.disabled {
            return;
        }
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
        let disabled = self.disabled;
        let off = t.state(StateKind::Disabled, Variant::Filled, self.color);
        let ring = t
            .state(StateKind::FocusRing, Variant::Filled, self.color)
            .border;
        let fill = if disabled {
            off.foreground.to_hsla()
        } else {
            t.variant(self.color, Variant::Filled).background.to_hsla()
        };
        let size = ctx_size(None);
        let thumb = (size.control_height() * 0.45).round();
        let (lo, hi) = (
            self.range.fraction(self.range.low),
            self.range.fraction(self.range.high),
        );
        let track = self.track.clone();
        let ring_on = !disabled && ring_visible(&self.focus, window);
        let tab = !disabled && guiraffe_core::tab_stop(self.focusable);
        let r = self.range;
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
        };
        let root = div()
            .id("range")
            .a11y_disabled(disabled)
            .test_support()
            .role(Role::Slider)
            .aria_label(self.label.clone())
            .aria_numeric_value(f64::from(r.low))
            .aria_min_numeric_value(f64::from(r.min))
            .aria_max_numeric_value(f64::from(r.max))
            .aria_numeric_value_step(f64::from(r.step))
            .aria_value(self.aria_value.clone())
            .track_focus(&self.focus.clone().tab_stop(tab))
            .key_context(RANGE_CONTEXT)
            .on_action(cx.listener(|this, _: &LowDown, _, cx| this.step(Thumb::Low, -1.0, cx)))
            .on_action(cx.listener(|this, _: &LowUp, _, cx| this.step(Thumb::Low, 1.0, cx)))
            .on_action(cx.listener(|this, _: &HighDown, _, cx| this.step(Thumb::High, -1.0, cx)))
            .on_action(cx.listener(|this, _: &HighUp, _, cx| this.step(Thumb::High, 1.0, cx)))
            .on_action(cx.listener(|this, _: &LowMin, _, cx| {
                if !this.disabled {
                    this.set(Thumb::Low, this.range.min, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &HighMax, _, cx| {
                if !this.disabled {
                    this.set(Thumb::High, this.range.max, cx)
                }
            }))
            .py(px(thumb / 2.0))
            .w_full()
            .rounded(px(4.))
            .map(|el| {
                if disabled {
                    el.cursor_not_allowed()
                } else {
                    el.cursor_pointer()
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    // A pointer press drags but never takes keyboard focus (§12.3).
                    window.prevent_default();
                    if this.disabled {
                        return;
                    }
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
                    .bg(if disabled { off.background } else { s.track }.to_hsla())
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
            );
        focus_ring(root, ring_on, px(4.), ring)
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
    /// Click, or Enter/Space when focused: the host should open a file picker
    /// (GPUI `App::prompt_for_paths`) and pass the result to [`Dropzone::receive`].
    BrowseRequested,
}

/// A file drop target with an accept filter (`[".png", "image/*"]`).
///
/// Interaction (spec/00 §12): also a button (AccessKit `Button` named by its label) that
/// emits [`DropEvent::BrowseRequested`] on click or Enter/Space. It never takes focus on
/// pointer press; it is a tab stop inside a focus scope or with [`Dropzone::set_focusable`].
pub struct Dropzone {
    accept: Vec<SharedString>,
    label: SharedString,
    disabled: bool,
    focusable: Option<bool>,
    focus: FocusHandle,
}

impl EventEmitter<DropEvent> for Dropzone {}

impl Dropzone {
    pub fn new(
        accept: Vec<&'static str>,
        label: impl Into<SharedString>,
        cx: &mut App,
    ) -> Entity<Self> {
        let label = label.into();
        cx.new(|cx| Self {
            accept: accept.into_iter().map(Into::into).collect(),
            label,
            disabled: false,
            focusable: None,
            focus: cx.focus_handle(),
        })
    }
    /// Disabled: drops, clicks and keys are ignored; disabled tokens and cursor.
    pub fn set_disabled(&mut self, v: bool, cx: &mut Context<Self>) {
        self.disabled = v;
        cx.notify();
    }
    pub fn set_focusable(&mut self, v: Option<bool>, cx: &mut Context<Self>) {
        self.focusable = v;
        cx.notify();
    }
    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Split dropped paths into accepted and rejected (testable without a real drop).
    pub fn receive(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let accepted: Vec<_> = self.accept.iter().map(SharedString::as_ref).collect();
        let (ok, bad): (Vec<_>, Vec<_>) = paths.iter().cloned().partition(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| accepts(&accepted, n))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let hover = t.variant(None, Variant::Light);
        let mut colors = t.press_colors(Variant::Subtle, Some(ColorName::Gray));
        // The dashed outline and dimmed label are the zone's rest look.
        colors.rest.border = s.border;
        colors.rest.foreground = s.dimmed;
        colors.hover.border = s.border;
        colors.hover.foreground = s.text;
        let r = px(t.default_radius.radius());
        let disabled = self.disabled;
        let press = Press::new(colors)
            .disabled(disabled)
            .focusable(self.focusable)
            .radius(r)
            .label(self.label.clone());
        div()
            .id("dropzone")
            .a11y_disabled(disabled)
            .test_support()
            .p(px(Size::Xl.spacing()))
            .flex()
            .items_center()
            .justify_center()
            .rounded(r)
            .border_2()
            .border_dashed()
            .press(&press, &self.focus, window)
            .when(!disabled, |el| {
                el.drag_over::<ExternalPaths>(move |st, _, _, _| {
                    st.bg(hover.background.to_hsla())
                        .border_color(hover.foreground.to_hsla())
                })
                .on_drop(
                    cx.listener(|this, paths: &ExternalPaths, _, cx| {
                        this.receive(paths.paths(), cx)
                    }),
                )
            })
            .on_press(
                disabled,
                cx.listener(|_, _, _, cx| cx.emit(DropEvent::BrowseRequested)),
            )
            .child(self.label.clone())
    }
}

mod document;
