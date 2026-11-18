//! GPUI render tests for stateless components: build them in a real window and assert on
//! the rendered geometry and painted text. Controls get their interaction and render-count
//! tests in `interaction.rs`.

use gpui_kit::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, div, px,
};
use guiraffe_components::{KbdGroup, KbdSeparator, Timeline, TimelineItem};
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
