//! Shared, chart-independent plot composition. Layers see only visible coordinates.
use crate::{Axis, TimeFormat, ValueRange};
use gpui_kit::{
    AnyElement, IntoElement, ParentElement, PathBuilder, Styled, canvas, div, point, px, size,
};
use guiraffe_core::{ColorName, GuiraffeTheme, StateKind, Variant};
use std::{ops::Range, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    Border,
    Minor,
    Text,
    Dimmed,
    Accent,
    Selection,
    SelectionText,
    LabelBackground,
    Stripe,
    Color(ColorName),
}
impl Ink {
    fn color(self, theme: &GuiraffeTheme) -> gpui_kit::Hsla {
        let s = theme.surfaces();
        match self {
            Self::Border => s.border,
            Self::LabelBackground => s.body,
            Self::Minor | Self::Stripe => s.surface,
            Self::Text => s.text,
            Self::Dimmed => s.dimmed,
            Self::Accent => theme.variant(None, Variant::Filled).background,
            Self::SelectionText => {
                theme
                    .state(StateKind::Selected, Variant::Filled, None)
                    .foreground
            }
            Self::Selection => {
                theme
                    .state(StateKind::Selected, Variant::Filled, None)
                    .background
            }
            Self::Color(c) => theme.variant(Some(c), Variant::Filled).background,
        }
        .to_hsla()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Line {
        from: (f64, f64),
        to: (f64, f64),
        ink: Ink,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        ink: Ink,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        ink: Ink,
    },
}
#[derive(Clone, Copy)]
pub struct PlotContext {
    pub x: Axis,
    pub y: Option<ValueRange>,
    pub height: f64,
    pub row_height: f64,
    pub row_offset: f64,
    pub header: bool,
}
impl PlotContext {
    pub fn visible_rows(&self, count: usize) -> Range<usize> {
        let row = self.row_height.max(1.);
        let start = (self.row_offset.max(0.) / row).floor() as usize;
        start.min(count)
            ..(((self.row_offset.max(0.) + self.height.max(0.)) / row)
                .ceil()
                .max(0.) as usize)
                .min(count)
    }
}
/// Custom layers append bounded viewport drawing commands in stack order.
pub trait Layer: 'static {
    fn prepaint(&self, plot: &PlotContext, out: &mut Vec<Primitive>);
}
fn line(out: &mut Vec<Primitive>, a: (f64, f64), b: (f64, f64), ink: Ink) {
    out.push(Primitive::Line {
        from: a,
        to: b,
        ink,
    });
}
#[derive(Clone, Copy)]
pub struct Ruler(pub TimeFormat);
impl Layer for Ruler {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if !p.header {
            return;
        }
        for t in p.x.ticks(p.x.tick_step(90., self.0)) {
            let x = p.x.x(t);
            line(o, (x, 0.), (x, p.height), Ink::Border);
            o.push(Primitive::Text {
                x: x + 4.,
                y: 4.,
                text: crate::axis::label(t, self.0),
                ink: Ink::Dimmed,
            });
        }
    }
}
#[derive(Clone, Copy)]
pub struct Gridlines {
    pub x: bool,
    pub y: bool,
    pub minor: bool,
    pub format: TimeFormat,
}
impl Default for Gridlines {
    fn default() -> Self {
        Self {
            x: true,
            y: true,
            minor: true,
            format: TimeFormat::Seconds,
        }
    }
}
impl Layer for Gridlines {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if p.header {
            return;
        }
        if self.x {
            let major = p.x.tick_step(90., self.format);
            let step = if self.minor { major / 5. } else { major };
            for t in p.x.ticks(step) {
                let x = p.x.x(t);
                let major_tick = (t / major - (t / major).round()).abs() < 1e-6;
                line(
                    o,
                    (x, 0.),
                    (x, p.height),
                    if major_tick { Ink::Border } else { Ink::Minor },
                );
            }
        }
        if self.y
            && let Some(y) = p.y
        {
            let axis = Axis::new(y.lo, y.hi, y.height.max(1.));
            for v in axis.ticks(axis.tick_step(48., TimeFormat::Seconds)) {
                let yy = y.y(v);
                line(o, (0., yy), (p.x.width, yy), Ink::Border);
            }
        }
    }
}
/// Value labels paint in a foreground pass so data never obscures them.
#[derive(Clone, Copy)]
pub struct ValueLabels;
impl Layer for ValueLabels {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if p.header {
            return;
        }
        let Some(y) = p.y else { return };
        let axis = Axis::new(y.lo, y.hi, y.height.max(1.));
        for value in axis.ticks(axis.tick_step(48., TimeFormat::Seconds)) {
            let text = format!("{value:.2}");
            let yy = (y.y(value) - 8.).clamp(2., (p.height - 18.).max(2.));
            let width = (text.len() as f64 * 7. + 8.).min(p.x.width);
            o.push(Primitive::Rect {
                x: 2.,
                y: yy,
                w: width,
                h: 16.,
                ink: Ink::LabelBackground,
            });
            o.push(Primitive::Text {
                x: 6.,
                y: yy,
                text,
                ink: Ink::Dimmed,
            });
        }
    }
}
#[derive(Clone, Copy)]
pub struct GridMarks(pub TimeFormat);
impl Layer for GridMarks {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if p.header {
            for t in p.x.ticks(p.x.tick_step(18., self.0)) {
                let x = p.x.x(t);
                line(o, (x, p.height - 4.), (x, p.height), Ink::Dimmed);
            }
        }
    }
}
#[derive(Clone, Copy)]
pub struct Playhead {
    pub time: f64,
    pub format: TimeFormat,
}
impl Layer for Playhead {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        let x = p.x.x(self.time);
        if !(0. ..=p.x.width).contains(&x) {
            return;
        }
        line(o, (x, 0.), (x, p.height), Ink::Accent);
        if p.header {
            let text = crate::axis::label(self.time, self.format);
            let width = (text.len() as f64 * 8. + 8.)
                .clamp(24., 150.)
                .min(p.x.width);
            let left = (x - width / 2.).clamp(0., (p.x.width - width).max(0.));
            o.push(Primitive::Rect {
                x: left,
                y: 0.,
                w: width,
                h: p.height,
                ink: Ink::Selection,
            });
            o.push(Primitive::Text {
                x: left + 4.,
                y: 4.,
                text,
                ink: Ink::SelectionText,
            });
        }
    }
}
#[derive(Clone, Copy)]
pub struct SelectionBox {
    pub start: (f64, f64),
    pub end: (f64, f64),
}
impl Layer for SelectionBox {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if p.header {
            return;
        }
        let (x0, x1) = (self.start.0.min(self.end.0), self.start.0.max(self.end.0));
        let (y0, y1) = (self.start.1.min(self.end.1), self.start.1.max(self.end.1));
        for (a, b) in [
            ((x0, y0), (x1, y0)),
            ((x1, y0), (x1, y1)),
            ((x1, y1), (x0, y1)),
            ((x0, y1), (x0, y0)),
        ] {
            line(o, a, b, Ink::Accent);
        }
    }
}

#[derive(Clone)]
pub struct Markers(pub Rc<[(f64, String)]>);
impl Layer for Markers {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        for (t, text) in self
            .0
            .iter()
            .filter(|(t, _)| *t >= p.x.start && *t <= p.x.end())
        {
            let x = p.x.x(*t);
            line(o, (x, 0.), (x, p.height), Ink::Dimmed);
            if p.header {
                o.push(Primitive::Text {
                    x: x + 4.,
                    y: 4.,
                    text: text.clone(),
                    ink: Ink::Text,
                });
            }
        }
    }
}
#[derive(Clone, Copy)]
pub struct Zebra {
    pub rows: usize,
}
impl Layer for Zebra {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if !p.header {
            for r in p.visible_rows(self.rows).filter(|r| r % 2 == 1) {
                o.push(Primitive::Rect {
                    x: 0.,
                    y: r as f64 * p.row_height - p.row_offset,
                    w: p.x.width,
                    h: p.row_height,
                    ink: Ink::Stripe,
                });
            }
        }
    }
}
#[derive(Clone)]
pub struct BackgroundBands {
    pub intervals: Rc<[(f64, f64)]>,
    pub ink: Ink,
}
impl Layer for BackgroundBands {
    fn prepaint(&self, p: &PlotContext, o: &mut Vec<Primitive>) {
        if !p.header {
            for (a, b) in self
                .intervals
                .iter()
                .filter(|(a, b)| *b >= p.x.start && *a <= p.x.end())
            {
                let x = p.x.x(*a).max(0.);
                o.push(Primitive::Rect {
                    x,
                    y: 0.,
                    w: p.x.x(*b).min(p.x.width) - x,
                    h: p.height,
                    ink: self.ink,
                });
            }
        }
    }
}
#[derive(Clone)]
pub struct PlotSurface {
    pub axis: Axis,
    pub range: Option<ValueRange>,
    layers: Rc<Vec<Rc<dyn Layer>>>,
}
impl PlotSurface {
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            range: None,
            layers: Rc::new(Vec::new()),
        }
    }
    pub fn timeline(axis: Axis, format: TimeFormat) -> Self {
        let mut s = Self::new(axis);
        s.add_layer(Ruler(format));
        s.add_layer(GridMarks(format));
        s.add_layer(Gridlines {
            format,
            ..Default::default()
        });
        s
    }
    pub fn add_layer(&mut self, layer: impl Layer) {
        Rc::make_mut(&mut self.layers).push(Rc::new(layer));
    }
    pub fn insert_layer(&mut self, index: usize, layer: impl Layer) {
        let layers = Rc::make_mut(&mut self.layers);
        layers.insert(index.min(layers.len()), Rc::new(layer));
    }
    pub fn clear_layers(&mut self) {
        self.layers = Rc::default();
    }
    pub fn commands(&self, context: &PlotContext) -> Vec<Primitive> {
        let mut out = Vec::new();
        for l in self.layers.iter() {
            l.prepaint(context, &mut out)
        }
        out
    }
    pub fn navigate(&mut self, x: f64, dx: f64, dy: f64, zoom: bool) -> bool {
        if zoom {
            self.axis.zoom_at(x, (1. + dy * 0.002).max(0.1))
        } else {
            let d = if dx != 0. { dx } else { dy };
            if !d.is_finite() || d == 0. {
                false
            } else {
                self.axis.pan_px(-d);
                true
            }
        }
    }
    pub fn fit(&mut self, start: f64, end: f64, margin: f64) {
        self.axis.fit(start, end, margin);
    }
    pub fn value_labels(&self, theme: GuiraffeTheme) -> AnyElement {
        let mut labels = Self::new(self.axis);
        labels.range = self.range;
        labels.add_layer(ValueLabels);
        labels.element(theme, false, 1., 0.)
    }
    pub fn element(
        &self,
        theme: GuiraffeTheme,
        header: bool,
        row_height: f64,
        row_offset: f64,
    ) -> AnyElement {
        let plot = self.clone();
        div()
            .absolute()
            .size_full()
            .child(
                canvas(
                    move |b, _, _| {
                        let mut x = plot.axis;
                        x.set_width(f64::from(f32::from(b.size.width)));
                        let height = f64::from(f32::from(b.size.height));
                        let y = plot.range.map(|mut y| {
                            y.height = height;
                            y
                        });
                        plot.commands(&PlotContext {
                            x,
                            y,
                            height,
                            row_height,
                            row_offset,
                            header,
                        })
                    },
                    move |b, commands, window, cx| {
                        let at = |x: f64, y: f64| {
                            point(b.origin.x + px(x as f32), b.origin.y + px(y as f32))
                        };
                        for command in commands {
                            match command {
                                Primitive::Line { from, to, ink } => {
                                    let mut path = PathBuilder::stroke(px(1.));
                                    path.move_to(at(from.0, from.1));
                                    path.line_to(at(to.0, to.1));
                                    if let Ok(path) = path.build() {
                                        window.paint_path(path, ink.color(&theme));
                                    }
                                }
                                Primitive::Rect { x, y, w, h, ink } => {
                                    window.paint_quad(gpui_kit::fill(
                                        gpui_kit::Bounds::new(
                                            at(x, y),
                                            size(px(w.max(0.) as f32), px(h.max(0.) as f32)),
                                        ),
                                        ink.color(&theme),
                                    ));
                                }
                                Primitive::Text { x, y, text, ink } => {
                                    let color = ink.color(&theme);
                                    let shaped = window.text_system().shape_line(
                                        text.clone().into(),
                                        px(11.),
                                        &[gpui_kit::TextRun {
                                            len: text.len(),
                                            font: gpui_kit::Font::default(),
                                            color,
                                            background_color: None,
                                            underline: None,
                                            strikethrough: None,
                                        }],
                                        None,
                                    );
                                    let _ = shaped.paint(
                                        at(x, y),
                                        px(16.),
                                        gpui_kit::TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                            }
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layers_are_ordered_culled_and_share_navigation() {
        let mut surface = PlotSurface::timeline(Axis::new(0., 10., 500.), TimeFormat::Seconds);
        surface.add_layer(BackgroundBands {
            intervals: Rc::from([(-10., -1.), (2., 4.), (12., 20.)]),
            ink: Ink::Stripe,
        });
        let p = PlotContext {
            x: surface.axis,
            y: None,
            height: 100.,
            row_height: 20.,
            row_offset: 40.,
            header: false,
        };
        assert_eq!(p.visible_rows(100), 2..7);
        let commands = surface.commands(&p);
        assert!(matches!(
            commands.last(),
            Some(Primitive::Rect {
                x: 100.,
                w: 100.,
                ..
            })
        ));
        let anchor = surface.axis.t(125.);
        assert!(surface.navigate(125., 0., 100., true));
        assert!((surface.axis.t(125.) - anchor).abs() < 1e-10);
        assert!(!surface.navigate(0., 0., 0., false));
        surface.fit(-1., 1., 0.);
        assert!((surface.axis.end() - 1.).abs() < 1e-10);
    }
}
