//! Controls keep native focus, keyboard and accessibility behavior while their
//! size and color recipes come from GUIraffe's inherited context and theme.
use gpui_kit::{
    App, ElementId, Styled,
    component::{
        Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
    },
    px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, StyleContext, Variant};
pub(crate) fn button(
    id: impl Into<ElementId>,
    variant: Variant,
    color: Option<ColorName>,
    cx: &App,
) -> Button {
    let theme = GuiraffeTheme::global(cx);
    let context = StyleContext::current();
    let colors = theme.variant(color, variant);
    Button::new(id)
        .with_size(px(context.control_size.control_height()))
        .rounded(px(theme.default_radius.radius()))
        .custom(
            ButtonCustomVariant::new(cx)
                .color(colors.background.to_hsla())
                .foreground(colors.foreground.to_hsla())
                .hover(colors.hover.to_hsla())
                .active(colors.hover.to_hsla()),
        )
        .border_color(colors.border.to_hsla())
}
