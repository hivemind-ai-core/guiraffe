//! Controlled floating tooltip; anchor coordinates are window logical pixels.
use crate::position::{FloatingOptions, Rect};
use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, px,
};
use gpui_kit::{InteractiveElement, div};
use guiraffe_core::{GuiraffeTheme, Size};
#[derive(IntoElement)]
pub struct FloatingTooltip {
    id: ElementId,
    text: SharedString,
    anchor: Rect,
    options: FloatingOptions,
}
impl FloatingTooltip {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>, anchor: Rect) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            anchor,
            options: FloatingOptions {
                gap: Size::Sm.radius(),
                padding: Size::Md.radius(),
                arrow_padding: Size::Md.radius(),
                ..Default::default()
            },
        }
    }
    pub fn options(mut self, options: FloatingOptions) -> Self {
        self.options = options;
        self
    }
}
impl RenderOnce for FloatingTooltip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surface = theme.surfaces();
        let content = div().id(self.id).test_support().child(
            gpui_kit::base::Tooltip::new("tooltip-surface")
                .bg(surface.surface.to_hsla())
                .border_1()
                .border_color(surface.border.to_hsla())
                .shadow_sm()
                .text_color(surface.text.to_hsla())
                .p(px(Size::Xs.spacing()))
                .rounded(px(theme.default_radius.radius()))
                .child(self.text),
        );
        FloatingPositioner {
            anchor: self.anchor,
            options: self.options,
            child: content.into_any_element(),
        }
    }
}
struct FloatingPositioner {
    anchor: Rect,
    options: FloatingOptions,
    child: gpui_kit::AnyElement,
}
impl IntoElement for FloatingPositioner {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl gpui_kit::Element for FloatingPositioner {
    type RequestLayoutState = gpui_kit::LayoutId;
    type PrepaintState = bool;
    fn id(&self) -> Option<ElementId> {
        None
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
                ..Default::default()
            },
            [child],
            cx,
        );
        (layout, child)
    }
    fn prepaint(
        &mut self,
        _: Option<&gpui_kit::GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let measured = window.layout_bounds(*child);
        let viewport = window.viewport_size();
        let Some(resolved) = crate::position::position(
            self.anchor,
            (measured.size.width.into(), measured.size.height.into()),
            Rect {
                x: 0.,
                y: 0.,
                width: viewport.width.into(),
                height: viewport.height.into(),
            },
            self.options,
        ) else {
            return false;
        };
        let offset = gpui_kit::point(
            px(resolved.x) - bounds.origin.x,
            px(resolved.y) - bounds.origin.y,
        );
        window.with_element_offset(offset, |window| {
            self.child.prepaint(window, cx);
        });
        true
    }
    fn paint(
        &mut self,
        _: Option<&gpui_kit::GlobalElementId>,
        _: Option<&gpui_kit::InspectorElementId>,
        _: gpui_kit::Bounds<gpui_kit::Pixels>,
        _: &mut Self::RequestLayoutState,
        visible: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *visible {
            self.child.paint(window, cx);
        }
    }
}
