//! ThemeIcon, ActionIcon and CloseButton.

use gpui_kit::{
    App, ClickEvent, ElementId, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled,
    Window, div, prelude::*, px,
};
use gpui_kit::assets::IconName;
use guiraffe_core::{ColorName, Size, Variant};

use crate::style::{hsla, radius, variant};

/// An icon inside a colored, variant-styled square.
#[derive(IntoElement)]
pub struct ThemeIcon {
    icon: IconName,
    size: Size,
    color: Option<ColorName>,
    variant: Variant,
    radius: Option<Size>,
    style: StyleRefinement,
}

impl ThemeIcon {
    pub fn new(icon: IconName) -> Self {
        Self {
            icon,
            size: Size::Md,
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
        let v = variant(cx, self.color, self.variant);
        let side = px(icon_box(self.size));
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
            .text_size(px(self.size.icon_size()))
            .child(self.icon);
        crate::style::refine(el, &self.style)
    }
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// An icon-only button with Mantine variants.
#[derive(IntoElement)]
pub struct ActionIcon {
    id: ElementId,
    icon: IconName,
    size: Size,
    color: Option<ColorName>,
    variant: Variant,
    disabled: bool,
    on_click: Option<OnClick>,
    style: StyleRefinement,
}

impl ActionIcon {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            icon,
            size: Size::Md,
            color: None,
            variant: Variant::Subtle,
            disabled: false,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

crate::props!(ActionIcon { size: Size => size, variant: Variant => variant, disabled: bool => disabled });

impl RenderOnce for ActionIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let v = variant(cx, self.color, self.variant);
        let side = px(icon_box(self.size));
        let hover = hsla(v.hover);
        let mut el = crate::style::refine(div(), &self.style)
            .id(self.id)
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(None, cx))
            .bg(hsla(v.background))
            .border_1()
            .border_color(hsla(v.border))
            .text_color(hsla(v.foreground))
            .text_size(px(self.size.icon_size()))
            .child(self.icon);
        if self.disabled {
            el = el.opacity(0.5);
        } else {
            el = el.cursor_pointer().hover(move |s| s.bg(hover));
            if let Some(f) = self.on_click {
                el = el.on_click(move |e, w, cx| f(e, w, cx));
            }
        }
        el
    }
}

/// The standard × button (dialogs, pills, notifications).
pub fn close_button(id: impl Into<ElementId>) -> ActionIcon {
    ActionIcon::new(id, IconName::X)
        .variant(Variant::Subtle)
        .color(ColorName::Gray)
        .size(Size::Sm)
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
