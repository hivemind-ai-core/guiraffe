//! Render document layout nodes using their ordinary Rust builders.

use gpui_kit::{App, IntoElement, px};
use guiraffe_core::{
    widget_catalog::{WidgetRegistration, WidgetRegistry},
    widget_spec::Schema,
};

use crate::{AspectRatio, Center, Space, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: &Schema {
                    props: &["min-height"],
                    events: &[],
                },
                render: |node, children, _, _| {
                    let mut center = Center::new();
                    if let Some(height) = node.prop(0).as_f64() {
                        center = center.min_height(px(height as f32));
                    }
                    center.children(children).into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[1],
                schema: &Schema {
                    props: &["ratio"],
                    events: &[],
                },
                render: |node, children, _, _| {
                    AspectRatio::new(node.prop(0).as_f64().unwrap_or(1.0) as f32)
                        .children(children)
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[2],
                schema: &Schema {
                    props: &["width", "height", "width-px", "height-px"],
                    events: &[],
                },
                render: |node, _, _, _| {
                    let mut space = Space::new();
                    if let Some(width) = node.prop(2).as_f64() {
                        space = space.width_px(width as f32);
                    } else if let Some(width) = node.size(0) {
                        space = space.width(width);
                    }
                    if let Some(height) = node.prop(3).as_f64() {
                        space = space.height_px(height as f32);
                    } else if let Some(height) = node.size(1) {
                        space = space.height(height);
                    }
                    space.into_any_element()
                },
            },
        ],
    );
}
