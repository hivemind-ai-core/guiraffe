use crate::{
    Arrangement, Color, DatumId, Domain, Mark, Orientation, PreparedChart, SeriesId, Tick,
    model::{Run, Sample},
    sample::lttb,
};
use std::{collections::BTreeSet, ops::Range, sync::Arc};

pub(crate) type Position = (f32, f32);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn width(self) -> f32 {
        self.right - self.left
    }

    pub fn height(self) -> f32 {
        self.bottom - self.top
    }

    pub fn contains(self, (x, y): Position) -> bool {
        (self.left..=self.right).contains(&x) && (self.top..=self.bottom).contains(&y)
    }

    fn project(self, x: f64, y: f64) -> Position {
        (
            self.left + self.width() * x.clamp(-1e4, 1e4) as f32,
            self.bottom - self.height() * y.clamp(-1e4, 1e4) as f32,
        )
    }

    pub fn fraction(self, x: f32) -> f64 {
        f64::from(((x - self.left) / self.width()).clamp(0., 1.))
    }
}

pub(crate) enum Geometry {
    Path { points: Vec<Position>, closed: bool },
    Segment(Position, Position),
    Box(Position, Position),
    Dot(Position, f32),
    Text(Position, Arc<str>),
}

pub(crate) struct Draw {
    pub geometry: Geometry,
    pub series: SeriesId,
    pub color: Color,
}

pub(crate) struct Panel {
    pub rect: Rect,
    pub facet: u32,
    pub label: String,
    pub draws: Vec<Draw>,
    pub ticks: [Vec<Tick>; 3],
}

/// Renderer-neutral viewport output. Build off the UI thread for expensive uncached views.
pub struct Scene {
    pub(crate) panels: Vec<Panel>,
}

impl Scene {
    pub fn primitive_count(&self) -> usize {
        self.panels.iter().map(|panel| panel.draws.len()).sum()
    }

    pub fn vertex_count(&self) -> usize {
        self.panels
            .iter()
            .flat_map(|panel| &panel.draws)
            .map(|draw| match &draw.geometry {
                Geometry::Path { points, .. } => points.len(),
                _ => 1,
            })
            .sum()
    }

    pub(crate) fn panel(&self, position: Position) -> Option<&Panel> {
        self.panels
            .iter()
            .find(|panel| panel.rect.contains(position))
    }
}

#[derive(PartialEq)]
pub(crate) struct Hit {
    pub id: DatumId,
    pub point: Position,
    pub x: f64,
    pub y: f64,
    pub secondary: bool,
}

impl PreparedChart {
    pub fn scene(
        &self,
        width: f32,
        height: f32,
        domain: Domain,
        hidden: &BTreeSet<SeriesId>,
    ) -> Scene {
        if !width.is_finite() || !height.is_finite() {
            return Scene { panels: Vec::new() };
        }
        let count = self.facets.len();
        let columns = if width >= 520. && count > 1 { 2 } else { 1 };
        let rows = count.div_ceil(columns).max(1);
        let cell_width = width / columns as f32;
        let cell_height = height / rows as f32;
        let has_y_title = !self.axes[1].title.is_empty()
            || (self.has_right_axis && !self.axes[2].title.is_empty());
        let top = match (count > 1, has_y_title) {
            (true, true) => 48.,
            (false, false) => 12.,
            _ => 30.,
        };
        let bottom = if self.axes[0].title.is_empty() {
            25.
        } else {
            44.
        };
        if cell_height <= top + bottom {
            return Scene { panels: Vec::new() };
        }
        let domains = [domain, self.domains[1], self.domains[2]];
        let y_ticks = |axis: usize| {
            self.axes[axis]
                .scale
                .ticks(domains[axis], ((cell_height - top - bottom) / 46.) as usize)
        };
        let left_ticks = y_ticks(1);
        let right_ticks = if self.has_right_axis {
            y_ticks(2)
        } else {
            Vec::new()
        };
        // Conservative 11 px text widths keep the pure scene independent of GPUI fonts.
        let margin = |axis: usize, ticks: &[crate::Tick]| {
            let characters = ticks
                .iter()
                .map(|tick| tick.label.chars().count())
                .chain(std::iter::once(self.axes[axis].title.chars().count()))
                .max()
                .unwrap_or_default();
            (12. + 11. * characters as f32).max(75.)
        };
        let left = margin(1, &left_ticks);
        let right = if self.has_right_axis {
            margin(2, &right_ticks)
        } else {
            20.
        };
        if cell_width <= left + right {
            return Scene { panels: Vec::new() };
        }

        let mut scratch = Vec::new();
        let mut panels = Vec::with_capacity(count);
        for (index, (facet, label)) in self.facets.iter().enumerate() {
            let x = (index % columns) as f32 * cell_width;
            let y = (index / columns) as f32 * cell_height;
            let rect = Rect {
                left: x + left,
                top: y + top,
                right: x + cell_width - right,
                bottom: y + cell_height - bottom,
            };
            let ticks = [
                self.axes[0]
                    .scale
                    .ticks(domain, (rect.width() / 85.) as usize),
                left_ticks.clone(),
                right_ticks.clone(),
            ];
            let mut panel = Panel {
                rect,
                facet: *facet,
                label: label.clone(),
                draws: Vec::new(),
                ticks,
            };
            let mut stack_indices = std::collections::BTreeMap::new();
            for run in self
                .runs
                .iter()
                .filter(|r| r.facet == *facet && !hidden.contains(&r.id))
            {
                let projection = Projection {
                    chart: self,
                    domains,
                    rect,
                    run,
                };
                if matches!(
                    run.mark,
                    Mark::Area(Arrangement::Stack | Arrangement::Normalize)
                ) {
                    let visible = run.visible(domain.min(), domain.max(), true);
                    let indices = stack_indices.entry(run.id.layer).or_insert_with(|| {
                        let aligned = self
                            .runs
                            .iter()
                            .filter(|other| {
                                other.id.layer == run.id.layer && other.facet == run.facet
                            })
                            .all(|other| {
                                other.rows.len() == run.rows.len()
                                    && other.rows.iter().zip(&run.rows).all(|(a, b)| a.x == b.x)
                            });
                        if !aligned {
                            return None;
                        }
                        let mut indices = Vec::new();
                        let budget = (rect.width().ceil() as usize).clamp(3, 4096);
                        lttb(
                            visible.len(),
                            budget,
                            |i| {
                                let row = &run.rows[visible.start + i];
                                (row.x, row.top)
                            },
                            &mut indices,
                        );
                        Some(
                            indices
                                .into_iter()
                                .map(|i| i + visible.start)
                                .collect::<Vec<_>>(),
                        )
                    });
                    if let Some(indices) = indices {
                        projection.connected_path(indices.iter().copied(), &mut panel.draws);
                    } else {
                        // Preserve all source knots when series do not share an x grid.
                        projection.connected_path(visible, &mut panel.draws);
                    }
                    continue;
                }
                projection.generate(&mut panel.draws, &mut scratch);
            }
            panels.push(panel);
        }
        Scene { panels }
    }

    pub(crate) fn nearest(
        &self,
        panel: &Panel,
        domain: Domain,
        hidden: &BTreeSet<SeriesId>,
        cursor: Position,
        radius: f32,
    ) -> Option<Hit> {
        let domains = [domain, self.domains[1], self.domains[2]];
        let x_scale = &self.axes[0].scale;
        let min = x_scale.invert(domain, panel.rect.fraction(cursor.0 - radius));
        let max = x_scale.invert(domain, panel.rect.fraction(cursor.0 + radius));
        let mut best = None;
        let mut distance = radius * radius;
        for run in self
            .runs
            .iter()
            .filter(|r| r.facet == panel.facet && !hidden.contains(&r.id))
        {
            let projection = Projection {
                chart: self,
                domains,
                rect: panel.rect,
                run,
            };
            let range = match run.mark {
                Mark::Bar(_, Orientation::Vertical) => {
                    let reach = radius + projection.thickness(0) / 2.;
                    let left = x_scale.invert(domain, panel.rect.fraction(cursor.0 - reach));
                    let right = x_scale.invert(domain, panel.rect.fraction(cursor.0 + reach));
                    run.visible(left, right, false)
                }
                _ => run.visible(min, max, false),
            };
            for row in &run.rows[range] {
                if let Some(mut point) = projection.anchor(row) {
                    let mut d = (point.0 - cursor.0).powi(2) + (point.1 - cursor.1).powi(2);
                    if let Mark::Bar(arrangement, orientation) = run.mark {
                        let (a, b) = projection.bar_bounds(row, point, arrangement, orientation);
                        if panel.rect.contains(cursor)
                            && (a.0.min(b.0)..=a.0.max(b.0)).contains(&cursor.0)
                            && (a.1.min(b.1)..=a.1.max(b.1)).contains(&cursor.1)
                        {
                            point = cursor;
                            d = 0.;
                        }
                    }
                    if !panel.rect.contains(point) {
                        continue;
                    }
                    if d <= distance {
                        distance = d;
                        best = Some(Hit {
                            id: row.id,
                            point,
                            x: row.x,
                            y: row.y,
                            secondary: run.secondary,
                        });
                    }
                }
            }
        }
        best
    }
}

impl Run {
    fn visible(&self, min: f64, max: f64, neighbors: bool) -> Range<usize> {
        let start = self.rows.partition_point(|r| r.x < min - self.max_x_extent);
        let end = self
            .rows
            .partition_point(|r| r.x <= max + self.max_x_extent);
        let adjacent = usize::from(neighbors);
        start.saturating_sub(adjacent)..end.saturating_add(adjacent).min(self.rows.len())
    }

    pub(crate) fn build_levels(&mut self) {
        if !self.mark.connected() || !self.line_breaks.is_empty() {
            return;
        }
        let mut len = self.rows.len();
        let mut scratch = Vec::new();
        while len > 16_384 {
            let previous = self.levels.last();
            lttb(
                len,
                len.div_ceil(8),
                |i| {
                    let row = &self.rows[previous.map_or(i, |level| level[i])];
                    (row.x, row.top)
                },
                &mut scratch,
            );
            let level = scratch
                .iter()
                .map(|&i| previous.map_or(i, |level| level[i]))
                .collect();
            self.levels.push(level);
            len = scratch.len();
        }
    }
}

struct Projection<'a> {
    chart: &'a PreparedChart,
    domains: [Domain; 3],
    rect: Rect,
    run: &'a Run,
}

impl Projection<'_> {
    fn y_axis(&self) -> usize {
        if self.run.secondary { 2 } else { 1 }
    }

    fn xy(&self, x: f64, y: f64) -> Option<Position> {
        let axis = self.y_axis();
        let x = self.chart.axes[0].scale.project(self.domains[0], x)?;
        let y = self.chart.axes[axis].scale.project(self.domains[axis], y)?;
        Some(self.rect.project(x, y))
    }

    fn anchor(&self, row: &Sample) -> Option<Position> {
        match self.run.mark {
            Mark::Area(_) | Mark::Bar(_, Orientation::Vertical) => self.xy(row.x, row.top),
            Mark::Bar(_, Orientation::Horizontal) => self.xy(row.top, row.y),
            Mark::Candlestick => self.xy(row.x, row.ohlc?.close),
            _ => self.xy(row.x, row.y),
        }
    }

    fn color(&self, row: &Sample) -> Color {
        row.color.unwrap_or(Color::Category(self.run.palette))
    }

    fn generate(&self, out: &mut Vec<Draw>, scratch: &mut Vec<usize>) {
        let domain = self.domains[0];
        let visible = if self.run.mark == Mark::Rule(Orientation::Horizontal) {
            0..self.run.rows.len()
        } else {
            self.run
                .visible(domain.min(), domain.max(), self.run.mark.connected())
        };
        if visible.is_empty() {
            return;
        }
        if self.run.mark.connected() {
            let mut start = visible.start;
            for end in self
                .run
                .line_breaks
                .iter()
                .copied()
                .filter(|&i| visible.start < i && i < visible.end)
                .chain(std::iter::once(visible.end))
            {
                self.connected(start..end, out, scratch);
                start = end;
            }
            return;
        }
        if matches!(self.run.mark, Mark::Point | Mark::Bubble) && visible.len() > 8_192 {
            self.scatter(visible, out);
            return;
        }
        for row in &self.run.rows[visible] {
            self.individual(row, out);
        }
    }

    fn connected(&self, visible: Range<usize>, out: &mut Vec<Draw>, scratch: &mut Vec<usize>) {
        let budget = (self.rect.width().ceil() as usize).clamp(3, 4096);
        let mut source = None;
        let mut range = visible.clone();
        for level in &self.run.levels {
            if range.len() <= budget * 8 {
                break;
            }
            let start = level
                .partition_point(|&i| i < visible.start)
                .saturating_sub(1);
            let end = level
                .partition_point(|&i| i < visible.end)
                .saturating_add(1)
                .min(level.len());
            if end.saturating_sub(start) < budget {
                break;
            }
            source = Some(level);
            range = start..end;
        }
        let row =
            |i| &self.run.rows[source.map_or(range.start + i, |level| level[range.start + i])];
        lttb(
            range.len(),
            budget,
            |i| {
                let (x, y) = self.anchor(row(i)).unwrap_or_default();
                (f64::from(x), f64::from(y))
            },
            scratch,
        );
        self.connected_path(
            scratch
                .iter()
                .map(|&i| source.map_or(range.start + i, |level| level[range.start + i])),
            out,
        );
    }

    fn connected_path(
        &self,
        indices: impl DoubleEndedIterator<Item = usize> + Clone,
        out: &mut Vec<Draw>,
    ) {
        let row = |i| &self.run.rows[i];
        let Some(first) = indices.clone().next() else {
            return;
        };
        let area = matches!(self.run.mark, Mark::Area(_));
        let mut points: Vec<_> = indices
            .clone()
            .filter_map(|i| self.anchor(row(i)))
            .collect();
        if area {
            points.extend(indices.rev().filter_map(|i| {
                let row = row(i);
                self.xy(row.x, row.base)
                    .or_else(|| self.xy(row.x, self.domains[self.y_axis()].min()))
            }));
        }
        if points.len() > 1 {
            out.push(Draw {
                series: self.run.id,
                geometry: Geometry::Path {
                    points,
                    closed: area,
                },
                color: self.color(row(first)),
            });
        } else if let Some(point) = points.first() {
            out.push(Draw {
                series: self.run.id,
                geometry: Geometry::Dot(*point, 2.),
                color: self.color(row(first)),
            });
        }
    }

    fn scatter(&self, visible: Range<usize>, out: &mut Vec<Draw>) {
        // One representative per screen cell. The source-backed hit index retains all points.
        let columns = (self.rect.width() / 6.).clamp(1., 128.) as usize;
        let rows = (self.rect.height() / 6.).clamp(1., 64.) as usize;
        let mut occupied = vec![false; columns * rows];
        for row in &self.run.rows[visible] {
            let Some(point) = self.anchor(row).filter(|&p| self.rect.contains(p)) else {
                continue;
            };
            let x = (self.rect.fraction(point.0) * columns as f64) as usize;
            let y = ((point.1 - self.rect.top) / self.rect.height() * rows as f32) as usize;
            let cell = y.min(rows - 1) * columns + x.min(columns - 1);
            if !occupied[cell] {
                occupied[cell] = true;
                self.individual(row, out);
            }
        }
    }

    fn thickness(&self, axis: usize) -> f32 {
        let length = if axis == 0 {
            self.rect.width()
        } else {
            self.rect.height()
        };
        let band = self.chart.axes[axis]
            .scale
            .band_fraction(self.domains[axis]) as f32
            * length;
        if band > 0. { band } else { 12. }
    }

    fn bar_bounds(
        &self,
        row: &Sample,
        p: Position,
        arrangement: Arrangement,
        orientation: Orientation,
    ) -> (Position, Position) {
        let horizontal = orientation == Orientation::Horizontal;
        let axis = if horizontal { self.y_axis() } else { 0 };
        let mut width = self.thickness(axis);
        let mut offset = 0.;
        if arrangement == Arrangement::Group {
            width /= self.run.slots as f32;
            offset = (self.run.slot as f32 - (self.run.slots - 1) as f32 / 2.) * width;
        }
        let base = if horizontal {
            self.xy(row.base, row.y).map_or(self.rect.left, |p| p.0)
        } else {
            self.xy(row.x, row.base).map_or(self.rect.bottom, |p| p.1)
        };
        let half = width / 2.;
        if horizontal {
            ((base, p.1 + offset - half), (p.0, p.1 + offset + half))
        } else {
            ((p.0 + offset - half, base), (p.0 + offset + half, p.1))
        }
    }

    fn individual(&self, row: &Sample, out: &mut Vec<Draw>) {
        let Some(p) = self.anchor(row) else { return };
        let color = self.color(row);
        let mut push = |geometry| {
            out.push(Draw {
                geometry,
                color,
                series: self.run.id,
            })
        };
        match self.run.mark {
            Mark::Line | Mark::Area(_) => {}
            Mark::Point | Mark::Bubble if self.rect.contains(p) => {
                let radius = if self.run.mark == Mark::Point {
                    3.
                } else {
                    row.size.clamp(1., 48.)
                };
                if radius.is_finite() {
                    push(Geometry::Dot(p, radius));
                }
            }
            Mark::Text if self.rect.contains(p) => {
                if let Some(text) = &row.text {
                    push(Geometry::Text(p, text.clone()));
                }
            }
            Mark::Bar(arrangement, orientation) => {
                let (start, end) = self.bar_bounds(row, p, arrangement, orientation);
                push(Geometry::Box(start, end));
            }
            Mark::Rect => {
                let other = row.x2.zip(row.y2).and_then(|(x, y)| self.xy(x, y));
                if let Some(other) = other {
                    push(Geometry::Box(p, other));
                } else if self.rect.contains(p) {
                    let x = self.thickness(0) / 2.;
                    let y = self.thickness(self.y_axis()) / 2.;
                    push(Geometry::Box((p.0 - x, p.1 - y), (p.0 + x, p.1 + y)));
                }
            }
            Mark::Rule(Orientation::Vertical) => {
                push(Geometry::Segment(
                    (p.0, self.rect.top),
                    (p.0, self.rect.bottom),
                ));
            }
            Mark::Rule(Orientation::Horizontal) => {
                push(Geometry::Segment(
                    (self.rect.left, p.1),
                    (self.rect.right, p.1),
                ));
            }
            Mark::ErrorBar => {
                if let Some((low, high)) = row
                    .low
                    .zip(row.y2)
                    .and_then(|(lo, hi)| self.xy(row.x, lo).zip(self.xy(row.x, hi)))
                {
                    push(Geometry::Segment(low, high));
                    for (x, y) in [low, high] {
                        push(Geometry::Segment((x - 4., y), (x + 4., y)));
                    }
                }
            }
            Mark::Candlestick => {
                if let Some(o) = row.ohlc
                    && let Some((low, high)) = self.xy(row.x, o.low).zip(self.xy(row.x, o.high))
                    && let Some((open, close)) = self.xy(row.x, o.open).zip(self.xy(row.x, o.close))
                {
                    push(Geometry::Segment(low, high));
                    let thickness = self.thickness(0) * 0.35;
                    let bottom = if (open.1 - close.1).abs() < 1. {
                        open.1 + 1.
                    } else {
                        close.1
                    };
                    if o.close >= o.open {
                        let a = (p.0 - thickness, open.1);
                        let b = (p.0 + thickness, bottom);
                        push(Geometry::Path {
                            points: vec![a, (b.0, a.1), b, (a.0, b.1), a],
                            closed: false,
                        });
                    } else {
                        push(Geometry::Box(
                            (p.0 - thickness, open.1),
                            (p.0 + thickness, bottom),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channels, ChartSpec, Layer};

    #[test]
    fn nan_splits_a_line_into_two_paths() {
        let chart = ChartSpec::new("gaps")
            .layer(Layer::new(
                Mark::Line,
                vec![(0., 1.), (1., 2.), (2., f64::NAN), (3., 1.), (4., 2.)],
                Channels::new(|p: &(f64, f64)| p.0, |p| p.1),
            ))
            .prepare();
        let scene = chart.scene(600., 300., chart.x_domain(), &BTreeSet::new());
        assert_eq!(scene.primitive_count(), 2);
        assert_eq!(scene.vertex_count(), 4);
    }

    #[test]
    fn stacked_area_paths_share_sampled_x_coordinates() {
        let model = ChartSpec::new("stack")
            .layer(Layer::new(
                Mark::Area(Arrangement::Stack),
                (0..1000)
                    .flat_map(|x| {
                        [
                            (x as f64, 2. + (x as f64).sin(), 0),
                            (x as f64, 2. + (x as f64 / 7.).cos(), 1),
                        ]
                    })
                    .collect::<Vec<_>>(),
                Channels::new(|p: &(f64, f64, u32)| p.0, |p| p.1).group(|p| p.2),
            ))
            .prepare();
        let scene = model.scene(300., 300., model.x_domain(), &BTreeSet::new());
        let draws = &scene.panels[0].draws;
        let Geometry::Path { points: first, .. } = &draws[0].geometry else {
            panic!("expected area")
        };
        let Geometry::Path { points: second, .. } = &draws[1].geometry else {
            panic!("expected area")
        };
        let count = first.len() / 2;
        assert!(count < 1000);
        assert_eq!(
            &first[..count],
            second[count..].iter().rev().copied().collect::<Vec<_>>()
        );
    }

    #[test]
    fn left_margin_grows_for_long_axis_title() {
        let mut spec = ChartSpec::new("load");
        spec.y.title = "CPU load (%)".into();
        let model = spec.prepare();
        let scene = model.scene(600., 300., model.x_domain(), &BTreeSet::new());
        assert!(scene.panels[0].rect.left > 75.);
    }

    #[test]
    fn nearest_hit_prefers_closer_candidate_within_radius() {
        let mut spec = ChartSpec::new("points").layer(Layer::new(
            Mark::Point,
            vec![(5., 5.), (5.15, 5.)],
            Channels::new(|p: &(f64, f64)| p.0, |p| p.1),
        ));
        spec.x.domain = Domain::new(0., 10.);
        spec.y.domain = Domain::new(0., 10.);
        let model = spec.prepare();
        let domain = model.x_domain();
        let hidden = BTreeSet::new();
        let scene = model.scene(600., 300., domain, &hidden);
        let panel = &scene.panels[0];
        let center = (
            (panel.rect.left + panel.rect.right) / 2.,
            (panel.rect.top + panel.rect.bottom) / 2.,
        );
        assert_eq!(
            model
                .nearest(panel, domain, &hidden, center, 20.)
                .unwrap()
                .id
                .key,
            0
        );
        assert_eq!(
            model
                .nearest(panel, domain, &hidden, (center.0 + 7., center.1), 20.)
                .unwrap()
                .id
                .key,
            1
        );
    }

    #[test]
    fn bar_hit_uses_its_interior() {
        let model = ChartSpec::new("bars")
            .layer(Layer::new(
                Mark::Bar(Arrangement::Overlap, Orientation::Vertical),
                vec![(1., 100.)],
                Channels::new(|p: &(f64, f64)| p.0, |p| p.1),
            ))
            .prepare();
        let hidden = BTreeSet::new();
        let domain = model.x_domain();
        let scene = model.scene(600., 300., domain, &hidden);
        let panel = &scene.panels[0];
        let Geometry::Box(a, b) = panel.draws[0].geometry else {
            panic!("expected bar");
        };
        let center = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
        assert!(model.nearest(panel, domain, &hidden, center, 8.).is_some());
    }

    #[test]
    fn viewport_culls_and_downsamples_connected_runs() {
        let rows = (0..160_000)
            .map(|i| (f64::from(i), f64::from(i).sin()))
            .collect::<Vec<_>>();
        let model = ChartSpec::default()
            .layer(Layer::new(
                Mark::Line,
                rows,
                Channels::new(|r: &(f64, f64)| r.0, |r| r.1),
            ))
            .prepare();
        assert!(!model.runs[0].levels.is_empty());
        let Some(domain) = Domain::new(600., 660.) else {
            panic!("valid viewport")
        };
        let scene = model.scene(900., 370., domain, &BTreeSet::new());
        assert_eq!(scene.primitive_count(), 1);
        assert_eq!(scene.vertex_count(), 63);
        assert!(
            model
                .scene(900., 370., model.x_domain(), &BTreeSet::new())
                .vertex_count()
                <= 900
        );
    }

    #[test]
    fn categorical_facets_keep_secondary_domain_and_visibility() {
        let rows = vec![(0., 10., 0), (1., 200., 1)];
        let mut spec = ChartSpec::new("facets").layer(
            Layer::new(
                Mark::Point,
                rows,
                Channels::new(|r: &(f64, f64, u32)| r.0, |r| r.1).facet(|r| r.2),
            )
            .secondary_axis(true),
        );
        spec.x.scale = crate::Scale::Ordinal(Arc::from(["A".into(), "B".into()]));
        let model = spec.prepare();
        let scene = model.scene(800., 300., model.x_domain(), &BTreeSet::new());
        assert_eq!(scene.panels.len(), 2);
        assert!(!scene.panels[0].ticks[2].is_empty());
        assert_eq!(scene.primitive_count(), 2);
        let hidden = BTreeSet::from([SeriesId::default()]);
        assert_eq!(
            model
                .scene(800., 300., model.x_domain(), &hidden)
                .primitive_count(),
            0
        );
    }
}
