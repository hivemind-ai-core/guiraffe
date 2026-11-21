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

type Visit = dyn Fn(usize, Option<usize>, &dyn Fn() -> bool, &mut dyn FnMut(Sample)) + Send + Sync;

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
        let visit = Box::new(
            move |layer,
                  limit: Option<usize>,
                  cancelled: &dyn Fn() -> bool,
                  emit: &mut dyn FnMut(Sample)| {
                let count = source.len();
                let stride = limit.map_or(1, |limit| count.div_ceil(limit).max(1));
                for (slot, start) in (0..count).step_by(stride).enumerate() {
                    if slot.is_multiple_of(256) && cancelled() {
                        return;
                    }
                    let remaining = count - start;
                    let offset = if stride == 1 || slot == 0 {
                        0
                    } else if remaining <= stride {
                        remaining - 1
                    } else {
                        slot.wrapping_mul(41) % stride
                    };
                    let index = start + offset;
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
            },
        );
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

#[derive(Default)]
pub(crate) struct Coordinates {
    x: Vec<f64>,
    y: Vec<f64>,
    ids: Vec<(usize, u64)>,
    first: Option<DatumId>,
    stride: (usize, u64),
}

impl Coordinates {
    fn id(&self, index: usize, first: DatumId) -> DatumId {
        let (index, key) = self.ids.get(index).copied().unwrap_or_else(|| {
            self.regular_id(first, index)
                .unwrap_or((first.index, first.key))
        });
        DatumId {
            index,
            key,
            ..first
        }
    }

    fn regular_id(&self, first: DatumId, offset: usize) -> Option<(usize, u64)> {
        Some((
            first
                .index
                .checked_add(self.stride.0.checked_mul(offset)?)?,
            first
                .key
                .checked_add(self.stride.1.checked_mul(offset as u64)?)?,
        ))
    }

    fn push(&mut self, row: Sample) {
        let first = *self.first.get_or_insert(row.id);
        let len = self.x.len();
        if len == 1 {
            self.stride = (
                row.id.index.saturating_sub(first.index),
                row.id.key.saturating_sub(first.key),
            );
        }
        if self.ids.is_empty() && self.regular_id(first, len) != Some((row.id.index, row.id.key)) {
            self.ids = (0..len).filter_map(|i| self.regular_id(first, i)).collect();
        }
        if !self.ids.is_empty() {
            self.ids.push((row.id.index, row.id.key));
        }
        self.x.push(row.x);
        self.y.push(row.y);
    }

    fn row(&self, index: usize) -> Sample {
        Sample {
            id: self.id(
                index,
                self.first.unwrap_or(DatumId {
                    series: SeriesId::default(),
                    index: 0,
                    key: 0,
                    facet: 0,
                }),
            ),
            x: self.x[index],
            y: self.y[index],
            top: self.y[index],
            base: 0.,
            x2: None,
            y2: None,
            low: None,
            ohlc: None,
            size: 3.,
            color: None,
            text: None,
        }
    }
}

pub(crate) enum Rows {
    Plain(Coordinates),
    Styled(Vec<Sample>),
}

impl Rows {
    fn new(mark: Mark) -> Self {
        if matches!(mark, Mark::Line | Mark::Point | Mark::Bubble) {
            Self::Plain(Coordinates::default())
        } else {
            Self::Styled(Vec::new())
        }
    }

    fn push(&mut self, sample: Sample) {
        if let Self::Plain(columns) = self {
            if sample.color.is_none() && sample.size == 3. && sample.text.is_none() {
                columns.push(sample);
                return;
            }
            *self = Self::Styled((0..columns.x.len()).map(|i| columns.row(i)).collect());
        }
        if let Self::Styled(rows) = self {
            rows.push(sample);
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Plain(columns) => columns.x.len(),
            Self::Styled(rows) => rows.len(),
        }
    }

    pub fn row(&self, index: usize) -> Sample {
        match self {
            Self::Plain(columns) => columns.row(index),
            Self::Styled(rows) => rows[index].clone(),
        }
    }

    pub fn xy(&self, index: usize) -> (f64, f64) {
        match self {
            Self::Plain(c) => (c.x[index], c.y[index]),
            Self::Styled(rows) => (rows[index].x, rows[index].y),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Sample> + '_ {
        (0..self.len()).map(|i| self.row(i))
    }

    pub fn partition_x(&self, predicate: impl Fn(f64) -> bool) -> usize {
        match self {
            Self::Plain(columns) => columns.x.partition_point(|&x| predicate(x)),
            Self::Styled(rows) => rows.partition_point(|row| predicate(row.x)),
        }
    }

    fn sort(&mut self) {
        match self {
            Self::Plain(columns) if !columns.x.is_sorted() => {
                let mut order: Vec<_> = (0..columns.x.len()).collect();
                order.sort_unstable_by(|&a, &b| {
                    columns.x[a].total_cmp(&columns.x[b]).then(a.cmp(&b))
                });
                let mut sorted = Coordinates {
                    x: Vec::with_capacity(order.len()),
                    y: Vec::with_capacity(order.len()),
                    ..Coordinates::default()
                };
                for index in order {
                    sorted.push(columns.row(index));
                }
                *columns = sorted;
            }
            Self::Styled(rows) if !rows.is_sorted_by(|a, b| a.x <= b.x) => {
                rows.sort_by(|a, b| a.x.total_cmp(&b.x));
            }
            _ => {}
        }
    }
}

pub(crate) struct Run {
    pub pick_bounds: Vec<(f64, f64)>,
    pub mark: Mark,
    pub id: SeriesId,
    pub facet: u32,
    pub secondary: bool,
    pub palette: u32,
    pub slot: usize,
    pub slots: usize,
    pub rows: Rows,
    pub max_x_extent: f64,
    pub levels: Vec<Vec<usize>>,
    pub line_breaks: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct LegendEntry {
    pub id: SeriesId,
    pub label: String,
    pub palette: u32,
    pub secondary: bool,
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
        self.prepare_with_cancel(&|| false)
    }

    pub(crate) fn prepare_with_cancel(&self, cancelled: &impl Fn() -> bool) -> PreparedChart {
        self.prepare_samples(None, cancelled)
    }

    pub(crate) fn preview(&self) -> PreparedChart {
        if self.layers.len() > 64
            || self
                .layers
                .iter()
                .any(|layer| !matches!(layer.mark, Mark::Line | Mark::Point | Mark::Bubble))
        {
            return Self::new(self.summary.clone()).prepare();
        }
        self.prepare_samples(Some(2_048 / self.layers.len().max(1)), &|| false)
    }

    fn prepare_samples(
        &self,
        limit: Option<usize>,
        cancelled: &impl Fn() -> bool,
    ) -> PreparedChart {
        let mut extents = [Extent::default(); 3];
        let mut runs = Vec::new();
        let mut legend = Vec::new();
        let mut facets = BTreeMap::new();
        let mut count = 0;
        let mut nonpositive_log_rows = 0;
        let mut has_right_axis = false;
        'layers: for (index, layer) in self.layers.iter().enumerate() {
            if cancelled() {
                break;
            }
            let y_axis = if layer.secondary { 2 } else { 1 };
            let y_scale = if layer.secondary {
                &self.right.scale
            } else {
                &self.y.scale
            };
            let mut groups: BTreeMap<(u32, u32), Rows> = BTreeMap::new();
            let mut gaps = BTreeMap::<(u32, u32), Vec<f64>>::new();
            (layer.visit)(index, limit, cancelled, &mut |mut sample| {
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
                if layer.mark == Mark::Line && self.x.scale.accepts(sample.x) && sample.y.is_nan() {
                    gaps.entry((sample.id.facet, sample.id.series.group))
                        .or_default()
                        .push(sample.x);
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
                    .or_insert_with(|| Rows::new(layer.mark))
                    .push(sample);
            });
            if cancelled() {
                break;
            }
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
                            format!("{} · Series {}", layer.title, u64::from(group) + 1)
                        }
                    });
                    legend.push(LegendEntry {
                        id: SeriesId {
                            layer: index,
                            group,
                        },
                        label,
                        palette: color,
                        secondary: layer.secondary,
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
                for (index, row) in rows.iter().enumerate() {
                    if index.is_multiple_of(256) && cancelled() {
                        break 'layers;
                    }
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
                rows.sort();
                let mut line_breaks: Vec<_> = gaps
                    .remove(&(facet, group))
                    .into_iter()
                    .flatten()
                    .map(|x| rows.partition_x(|value| value <= x))
                    .filter(|&index| index > 0 && index < rows.len())
                    .collect();
                line_breaks.sort_unstable();
                line_breaks.dedup();
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
                    line_breaks,
                    pick_bounds: Vec::new(),
                };
                run.build_levels(cancelled);
                run.build_pick_bounds(cancelled);
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

fn arrange(mark: Mark, groups: &mut BTreeMap<(u32, u32), Rows>) {
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
        for row in groups
            .values()
            .filter_map(|rows| match rows {
                Rows::Styled(rows) => Some(rows),
                _ => None,
            })
            .flatten()
        {
            *totals.entry(key(row)).or_insert(0.) += row.top.abs();
        }
    }
    let mut previous = BTreeMap::new();
    for row in groups
        .values_mut()
        .filter_map(|rows| match rows {
            Rows::Styled(rows) => Some(rows),
            _ => None,
        })
        .flatten()
    {
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
    fn preview_limits_row_reads_and_retains_original_datum_indices() {
        let spec = ChartSpec::new("large line").layer(Layer::new(
            Mark::Line,
            (0..100_000).collect::<Vec<i32>>(),
            Channels::new(|i| f64::from(*i), |i| f64::from(*i % 10)),
        ));
        let chart = spec.preview();
        assert!(chart.len() <= 2_048);
        let rows = &chart.runs[0].rows;
        assert_eq!(rows.row(0).id.index, 0);
        assert_eq!(rows.row(rows.len() - 1).id.index, 99_999);
        assert!(rows.iter().all(|row| row.x == row.id.index as f64));
    }

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

    #[test]
    fn prepared_line_buffers_have_bounded_bytes_per_point() {
        const POINTS: usize = 100_000;
        let chart = ChartSpec::new("storage")
            .layer(Layer::new(
                Mark::Line,
                (0..POINTS).map(|index| index as f64).collect::<Vec<_>>(),
                Channels::new(|x| *x, |x| x.sin()),
            ))
            .prepare();
        let run = &chart.runs[0];
        let Rows::Plain(columns) = &run.rows else {
            panic!("expected compact storage")
        };
        let bytes = (columns.x.capacity() + columns.y.capacity()) * size_of::<f64>()
            + columns.ids.capacity() * size_of::<(usize, u64)>()
            + run
                .levels
                .iter()
                .map(|level| level.capacity())
                .sum::<usize>()
                * size_of::<usize>()
            + run.line_breaks.capacity() * size_of::<usize>()
            + run.pick_bounds.capacity() * size_of::<(f64, f64)>();
        assert!(
            bytes <= POINTS * 24,
            "prepared line buffers: {} bytes per point",
            bytes.div_ceil(POINTS)
        );
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
                    for row in chart.runs.iter().flat_map(|r| r.rows.iter()).filter(|r| r.y.is_sign_negative() == negative) {
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
