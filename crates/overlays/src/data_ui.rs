//! Adapters for document overlay nodes.

use gpui_kit::{App, IntoElement, ParentElement};
use guiraffe_core::{
    Variant,
    widget_catalog::{WidgetRegistration, WidgetRegistry},
    widget_spec::Schema,
};

use crate::{Affix, LoadingOverlay, toast::ToastPosition, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: &Schema {
                    props: &["position", "offset"],
                    events: &[],
                },
                render: |node, children, _, _| {
                    let position = match node.prop(0).as_str() {
                        Some("top-left") => ToastPosition::TopLeft,
                        Some("top-center") => ToastPosition::TopCenter,
                        Some("top-right") => ToastPosition::TopRight,
                        Some("bottom-left") => ToastPosition::BottomLeft,
                        Some("bottom-center") => ToastPosition::BottomCenter,
                        _ => ToastPosition::BottomRight,
                    };
                    let mut affix = Affix::new().position(position);
                    if let Some(offset) = node.size(1) {
                        affix = affix.offset(offset);
                    }
                    affix.children(children).into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[1],
                schema: &Schema {
                    props: &["visible", "size", "color", "variant"],
                    events: &[],
                },
                render: |node, _, _, _| {
                    let mut overlay =
                        LoadingOverlay::new().visible(node.prop(0).as_bool().unwrap_or(true));
                    if let Some(size) = node.size(1) {
                        overlay = overlay.size(size);
                    }
                    if let Some(color) = node.color(2) {
                        overlay = overlay.color(color);
                    }
                    let variant = match node.prop(3).as_str() {
                        Some("filled") => Variant::Filled,
                        Some("outline") => Variant::Outline,
                        Some("subtle") => Variant::Subtle,
                        Some("transparent") => Variant::Transparent,
                        Some("white") => Variant::White,
                        Some("default") => Variant::Default,
                        _ => Variant::Light,
                    };
                    overlay.variant(variant).into_any_element()
                },
            },
        ],
    );
}
