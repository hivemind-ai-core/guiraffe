//! Stepper: a horizontal progress rail with completed, active and upcoming steps.
//!
//! GPUI Controls ships a `Stepper`, but it draws the active step exactly like the completed ones
//! (both take the primary background) and has no descriptions, so this fills that gap. It is a
//! stateless *component*: the caller owns the active index and receives clicks.

use gpui_kit::{
    AnyElement, App, ClickEvent, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StyleRefinement, Styled, TestSupportExt, Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Press, PressableExt, Size, Variant, keyed_focus};

use crate::color_swatch::check_color;
use crate::style::{hsla, surfaces, theme, variant};

/// Where a step sits relative to the stepper's active index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Completed,
    Active,
    Upcoming,
}

/// The state of step `index` when step `active` is current.
pub fn step_state(index: usize, active: usize) -> StepState {
    match index.cmp(&active) {
        std::cmp::Ordering::Less => StepState::Completed,
        std::cmp::Ordering::Equal => StepState::Active,
        std::cmp::Ordering::Greater => StepState::Upcoming,
    }
}

/// Whether the connector leaving step `index` is complete (it is, up to the active step).
pub fn connector_complete(index: usize, active: usize) -> bool {
    index < active
}

/// One step: a label, an optional description, and an optional icon in the bubble.
#[derive(IntoElement)]
pub struct Step {
    label: SharedString,
    description: Option<SharedString>,
    icon: Option<AnyElement>,
    style: StyleRefinement,
}

impl Step {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            description: None,
            icon: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Replace the bubble's number with content (an icon, a check mark).
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }
}

crate::styled!(Step);

impl RenderOnce for Step {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        // Steps are only meaningful inside a `Stepper`; alone they render their text.
        crate::style::refine(
            div()
                .flex()
                .flex_col()
                .child(self.label)
                .when_some(self.description, |el, d| el.child(d)),
            &self.style,
        )
    }
}

type OnStepClick = Box<dyn Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static>;

/// Diameter of a step's number bubble at `size`.
pub fn bubble_size(size: Size) -> f32 {
    (size.control_height() * 0.78).round()
}

/// Thickness of the rail between steps.
pub fn rail_thickness(size: Size) -> f32 {
    (size.radius() / 2.0).max(1.0)
}

/// A horizontal stepper: numbered bubbles on a rail, labelled steps beneath them.
///
/// ```
/// use guiraffe_components::{Step, Stepper};
///
/// let stepper = Stepper::new([
///     Step::new("Account").description("Your details"),
///     Step::new("Verify").description("Confirm email"),
///     Step::new("Done"),
/// ])
/// .active(1);
/// ```
#[derive(IntoElement)]
pub struct Stepper {
    steps: Vec<Step>,
    active: usize,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    on_step_click: Option<OnStepClick>,
    /// `None`: clickable steps are tab stops only inside a focus scope (spec/00 §12.3).
    focusable: Option<bool>,
    style: StyleRefinement,
}

impl Stepper {
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            active: 0,
            size: None,
            color: None,
            on_step_click: None,
            focusable: None,
            style: StyleRefinement::default(),
        }
    }

    /// Index of the current step; earlier steps are complete, later ones upcoming. Clamped.
    pub fn active(mut self, active: usize) -> Self {
        self.active = active;
        self
    }

    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }

    /// Opt clickable steps in or out of the Tab order (`None`: only inside a focus scope).
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = Some(focusable);
        self
    }

    /// Called with the clicked step's index (the component stays stateless).
    pub fn on_step_click(
        mut self,
        f: impl Fn(usize, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_step_click = Some(Box::new(f));
        self
    }
}

crate::props!(Stepper { size: Size => size });
crate::styled!(Stepper);

impl RenderOnce for Stepper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let colors = GuiraffeTheme::global(cx).press_colors(Variant::Subtle, self.color);
        let r = crate::style::radius(None, cx);
        let focusable = self.focusable;
        let scale = theme(cx).scale;
        let s = surfaces(cx);
        let bubble = px(bubble_size(size) * scale);
        let rail = px(rail_thickness(size));
        let v_filled = variant(cx, self.color, Variant::Filled);
        let v_light = variant(cx, self.color, Variant::Light);
        let done_bg = hsla(v_filled.background);
        let done_fg = hsla(check_color(v_filled.background));
        let active_border = hsla(v_light.foreground);
        let active_fg = hsla(v_light.foreground);
        let upcoming = hsla(s.dimmed);
        let label_text = px(size.font_size() * scale);
        let count = self.steps.len();
        let active = if count == 0 {
            0
        } else {
            self.active.min(count - 1)
        };
        let click: Option<std::rc::Rc<OnStepClick>> = self.on_step_click.map(std::rc::Rc::from);
        let gap_v = size.spacing() * 0.75;
        let gap = px(gap_v);
        let rows: Vec<AnyElement> = self
            .steps
            .into_iter()
            .enumerate()
            .map(|(i, step)| {
                let state = step_state(i, active);
                let number = match state {
                    StepState::Completed => "✓".to_string(),
                    _ => format!("{}", i + 1),
                };
                let content = step.icon.map(|icon| (icon, number.clone()));
                let name = step.label.clone();
                let bubble_el = div()
                    .debug_selector(move || format!("gr-step-bubble:{i}"))
                    .size(bubble)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_size(px(size.font_size() * scale * 0.85))
                    .map(|el| match state {
                        StepState::Completed => el.bg(done_bg).text_color(done_fg),
                        StepState::Active => el
                            .bg(hsla(s.body))
                            .border_2()
                            .border_color(active_border)
                            .text_color(active_fg),
                        StepState::Upcoming => el
                            .bg(hsla(s.track))
                            .border_1()
                            .border_color(hsla(s.border))
                            .text_color(upcoming),
                    })
                    .map(|el| match content {
                        // A custom icon replaces the number (a completed step's ✓ included).
                        Some((icon, _)) => el.child(icon),
                        None => el.child(number.clone()),
                    });
                let label_el = div()
                    .flex()
                    .flex_col()
                    .gap(px(size.spacing() / 4.0))
                    .text_size(label_text)
                    .map(|el| match state {
                        StepState::Upcoming => el.text_color(upcoming),
                        StepState::Completed => el.text_color(hsla(s.text)),
                        StepState::Active => el.text_color(active_fg),
                    })
                    .child(step.label)
                    .when_some(step.description, |el, d| {
                        el.child(
                            div()
                                .text_size(px(Size::Sm.font_size() * scale))
                                .text_color(hsla(s.dimmed))
                                .child(d),
                        )
                    });
                // The rail runs from this bubble to the next one, behind the label column.
                let rail_el = div()
                    .absolute()
                    .top(px(
                        bubble_size(size) * scale / 2.0 - rail_thickness(size) / 2.0
                    ))
                    .left(bubble)
                    // Reach into the inter-column gap, so the rail reads as one line.
                    .right(px(-gap_v))
                    .h(rail)
                    .bg(hsla(if connector_complete(i, active) {
                        v_filled.background
                    } else {
                        s.border
                    }));
                // The rail is a child *before* the bubble, so it paints underneath it.
                let mut column = div()
                    .relative()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(size.spacing() / 2.0));
                if i + 1 < count {
                    column = column.child(rail_el);
                }
                let column = column.child(bubble_el).child(label_el);
                let column = div()
                    .flex_1()
                    .flex()
                    .items_start()
                    .child(column.flex_1())
                    .when(i + 1 < count, |el| el.child(div().w(gap)));
                match click.clone() {
                    Some(f) => {
                        let id = SharedString::from(format!("gr-step:{i}"));
                        let focus = keyed_focus(id.clone(), window, cx);
                        let press = Press::new(colors)
                            .radius(r)
                            .paint(false)
                            .focusable(focusable)
                            .selected(state == StepState::Active)
                            .label(name);
                        let hover = colors.hover.background.to_hsla();
                        column
                            .id(id)
                            .test_support()
                            .debug_selector(move || format!("gr-step:{i}"))
                            .rounded(r)
                            .hover(move |el| el.bg(hover))
                            .press(&press, &focus, window)
                            .on_click(move |event, window: &mut Window, cx: &mut App| {
                                f(i, event, window, cx)
                            })
                            .into_any_element()
                    }
                    None => column.into_any_element(),
                }
            })
            .collect();
        crate::style::refine(div().flex().items_start(), &self.style).children(rows)
    }
}

#[cfg(test)]
mod tests {
    use guiraffe_core::Size;

    use super::{StepState, bubble_size, connector_complete, rail_thickness, step_state};

    #[test]
    fn three_states_meet_at_the_active_step() {
        assert_eq!(step_state(0, 1), StepState::Completed);
        assert_eq!(step_state(1, 1), StepState::Active);
        assert_eq!(step_state(2, 1), StepState::Upcoming);
        assert_eq!(step_state(0, 0), StepState::Active, "first step active");
        assert_eq!(step_state(5, usize::MAX), StepState::Completed, "clamped");
    }

    #[test]
    fn the_rail_completes_up_to_the_active_step() {
        assert!(
            connector_complete(0, 1),
            "rail into the active step is done"
        );
        assert!(
            !connector_complete(1, 1),
            "rail out of the active step is not"
        );
        assert!(!connector_complete(2, 1));
    }

    #[test]
    fn metrics_scale_with_the_size() {
        for w in Size::ALL.windows(2) {
            assert!(bubble_size(w[1]) > bubble_size(w[0]));
            assert!(rail_thickness(w[1]) >= rail_thickness(w[0]));
        }
        assert!(rail_thickness(Size::Xs) >= 1.0, "a rail is always visible");
    }
}
