//! Small token-styled overlay elements.
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, Variant};
/// Place last inside a relatively positioned container to intercept pointer input while loading.
#[derive(IntoElement)]
pub struct LoadingOverlay {
    visible: bool,
    size: Size,
    color: Option<ColorName>,
    variant: Variant,
}
impl Default for LoadingOverlay {
    fn default() -> Self {
        Self {
            visible: true,
            size: Size::Md,
            color: None,
            variant: Variant::Light,
        }
    }
}
impl LoadingOverlay {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            ..Self::default()
        }
    }
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
    pub fn color(mut self, color: ColorName) -> Self {
        self.color = Some(color);
        self
    }
    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }
}
impl RenderOnce for LoadingOverlay {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.visible {
            return div().into_any_element();
        }
        let theme = GuiraffeTheme::global(cx);
        let colors = theme.variant(self.color, self.variant);
        div()
            .id("guiraffe-loading-overlay")
            .test_support()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(colors.background.to_hsla())
            .rounded(px(theme.default_radius.radius()))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(self.size.control_height()))
                    .h(px(self.size.control_height()))
                    .child(Spinner::new()),
            )
            .into_any_element()
    }
}
/// Corner anchoring relative to the containing block (use inside a window-sized root for a fixed Affix).
#[derive(IntoElement)]
pub struct Affix {
    side: crate::toast::ToastPosition,
    offset: Size,
    child: AnyElement,
}
impl Affix {
    pub fn new(child: impl IntoElement) -> Self {
        Self {
            side: crate::toast::ToastPosition::BottomRight,
            offset: Size::Md,
            child: child.into_any_element(),
        }
    }
    pub fn position(mut self, position: crate::toast::ToastPosition) -> Self {
        self.side = position;
        self
    }
    pub fn offset(mut self, offset: Size) -> Self {
        self.offset = offset;
        self
    }
}
impl RenderOnce for Affix {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        use crate::toast::ToastPosition::*;
        let offset = px(self.offset.spacing());
        let mut element = div().absolute();
        element = match self.side {
            TopLeft | TopCenter | TopRight => element.top(offset),
            _ => element.bottom(offset),
        };
        element = match self.side {
            TopLeft | BottomLeft => element.left(offset),
            TopRight | BottomRight => element.right(offset),
            _ => element.left_0().w_full().flex().justify_center(),
        };
        element.child(self.child)
    }
}
