//! Regression guard: icons must render in headless snapshots. (They once appeared blank because
//! the snapshot helper passed an empty asset source — a GUIraffe bug, not a renderer limit.)

use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb, size,
    svg,
};
use guiraffe_test::snapshot::Snapshotter;

struct Raw;
impl Render for Raw {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0xffffff)).child(
            svg()
                .path("icons/bell.svg")
                .size(px(40.))
                .text_color(rgb(0xff0000)),
        )
    }
}

#[test]
fn headless_renderer_draws_svg_icons() {
    let img = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .render(size(px(60.), px(60.)), |_, cx| cx.new(|_| Raw))
        .unwrap_or_else(|e| panic!("{e:#}"));
    let drawn = img.pixels().filter(|p| p.0[..3] != [255, 255, 255]).count();
    assert!(drawn > 50, "svg icon pixels drawn: {drawn}");
}
