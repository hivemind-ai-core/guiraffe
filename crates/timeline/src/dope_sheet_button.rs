//! A channel button has a fixed size and one centered icon. Keep the native
//! button's hitbox, tooltip, styles and accessibility on its single layout node.
use gpui_kit::{
    App, Bounds, Div, Element, ElementId, GlobalElementId, Hitbox, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, Pixels, Role, SharedString, Stateful, Window,
    accesskit, component::resolve_icon_path, size,
};
use guiraffe_core::control::A11yDisabled;

pub(super) struct ChannelButton {
    pub inner: A11yDisabled<Stateful<Div>>,
    pub icon: SharedString,
    pub foreground: gpui_kit::Hsla,
    pub hover_foreground: Option<gpui_kit::Hsla>,
}

impl IntoElement for ChannelButton {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ChannelButton {
    type RequestLayoutState = <Stateful<Div> as Element>::RequestLayoutState;
    type PrepaintState = Option<Hitbox>;
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.inner)
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn a11y_role(&self) -> Option<Role> {
        self.inner.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let color = self
            .hover_foreground
            .filter(|_| hitbox.as_ref().is_some_and(|h| h.is_hovered(window)))
            .unwrap_or(self.foreground);
        self.inner.interactivity().paint(
            id,
            inspector,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |_, window, cx| {
                let side = window.rem_size() * 0.75;
                let rect = Bounds::centered_at(bounds.center(), size(side, side));
                let _ = window.paint_svg(
                    rect,
                    resolve_icon_path(self.icon.clone(), cx),
                    None,
                    Default::default(),
                    color,
                    cx,
                );
            },
        );
    }
}
