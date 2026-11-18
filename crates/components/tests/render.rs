//! GPUI render tests for stateless components: build them in a real window and assert on
//! the rendered geometry and painted text. Controls get their interaction and render-count
//! tests in `interaction.rs`.

use gpui_kit::{ClickEvent, Modifiers, MouseButton};
use gpui_kit::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, div, px,
};
use guiraffe_components::{
    ColorSwatch, KbdGroup, KbdSeparator, SwatchSelected, Timeline, TimelineItem,
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
