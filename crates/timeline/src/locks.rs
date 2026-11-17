//! Lock admission and appearance are independent.
use gpui_kit::{Div, ParentElement, Styled, div, px};
use guiraffe_core::{GuiraffeTheme, StateKind, Variant};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LockStyle {
    None,
    #[default]
    Badge,
    Dimmed,
    Hatched,
}
impl LockStyle {
    pub(crate) fn decorate(self, element: Div, theme: &GuiraffeTheme) -> Div {
        match self {
            Self::None => element,
            Self::Badge => element.child(
                div()
                    .absolute()
                    .right(px(2.))
                    .top_0()
                    .text_size(px(10.))
                    .child(
                        gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Lock)
                            .size(px(10.)),
                    ),
            ),
            Self::Dimmed => element
                .bg(theme
                    .state(StateKind::Dimmed, Variant::Light, None)
                    .background
                    .to_hsla())
                .text_color(
                    theme
                        .state(StateKind::Dimmed, Variant::Light, None)
                        .foreground
                        .to_hsla(),
                ),
            Self::Hatched => element
                .border_dashed()
                .child(div().absolute().right(px(2.)).child("╱╱")),
        }
    }
}
