//! Spoiler: content that expands and collapses with an animated height.
//!
//! The reveal is a Control: it owns an [`Animated`] progress and samples it in its own view, so
//! animation frames re-render only the spoiler (conventions §6.5). The body is laid out inside a
//! clipping wrapper whose height runs from the preview to the body's measured height, so any
//! child — text, cards, a list — expands smoothly. Reduced motion jumps straight to the end
//! state, because the timing comes from guiraffe-motion's prefs.

use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, TestSupportExt, Window, canvas, div,
    prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Press, PressableExt, Size, Variant};
use guiraffe_motion::{Animated, Easing, MotionPrefs, Timing};

use crate::style::{radius, theme};

/// Height of the collapsed preview: `lines` lines of text at `size`.
pub fn collapsed_height(size: Size, lines: usize, scale: f32) -> f32 {
    let lines = lines.max(1) as f32;
    size.font_size() * scale * size.line_height() * lines
}

/// The label for a spoiler in state `expanded`.
pub fn label<'a>(expanded: bool, more: &'a str, less: &'a str) -> &'a str {
    if expanded { less } else { more }
}

/// Height of the reveal at `progress`: from the `peek` height to the measured content height.
///
/// `None` until the content has been measured; the caller then renders at `peek` until a frame
/// re-measures. Exposed for tests.
pub fn reveal_height(measured: Option<f32>, peek: f32, progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    match measured {
        Some(full) if full > 0.0 => peek + (full - peek).max(0.0) * progress,
        _ => peek + (progress * 0.0),
    }
}

/// A region of content that expands to its full height on demand.
///
/// ```
/// use guiraffe_components::Spoiler;
///
/// # fn build(cx: &mut gpui_kit::App) {
/// let spoiler = Spoiler::new(cx);
/// spoiler.update(cx, |s, cx| s.set_collapsed_lines(3, cx));
/// # }
/// ```
pub struct Spoiler {
    id: String,
    content: Build,
    expanded: bool,
    /// Reveal progress in 0..=1; sampling it re-renders only this view.
    progress: Animated<f32>,
    collapsed_lines: usize,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    more: String,
    less: String,
    timing: Option<Timing>,
    /// Natural height of the body, as last measured by the recorder canvas.
    measured: std::rc::Rc<std::cell::Cell<f32>>,
    focus: FocusHandle,
    /// `None`: the toggle is a tab stop only inside a focus scope (spec/00 §12.3).
    focusable: Option<bool>,
}

/// Builds the spoiler body. A closure rather than an element, because an `AnyElement` is
/// consumed when painted and the body is rebuilt whenever the spoiler's own view renders.
type Build = std::rc::Rc<dyn Fn(&mut Window, &mut App) -> AnyElement + 'static>;

impl Spoiler {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus: cx.focus_handle(),
            focusable: None,
            id: "gr-spoiler".into(),
            content: std::rc::Rc::new(|_, _| div().into_any_element()),
            expanded: false,
            progress: Animated::new(
                0.0,
                Timing::tween(std::time::Duration::ZERO, Easing::Linear),
            ),
            collapsed_lines: 3,
            size: None,
            color: None,
            more: "Show more".into(),
            less: "Show less".into(),
            timing: None,
            measured: Default::default(),
        })
    }

    /// A stable identity for the reveal state; distinct spoilers need distinct ids.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Replace the body. The closure runs on every render, so it must be cheap (build
    /// stateless components, not stateful work).
    pub fn set_content(
        &mut self,
        build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        cx: &mut Context<Self>,
    ) {
        self.content = std::rc::Rc::new(build);
        cx.notify();
    }

    /// Opt the toggle in or out of the Tab order (`None`: only inside a focus scope).
    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        if self.focusable != focusable {
            self.focusable = focusable;
            cx.notify();
        }
    }

    pub fn set_collapsed_lines(&mut self, lines: usize, cx: &mut Context<Self>) {
        let lines = lines.max(1);
        if self.collapsed_lines != lines {
            self.collapsed_lines = lines;
            cx.notify();
        }
    }

    pub fn set_size(&mut self, size: Option<Size>, cx: &mut Context<Self>) {
        if self.size != size {
            self.size = size;
            cx.notify();
        }
    }

    pub fn set_color(&mut self, color: Option<ColorName>, cx: &mut Context<Self>) {
        if self.color != color {
            self.color = color;
            cx.notify();
        }
    }

    /// Override the reveal timing (defaults to the theme's enter timing).
    pub fn set_timing(&mut self, timing: Timing, cx: &mut Context<Self>) {
        self.timing = Some(timing);
        cx.notify();
    }

    /// Expand or collapse. Does nothing (and notifies nobody) when already in that state.
    pub fn set_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        if self.expanded == expanded {
            return;
        }
        self.expanded = expanded;
        let timing = self
            .timing
            .unwrap_or_else(|| MotionPrefs::global(cx).tokens.enter());
        self.progress.set_timing(timing);
        // One render to start sampling; the animation re-renders this view from then on.
        if self.progress.set(if expanded { 1.0 } else { 0.0 }, cx) {
            cx.notify();
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.set_expanded(!self.expanded, cx);
    }
}

impl Render for Spoiler {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let scale = theme(cx).scale;
        let colors = GuiraffeTheme::global(cx).press_colors(Variant::Subtle, self.color);
        let r = radius(None, cx);
        let cell = self.measured.clone();
        let owner = cx.entity();
        let progress = self.progress.sample(window, cx).clamp(0.0, 1.0);
        let peek = collapsed_height(size, self.collapsed_lines, scale);
        // The measurement taken last frame decides this frame's height; the canvas below
        // re-measures the body every frame and asks for one render when it changed.
        let measured = self.measured.get();
        let height = reveal_height((measured > 0.0).then_some(measured), peek, progress);
        let label = label(self.expanded, &self.more, &self.less).to_owned();
        let press = Press::new(colors)
            .radius(r)
            .focusable(self.focusable)
            .label(label.clone());
        let expanded = self.expanded;
        div()
            .id(SharedString::from(format!("gr-spoiler:{}", self.id)))
            .debug_selector({
                let id = self.id.clone();
                move || format!("gr-spoiler:{id}")
            })
            .w_full()
            .flex()
            .flex_col()
            .gap(px(size.spacing() / 2.0))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(height))
                    .overflow_hidden()
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .child((self.content)(window, cx))
                            .child(
                                // Absolute, so the recorder never affects layout; it reports
                                // the body's natural height, which the clip then crops.
                                canvas(
                                    move |bounds, _, cx| {
                                        let h = f32::from(bounds.size.height);
                                        if cell.get() != h {
                                            cell.set(h);
                                            // The new height applies on the next render; the
                                            // body's own frames do the rest of the animation.
                                            cx.defer(move |cx| cx.notify(owner.entity_id()));
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            ),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("gr-spoiler-toggle:{}", self.id)))
                    .test_support()
                    .debug_selector({
                        let id = self.id.clone();
                        move || format!("gr-spoiler-toggle:{id}")
                    })
                    .flex()
                    .items_center()
                    .h(px(size.control_height()))
                    .px(px(size.spacing() / 2.0))
                    .rounded(r)
                    .border_1()
                    .text_size(px(size.font_size() * scale))
                    .aria_expanded(expanded)
                    .press(&press, &self.focus, window)
                    .on_click(
                        cx.listener(|this: &mut Self, _: &gpui_kit::ClickEvent, _, cx| {
                            this.toggle(cx)
                        }),
                    )
                    .child(label),
            )
    }
}

impl Focusable for Spoiler {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use guiraffe_core::Size;

    use super::{collapsed_height, label, reveal_height};

    #[test]
    fn collapsed_height_scales_with_lines_and_size() {
        assert!(collapsed_height(Size::Sm, 3, 1.0) > collapsed_height(Size::Sm, 2, 1.0));
        assert!(collapsed_height(Size::Md, 3, 1.0) > collapsed_height(Size::Sm, 3, 1.0));
        assert_eq!(
            collapsed_height(Size::Sm, 0, 1.0),
            collapsed_height(Size::Sm, 1, 1.0),
            "at least one line"
        );
        assert!(collapsed_height(Size::Sm, 3, 2.0) > collapsed_height(Size::Sm, 3, 1.0));
    }

    #[test]
    fn label_follows_the_state() {
        assert_eq!(label(false, "more", "less"), "more");
        assert_eq!(label(true, "more", "less"), "less");
    }

    #[test]
    fn reveal_runs_from_peek_to_full_height() {
        assert_eq!(reveal_height(Some(120.0), 40.0, 0.0), 40.0);
        assert_eq!(reveal_height(Some(120.0), 40.0, 0.5), 80.0);
        assert_eq!(reveal_height(Some(120.0), 40.0, 1.0), 120.0);
        assert_eq!(reveal_height(Some(120.0), 40.0, 2.0), 120.0, "clamped");
        assert_eq!(reveal_height(None, 40.0, 1.0), 40.0, "unmeasured: peek");
        assert_eq!(
            reveal_height(Some(10.0), 40.0, 1.0),
            40.0,
            "content shorter than the peek never shrinks below it"
        );
    }
}
