//! Loading must dim the surface while leaving its contents visible.
use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_overlays::LoadingOverlay;
use guiraffe_test::snapshot::Snapshotter;
struct Backdrop;
impl Render for Backdrop {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .flex()
            .child(div().w_1_2().h_full().bg(gpui_kit::rgb(0xff0000)))
            .child(div().w_1_2().h_full().bg(gpui_kit::rgb(0x0000ff)))
            .child(LoadingOverlay::new())
    }
}
#[test]
fn loading_veils_without_hiding_the_underlying_surface() {
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut snap = Snapshotter::new()
            .expect("snapshotter")
            .with_setup(move |cx| {
                guiraffe_showcase::init(cx);
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            });
        let image = snap
            .render(size(px(300.), px(200.)), |_, cx| cx.new(|_| Backdrop))
            .expect("overlay");
        let left = image.get_pixel(40, 40).0;
        let right = image.get_pixel(560, 40).0;
        assert!(
            i16::from(left[0]) > i16::from(right[0]) + 50
                && i16::from(right[2]) > i16::from(left[2]) + 50,
            "backdrop must preserve distinct underlying colors, got {left:?} and {right:?}"
        );
        // The veil token (spec/00 §12.1) visibly fades the surface toward the body colour
        // (lighter in light mode, darker in dark mode) without hiding it.
        let body = if scheme == Scheme::Light { 255i16 } else { 36 };
        let towards_body = |c: u8| (i16::from(c) - body).abs();
        assert!(
            towards_body(right[0]) < 230,
            "overlay must veil the surface, got {right:?}"
        );
    }
}
