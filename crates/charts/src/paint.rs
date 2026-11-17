use crate::{
    Color, Domain, PreparedChart, Scene, SeriesId,
    scene::{Geometry, Position, Rect},
};
use gpui_kit::{
    App, Bounds, ContentMask, Hsla, PathBuilder, Pixels, Point, SharedString, Window, fill, point,
    px, size,
};
use guiraffe_core::{ColorName, Contrast, GuiraffeTheme, Variant, color::Rgb};

/// Series inks contrast at least 3:1 with the plot body in standard mode and 4.5:1 in high contrast.
pub fn categorical_palette(theme: &GuiraffeTheme) -> [Rgb; 8] {
    let mut colors = [
        theme.primary,
        ColorName::Teal,
        ColorName::Orange,
        ColorName::Grape,
        ColorName::Pink,
        ColorName::Cyan,
        ColorName::Lime,
        ColorName::Red,
    ];
    if let Some(duplicate) = colors[1..].iter().position(|&name| name == theme.primary) {
        colors[duplicate + 1] = ColorName::Indigo;
    }
    let surfaces = theme.surfaces();
    let floor = if theme.contrast == Contrast::High {
        4.5
    } else {
        3.
    };
    colors.map(|name| {
        let start = theme.variant(Some(name), Variant::Filled).background;
        if start.contrast(surfaces.body) >= floor {
            return start;
        }
        (1..=16)
            .map(|step| start.mix(surfaces.text, step as f32 / 16.))
            .find(|color| color.contrast(surfaces.body) >= floor)
            .unwrap_or(surfaces.text)
    })
}

pub(crate) fn at(origin: Point<Pixels>, (x, y): Position) -> Point<Pixels> {
    origin + point(px(x), px(y))
}

pub(crate) fn box_fill(
    window: &mut Window,
    origin: Point<Pixels>,
    a: Position,
    b: Position,
    color: Hsla,
    radius: f32,
) {
    let bounds = Bounds::new(
        at(origin, (a.0.min(b.0), a.1.min(b.1))),
        size(px((a.0 - b.0).abs()), px((a.1 - b.1).abs())),
    );
    window.paint_quad(fill(bounds, color).corner_radii(px(radius)));
}

pub(crate) fn segment(
    window: &mut Window,
    origin: Point<Pixels>,
    a: Position,
    b: Position,
    color: Hsla,
) {
    let mut path = PathBuilder::stroke(px(1.));
    path.move_to(at(origin, a));
    path.line_to(at(origin, b));
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

pub(crate) enum Align {
    Left,
    Center,
    Right,
}

pub(crate) fn label(
    window: &mut Window,
    cx: &mut App,
    position: Point<Pixels>,
    text: impl Into<SharedString>,
    color: Hsla,
    align: Align,
) {
    let text = text.into();
    let len = text.len();
    let shaped = window.text_system().shape_line(
        text,
        px(11.),
        &[gpui_kit::TextRun {
            len,
            font: gpui_kit::Font::default(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let shift = match align {
        Align::Left => px(0.),
        Align::Center => shaped.width / 2.,
        Align::Right => shaped.width,
    };
    let _ = shaped.paint(
        position - point(shift, px(0.)),
        px(15.),
        gpui_kit::TextAlign::Left,
        None,
        window,
        cx,
    );
}

fn mask(origin: Point<Pixels>, rect: Rect) -> ContentMask<Pixels> {
    ContentMask {
        bounds: Bounds::new(
            at(origin, (rect.left, rect.top)),
            size(px(rect.width()), px(rect.height())),
        ),
    }
}

pub(crate) struct PaintScene<'a> {
    pub scene: &'a Scene,
    pub chart: &'a PreparedChart,
    pub domain: Domain,
    pub theme: &'a GuiraffeTheme,
    pub origin: Point<Pixels>,
    pub focused: Option<SeriesId>,
}

pub(crate) fn paint_scene(options: PaintScene<'_>, window: &mut Window, cx: &mut App) {
    let PaintScene {
        scene,
        chart,
        domain,
        theme,
        origin,
        focused,
    } = options;
    let palette = categorical_palette(theme);
    let surfaces = theme.surfaces();
    let dim = surfaces.dimmed.to_hsla();
    let grid = surfaces.border.to_hsla();
    let domains = [domain, chart.domains[1], chart.domains[2]];
    for panel in &scene.panels {
        let r = panel.rect;
        box_fill(
            window,
            origin,
            (r.left, r.top),
            (r.right, r.bottom),
            surfaces.body.to_hsla(),
            0.,
        );
        for (axis, spec) in chart.axes.iter().enumerate() {
            if axis == 2 && !chart.has_right_axis {
                continue;
            }
            for tick in &panel.ticks[axis] {
                let Some(fraction) = spec.scale.project(domains[axis], tick.value) else {
                    continue;
                };
                if axis == 0 {
                    let x = r.left + r.width() * fraction as f32;
                    if spec.grid {
                        segment(window, origin, (x, r.top), (x, r.bottom), grid);
                    }
                    label(
                        window,
                        cx,
                        at(origin, (x, r.bottom + 5.)),
                        tick.label.as_str(),
                        dim,
                        Align::Center,
                    );
                } else {
                    let y = r.bottom - r.height() * fraction as f32;
                    if spec.grid && axis == 1 {
                        segment(window, origin, (r.left, y), (r.right, y), grid);
                    }
                    let (x, align) = if axis == 1 {
                        (r.left - 6., Align::Right)
                    } else {
                        (r.right + 6., Align::Left)
                    };
                    label(
                        window,
                        cx,
                        at(origin, (x, y - 7.)),
                        tick.label.as_str(),
                        dim,
                        align,
                    );
                }
            }
            if !spec.title.is_empty() {
                let (pos, align) = match axis {
                    0 => (((r.left + r.right) / 2., r.bottom + 23.), Align::Center),
                    1 => ((r.left - 6., r.top - 14.), Align::Right),
                    _ => ((r.right + 6., r.top - 14.), Align::Left),
                };
                label(
                    window,
                    cx,
                    at(origin, pos),
                    spec.title.as_str(),
                    surfaces.text.to_hsla(),
                    align,
                );
            }
        }
        if scene.panels.len() > 1 {
            label(
                window,
                cx,
                at(origin, (r.left, r.top - 19.)),
                panel.label.as_str(),
                surfaces.text.to_hsla(),
                Align::Left,
            );
        }
        for ((x, y), end) in [
            ((r.left, r.top), (r.left, r.bottom)),
            ((r.left, r.bottom), (r.right, r.bottom)),
            ((r.right, r.bottom), (r.right, r.top)),
        ] {
            segment(window, origin, (x, y), end, grid);
        }
        window.with_content_mask(Some(mask(origin, r)), |window| {
            for draw in &panel.draws {
                let ink = match draw.color {
                    Color::Category(index) => palette[index as usize % palette.len()],
                    Color::Continuous(fraction) => {
                        let fraction = if fraction.is_finite() { fraction } else { 0. };
                        surfaces
                            .body
                            .mix(palette[0], 0.15 + 0.85 * fraction.clamp(0., 1.))
                    }
                };
                let ink = if focused.is_some_and(|id| id != draw.series) {
                    let dimmed = theme
                        .state(guiraffe_core::StateKind::Disabled, Variant::Default, None)
                        .foreground;
                    ink.mix(dimmed, 0.8)
                } else {
                    ink
                };
                let color = ink.to_hsla();
                match &draw.geometry {
                    Geometry::Segment(a, b) => segment(window, origin, *a, *b, color),
                    Geometry::Box(a, b) => box_fill(window, origin, *a, *b, color, 0.),
                    Geometry::Dot((x, y), radius) => {
                        box_fill(
                            window,
                            origin,
                            (x - radius, y - radius),
                            (x + radius, y + radius),
                            color,
                            *radius,
                        );
                    }
                    Geometry::Text(pos, text) => label(
                        window,
                        cx,
                        at(origin, *pos),
                        text.clone(),
                        color,
                        Align::Left,
                    ),
                    Geometry::Path { points, closed } => {
                        let Some((&first, rest)) = points.split_first() else {
                            continue;
                        };
                        let mut path = if *closed {
                            PathBuilder::fill()
                        } else {
                            PathBuilder::stroke(px(1.75))
                        };
                        path.move_to(at(origin, first));
                        for &point in rest {
                            path.line_to(at(origin, point));
                        }
                        if *closed {
                            path.close();
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(
                                path,
                                if *closed {
                                    ink.alpha(0.36).to_hsla()
                                } else {
                                    color
                                },
                            );
                        }
                        if *closed {
                            let upper = points.len() / 2;
                            if upper > 1 {
                                let mut edge = PathBuilder::stroke(px(1.5));
                                edge.move_to(at(origin, points[0]));
                                for &p in &points[1..upper] {
                                    edge.line_to(at(origin, p));
                                }
                                if let Ok(edge) = edge.build() {
                                    window.paint_path(edge, color);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_core::Scheme;

    #[test]
    fn categorical_colors_meet_surface_contrast() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            for primary in ColorName::ALL {
                for contrast in [Contrast::Standard, Contrast::High] {
                    let theme = GuiraffeTheme {
                        scheme,
                        primary,
                        contrast,
                        ..Default::default()
                    };
                    let floor = if contrast == Contrast::High { 4.5 } else { 3. };
                    for ink in categorical_palette(&theme) {
                        assert!(ink.contrast(theme.surfaces().body) >= floor);
                    }
                }
            }
        }
    }
}
