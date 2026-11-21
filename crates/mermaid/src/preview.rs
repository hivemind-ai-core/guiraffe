//! Bounded native geometry for the temporary flowchart overview.

use gpui_kit::{
    App, Bounds, PathBuilder, Pixels, SharedString, TextAlign, TextRun, Window, fill, point, px,
    size,
};
use mermaid_rs_renderer::{DiagramKind, Layout, Theme, ir::EdgeStyle, render::SvgDimensions};

/// Bound the transient scene independently of the parser's source limit.
const MAX_PRIMITIVES: usize = 4_096;

type Position = (f32, f32);

struct BoxMark {
    origin: Position,
    size: Position,
    color: u32,
    border: u32,
}

struct Label {
    center: Position,
    text: SharedString,
    color: u32,
}

struct Lines {
    color: u32,
    width: f32,
    muted: bool,
    segments: Vec<[Position; 2]>,
}

pub(crate) struct Overview {
    groups: Vec<BoxMark>,
    nodes: Vec<BoxMark>,
    lines: Vec<Lines>,
    labels: Vec<Label>,
    font: f32,
}

fn color(value: Option<&str>, fallback: u32) -> u32 {
    value
        .and_then(|value| u32::from_str_radix(value.strip_prefix('#')?, 16).ok())
        .unwrap_or(fallback)
}

impl Overview {
    pub(crate) fn new(layout: &Layout, dims: &SvgDimensions, theme: &Theme) -> Option<Self> {
        if layout.kind != DiagramKind::Flowchart {
            return None;
        }
        let primitives = layout.nodes.len()
            + layout.subgraphs.len()
            + layout
                .edges
                .iter()
                .map(|edge| edge.points.len().max(1))
                .sum::<usize>();
        if primitives > MAX_PRIMITIVES {
            return None;
        }
        let normalize = |x, y| {
            (
                (x - dims.viewbox_x) / dims.viewbox_width,
                (y - dims.viewbox_y) / dims.viewbox_height,
            )
        };
        let fill = color(Some(&theme.primary_color), 0xcccccc);
        let border = color(Some(&theme.primary_border_color), 0x555555);
        let stroke = color(Some(&theme.line_color), border);
        let text = color(Some(&theme.primary_text_color), border);
        let mut overview = Self {
            groups: Vec::with_capacity(layout.subgraphs.len()),
            nodes: Vec::with_capacity(layout.nodes.len()),
            lines: Vec::new(),
            labels: Vec::new(),
            font: theme.font_size / dims.viewbox_width,
        };
        let mark = |x, y, width, height, style: &mermaid_rs_renderer::ir::NodeStyle| BoxMark {
            origin: normalize(x, y),
            size: (width / dims.viewbox_width, height / dims.viewbox_height),
            color: color(style.fill.as_deref(), fill),
            border: color(style.stroke.as_deref(), border),
        };
        let mut label = |center, value: &str, style: Option<&str>| {
            if !value.is_empty() {
                overview.labels.push(Label {
                    center,
                    text: value.into(),
                    color: color(style, text),
                });
            }
        };
        for group in &layout.subgraphs {
            overview.groups.push(mark(
                group.x,
                group.y,
                group.width,
                group.height,
                &group.style,
            ));
            label(
                normalize(group.x + group.width * 0.5, group.y + theme.font_size),
                &group.label,
                group.style.text_color.as_deref(),
            );
        }
        for node in layout.nodes.values().filter(|node| !node.hidden) {
            overview
                .nodes
                .push(mark(node.x, node.y, node.width, node.height, &node.style));
            if let Some(line) = node.label.lines.first() {
                label(
                    normalize(node.x + node.width * 0.5, node.y + node.height * 0.5),
                    line,
                    node.style.text_color.as_deref(),
                );
            }
        }
        let mut batches = std::collections::BTreeMap::new();
        for edge in &layout.edges {
            let muted = edge.style == EdgeStyle::Dotted || edge.override_style.dasharray.is_some();
            let width = edge
                .override_style
                .stroke_width
                .unwrap_or(match edge.style {
                    EdgeStyle::Thick => 1.25,
                    _ => 0.65,
                })
                .clamp(0.35, 4.0);
            let stroke = color(edge.override_style.stroke.as_deref(), stroke);
            let index = *batches
                .entry((stroke, width.to_bits(), muted))
                .or_insert_with(|| {
                    let index = overview.lines.len();
                    overview.lines.push(Lines {
                        color: stroke,
                        width,
                        muted,
                        segments: Vec::new(),
                    });
                    index
                });
            for pair in edge.points.windows(2) {
                overview.lines[index].segments.push([
                    normalize(pair[0].0, pair[0].1),
                    normalize(pair[1].0, pair[1].1),
                ]);
            }
            for (block, anchor) in [
                (&edge.label, edge.label_anchor),
                (&edge.start_label, edge.start_label_anchor),
                (&edge.end_label, edge.end_label_anchor),
            ] {
                if let Some(line) = block.as_ref().and_then(|block| block.lines.first())
                    && let Some((x, y)) = anchor
                {
                    label(
                        normalize(x, y),
                        line,
                        edge.override_style.label_color.as_deref(),
                    );
                }
            }
        }
        Some(overview)
    }

    pub(crate) fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let at = |(x, y)| bounds.origin + point(bounds.size.width * x, bounds.size.height * y);
        let paint_box = |mark: &BoxMark, group: bool, window: &mut Window| {
            let size = size(
                bounds.size.width * mark.size.0,
                bounds.size.height * mark.size.1,
            );
            let bounds = Bounds::new(at(mark.origin), size);
            let mut color = gpui_kit::rgb(if size.width <= px(1.0) || size.height <= px(1.0) {
                mark.border
            } else {
                mark.color
            });
            if group {
                color.a = 0.2;
            }
            window.paint_quad(fill(bounds, color));
        };
        for mark in &self.groups {
            paint_box(mark, true, window);
        }
        for lines in &self.lines {
            let mut path = PathBuilder::stroke(px(lines.width));
            for &[start, end] in &lines.segments {
                path.move_to(at(start));
                path.line_to(at(end));
            }
            if let Ok(path) = path.build() {
                let mut color = gpui_kit::rgb(lines.color);
                color.a = if lines.muted { 0.35 } else { 0.65 };
                window.paint_path(path, color);
            }
        }
        for mark in &self.nodes {
            paint_box(mark, false, window);
        }
        let font = bounds.size.width * self.font;
        if font >= px(7.0) {
            let mask = window.content_mask().bounds;
            for label in &self.labels {
                let center = at(label.center);
                if !mask.contains(&center) {
                    continue;
                }
                let shaped = window.text_system().shape_line(
                    label.text.clone(),
                    font,
                    &[TextRun {
                        len: label.text.len(),
                        font: gpui_kit::Font::default(),
                        color: gpui_kit::rgb(label.color).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let _ = shaped.paint(
                    center - point(shaped.width * 0.5, font * 0.5),
                    font,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }
    }
}
