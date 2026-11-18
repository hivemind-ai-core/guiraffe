//! Burger: the animated menu toggle. Three bars rotate into an × (Mantine's `Burger`).
//!
//! The control owns an [`Animated`] open amount and samples it in its own view, so an animation
//! frame re-renders only the burger. Reduced motion jumps to the end state.

use gpui_kit::{
    App, AppContext, ClickEvent, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, TestSupportExt, Window, canvas, div,
    prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Press, PressableExt, Size, Variant};
use guiraffe_motion::{Animated, Easing, MotionPrefs, Timing};

use crate::style::{hsla, radius, theme};

/// The geometry of the three bars at open amount `t` (0 = ☰, 1 = ×), in px.
///
/// `(offset, rotation_degrees, scale_x)` for the top, middle and bottom bars, where `offset`
/// is in half-gaps from the centre: closed is `-1, 0, +1`. Rotating starts once the outer bars
/// have travelled, so they never cross while horizontal; the middle bar shrinks to nothing
/// instead of popping out.
pub fn bars(t: f32) -> [(f32, f32, f32); 3] {
    let t = t.clamp(0.0, 1.0);
    let rot = ((t - 0.4) / 0.6).clamp(0.0, 1.0);
    [
        (-(1.0 - t), 45.0 * rot, 1.0),
        (0.0, 0.0, 1.0 - t),
        (1.0 - t, -45.0 * rot, 1.0),
    ]
}

type OnChange = Box<dyn Fn(bool, &mut Window, &mut App) + 'static>;

/// A menu toggle whose bars animate into a close mark.
///
/// ```
/// use guiraffe_components::Burger;
///
/// # fn build(cx: &mut gpui_kit::App) {
/// let burger = Burger::new(cx);
/// burger.update(cx, |b, cx| b.set_open(true, cx));
/// # }
/// ```
pub struct Burger {
    id: String,
    open: bool,
    /// 0..=1 as animated; sampling it re-renders only this view.
    amount: Animated<f32>,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    timing: Option<Timing>,
    /// Called with the new state after a click.
    on_change: Option<OnChange>,
    focus: FocusHandle,
    /// `None`: a tab stop only inside a focus scope (spec/00 §12.3).
    focusable: Option<bool>,
    /// Accessible names for the closed / open states.
    labels: (SharedString, SharedString),
}

impl Burger {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus: cx.focus_handle(),
            focusable: None,
            labels: ("Open navigation".into(), "Close navigation".into()),
            id: "gr-burger".into(),
            open: false,
            amount: Animated::new(
                0.0f32,
                Timing::tween(std::time::Duration::ZERO, Easing::Linear),
            ),
            size: None,
            color: None,
            timing: None,
            on_change: None,
        })
    }

    /// A stable identity; distinct burgers need distinct ids only for tests and debug selectors.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_color(&mut self, color: Option<ColorName>, cx: &mut Context<Self>) {
        if self.color != color {
            self.color = color;
            cx.notify();
        }
    }

    pub fn set_size(&mut self, size: Option<Size>, cx: &mut Context<Self>) {
        if self.size != size {
            self.size = size;
            cx.notify();
        }
    }

    /// Opt in or out of the Tab order (`None`: only inside a focus scope).
    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        if self.focusable != focusable {
            self.focusable = focusable;
            cx.notify();
        }
    }

    /// Accessible names announced while closed and while open.
    pub fn set_labels(
        &mut self,
        closed: impl Into<SharedString>,
        open: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.labels = (closed.into(), open.into());
        cx.notify();
    }

    pub fn on_change(&mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) {
        self.on_change = Some(Box::new(f));
    }

    /// Set the open state; no-op (and no notify) when unchanged.
    pub fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open == open {
            return;
        }
        self.open = open;
        let timing = self
            .timing
            .unwrap_or_else(|| MotionPrefs::global(cx).tokens.fast());
        self.amount.set_timing(timing);
        // One render starts the sampling; the animation drives this view from then on.
        if self.amount.set(if open { 1.0 } else { 0.0 }, cx) {
            cx.notify();
        }
    }

    /// Set the state without animating (initial state, restored state, reduced motion).
    pub fn set_open_instant(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open != open || *self.amount.target() != if open { 1.0 } else { 0.0 } {
            self.open = open;
            self.amount.jump(if open { 1.0 } else { 0.0 });
            cx.notify();
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.set_open(!self.open, cx);
    }
}

/// Bar metrics for a control of `side` px: (bar thickness, half the distance between bars).
pub fn bar_metrics(side: f32) -> (f32, f32) {
    ((side * 0.095).max(1.5), (side * 0.21).max(1.5))
}

impl Render for Burger {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let side = size.control_height() * scale;
        let t = self.amount.sample(window, cx).clamp(0.0, 1.0);
        let colors = GuiraffeTheme::global(cx).press_colors(Variant::Subtle, self.color);
        let color = hsla(colors.rest.foreground);
        let (thickness, half_gap) = bar_metrics(side);
        let geometry = bars(t);
        let id = self.id.clone();
        let r = radius(None, cx);
        let label = if self.open {
            self.labels.1.clone()
        } else {
            self.labels.0.clone()
        };
        let press = Press::new(colors)
            .radius(r)
            .focusable(self.focusable)
            .label(label);
        div()
            .id(SharedString::from(format!("gr-burger:{id}")))
            .test_support()
            .debug_selector(move || format!("gr-burger:{id}"))
            .relative()
            .size(px(side))
            .rounded(r)
            .border_1()
            .aria_expanded(self.open)
            .press(&press, &self.focus, window)
            .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, window, cx| {
                this.toggle(cx);
                if let Some(f) = this.on_change.take() {
                    f(this.open, window, cx);
                    this.on_change = Some(f);
                }
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let c = bounds.center();
                        let length = f32::from(bounds.size.width) * 0.5;
                        for (offset, rotate, scale_x) in geometry {
                            if scale_x <= 0.01 {
                                continue;
                            }
                            let cy = c.y + px(offset * half_gap);
                            let angle = rotate.to_radians();
                            let half = length * scale_x / 2.0;
                            let (dx, dy) = (half * angle.cos(), half * angle.sin());
                            let mut b = gpui_kit::PathBuilder::stroke(px(thickness));
                            b.move_to(gpui_kit::point(c.x - px(dx), cy - px(dy)));
                            b.line_to(gpui_kit::point(c.x + px(dx), cy + px(dy)));
                            if let Ok(path) = b.build() {
                                window.paint_path(path, color);
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}

impl Focusable for Burger {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{bar_metrics, bars};

    #[test]
    fn closed_state_is_three_parallel_bars() {
        let closed = bars(0.0);
        assert_eq!(closed, [(-1.0, 0.0, 1.0), (0.0, 0.0, 1.0), (1.0, 0.0, 1.0)]);
    }

    #[test]
    fn open_state_is_a_cross() {
        let open = bars(1.0);
        assert_eq!(open[0].1, 45.0, "top bar rotated one way");
        assert_eq!(open[2].1, -45.0, "bottom bar the other");
        assert_eq!(open[1].2, 0.0, "the middle bar is gone");
        assert_eq!(open[0].0, 0.0, "top bar sits on the middle");
        assert_eq!(open[2].0, 0.0, "bottom bar sits on the middle");
    }

    #[test]
    fn rotation_waits_until_the_bars_have_moved() {
        assert_eq!(bars(0.2)[0].1, 0.0, "no rotation while they travel");
        assert!(
            bars(0.7)[0].1 > 0.0 && bars(0.7)[0].1 < 45.0,
            "mid-rotation"
        );
        assert_eq!(bars(2.0)[0], (0.0, 45.0, 1.0), "clamped at open");
        assert_eq!(bars(-1.0), bars(0.0), "clamped at closed");
    }

    #[test]
    fn metrics_scale_with_the_control() {
        let (h1, t1) = bar_metrics(30.0);
        let (h2, t2) = bar_metrics(60.0);
        assert!(h2 > h1 && t2 > t1);
        assert!(h1 >= 1.5, "a bar is always visible: {h1}");
    }
}
