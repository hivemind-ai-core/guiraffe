//! Known issue (KNOWN_ISSUES.md #1): GPUI 0.3.12's headless wgpu renderer draws no monochrome
//! sprites (SVG icons) on software adapters, so icons are blank in offscreen snapshots.
//! Text renders (subpixel sprites). Run with `--ignored` to check whether it is fixed.

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
#[ignore = "KNOWN_ISSUES #1: headless renderer draws no SVG icons"]
fn headless_renderer_draws_svg_icons() {
    let img = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .render(size(px(60.), px(60.)), |_, cx| cx.new(|_| Raw))
        .unwrap_or_else(|e| panic!("{e:#}"));
    let drawn = img.pixels().filter(|p| p.0[..3] != [255, 255, 255]).count();
    assert!(drawn > 50, "svg icon pixels drawn: {drawn}");
}
