//! GPUI render tests for stateless components: build them in a real window and assert on
//! the rendered geometry and painted text. Controls get their interaction and render-count
//! tests in `interaction.rs`.

use gpui_kit::{ClickEvent, Modifiers, MouseButton};
use gpui_kit::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, div, px,
};
use guiraffe_components::{
    AvatarGroup, ColorSwatch, KbdGroup, KbdSeparator, Person, Step, StepState, Stepper,
    SwatchSelected, Timeline, TimelineItem, step_state,
};
use guiraffe_core::{ColorName, Size};

struct TimelineScene;

impl Render for TimelineScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(320.)).p(px(16.)).child(
            div().debug_selector(|| "timeline".into()).child(
                Timeline::new([
                    TimelineItem::new("Created").description("2 hours ago"),
                    TimelineItem::new("Deployed").color(ColorName::Teal),
                    TimelineItem::new("Verified"),
                ])
                .active(1),
            ),
        )
    }
}

#[gpui_kit::test]
fn timeline_lays_out_every_item(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_view, vcx) = cx.add_window_view(|_, _| TimelineScene);
    vcx.run_until_parked();
    let bounds = vcx.debug_bounds("timeline").expect("timeline is laid out");
    assert_eq!(f32::from(bounds.size.width), 288.0, "fills its column");
    assert!(
        f32::from(bounds.size.height) > 30.0,
        "three items and their descriptions stack: {bounds:?}"
    );
}

struct KbdScene;

impl Render for KbdScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().gap(px(12.0)).children([
            div()
                .debug_selector(|| "chord".into())
                .child(KbdGroup::new(["Ctrl", "K"]).size(Size::Md)),
            div().debug_selector(|| "sequence".into()).child(
                KbdGroup::new(["g", "d"])
                    .size(Size::Md)
                    .separator(KbdSeparator::Then),
            ),
        ])
    }
}

#[gpui_kit::test]
fn kbd_group_lays_out_every_key_and_the_separator(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_view, vcx) = cx.add_window_view(|_, _| KbdScene);
    vcx.run_until_parked();
    let ctrl = vcx.debug_bounds("gr-kbd:Ctrl").expect("first key");
    let k = vcx.debug_bounds("gr-kbd:K").expect("second key");
    let g = vcx.debug_bounds("gr-kbd:g").expect("sequence key");
    assert!(
        k.origin.x > ctrl.origin.x,
        "keys are laid out left to right"
    );
    assert_eq!(
        f32::from(ctrl.size.height),
        f32::from(g.size.height),
        "every keycap in a group shares a height"
    );
    let chord = vcx.debug_bounds("chord").expect("the chord");
    let sequence = vcx.debug_bounds("sequence").expect("the sequence");
    assert!(
        f32::from(sequence.size.width) > 0.0 && f32::from(chord.size.width) > 0.0,
        "both groups are laid out"
    );
}

struct SwatchScene {
    clicks: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Render for SwatchScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = self.clicks.clone();
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .debug_selector(|| "plain".into())
                    .child(ColorSwatch::new("plain", ColorName::Blue.shade(6))),
            )
            .child(
                div().debug_selector(|| "checked".into()).child(
                    ColorSwatch::new("checked", ColorName::Grape.shade(6))
                        .selected(SwatchSelected::Check),
                ),
            )
            .child(
                div()
                    .debug_selector(|| "labelled".into())
                    .child(ColorSwatch::new("labelled", ColorName::Teal.shade(6)).label("Teal")),
            )
            .child(
                div().debug_selector(|| "clickable".into()).child(
                    ColorSwatch::new("clickable", ColorName::Orange.shade(6))
                        .on_click(move |_: &ClickEvent, _, _| clicks.set(clicks.get() + 1)),
                ),
            )
    }
}

#[gpui_kit::test]
fn color_swatches_render_at_the_same_size_and_labels_extend_them(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_view, vcx) = cx.add_window_view(|_, _| SwatchScene {
        clicks: Default::default(),
    });
    vcx.run_until_parked();
    let plain = vcx.debug_bounds("plain").expect("the plain swatch");
    let checked = vcx.debug_bounds("checked").expect("the checked swatch");
    let labelled = vcx.debug_bounds("labelled").expect("the labelled swatch");
    assert_eq!(
        f32::from(plain.size.height),
        f32::from(checked.size.height),
        "check and ring do not change the size"
    );
    assert!(
        f32::from(labelled.size.width) > f32::from(plain.size.width),
        "a label makes the swatch wider"
    );
}

#[gpui_kit::test]
fn color_swatch_click_reports_once_per_press(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let c = clicks.clone();
    let (_view, vcx) = cx.add_window_view(move |_, _| SwatchScene { clicks: c });
    vcx.run_until_parked();
    let target = vcx.debug_bounds("clickable").expect("the clickable swatch");
    vcx.simulate_mouse_move(target.center(), None, Modifiers::default());
    vcx.simulate_click(target.center(), Modifiers::default());
    assert_eq!(clicks.get(), 1, "the handler ran exactly once");
    let _ = MouseButton::Left;
}

struct AvatarsScene;

impl Render for AvatarsScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap(px(16.0))
            .items_center()
            .child(
                div()
                    .debug_selector(|| "unlimited".into())
                    .child(AvatarGroup::new([
                        Person::new("Ada Lovelace"),
                        Person::new("Grace Hopper"),
                    ])),
            )
            .child(
                div().debug_selector(|| "limited".into()).child(
                    AvatarGroup::new([
                        Person::new("Ada Lovelace"),
                        Person::new("Grace Hopper"),
                        Person::new("Alan Turing"),
                        Person::new("Katherine Johnson"),
                        Person::new("Edsger Dijkstra"),
                    ])
                    .limit(3),
                ),
            )
    }
}

#[gpui_kit::test]
fn avatar_group_overlaps_and_hides_the_overflow(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_view, vcx) = cx.add_window_view(|_, _| AvatarsScene);
    vcx.run_until_parked();
    let two = vcx.debug_bounds("unlimited").expect("two avatars");
    let three = vcx.debug_bounds("limited").expect("three avatars plus +2");
    let one = f32::from(two.size.height);
    assert!(one > 0.0, "an avatar has a height");
    assert!(
        f32::from(two.size.width) < one * 2.0,
        "adjacent avatars overlap: {} vs {one}",
        f32::from(two.size.width)
    );
    assert!(
        f32::from(three.size.width) < f32::from(two.size.width) * 2.0,
        "the limit replaces two avatars with one +N chip"
    );
}

#[gpui_kit::test]
fn avatar_group_limit_of_zero_and_empty_render_without_panicking(cx: &mut TestAppContext) {
    struct Edge;
    impl Render for Edge {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .gap(px(16.0))
                .items_center()
                .child(
                    div()
                        .debug_selector(|| "empty".into())
                        .child(AvatarGroup::new(Vec::<Person>::new())),
                )
                .child(
                    div().debug_selector(|| "zero-limit".into()).child(
                        AvatarGroup::new([Person::new("Ada Lovelace")])
                            .limit(0)
                            .more(div().child("nobody")),
                    ),
                )
        }
    }
    cx.update(guiraffe_components::init);
    let (_view, vcx) = cx.add_window_view(|_, _| Edge);
    vcx.run_until_parked();
    let empty = vcx
        .debug_bounds("empty")
        .expect("an empty group is laid out");
    assert_eq!(f32::from(empty.size.width), 0.0, "nothing to draw");
    assert!(vcx.debug_bounds("zero-limit").is_some());
}

struct StepperScene {
    clicked: std::rc::Rc<std::cell::Cell<Option<usize>>>,
}

impl Render for StepperScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicked = self.clicked.clone();
        div().w(px(420.)).child(
            Stepper::new([
                Step::new("Account").description("Your details"),
                Step::new("Verify").description("Confirm email"),
                Step::new("Done"),
            ])
            .active(1)
            .on_step_click(move |i, _, _, _| clicked.set(Some(i))),
        )
    }
}

#[gpui_kit::test]
fn stepper_draws_three_distinct_states_and_reports_clicks(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let clicked = std::rc::Rc::new(std::cell::Cell::new(None));
    let c = clicked.clone();
    let (_view, vcx) = cx.add_window_view(move |_, _| StepperScene { clicked: c });
    vcx.run_until_parked();
    let first = vcx.debug_bounds("gr-step-bubble:0").expect("first bubble");
    let second = vcx.debug_bounds("gr-step-bubble:1").expect("active bubble");
    let third = vcx.debug_bounds("gr-step-bubble:2").expect("last bubble");
    assert!(first.origin.x < second.origin.x && second.origin.x < third.origin.x);
    assert_eq!(
        f32::from(first.size.width),
        f32::from(third.size.width),
        "bubbles share a size"
    );
    assert_eq!(step_state(0, 1), StepState::Completed);
    // Clicking the third step reports its index; the component stays stateless.
    let target = vcx
        .debug_bounds("gr-step:2")
        .expect("the third step column");
    vcx.simulate_mouse_move(target.center(), None, Modifiers::default());
    vcx.simulate_click(target.center(), Modifiers::default());
    assert_eq!(clicked.get(), Some(2));
}

#[gpui_kit::test]
fn stepper_with_no_steps_is_safe(cx: &mut TestAppContext) {
    struct Empty;
    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(200.))
                .child(Stepper::new(Vec::<Step>::new()).active(3))
        }
    }
    cx.update(guiraffe_components::init);
    let (_view, vcx) = cx.add_window_view(|_, _| Empty);
    vcx.run_until_parked();
    assert!(
        vcx.debug_bounds("gr-step-bubble:0").is_none(),
        "nothing to draw"
    );
}
