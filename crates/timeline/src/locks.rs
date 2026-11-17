//! Lock admission and appearance are independent.
use gpui_kit::{Div, InteractiveElement, ParentElement, Styled, div, px};
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
    pub(crate) fn decorate<E: ParentElement + Styled>(
        self,
        element: E,
        theme: &GuiraffeTheme,
    ) -> E {
        match self {
            Self::None => element,
            Self::Badge => element
                .bg(theme
                    .state(StateKind::Disabled, Variant::Light, None)
                    .background
                    .to_hsla())
                .border_color(
                    theme
                        .state(StateKind::Disabled, Variant::Light, None)
                        .border
                        .to_hsla(),
                )
                .child(
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
            Self::Hatched => {
                let ink = theme
                    .state(StateKind::Dimmed, Variant::Light, None)
                    .foreground
                    .to_hsla();
                element.overflow_hidden().child(
                    gpui_kit::canvas(
                        |_, _, _| (),
                        move |b, _, window, _| {
                            let width = f32::from(b.size.width);
                            let height = f32::from(b.size.height);
                            let mut path = gpui_kit::PathBuilder::stroke(px(1.));
                            let mut x = -height;
                            while x < width {
                                path.move_to(gpui_kit::point(
                                    b.origin.x + px(x.max(0.)),
                                    b.origin.y + px((height + x.min(0.)).max(0.)),
                                ));
                                path.line_to(gpui_kit::point(
                                    b.origin.x + px((x + height).min(width)),
                                    b.origin.y + px((x + height - width).max(0.)),
                                ));
                                x += 8.;
                            }
                            if let Ok(path) = path.build() {
                                window.paint_path(path, ink);
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            }
        }
    }
}

pub(crate) fn probe(
    kind: &'static str,
    index: usize,
    button: gpui_kit::component::button::Button,
) -> Div {
    div()
        .debug_selector(move || format!("{kind}-{index}"))
        .child(button)
}
