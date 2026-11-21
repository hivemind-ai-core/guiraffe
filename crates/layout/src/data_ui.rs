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
            WidgetRegistration {
                spec: &widget_specs::SPLITTER,
                schema: guiraffe_core::widget_schema!(&widget_specs::SPLITTER),
                render: |node, children, window, cx| {
                    use crate::{Splitter, SplitterAxis, SplitterState};
                    use guiraffe_core::widget_value::Value;

                    let state = window
                        .use_keyed_state(node.state_id(), cx, |_, cx| SplitterState::new(cx))
                        .read(cx)
                        .clone();
                    let mut splitter = Splitter::new(&state)
                        .axis(if node.prop(0).as_str() == Some("vertical") {
                            SplitterAxis::Vertical
                        } else {
                            SplitterAxis::Horizontal
                        })
                        .panes(children);
                    let size = |value: &Value| {
                        value
                            .as_f64()
                            .map(|value| value.clamp(0.0, f64::from(f32::MAX)) as f32)
                    };
                    let minimum = size(node.prop(3)).unwrap_or_else(crate::theme::panel_min_size);
                    splitter = splitter.min_size(px(minimum));
                    if let Some(maximum) = size(node.prop(4)) {
                        splitter = splitter.max_size(px(maximum.max(minimum)));
                    }
                    let sizes = node.prop(1);
                    for index in 0..sizes.list_len().unwrap_or_default() {
                        if let Some(value) = sizes.list_get(index).and_then(size) {
                            splitter = splitter.pane_size(index, px(value));
                        }
                    }
                    if let Some(value) = size(node.prop(2)) {
                        splitter = splitter.first_size(px(value));
                    }
                    if let Some(key) = node.prop(5).as_str() {
                        splitter = splitter.persist_key(key.to_owned());
                    }
                    if let Some(handler) = node.handler(0) {
                        splitter = splitter.on_resize(move |sizes, window, cx| {
                            let values = sizes
                                .iter()
                                .map(|size| Value::Num(f64::from(f32::from(*size))))
                                .collect::<Vec<_>>();
                            handler(Value::List(values.into()), window, cx);
                        });
                    }
                    splitter.into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::RESPONSIVE_GRID,
                schema: guiraffe_core::widget_schema!(&widget_specs::RESPONSIVE_GRID),
                render: |node, children, _, _| {
                    use crate::{Col, ContainerWidth, Grid, Span};

                    let mut grid = Grid::new()
                        .with_key(node.state_id())
                        .columns(integer(node.prop(0)).unwrap_or(12))
                        .grow(node.prop(4).as_bool().unwrap_or(false))
                        .justify(justify(node.prop(3).as_str()));
                    if let Some(gutter) = node.prop(2).as_f64() {
                        grid = grid.gutter_px(gutter as f32);
                    } else if let Some(gutter) = node.size(1) {
                        grid = grid.gutter(gutter);
                    }
                    if let Some(width) = node.prop(6).as_f64() {
                        grid = grid.container_width(ContainerWidth::fixed(px(width as f32)));
                    } else if node.prop(5).as_str() == Some("viewport") {
                        grid = grid.container_width(ContainerWidth::viewport());
                    }
                    let placements = node.prop(7);
                    let span = ["span".into()];
                    let offset = ["offset".into()];
                    grid.children(children.into_iter().enumerate().map(|(index, child)| {
                        let placement = placements
                            .list_get(index)
                            .unwrap_or(&guiraffe_core::widget_value::Value::Null);
                        let span = placement.field(&span);
                        let offset = placement.field(&offset);
                        Col::span(responsive(span, Span::Full, |value| {
                            if value.as_str() == Some("full") {
                                Some(Span::Full)
                            } else {
                                integer(value).map(Span::Columns)
                            }
                        }))
                        .offset(responsive(offset, 0, integer))
                        .child(child)
                    }))
                    .into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::APP_SHELL,
                schema: guiraffe_core::widget_schema!(&widget_specs::APP_SHELL),
                render: |node, children, window, cx| {
                    use crate::{AppShell, AppShellEvent, AppShellState};
                    use gpui_kit::{ParentElement as _, div};

                    struct DocumentShell {
                        shell: gpui_kit::Entity<AppShellState>,
                        handlers: [Option<guiraffe_core::widget_catalog::EventHandler>; 4],
                        collapsed: [Option<bool>; 2],
                        widths: [Option<f64>; 2],
                        _subscription: gpui_kit::Subscription,
                    }

                    let state = window.use_keyed_state(node.state_id(), cx, |window, cx| {
                        let shell = AppShellState::new(cx);
                        let subscription = cx.subscribe_in(
                            &shell,
                            window,
                            |state: &mut DocumentShell, _, event, window, cx| {
                                use guiraffe_core::widget_value::Value;
                                let (index, value) = match *event {
                                    AppShellEvent::NavbarCollapsed { collapsed } => {
                                        (0, Value::Bool(collapsed))
                                    }
                                    AppShellEvent::AsideCollapsed { collapsed } => {
                                        (1, Value::Bool(collapsed))
                                    }
                                    AppShellEvent::NavbarResized { width } => {
                                        (2, Value::Num(f64::from(width)))
                                    }
                                    AppShellEvent::AsideResized { width } => {
                                        (3, Value::Num(f64::from(width)))
                                    }
                                };
                                if let Some(handler) = state.handlers[index].clone() {
                                    handler(value, window, cx);
                                }
                            },
                        );
                        DocumentShell {
                            shell,
                            handlers: [None, None, None, None],
                            collapsed: [None; 2],
                            widths: [None; 2],
                            _subscription: subscription,
                        }
                    });
                    let shell = state.update(cx, |state, cx| {
                        state.handlers = std::array::from_fn(|index| node.handler(index));
                        let collapsed = [node.prop(8).as_bool(), node.prop(9).as_bool()];
                        let widths = [node.prop(10).as_f64(), node.prop(11).as_f64()];
                        if collapsed != state.collapsed || widths != state.widths {
                            let previous_collapsed =
                                std::mem::replace(&mut state.collapsed, collapsed);
                            let previous_widths = std::mem::replace(&mut state.widths, widths);
                            state.shell.update(cx, |shell, cx| {
                                shell.apply_document_layout(
                                    std::array::from_fn(|index| {
                                        collapsed[index].filter(|_| {
                                            collapsed[index] != previous_collapsed[index]
                                        })
                                    }),
                                    std::array::from_fn(|index| {
                                        widths[index]
                                            .filter(|_| widths[index] != previous_widths[index])
                                            .map(|width| px(width as f32))
                                    }),
                                    cx,
                                );
                            });
                        }
                        state.shell.clone()
                    });
                    let mut shell = AppShell::new(&shell);
                    if let Some(height) = node.prop(1).as_f64() {
                        shell = shell.header_height(px(height as f32));
                    }
                    if let Some(height) = node.prop(2).as_f64() {
                        shell = shell.footer_height(px(height as f32));
                    }
                    if let Some(padding) = node.size(3) {
                        shell = shell.padding(padding);
                    }
                    if node.prop(4).as_bool() == Some(true) {
                        shell = shell.border();
                    }
                    if let Some(band) = breakpoint(node.prop(5).as_str()) {
                        shell = shell.collapse_below(band);
                    }
                    if let Some(band) = breakpoint(node.prop(6).as_str()) {
                        shell = shell.collapse_aside_below(band);
                    }
                    if node.prop(7).as_bool() == Some(true) {
                        shell = shell.resizable_navbar();
                    }
                    if node.prop(12).as_bool() == Some(true) {
                        shell = shell.resizable_aside();
                    }

                    let mut slots: [Vec<_>; 5] = std::array::from_fn(|_| Vec::new());
                    for (index, child) in children.into_iter().enumerate() {
                        let slot = match node
                            .prop(0)
                            .list_get(index)
                            .and_then(|value| value.as_str())
                        {
                            Some("header") => 0,
                            Some("navbar") => 1,
                            Some("aside") => 3,
                            Some("footer") => 4,
                            _ => 2,
                        };
                        slots[slot].push(child);
                    }
                    for (slot, children) in slots.into_iter().enumerate() {
                        if children.is_empty() {
                            continue;
                        }
                        let content = div().children(children);
                        shell = match slot {
                            0 => shell.header(content),
                            1 => shell.navbar(content),
                            3 => shell.aside(content),
                            4 => shell.footer(content),
                            _ => shell.document_main(content.into_any_element()),
                        };
                    }
                    shell.into_any_element()
                },
            },
        ],
    );
}

fn integer(value: &guiraffe_core::widget_value::Value) -> Option<u16> {
    value
        .as_f64()
        .map(|number| number.clamp(0., f64::from(u16::MAX)) as u16)
}

fn responsive<T>(
    value: &guiraffe_core::widget_value::Value,
    fallback: T,
    parse: impl Fn(&guiraffe_core::widget_value::Value) -> Option<T>,
) -> crate::Responsive<T> {
    use crate::{Breakpoint, Responsive};

    if let Some(constant) = parse(value) {
        return Responsive::value(constant);
    }
    let at = |name: &str| parse(&value.field(&[name.into()]));
    let mut result = Responsive::base(at("base").unwrap_or(fallback));
    for (name, breakpoint) in [
        ("xs", Breakpoint::Xs),
        ("sm", Breakpoint::Sm),
        ("md", Breakpoint::Md),
        ("lg", Breakpoint::Lg),
        ("xl", Breakpoint::Xl),
    ] {
        if let Some(override_value) = at(name) {
            result = result.at(breakpoint, override_value);
        }
    }
    result
}

fn justify(value: Option<&str>) -> Justify {
    match value {
        Some("center") => Justify::Center,
        Some("end") => Justify::End,
        Some("between") => Justify::Between,
        Some("around") => Justify::Around,
        Some("evenly") => Justify::Evenly,
        _ => Justify::Start,
    }
}

fn breakpoint(value: Option<&str>) -> Option<crate::Breakpoint> {
    use crate::Breakpoint;
    Some(match value? {
        "base" => Breakpoint::Base,
        "xs" => Breakpoint::Xs,
        "sm" => Breakpoint::Sm,
        "md" => Breakpoint::Md,
        "lg" => Breakpoint::Lg,
        "xl" => Breakpoint::Xl,
        _ => return None,
    })
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
    let justify = justify(node.prop(4).as_str());
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
