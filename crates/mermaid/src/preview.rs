//! Native overview for plain flowcharts at subpixel label sizes.

use gpui_kit::{Bounds, PathBuilder, Pixels, Window, fill, point, px, size};
use mermaid_rs_renderer::{
    DiagramKind, Layout, Theme,
    ir::{EdgeStyle, NodeShape},
    render::SvgDimensions,
};

/// Bound the transient scene independently of the parser's source limit.
const MAX_PRIMITIVES: usize = 2_048;

pub(crate) struct Overview {
    nodes: Vec<[f32; 4]>,
    edges: Vec<[(f32, f32); 2]>,
    dotted: Vec<[(f32, f32); 2]>,
    fill: u32,
    stroke: u32,
    line: u32,
}

impl Overview {
    pub(crate) fn new(layout: &Layout, dims: &SvgDimensions, theme: &Theme) -> Option<Self> {
        if layout.kind != DiagramKind::Flowchart
            || !layout.subgraphs.is_empty()
            || layout.nodes.len() + layout.edges.len() > MAX_PRIMITIVES
        {
            return None;
        }
        let color = |hex: &str| u32::from_str_radix(hex.strip_prefix('#')?, 16).ok();
        let mut overview = Self {
            nodes: Vec::with_capacity(layout.nodes.len()),
            edges: Vec::with_capacity(layout.edges.len()),
            dotted: Vec::new(),
            fill: color(&theme.primary_color)?,
            stroke: color(&theme.primary_border_color)?,
            line: color(&theme.line_color)?,
        };
        let normalize = |x, y| {
            (
                (x - dims.viewbox_x) / dims.viewbox_width,
                (y - dims.viewbox_y) / dims.viewbox_height,
            )
        };
        for node in layout.nodes.values().filter(|node| !node.hidden) {
            if node.shape != NodeShape::Rectangle
                || node.icon.is_some()
                || node.style.fill.is_some()
                || node.style.stroke.is_some()
                || node.style.stroke_width.is_some()
                || node.style.stroke_dasharray.is_some()
            {
                return None;
            }
            let (x, y) = normalize(node.x, node.y);
            overview.nodes.push([
                x,
                y,
                node.width / dims.viewbox_width,
                node.height / dims.viewbox_height,
            ]);
        }
        for edge in &layout.edges {
            if edge.label.is_some()
                || edge.start_label.is_some()
                || edge.end_label.is_some()
                || edge.override_style.stroke.is_some()
                || edge.override_style.stroke_width.is_some()
                || edge.override_style.dasharray.is_some()
                || edge.start_decoration.is_some()
                || edge.end_decoration.is_some()
            {
                return None;
            }
            let segment_count = edge.points.len().saturating_sub(1);
            if overview.nodes.len() + overview.edges.len() + overview.dotted.len() + segment_count
                > MAX_PRIMITIVES
            {
                return None;
            }
            let lines = match edge.style {
                EdgeStyle::Solid => &mut overview.edges,
                EdgeStyle::Dotted => &mut overview.dotted,
                EdgeStyle::Thick => return None,
            };
            for pair in edge.points.windows(2) {
                let (x1, y1) = pair[0];
                let (x2, y2) = pair[1];
                lines.push([normalize(x1, y1), normalize(x2, y2)]);
            }
        }
        Some(overview)
    }

    pub(crate) fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let at = |x, y| bounds.origin + point(bounds.size.width * x, bounds.size.height * y);
        for (lines, alpha) in [(&self.edges, 0.65), (&self.dotted, 0.35)] {
            if lines.is_empty() {
                continue;
            }
            let mut path = PathBuilder::stroke(px(0.65));
            for &[(x1, y1), (x2, y2)] in lines {
                path.move_to(at(x1, y1));
                path.line_to(at(x2, y2));
            }
            if let Ok(path) = path.build() {
                let mut color = gpui_kit::rgb(self.line);
                color.a = alpha;
                window.paint_path(path, color);
            }
        }
        for &[x, y, width, height] in &self.nodes {
            let size = size(bounds.size.width * width, bounds.size.height * height);
            let bounds = Bounds::new(at(x, y), size);
            let color = if size.width <= px(1.0) || size.height <= px(1.0) {
                self.stroke
            } else {
                self.fill
            };
            window.paint_quad(fill(bounds, gpui_kit::rgb(color)));
        }
    }
}
