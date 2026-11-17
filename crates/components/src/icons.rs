//! ThemeIcon, ActionIcon and CloseButton.

use gpui_kit::assets::IconName;
use gpui_kit::{
    App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, TestSupportExt, Window, div, prelude::*, px,
};
use guiraffe_core::{
    A11yDisabledExt, ColorName, GuiraffeTheme, Press, PressableExt, Size, Variant, keyed_focus,
};

use crate::style::{hsla, radius, variant};

/// An icon inside a colored, variant-styled square.
#[derive(IntoElement)]
pub struct ThemeIcon {
    icon: IconName,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    variant: Variant,
    radius: Option<Size>,
    style: StyleRefinement,
}

impl ThemeIcon {
    pub fn new(icon: IconName) -> Self {
        Self {
            icon,
            size: None,
            color: None,
            variant: Variant::Filled,
            radius: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
}

crate::props!(ThemeIcon { size: Size => size, variant: Variant => variant });
crate::styled!(ThemeIcon, ActionIcon);

impl ThemeIcon {
    pub fn radius(mut self, r: Size) -> Self {
        self.radius = Some(r);
        self
    }
}

/// Box side for icon containers (Mantine's ThemeIcon/ActionIcon sizes are ~control height × 0.67).
pub fn icon_box(size: Size) -> f32 {
    (size.control_height() * 0.67).round()
}

impl RenderOnce for ThemeIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let v = variant(cx, self.color, self.variant);
        let side = px(icon_box(size));
        let el = div()
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(self.radius, cx))
            .bg(hsla(v.background))
            .border_1()
            .border_color(hsla(v.border))
            .text_color(hsla(v.foreground))
            .text_size(px(size.icon_size()))
            .child(self.icon);
        crate::style::refine(el, &self.style)
    }
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// An icon-only button with Mantine variants. Behaves like a Kit `Button` (spec/00 §12):
/// hover / pressed / disabled state tokens, pointer cursor, a press never takes focus,
/// opt-in tab stop ([`guiraffe_core::focus_scope`] or [`ActionIcon::focusable`]) with a
/// focus-visible ring and Enter/Space activation, and an AccessKit `Button` named by
/// [`ActionIcon::label`] (defaults to the icon name).
#[derive(IntoElement)]
pub struct ActionIcon {
    id: ElementId,
    icon: IconName,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    /// `None`: quiet regions (toolbars, tables) get Subtle, others Light.
    variant: Option<Variant>,
    disabled: bool,
    focusable: Option<bool>,
    label: Option<SharedString>,
    on_click: Option<OnClick>,
    style: StyleRefinement,
}

impl ActionIcon {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            icon,
            size: None,
            color: None,
            variant: None,
            disabled: false,
            focusable: None,
            label: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
    /// Accessible name (icon-only buttons need one).
    pub fn label(mut self, l: impl Into<SharedString>) -> Self {
        self.label = Some(l.into());
        self
    }
    /// Opt into Tab navigation outside a focus scope (or out of it inside one).
    pub fn focusable(mut self, v: bool) -> Self {
        self.focusable = Some(v);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

crate::props!(ActionIcon { size: Size => size, variant: Variant => variant, disabled: bool => disabled });

impl RenderOnce for ActionIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let v_kind = self.variant.unwrap_or(match ctx.emphasis {
            guiraffe_core::Emphasis::Quiet => Variant::Subtle,
            guiraffe_core::Emphasis::Default => Variant::Light,
        });
        let colors = GuiraffeTheme::global(cx).press_colors(v_kind, self.color);
        let side = px(icon_box(size));
        let focus = keyed_focus(self.id.clone(), window, cx);
        let label = self
            .label
            .unwrap_or_else(|| SharedString::from(format!("{:?}", self.icon)));
        let press = Press::new(colors)
            .disabled(self.disabled)
            .focusable(self.focusable)
            .label(label);
        let disabled = self.disabled;
        let on_click = self.on_click;
        let el = div()
            .id(self.id)
            .a11y_disabled(disabled)
            .test_support()
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(None, cx))
            .border_1()
            .text_size(px(size.icon_size()))
            .child(self.icon)
            .press(&press, &focus)
            .when_some(on_click, |el, f| el.on_press(disabled, f));
        crate::style::refine(el, &self.style)
    }
}

/// The standard × button (dialogs, pills, notifications), named "Close".
pub fn close_button(id: impl Into<ElementId>) -> ActionIcon {
    ActionIcon::new(id, IconName::X)
        .variant(Variant::Subtle)
        .color(ColorName::Gray)
        .size(Size::Sm)
        .label("Close")
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_box_grows_with_size() {
        use guiraffe_core::Size;
        let v: Vec<f32> = Size::ALL.iter().map(|s| super::icon_box(*s)).collect();
        assert!(v.windows(2).all(|w| w[0] < w[1]), "{v:?}");
    }
}
