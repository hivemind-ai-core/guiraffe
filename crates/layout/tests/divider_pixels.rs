//! A vertical divider inside a Group is actually drawn (it used to collapse to zero height).
use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb, size,
};
use guiraffe_layout::{Divider, Group};
use guiraffe_test::snapshot::Snapshotter;

struct Scene;
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0xffffff)).child(
            Group::new()
                .child(div().w(px(40.)).h(px(32.)).bg(rgb(0xdddddd)))
                .child(Divider::vertical().color(rgb(0xff0000)).thickness(2.))
                .child(div().w(px(40.)).h(px(32.)).bg(rgb(0xdddddd))),
        )
    }
}

#[test]
fn a_vertical_divider_in_a_group_is_drawn_the_full_row_height() {
    let mut s = Snapshotter::new().unwrap_or_else(|e| panic!("{e}"));
    let img = s
        .render(size(px(200.), px(60.)), |_, cx| cx.new(|_| Scene))
        .unwrap_or_else(|e| panic!("{e:#}"));
    let red = img
        .pixels()
        .filter(|p| p.0[0] > 200 && p.0[1] < 60 && p.0[2] < 60)
        .count();
    // 2px × 32px at scale 2 → 4 × 64 = 256 device pixels; allow antialiasing slack.
    assert!(red >= 200, "vertical divider drew only {red} red pixels");
}
