//! Loader: Mantine's three loading indicators — oval (a spinning arc), bars and dots.
//!
//! The animation lives in the loader's own view, so it re-renders alone while it moves
//! (conventions §6.1/§6.5). It requests animation frames only while it is rendered, and under
//! reduced motion it draws one static frame and requests none at all.

use std::time::Duration;

use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Entity, Hsla, IntoElement, ParentElement,
    PathBuilder, Pixels, Render, Styled, Window, canvas, div, point, px,
};
use guiraffe_core::{ColorName, Size, Variant};
use guiraffe_motion::MotionPrefs;

use crate::style::{hsla, surfaces, theme, variant};

/// Which indicator the loader draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoaderType {
    #[default]
    Oval,
    Bars,
    Dots,
}

/// Rotation of the oval at time `t`, in turns (0..1) — one full turn per period.
pub fn oval_angle(elapsed: Duration, period: Duration) -> f32 {
    let period = period.as_secs_f32();
    if !period.is_finite() || period <= 0.0 || !elapsed.as_secs_f32().is_finite() {
        return 0.0;
    }
    (elapsed.as_secs_f32() / period).rem_euclid(1.0)
}

/// Swell factor in 0..=1 for item `index` of `count` at `phase`; neighbours are out of step,
/// so the group reads as a travelling wave rather than a single pulse.
pub fn bar_scale(index: usize, count: usize, phase: f32) -> f32 {
    if count == 0 || !phase.is_finite() {
        return 1.0;
    }
    let p = (phase + index as f32 / count as f32).rem_euclid(1.0);
    0.5 + 0.5 * (p * std::f32::consts::TAU).sin()
}

/// Opacity in 0.25..=1 for dot `index` of `count` at `phase`; the dimmest dot stays visible.
pub fn dot_alpha(index: usize, count: usize, phase: f32) -> f32 {
    0.25 + 0.75 * bar_scale(index, count, phase)
}

/// Everything a loader draws.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Config {
    kind: LoaderType,
    count: usize,
    color: Hsla,
    track: Hsla,
    side: Pixels,
    thickness: Pixels,
    period: Duration,
}

/// Draw the indicator for `config` at clock `elapsed` (pure apart from painting).
fn indicator(config: Config, elapsed: Duration) -> AnyElement {
    let c = config;
    let phase = oval_angle(elapsed, c.period);
    match c.kind {
        LoaderType::Oval => div()
            .size(c.side)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // A quarter-turn visible arc, like Mantine's oval.
                        arc(bounds, f32::from(c.thickness), phase, 0.25, c.color, window);
                    },
                )
                .size_full(),
            )
            .into_any_element(),
        LoaderType::Bars | LoaderType::Dots => {
            let n = c.count.max(1) as f32;
            let side = f32::from(c.side);
            let bar_w = side / (n * 1.6 - 0.6);
            let gap = ((side - bar_w * n) / (n - 1.0).max(1.0)).max(1.0);
            div()
                .size(c.side)
                .flex()
                .items_center()
                .justify_center()
                .gap(px(gap))
                .children((0..c.count.max(1)).map(|i| {
                    if c.kind == LoaderType::Dots {
                        div()
                            .size(px(bar_w))
                            .rounded_full()
                            .bg(c.color)
                            .opacity(dot_alpha(i, c.count, phase))
                            .into_any_element()
                    } else {
                        div()
                            .w(px(bar_w))
                            .h(px(side * 0.5))
                            .rounded_full()
                            .bg(if i % 2 == 0 { c.color } else { c.track })
                            .opacity(bar_scale(i, c.count, phase))
                            .into_any_element()
                    }
                }))
                .into_any_element()
        }
    }
}

fn arc(
    bounds: Bounds<Pixels>,
    thickness: f32,
    from: f32,
    span: f32,
    color: Hsla,
    window: &mut Window,
) {
    let c = bounds.center();
    let r = (f32::from(bounds.size.width.min(bounds.size.height)) - thickness) / 2.0;
    if r <= 0.0 {
        return;
    }
    let at = |f: f32| {
        let a = f * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
    };
    let span = span.clamp(0.0, 1.0);
    let mut b = PathBuilder::stroke(px(thickness));
    b.move_to(at(from));
    b.arc_to(
        point(px(r), px(r)),
        px(0.0),
        false,
        true,
        at(from + span / 2.0),
    );
    b.arc_to(point(px(r), px(r)), px(0.0), false, true, at(from + span));
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// A loading indicator: a Control, because the animation outlives a frame.
///
/// The loader is its own view, so an animation frame re-renders only it — never the page
/// around it (conventions §6.1/§6.5). It requests frames while it is rendered and under
/// reduced motion it draws one static frame and requests none at all.
///
/// ```
/// use guiraffe_components::{Loader, LoaderType};
///
/// # fn build(cx: &mut gpui_kit::App) {
/// let dots = Loader::new(cx);
/// dots.update(cx, |loader, cx| loader.set_kind(LoaderType::Dots, cx));
/// # }
/// ```
pub struct Loader {
    kind: LoaderType,
    count: usize,
    period: Duration,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    elapsed: Duration,
    /// When the previous frame was drawn; `None` before the first one.
    last_drawn: Option<std::time::Instant>,
}

impl Loader {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            kind: LoaderType::Oval,
            count: 3,
            period: Duration::from_millis(1200),
            size: None,
            color: None,
            elapsed: Duration::ZERO,
            last_drawn: None,
        })
    }

    pub fn kind(&self) -> LoaderType {
        self.kind
    }

    pub fn set_kind(&mut self, kind: LoaderType, cx: &mut Context<Self>) {
        if self.kind != kind {
            self.kind = kind;
            cx.notify();
        }
    }

    /// Number of bars or dots (clamped to 1..=12; unused by the oval).
    pub fn set_count(&mut self, count: usize, cx: &mut Context<Self>) {
        let count = count.clamp(1, 12);
        if self.count != count {
            self.count = count;
            cx.notify();
        }
    }

    /// One full cycle takes `period`.
    pub fn set_period(&mut self, period: Duration, cx: &mut Context<Self>) {
        if self.period != period {
            self.period = period;
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

    /// The colors and metrics for the inherited style context.
    fn config(&self, cx: &App) -> Config {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        Config {
            kind: self.kind,
            count: self.count,
            color: hsla(variant(cx, self.color, Variant::Filled).background),
            track: hsla(surfaces(cx).track),
            side: px(size.control_height() * scale),
            thickness: px((size.icon_size() * 0.25 * scale).max(2.0)),
            period: self.period,
        }
    }
}

impl Render for Loader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !MotionPrefs::global(cx).reduced {
            let now = cx.background_executor().now();
            if let Some(last) = self.last_drawn {
                self.elapsed += now.saturating_duration_since(last);
            }
            self.last_drawn = Some(now);
            window.request_animation_frame();
        }
        indicator(self.config(cx), self.elapsed)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{bar_scale, dot_alpha, oval_angle};

    #[test]
    fn oval_wraps_once_per_period() {
        let period = Duration::from_millis(1000);
        assert!((oval_angle(Duration::ZERO, period) - 0.0).abs() < 1e-6);
        assert!((oval_angle(Duration::from_millis(500), period) - 0.5).abs() < 1e-6);
        assert!((oval_angle(Duration::from_millis(1500), period) - 0.5).abs() < 1e-6);
        assert_eq!(oval_angle(Duration::from_millis(1), Duration::ZERO), 0.0);
    }

    #[test]
    fn bars_are_out_of_phase_and_bounded() {
        let (a, b) = (bar_scale(0, 3, 0.0), bar_scale(1, 3, 0.0));
        assert!(
            (a - b).abs() > 0.1,
            "a travelling wave, not a pulse: {a} vs {b}"
        );
        for phase in [0.0, 0.1, 0.5, 0.75, 1.5] {
            let v = bar_scale(1, 4, phase);
            assert!((0.0..=1.0).contains(&v), "{phase}: {v}");
        }
        assert_eq!(bar_scale(0, 0, 0.5), 1.0, "an empty group does not divide");
    }

    #[test]
    fn dot_alpha_stays_readable() {
        for phase in [0.0, 0.25, 0.5, 0.9] {
            let a = dot_alpha(1, 3, phase);
            assert!((0.25..=1.0).contains(&a), "{a}");
        }
    }
}
