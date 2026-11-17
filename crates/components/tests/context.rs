//! Context-driven styling (spec/14): the same widget adapts to the layout it is in.

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, assets::IconName,
    div, prelude::*,
};
use guiraffe_components::{ActionIcon, icons::icon_box};
use guiraffe_core::{GuiraffeTheme, Size, StyleContext, provide};

struct Scene;

impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = GuiraffeTheme::global(cx).recipes.toolbar;
        div()
            .flex()
            .flex_col()
            .items_start()
            .child(
                div()
                    .debug_selector(|| "plain".into())
                    .child(ActionIcon::new("a", IconName::Star)),
            )
            .child(provide(
                toolbar,
                div()
                    .debug_selector(|| "toolbar".into())
                    .child(ActionIcon::new("b", IconName::Star)),
            ))
            .child(provide(
                toolbar,
                div()
                    .debug_selector(|| "explicit".into())
                    .child(ActionIcon::new("c", IconName::Star).size(Size::Lg)),
            ))
    }
}

#[gpui_kit::test]
fn widgets_inherit_size_from_their_layout_and_explicit_props_win(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_v, vcx) = cx.add_window_view(|_, _| Scene);
    vcx.run_until_parked();
    let mut w = |sel: &'static str| {
        vcx.debug_bounds(sel)
            .map(|b| f32::from(b.size.width))
            .unwrap_or(-1.0)
    };
    let default = StyleContext::default().control_size;
    assert_eq!(
        w("plain"),
        icon_box(default),
        "outside any layout: default control size"
    );
    assert_eq!(
        w("toolbar"),
        icon_box(Size::Xs),
        "toolbar recipe makes it compact"
    );
    assert_eq!(
        w("explicit"),
        icon_box(Size::Lg),
        "explicit prop is an escape hatch"
    );
}
