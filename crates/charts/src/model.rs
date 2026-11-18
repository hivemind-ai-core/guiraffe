use crate::{ChartSource, Domain, Scale};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arrangement {
    #[default]
    Overlap,
    Group,
    Stack,
    Normalize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Line,
    Area(Arrangement),
    Bar(Arrangement, Orientation),
    Point,
    Bubble,
    Rect,
    Rule(Orientation),
    Text,
    ErrorBar,
    Candlestick,
}

impl Mark {
    pub(crate) fn arrangement(self) -> Arrangement {
        match self {
            Self::Area(arrangement) | Self::Bar(arrangement, _) => arrangement,
            _ => Arrangement::Overlap,
        }
    }

    pub(crate) fn horizontal(self) -> bool {
        matches!(self, Self::Bar(_, Orientation::Horizontal))
    }

    pub(crate) fn connected(self) -> bool {
        matches!(self, Self::Line | Self::Area(_))
    }
}

/// Guide and scale for an axis. Explicit domains take precedence over inferred nice domains.
#[derive(Clone, Debug)]
pub struct AxisSpec {
    pub scale: Scale,
    pub domain: Option<Domain>,
    pub title: String,
    pub grid: bool,
    pub nice: bool,
}

impl Default for AxisSpec {
    fn default() -> Self {
        Self {
            scale: Scale::Linear,
            domain: None,
            title: String::new(),
            grid: true,
            nice: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeriesId {
    pub layer: usize,
    pub group: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatumId {
    pub series: SeriesId,
    pub index: usize,
    pub key: u64,
    pub facet: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Category(u32),
    /// Fraction on a theme-derived sequential ramp. Finite values are clamped.
    Continuous(f32),
}

#[derive(Clone, Copy, Debug)]
pub struct Ohlc {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

type Accessor<T, V> = Box<dyn Fn(&T) -> V + Send + Sync>;
type TextAccessor<T> = Box<dyn for<'a> Fn(&'a T) -> &'a str + Send + Sync>;

/// Typed accessors evaluated only during preparation.
pub struct Channels<T> {
    x: Accessor<T, f64>,
    y: Accessor<T, f64>,
    group: Accessor<T, u32>,
    facet: Accessor<T, u32>,
    color: Accessor<T, Option<Color>>,
    size: Accessor<T, f32>,
    x2: Accessor<T, Option<f64>>,
    y2: Accessor<T, Option<f64>>,
    low: Accessor<T, Option<f64>>,
    ohlc: Accessor<T, Option<Ohlc>>,
    text: TextAccessor<T>,
}

impl<T> Channels<T> {
    pub fn new(
        x: impl Fn(&T) -> f64 + Send + Sync + 'static,
        y: impl Fn(&T) -> f64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            x: Box::new(x),
            y: Box::new(y),
            group: Box::new(|_| 0),
            facet: Box::new(|_| 0),
            color: Box::new(|_| None),
            size: Box::new(|_| 3.),
            x2: Box::new(|_| None),
            y2: Box::new(|_| None),
            low: Box::new(|_| None),
            ohlc: Box::new(|_| None),
            text: Box::new(|_| ""),
        }
    }

    pub fn group(mut self, get: impl Fn(&T) -> u32 + Send + Sync + 'static) -> Self {
        self.group = Box::new(get);
        self
    }

    pub fn facet(mut self, get: impl Fn(&T) -> u32 + Send + Sync + 'static) -> Self {
        self.facet = Box::new(get);
        self
    }

    pub fn color(mut self, get: impl Fn(&T) -> Color + Send + Sync + 'static) -> Self {
        self.color = Box::new(move |row| Some(get(row)));
        self
    }

    pub fn size(mut self, get: impl Fn(&T) -> f32 + Send + Sync + 'static) -> Self {
        self.size = Box::new(get);
        self
    }

    /// Upper x/y endpoints for rects. Y is also the upper endpoint of an error bar.
    pub fn endpoints(
        mut self,
        x: impl Fn(&T) -> f64 + Send + Sync + 'static,
        y: impl Fn(&T) -> f64 + Send + Sync + 'static,
    ) -> Self {
        self.x2 = Box::new(move |row| Some(x(row)));
        self.y2 = Box::new(move |row| Some(y(row)));
        self
    }

    pub fn interval(
        mut self,
        low: impl Fn(&T) -> f64 + Send + Sync + 'static,
        high: impl Fn(&T) -> f64 + Send + Sync + 'static,
    ) -> Self {
        self.low = Box::new(move |row| Some(low(row)));
        self.y2 = Box::new(move |row| Some(high(row)));
        self
    }

    pub fn ohlc(mut self, get: impl Fn(&T) -> Ohlc + Send + Sync + 'static) -> Self {
        self.ohlc = Box::new(move |row| Some(get(row)));
        self
    }

    pub fn text(mut self, get: impl for<'a> Fn(&'a T) -> &'a str + Send + Sync + 'static) -> Self {
        self.text = Box::new(get);
        self
    }
}

type Visit = dyn Fn(usize, &mut dyn FnMut(Sample)) + Send + Sync;

/// Heterogeneous, mark-local source and channels.
pub struct Layer {
    mark: Mark,
    title: String,
    secondary: bool,
    group_labels: BTreeMap<u32, String>,
    visit: Box<Visit>,
}

impl Layer {
    pub fn new<S: ChartSource>(mark: Mark, source: S, channels: Channels<S::Row>) -> Self {
        let visit = Box::new(move |layer, emit: &mut dyn FnMut(Sample)| {
            for index in 0..source.len() {
                let Some(row) = source.row(index) else {
                    continue;
                };
                let group = (channels.group)(row);
                let text = (channels.text)(row);
                emit(Sample {
                    id: DatumId {
                        series: SeriesId { layer, group },
                        index,
                        key: source.key(index),
                        facet: (channels.facet)(row),
                    },
                    x: (channels.x)(row),
                    y: (channels.y)(row),
                    x2: (channels.x2)(row),
                    y2: (channels.y2)(row),
                    low: (channels.low)(row),
                    ohlc: (channels.ohlc)(row),
                    base: 0.,
                    top: 0.,
                    size: (channels.size)(row),
                    color: (channels.color)(row),
                    text: (!text.is_empty()).then(|| Arc::from(text)),
                });
            }
        });
        Self {
            mark,
            title: String::new(),
            secondary: false,
            group_labels: BTreeMap::new(),
            visit,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn group_label(mut self, group: u32, title: impl Into<String>) -> Self {
        self.group_labels.insert(group, title.into());
        self
    }

    pub fn secondary_axis(mut self, secondary: bool) -> Self {
        self.secondary = secondary;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Sample {
    pub id: DatumId,
    pub x: f64,
    pub y: f64,
    pub x2: Option<f64>,
    pub y2: Option<f64>,
    pub low: Option<f64>,
    pub ohlc: Option<Ohlc>,
    pub base: f64,
    pub top: f64,
    pub size: f32,
    pub color: Option<Color>,
    pub text: Option<Arc<str>>,
}

pub(crate) struct Run {
    pub mark: Mark,
    pub id: SeriesId,
    pub facet: u32,
    pub secondary: bool,
    pub palette: u32,
    pub slot: usize,
    pub slots: usize,
    pub rows: Vec<Sample>,
    pub max_x_extent: f64,
    pub levels: Vec<Vec<usize>>,
}

#[derive(Clone, Debug)]
pub struct LegendEntry {
    pub id: SeriesId,
    pub label: String,
    pub palette: u32,
}

#[derive(Default)]
pub struct ChartSpec {
    pub summary: String,
    pub x: AxisSpec,
    pub y: AxisSpec,
    pub right: AxisSpec,
    pub layers: Vec<Layer>,
    pub facet_labels: BTreeMap<u32, String>,
}

impl ChartSpec {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            ..Self::default()
        }
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layers.push(layer);
        self
    }

    pub fn prepare(&self) -> PreparedChart {
        let mut extents = [Extent::default(); 3];
        let mut runs = Vec::new();
        let mut legend = Vec::new();
        let mut facets = BTreeMap::new();
        let mut count = 0;
        let mut nonpositive_log_rows = 0;
        let mut has_right_axis = false;
        for (index, layer) in self.layers.iter().enumerate() {
            let y_axis = if layer.secondary { 2 } else { 1 };
            let y_scale = if layer.secondary {
                &self.right.scale
            } else {
                &self.y.scale
            };
            let mut groups: BTreeMap<(u32, u32), Vec<Sample>> = BTreeMap::new();
            (layer.visit)(index, &mut |mut sample| {
                let invalid_log = |scale: &Scale, value: f64| {
                    matches!(scale, Scale::Log) && value.is_finite() && value <= 0.
                };
                if invalid_log(&self.x.scale, sample.x)
                    || invalid_log(y_scale, sample.y)
                    || (matches!(layer.mark, Mark::Candlestick)
                        && sample.ohlc.is_some_and(|o| {
                            [o.open, o.high, o.low, o.close]
                                .into_iter()
                                .any(|value| invalid_log(y_scale, value))
                        }))
                {
                    nonpositive_log_rows += 1;
                    return;
                }
                if !self.x.scale.accepts(sample.x) || !y_scale.accepts(sample.y) {
                    return;
                }
                if matches!(layer.mark, Mark::Candlestick)
                    && !sample.ohlc.is_some_and(|o| {
                        [o.open, o.high, o.low, o.close]
                            .into_iter()
                            .all(|v| y_scale.accepts(v))
                            && o.low <= o.open.min(o.close)
                            && o.high >= o.open.max(o.close)
                    })
                {
                    return;
                }
                sample.top = if layer.mark.horizontal() {
                    sample.x
                } else {
                    sample.y
                };
                count += 1;
                groups
                    .entry((sample.id.facet, sample.id.series.group))
                    .or_default()
                    .push(sample);
            });
            arrange(layer.mark, &mut groups);
            let mut palette = BTreeMap::new();
            let mut slots = BTreeMap::new();
            for &(facet, group) in groups.keys() {
                *slots.entry(facet).or_insert(0) += 1;
                palette.entry(group).or_insert_with(|| {
                    let color = legend.len() as u32;
                    let label = layer.group_labels.get(&group).cloned().unwrap_or_else(|| {
                        if layer.title.is_empty() {
                            format!("Layer {} · Series {}", index + 1, u64::from(group) + 1)
                        } else if group == 0 {
                            layer.title.clone()
                        } else {
                            format!("{} · {}", layer.title, group)
                        }
                    });
                    legend.push(LegendEntry {
                        id: SeriesId {
                            layer: index,
                            group,
                        },
                        label,
                        palette: color,
                    });
                    color
                });
            }
            let mut next_slot = BTreeMap::new();
            for ((facet, group), mut rows) in groups {
                let slot = next_slot.entry(facet).or_insert(0);
                let mut max_x_extent: f64 = 0.;
                facets.entry(facet).or_insert_with(|| {
                    self.facet_labels
                        .get(&facet)
                        .cloned()
                        .unwrap_or_else(|| format!("Facet {facet}"))
                });
                has_right_axis |= layer.secondary;
                for row in &rows {
                    let normalize = layer.mark.arrangement() == Arrangement::Normalize;
                    if !(normalize && layer.mark.horizontal()) {
                        extents[0].add(&self.x.scale, row.x);
                    }
                    if !(normalize && !layer.mark.horizontal()) {
                        extents[y_axis].add(y_scale, row.y);
                    }
                    match layer.mark {
                        Mark::Bar(_, Orientation::Horizontal) => {
                            extents[0].add(&self.x.scale, row.base);
                            extents[0].add(&self.x.scale, row.top);
                            max_x_extent = max_x_extent
                                .max((row.top - row.x).abs())
                                .max((row.base - row.x).abs());
                        }
                        Mark::Area(_) | Mark::Bar(_, _) => {
                            extents[y_axis].add(y_scale, row.base);
                            extents[y_axis].add(y_scale, row.top);
                        }
                        Mark::Rect => {
                            if let Some(x2) = row.x2.filter(|&x| x.is_finite()) {
                                extents[0].add(&self.x.scale, x2);
                                max_x_extent = max_x_extent.max((x2 - row.x).abs());
                            }
                            if let Some(y2) = row.y2 {
                                extents[y_axis].add(y_scale, y2);
                            }
                        }
                        Mark::ErrorBar => {
                            for value in [row.low, row.y2].into_iter().flatten() {
                                extents[y_axis].add(y_scale, value);
                            }
                        }
                        Mark::Candlestick => {
                            if let Some(ohlc) = row.ohlc {
                                extents[y_axis].add(y_scale, ohlc.low);
                                extents[y_axis].add(y_scale, ohlc.high);
                            }
                        }
                        _ => {}
                    }
                }
                if !rows.is_sorted_by(|a, b| a.x <= b.x) {
                    rows.sort_by(|a, b| a.x.total_cmp(&b.x));
                }
                let mut run = Run {
                    mark: layer.mark,
                    id: SeriesId {
                        layer: index,
                        group,
                    },
                    facet,
                    secondary: layer.secondary,
                    palette: palette[&group],
                    slot: *slot,
                    slots: slots[&facet],
                    rows,
                    max_x_extent,
                    levels: Vec::new(),
                };
                run.build_levels();
                runs.push(run);
                *slot += 1;
            }
        }
        if facets.is_empty() {
            facets.insert(0, String::new());
        }
        let axes = [self.x.clone(), self.y.clone(), self.right.clone()];
        let domains = std::array::from_fn(|i| extents[i].resolve(&axes[i]));
        PreparedChart {
            summary: self.summary.clone(),
            axes,
            domains,
            facets: facets.into_iter().collect(),
            legend,
            count,
            nonpositive_log_rows,
            has_right_axis,
            runs,
        }
    }
}

pub struct PreparedChart {
    pub(crate) summary: String,
    pub(crate) axes: [AxisSpec; 3],
    pub(crate) domains: [Domain; 3],
    pub(crate) facets: Vec<(u32, String)>,
    pub(crate) legend: Vec<LegendEntry>,
    pub(crate) count: usize,
    nonpositive_log_rows: usize,
    pub(crate) has_right_axis: bool,
    pub(crate) runs: Vec<Run>,
}

impl PreparedChart {
    /// Number of source rows rejected for a nonpositive value on a log axis.
    pub fn nonpositive_log_rows(&self) -> usize {
        self.nonpositive_log_rows
    }

    pub fn x_domain(&self) -> Domain {
        self.domains[0]
    }

    pub fn legend(&self) -> &[LegendEntry] {
        &self.legend
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[derive(Clone, Copy)]
struct Extent {
    min: f64,
    max: f64,
}

impl Default for Extent {
    fn default() -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }
}

impl Extent {
    fn add(&mut self, scale: &Scale, value: f64) {
        if scale.accepts(value) {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
    }

    fn resolve(self, axis: &AxisSpec) -> Domain {
        if let Some(domain) = axis
            .domain
            .filter(|d| axis.scale.project(*d, d.min()).is_some())
        {
            return domain;
        }
        let domain = axis.scale.resolve(self.min, self.max);
        if axis.nice {
            axis.scale.nice(domain, 6)
        } else {
            domain
        }
    }
}

fn arrange(mark: Mark, groups: &mut BTreeMap<(u32, u32), Vec<Sample>>) {
    let arrangement = mark.arrangement();
    if !matches!(arrangement, Arrangement::Stack | Arrangement::Normalize) {
        return;
    }
    let horizontal = mark.horizontal();
    let key = |row: &Sample| {
        let category = if horizontal { row.y } else { row.x } + 0.;
        (row.id.facet, category.to_bits(), row.top.is_sign_negative())
    };
    let mut totals = BTreeMap::new();
    if arrangement == Arrangement::Normalize {
        for row in groups.values().flatten() {
            *totals.entry(key(row)).or_insert(0.) += row.top.abs();
        }
    }
    let mut previous = BTreeMap::new();
    for row in groups.values_mut().flatten() {
        let key = key(row);
        let base = previous.entry(key).or_insert(0.);
        let total = totals.get(&key).copied().filter(|&v| v > 0.).unwrap_or(1.);
        row.base = *base;
        row.top = *base + row.top / total;
        *base = row.top;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn reports_nonpositive_log_rows() {
        let mut spec = ChartSpec::new("log").layer(Layer::new(
            Mark::Line,
            vec![(1., 0.), (2., -1.), (3., 5.)],
            Channels::new(|p: &(f64, f64)| p.0, |p| p.1),
        ));
        spec.y.scale = Scale::Log;
        let chart = spec.prepare();
        assert_eq!(chart.nonpositive_log_rows(), 2);
        assert_eq!(chart.len(), 1);
    }

    proptest! {
        #[test]
        fn stacks_diverge_and_normalize(values in proptest::collection::vec(-100f64..100., 1..25)) {
            for arrangement in [Arrangement::Stack, Arrangement::Normalize] {
                let source: Vec<_> = values.iter().copied().enumerate().collect();
                let chart = ChartSpec::default().layer(Layer::new(
                    Mark::Area(arrangement), source,
                    Channels::new(|_| 0., |row: &(usize, f64)| row.1).group(|row| row.0 as u32),
                )).prepare();
                for negative in [false, true] {
                    let mut end: f64 = 0.;
                    let mut sum: f64 = 0.;
                    for row in chart.runs.iter().flat_map(|r| &r.rows).filter(|r| r.y.is_sign_negative() == negative) {
                        prop_assert!((row.base - end).abs() < 1e-10);
                        prop_assert!((row.top - row.base).is_sign_negative() == negative || row.y == 0.);
                        end = row.top;
                        sum += row.y;
                    }
                    let expected = if arrangement == Arrangement::Normalize { sum.signum() * f64::from(sum != 0.) } else { sum };
                    prop_assert!((end - expected).abs() < 1e-9);
                }
            }
        }
    }
}
