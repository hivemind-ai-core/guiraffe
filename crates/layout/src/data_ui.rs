//! Render document layout nodes using their ordinary Rust builders.

use gpui_kit::{App, IntoElement, px};
use guiraffe_core::widget_catalog::{WidgetNode, WidgetRegistration, WidgetRegistry};

use crate::{
    Align, AspectRatio, Center, Container, Divider, DividerOrientation, Flex, FlexDirection,
    Justify, LabelPosition, ScrollArea, SimpleGrid, Space, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
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
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
                render: |node, children, _, _| {
                    AspectRatio::new(node.prop(0).as_f64().unwrap_or(1.0) as f32)
                        .children(children)
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[2],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[2]),
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
            WidgetRegistration {
                spec: widget_specs::WIDGETS[3],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[3]),
                render: |node, children, _, _| {
                    container(node).children(children).into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[4],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[4]),
                render: |node, children, _, _| flex(node).children(children).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[5],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[5]),
                render: |node, _, _, _| divider(node).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[6],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[6]),
                render: |node, children, _, _| {
                    ScrollArea::new(node.state_id())
                        .scroll_x(node.prop(0).as_bool().unwrap_or(false))
                        .scroll_y(node.prop(1).as_bool().unwrap_or(true))
                        .children(children)
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[7],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[7]),
                render: |node, children, _, _| {
                    simple_grid(node).children(children).into_any_element()
                },
            },
        ],
    );
}

fn container(node: &dyn WidgetNode) -> Container {
    let mut element = Container::new().fluid(node.prop(1).as_bool().unwrap_or(false));
    if let Some(size) = node.size(0) {
        element = element.size(size);
    }
    if let Some(padding) = node.prop(3).as_f64() {
        element = element.padding_px(padding as f32);
    } else if let Some(padding) = node.size(2) {
        element = element.padding(padding);
    }
    element
}

fn flex(node: &dyn WidgetNode) -> Flex {
    let direction = match node.prop(0).as_str() {
        Some("row-reverse") => FlexDirection::RowReverse,
        Some("column") => FlexDirection::Column,
        Some("column-reverse") => FlexDirection::ColumnReverse,
        _ => FlexDirection::Row,
    };
    let align = match node.prop(3).as_str() {
        Some("center") => Align::Center,
        Some("end") => Align::End,
        Some("stretch") => Align::Stretch,
        Some("baseline") => Align::Baseline,
        _ => Align::Start,
    };
    let justify = match node.prop(4).as_str() {
        Some("center") => Justify::Center,
        Some("end") => Justify::End,
        Some("between") => Justify::Between,
        Some("around") => Justify::Around,
        Some("evenly") => Justify::Evenly,
        _ => Justify::Start,
    };
    let mut element = Flex::new()
        .direction(direction)
        .align(align)
        .justify(justify)
        .grow(node.prop(5).as_bool().unwrap_or(false))
        .wrap(node.prop(6).as_bool().unwrap_or(false));
    if let Some(gap) = node.prop(2).as_f64() {
        element = element.gap_px(gap as f32);
    } else if let Some(gap) = node.size(1) {
        element = element.gap(gap);
    }
    element
}

fn divider(node: &dyn WidgetNode) -> Divider {
    let orientation = match node.prop(0).as_str() {
        Some("vertical") => DividerOrientation::Vertical,
        _ => DividerOrientation::Horizontal,
    };
    let position = match node.prop(2).as_str() {
        Some("start") => LabelPosition::Start,
        Some("end") => LabelPosition::End,
        _ => LabelPosition::Center,
    };
    let mut element = Divider::new()
        .orientation(orientation)
        .label_position(position);
    if let Some(label) = node.prop(1).as_str() {
        element = element.label(label.to_owned());
    }
    if let Some(color) = node.color(3) {
        element = element.color(color.shade(6).to_hsla());
    }
    if let Some(thickness) = node.prop(4).as_f64() {
        element = element.thickness(thickness as f32);
    }
    element
}

fn simple_grid(node: &dyn WidgetNode) -> SimpleGrid {
    let mut element = SimpleGrid::new();
    element.key = node.state_id();
    if let Some(cols) = node.prop(0).as_f64() {
        element = element.cols(cols.clamp(1.0, 1024.0) as u16);
    }
    if let Some(spacing) = node.prop(1).as_f64() {
        element = element.spacing(spacing as f32);
    }
    if let Some(spacing) = node.prop(2).as_f64() {
        element = element.vertical_spacing(spacing as f32);
    }
    if let Some(width) = node.prop(3).as_f64() {
        element = element.min_child_width(px(width as f32));
    }
    element
}
