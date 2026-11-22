//! Adapters for document overlay nodes.

use gpui_kit::{App, IntoElement, ParentElement};
use guiraffe_core::{
    Variant,
    widget_catalog::{WidgetRegistration, WidgetRegistry},
};

use crate::{
    Affix, FloatingTooltip, LoadingOverlay,
    position::{Align, FloatingOptions, Rect, Side},
    toast::ToastPosition,
    widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
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
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
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
            WidgetRegistration {
                spec: widget_specs::WIDGETS[2],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[2]),
                render: |node, _, _, _| {
                    let number =
                        |index, default| node.prop(index).as_f64().unwrap_or(default) as f32;
                    let anchor = Rect {
                        x: number(1, 0.0),
                        y: number(2, 0.0),
                        width: number(3, 0.0),
                        height: number(4, 0.0),
                    };
                    let options = FloatingOptions {
                        side: match node.prop(5).as_str() {
                            Some("top") => Side::Top,
                            Some("left") => Side::Left,
                            Some("right") => Side::Right,
                            _ => Side::Bottom,
                        },
                        align: match node.prop(6).as_str() {
                            Some("start") => Align::Start,
                            Some("end") => Align::End,
                            _ => Align::Center,
                        },
                        gap: number(7, 4.0),
                        padding: number(8, 8.0),
                        arrow_padding: number(9, 8.0),
                        flip: node.prop(10).as_bool().unwrap_or(true),
                        shift: node.prop(11).as_bool().unwrap_or(true),
                    };
                    FloatingTooltip::new(
                        node.state_id(),
                        node.prop(0).as_str().unwrap_or("Tooltip").to_owned(),
                        anchor,
                    )
                    .options(options)
                    .into_any_element()
                },
            },
        ],
    );
}
