//! GPUI render tests for stateless components: build them in a real window and assert on
//! the rendered geometry and painted text. Controls get their interaction and render-count
//! tests in `interaction.rs`.

use gpui_kit::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, div, px,
};
use guiraffe_components::{Timeline, TimelineItem};
use guiraffe_core::ColorName;

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
