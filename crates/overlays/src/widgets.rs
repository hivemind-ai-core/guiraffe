//! Small token-styled overlay elements.
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{StyledExt, spinner::Spinner};
use gpui_kit::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, Variant};
/// Place last inside a relative container to occlude pointer input.
/// While mounted and visible, keyboard input is blocked for this window;
/// background focus is retained and resumes after unmount.
#[derive(IntoElement)]
pub struct LoadingOverlay {
    visible: bool,
    size: Size,
    color: Option<ColorName>,
    variant: Variant,
    style: gpui_kit::StyleRefinement,
}
impl Default for LoadingOverlay {
    fn default() -> Self {
        Self {
            visible: true,
            size: Size::Md,
            color: None,
            variant: Variant::Light,
            style: Default::default(),
        }
    }
}
impl LoadingOverlay {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
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
        // Scheme-aware scrim token (spec/00 §12.1), not a raw alpha on the variant color.
        let backdrop = theme.scrim().to_hsla();
        let surface = div()
            .id("guiraffe-loading-overlay")
            .test_support()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(backdrop)
            .rounded(px(theme.default_radius.radius()))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(self.size.control_height()))
                    .h(px(self.size.control_height()))
                    .child(Spinner::new()),
            )
            .refine_style(&self.style)
            .into_any_element();
        KeyboardShield { child: surface }.into_any_element()
    }
}
/// Corner anchoring relative to the containing block (use inside a window-sized root for a fixed Affix).
#[derive(IntoElement)]
pub struct Affix {
    side: crate::toast::ToastPosition,
    offset: Size,
    children: Vec<AnyElement>,
    style: gpui_kit::StyleRefinement,
}
impl Affix {
    pub fn new() -> Self {
        Self::default()
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
        element.children(self.children).refine_style(&self.style)
    }
}

impl Styled for LoadingOverlay {
    fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
        &mut self.style
    }
}
impl Styled for Affix {
    fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
        &mut self.style
    }
}

impl Default for Affix {
    fn default() -> Self {
        Self {
            side: crate::toast::ToastPosition::BottomRight,
            offset: Size::Md,
            children: Vec::new(),
            style: Default::default(),
        }
    }
}
impl ParentElement for Affix {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

// The subscription is element state: retained by cached frames and dropped on unmount.
// It intercepts before GPUI resolves text input or application action key bindings.
struct KeyboardShield {
    child: AnyElement,
}
impl IntoElement for KeyboardShield {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl gpui_kit::Element for KeyboardShield {
    type RequestLayoutState = gpui_kit::LayoutId;
    type PrepaintState = ();
    fn id(&self) -> Option<gpui_kit::ElementId> {
        Some("guiraffe-loading-keyboard-shield".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&gpui_kit::GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui_kit::LayoutId, Self::RequestLayoutState) {
        let child = self.child.request_layout(window, cx);
        let layout = window.request_layout(
            gpui_kit::Style {
                position: gpui_kit::Position::Absolute,
                inset: gpui_kit::Edges::all(px(0.).into()),
                ..Default::default()
            },
            [child],
            cx,
        );
        (layout, child)
    }
    fn prepaint(
        &mut self,
        id: Option<&gpui_kit::GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        _: gpui_kit::Bounds<gpui_kit::Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(id) = id else {
            return;
        };
        window.with_element_state(id, |state: Option<gpui_kit::Subscription>, window| {
            let window_id = window.window_handle().window_id();
            let subscription = state.unwrap_or_else(|| {
                cx.intercept_keystrokes(move |_, window, cx| {
                    if window.window_handle().window_id() == window_id {
                        cx.stop_propagation();
                    }
                })
            });
            self.child.prepaint(window, cx);
            ((), subscription)
        });
    }
    fn paint(
        &mut self,
        _: Option<&gpui_kit::GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        _: gpui_kit::Bounds<gpui_kit::Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
