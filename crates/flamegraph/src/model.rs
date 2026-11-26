use std::{collections::HashMap, ops::Range, sync::Arc};

pub type FrameId = u64;

#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub id: FrameId,
    pub parent: Option<FrameId>,
    pub self_value: f64,
    pub total: f64,
    pub label: &'a str,
    pub package: &'a str,
}

/// Frames in depth-first preorder. A snapshot must remain stable for the life of its index.
pub trait FrameSource: Send + Sync {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn frame(&self, index: usize) -> Frame<'_>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameClicked {
    pub id: FrameId,
    pub path: Vec<FrameId>,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidValue(FrameId),
    InvalidParent(FrameId),
    DuplicateId(FrameId),
}

#[derive(Clone, Copy, Debug)]
struct Span {
    index: usize,
    left: f64,
    right: f64,
}

#[derive(Clone, Debug)]
pub struct Segment {
    /// Present for a single frame. Aggregate segments represent several subpixel frames.
    pub id: Option<FrameId>,
    pub x: Range<f64>,
    pub highlighted: bool,
}

// Sequential IDs directly index a packed location. Reserve the high bit
// for non-root rows so a flat profile has room for over two billion spans.
#[derive(Default)]
struct SequentialLocations {
    packed: Vec<u32>,
    wide: HashMap<usize, (usize, usize)>,
}

impl SequentialLocations {
    fn get(&self, index: usize) -> Option<(usize, usize)> {
        let &value = self.packed.get(index)?;
        match value {
            u32::MAX => self.wide.get(&index).copied(),
            value if value < (1 << 31) => Some((0, value as usize)),
            value => Some((
                ((value >> 22) & 511) as usize,
                (value & ((1 << 22) - 1)) as usize,
            )),
        }
    }

    fn push(&mut self, depth: usize, offset: usize) {
        let value = if depth == 0 && offset < 1 << 31 {
            offset as u32
        } else if depth < 512 && offset < 1 << 22 {
            (1 << 31) | ((depth as u32) << 22) | offset as u32
        } else {
            u32::MAX
        };
        if value == u32::MAX {
            self.wide.insert(self.packed.len(), (depth, offset));
        }
        self.packed.push(value);
    }
}

enum Locations {
    Sequential(SequentialLocations),
    Arbitrary(HashMap<FrameId, (usize, usize)>),
}

impl Locations {
    fn get(&self, id: FrameId) -> Option<(usize, usize)> {
        match self {
            Self::Sequential(locations) => locations.get(usize::try_from(id).ok()?),
            Self::Arbitrary(locations) => locations.get(&id).copied(),
        }
    }

    fn insert(&mut self, id: FrameId, depth: usize, offset: usize) -> Result<(), LayoutError> {
        if let Self::Sequential(locations) = self {
            if id == locations.packed.len() as u64 {
                locations.push(depth, offset);
                return Ok(());
            }
            let mut index = HashMap::with_capacity(locations.packed.len() + 1);
            index.extend(
                (0..locations.packed.len()).filter_map(|id| Some((id as u64, locations.get(id)?))),
            );
            *self = Self::Arbitrary(index);
        }
        if let Self::Arbitrary(locations) = self
            && locations.insert(id, (depth, offset)).is_some()
        {
            return Err(LayoutError::DuplicateId(id));
        }
        Ok(())
    }
}

const SPAN_BLOCK: usize = 1024;

enum SpanBlock {
    Linear {
        first: Span,
        index_step: usize,
        coordinate_step: f64,
        len: usize,
    },
    Explicit(Vec<Span>),
}

impl SpanBlock {
    fn get(&self, offset: usize) -> Option<Span> {
        match self {
            Self::Linear { first, .. } if offset == 0 => Some(*first),
            Self::Linear {
                first,
                index_step,
                coordinate_step,
                len,
            } if offset < *len => {
                let delta = *coordinate_step * offset as f64;
                Some(Span {
                    index: first.index + *index_step * offset,
                    left: first.left + delta,
                    right: first.right + delta,
                })
            }
            Self::Explicit(spans) => spans.get(offset).copied(),
            _ => None,
        }
    }

    fn push(&mut self, span: Span) {
        if let Self::Linear {
            first,
            index_step,
            coordinate_step,
            len,
        } = self
        {
            if *len == 1 {
                *index_step = span.index - first.index;
                *coordinate_step = span.left - first.left;
            }
            let delta = *coordinate_step * *len as f64;
            if index_step
                .checked_mul(*len)
                .and_then(|step| first.index.checked_add(step))
                == Some(span.index)
                && (first.left + delta).to_bits() == span.left.to_bits()
                && (first.right + delta).to_bits() == span.right.to_bits()
            {
                *len += 1;
                return;
            }
            let mut spans = Vec::with_capacity((*len + 1).next_power_of_two().min(SPAN_BLOCK));
            spans.extend((0..*len).filter_map(|index| self.get(index)));
            *self = Self::Explicit(spans);
        }
        if let Self::Explicit(spans) = self {
            spans.push(span);
        }
    }
}

#[derive(Default)]
struct Spans {
    blocks: Vec<SpanBlock>,
    len: usize,
}

impl Spans {
    fn push(&mut self, span: Span) {
        if self.len.is_multiple_of(SPAN_BLOCK) {
            self.blocks.push(SpanBlock::Linear {
                first: span,
                index_step: 0,
                coordinate_step: 0.,
                len: 1,
            });
        } else if let Some(block) = self.blocks.last_mut() {
            block.push(span);
        }
        self.len += 1;
    }

    fn get(&self, index: usize) -> Option<Span> {
        self.blocks.get(index / SPAN_BLOCK)?.get(index % SPAN_BLOCK)
    }

    fn last(&self) -> Option<Span> {
        self.get(self.len.checked_sub(1)?)
    }

    fn partition_point(&self, range: Range<usize>, predicate: impl Fn(Span) -> bool) -> usize {
        let mut low = range.start;
        let mut high = range.end;
        while low < high {
            let mid = low + (high - low) / 2;
            if self.get(mid).is_some_and(&predicate) {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }
}

#[derive(Default)]
struct FlameRow {
    spans: Spans,
    widest: f64,
    last_right: f64,
    has_gap: bool,
}

impl FlameRow {
    fn push(&mut self, span: Span) {
        self.widest = self.widest.max(span.right - span.left);
        self.has_gap |= self.spans.len != 0 && self.last_right != span.left;
        self.last_right = span.right;
        self.spans.push(span);
    }

    fn overview(&self, domain: &Range<f64>, width: f64) -> Option<Range<f64>> {
        if !width.is_finite() || width <= 0. || domain.start >= domain.end {
            return None;
        }
        let scale = width / (domain.end - domain.start);
        if self.has_gap || !scale.is_finite() || scale <= 0. || self.widest * scale > 0.25 {
            return None;
        }
        let pixel = |x: f64| ((x - domain.start) * scale).clamp(0., width);
        let start = pixel(self.spans.get(0)?.left);
        let end = pixel(self.spans.last()?.right);
        // Keep the detailed path for tiny clipped fragments at either edge.
        (end - start >= 1.).then_some(start..end)
    }
}

pub struct FlameLayout {
    source: Arc<dyn FrameSource>,
    rows: Vec<FlameRow>,
    locations: Locations,
    extent: f64,
}

impl FlameLayout {
    /// Build outside the paint path. Child totals plus self must fit inside the parent.
    pub fn new(source: Arc<dyn FrameSource>) -> Result<Self, LayoutError> {
        Self::build(source, || false).map(|(layout, _)| layout)
    }

    pub(crate) fn empty(source: Arc<dyn FrameSource>) -> Self {
        Self {
            locations: Locations::Sequential(SequentialLocations::default()),
            source,
            rows: Vec::new(),
            extent: 0.,
        }
    }

    /// A cancelled build returns its private partial state for destruction on
    /// the worker. Callers only publish a completed layout.
    pub(crate) fn build(
        source: Arc<dyn FrameSource>,
        cancelled: impl Fn() -> bool,
    ) -> Result<(Self, bool), LayoutError> {
        let mut result = Self::empty(source);
        let mut stack: Vec<(FrameId, f64, f64)> = Vec::new();
        for index in 0..result.source.len() {
            if index.is_multiple_of(1024) && cancelled() {
                return Ok((result, false));
            }
            let frame = result.source.frame(index);
            if index == 0
                && frame.id == 0
                && let Locations::Sequential(locations) = &mut result.locations
            {
                locations.packed.reserve_exact(result.source.len());
            }
            if !frame.total.is_finite()
                || !frame.self_value.is_finite()
                || frame.self_value < 0.
                || frame.total < frame.self_value
            {
                return Err(LayoutError::InvalidValue(frame.id));
            }

            while stack.last().map(|entry| entry.0) != frame.parent {
                if stack.pop().is_none() {
                    return Err(LayoutError::InvalidParent(frame.id));
                }
            }

            let depth = stack.len();
            let (left, right) = if let Some((_, cursor, limit)) = stack.last_mut() {
                let left = *cursor;
                let right = left + frame.total;
                let tolerance = right.abs().max(limit.abs()) * (4. * f64::EPSILON);
                if !right.is_finite() || right - *limit > tolerance {
                    return Err(LayoutError::InvalidValue(frame.id));
                }
                let right = right.min(*limit);
                *cursor = right;
                (left, right)
            } else {
                let left = result.extent;
                result.extent += frame.total;
                if !result.extent.is_finite() {
                    return Err(LayoutError::InvalidValue(frame.id));
                }
                (left, result.extent)
            };

            if depth == result.rows.len() {
                result.rows.push(FlameRow::default());
            }
            let row = &mut result.rows[depth];
            result.locations.insert(frame.id, depth, row.spans.len)?;
            row.push(Span { index, left, right });
            stack.push((frame.id, left, (right - frame.self_value).max(left)));
        }
        Ok((result, true))
    }

    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    pub fn domain(&self) -> Range<f64> {
        0.0..self.extent
    }

    fn span(&self, id: FrameId) -> Option<Span> {
        let (depth, offset) = self.locations.get(id)?;
        self.rows.get(depth)?.spans.get(offset)
    }

    pub fn frame(&self, id: FrameId) -> Option<Frame<'_>> {
        Some(self.source.frame(self.span(id)?.index))
    }

    pub fn focus(&self, id: FrameId) -> Option<Range<f64>> {
        let span = self.span(id)?;
        (span.left < span.right).then_some(span.left..span.right)
    }

    pub fn click(&self, id: FrameId) -> Option<FrameClicked> {
        let frame = self.frame(id)?;
        let mut path = vec![id];
        let mut parent = frame.parent;
        while let Some(id) = parent {
            path.push(id);
            parent = self.frame(id)?.parent;
        }
        path.reverse();
        Some(FrameClicked {
            id,
            path,
            value: frame.total,
        })
    }

    pub(crate) fn first_frame(&self) -> Option<FrameId> {
        self.hit(0, 0.)
    }

    pub(crate) fn first_child(&self, id: FrameId) -> Option<FrameId> {
        let (depth, _) = self.locations.get(id)?;
        let domain = self.focus(id)?;
        let child = self.hit(depth + 1, domain.start)?;
        (self.frame(child)?.parent == Some(id)).then_some(child)
    }

    pub(crate) fn adjacent_sibling(&self, id: FrameId, forward: bool) -> Option<FrameId> {
        let (depth, offset) = self.locations.get(id)?;
        let row = &self.rows.get(depth)?.spans;
        let current = row.get(offset)?;
        let adjacent = if forward {
            self.hit(depth, current.right)?
        } else {
            let previous = row
                .partition_point(0..offset, |span| span.left < current.left)
                .checked_sub(1)?;
            let span = row.get(previous)?;
            if span.left == span.right || span.right != current.left {
                return None;
            }
            self.source.frame(span.index).id
        };
        (self.frame(adjacent)?.parent == self.frame(id)?.parent).then_some(adjacent)
    }

    pub fn hit(&self, depth: usize, x: f64) -> Option<FrameId> {
        let row = &self.rows.get(depth)?.spans;
        let span = row.get(row.partition_point(0..row.len, |span| span.right <= x))?;
        (span.left <= x && x < span.right).then(|| self.source.frame(span.index).id)
    }

    /// Visit visible spans in pixel coordinates without building a scratch list.
    /// A binary search skips all subpixel spans ending within a pixel bucket.
    pub fn visit_row(
        &self,
        depth: usize,
        domain: Range<f64>,
        width: f64,
        search: Option<&Search>,
        mut visit: impl FnMut(Segment),
    ) {
        self.visit_indexed_row(depth, domain, width, search, |single, x, highlighted| {
            visit(Segment {
                id: single.map(|(_, index)| self.source.frame(index).id),
                x,
                highlighted,
            });
        });
    }

    // Carry the row offset and source index through the paint traversal so a
    // renderer can address per-row data without a round trip through FrameId.
    pub(crate) fn visit_indexed_row(
        &self,
        depth: usize,
        domain: Range<f64>,
        width: f64,
        search: Option<&Search>,
        mut visit: impl FnMut(Option<(usize, usize)>, Range<f64>, bool),
    ) {
        let Some(row) = self.rows.get(depth) else {
            return;
        };
        let row = &row.spans;
        if !width.is_finite() || width <= 0. || domain.start >= domain.end {
            return;
        }
        let scale = width / (domain.end - domain.start);
        if !scale.is_finite() || scale <= 0. {
            return;
        }
        let pixel = |x: f64| ((x - domain.start) * scale).clamp(0., width);
        let mut cursor = row.partition_point(0..row.len, |span| span.right <= domain.start);
        while let Some(span) = row.get(cursor).filter(|span| span.left < domain.end) {
            let x = pixel(span.left);
            let right = pixel(span.right);
            let mut end = cursor + 1;
            if right - x < 1. {
                let bucket_end = domain.start + (x.floor() + 2.) / scale;
                end = skip_bucket(row, end, bucket_end);
            }
            let last = if end == cursor + 1 {
                right
            } else {
                pixel(row.get(end - 1).map_or(span.right, |last| last.right))
            };
            let range_end = last.max((x + 1.).min(width));
            let highlighted = search.is_some_and(|search| {
                let Some(matches) = search.rows.get(depth) else {
                    return false;
                };
                matches.partition_point(|&offset| offset < cursor)
                    < matches.partition_point(|&offset| offset < end)
            });
            if range_end > x {
                visit(
                    (end == cursor + 1).then_some((cursor, span.index)),
                    x..range_end,
                    highlighted,
                );
            }
            cursor = end;
        }
    }

    pub(crate) fn source_frame(&self, index: usize) -> Frame<'_> {
        self.source.frame(index)
    }

    pub(crate) fn has_dense_unlabelled_row(
        &self,
        depth: usize,
        domain: &Range<f64>,
        width: f64,
    ) -> bool {
        self.rows.get(depth).is_some_and(|row| {
            row.spans.len >= 48
                && row.widest * width / (domain.end - domain.start) < 32.
                && row.overview(domain, width).is_none()
        })
    }

    /// Coalesce touching aggregate spans with the same paint style. Hit testing
    /// and the public segment iterator retain the original frame boundaries.
    pub(crate) fn visit_paint_row(
        &self,
        depth: usize,
        domain: Range<f64>,
        width: f64,
        search: Option<&Search>,
        mut paint: impl FnMut(Segment),
    ) {
        if search.is_none()
            && let Some(x) = self
                .rows
                .get(depth)
                .and_then(|row| row.overview(&domain, width))
        {
            paint(Segment {
                id: None,
                x,
                highlighted: false,
            });
            return;
        }
        let mut pending: Option<Segment> = None;
        self.visit_row(depth, domain, width, search, |segment| {
            if let Some(previous) = &mut pending
                && segment.id.is_none()
                && previous.highlighted == segment.highlighted
                && previous.x.end >= segment.x.start
            {
                previous.x.end = previous.x.end.max(segment.x.end);
                return;
            }
            if let Some(previous) = pending.take() {
                paint(previous);
            }
            if segment.id.is_none() {
                pending = Some(segment);
            } else {
                paint(segment);
            }
        });
        if let Some(previous) = pending {
            paint(previous);
        }
    }

    /// Build on query changes, independently of the visible viewport.
    pub fn search(&self, query: &str) -> Option<Search> {
        if query.is_empty() {
            return None;
        }
        let query = query.to_lowercase();
        let mut label = String::new();
        Some(Search {
            rows: self
                .rows
                .iter()
                .map(|row| {
                    (0..row.spans.len)
                        .filter_map(|offset| row.spans.get(offset).map(|span| (offset, span)))
                        .filter(|(_, span)| {
                            label.clear();
                            label.extend(
                                self.source
                                    .frame(span.index)
                                    .label
                                    .chars()
                                    .flat_map(char::to_lowercase),
                            );
                            label.contains(&query)
                        })
                        .map(|(offset, _)| offset)
                        .collect()
                })
                .collect(),
        })
    }
}

pub struct Search {
    rows: Vec<Vec<usize>>,
}

fn skip_bucket(spans: &Spans, start: usize, right: f64) -> usize {
    let remaining = spans.len - start;
    let mut low = 0;
    let mut high = 1;
    while high <= remaining
        && spans
            .get(start + high - 1)
            .is_some_and(|span| span.right <= right)
    {
        low = high;
        high = high.saturating_mul(2);
    }
    spans.partition_point(start + low..start + high.min(remaining), |span| {
        span.right <= right
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    struct Uniform(usize);

    impl FrameSource for Uniform {
        fn len(&self) -> usize {
            self.0
        }

        fn frame(&self, index: usize) -> Frame<'_> {
            Frame {
                id: index as u64,
                parent: None,
                self_value: 1.,
                total: 1.,
                label: "sample",
                package: "",
            }
        }
    }

    #[test]
    fn cancelled_build_stops_before_the_next_batch() -> Result<(), LayoutError> {
        let checks = std::cell::Cell::new(0);
        let (layout, complete) = FlameLayout::build(Arc::new(Uniform(10_000)), || {
            checks.set(checks.get() + 1);
            checks.get() == 2
        })?;
        assert!(!complete);
        assert_eq!(layout.domain(), 0. ..1024.);
        Ok(())
    }

    #[test]
    fn overview_paints_one_contiguous_run_at_both_scales() -> Result<(), LayoutError> {
        for count in [10_000, 1_000_000, 10_000_000] {
            let layout = FlameLayout::new(Arc::new(Uniform(count)))?;
            assert!(matches!(&layout.locations, Locations::Sequential(index)
                if index.packed.len() == count && index.packed.capacity() == count && index.wide.is_empty()));
            let width = 1_000.;
            assert_eq!(
                layout.rows[0].overview(&layout.domain(), width),
                Some(0. ..width)
            );
            let mut painted = 0;
            layout.visit_paint_row(0, layout.domain(), width, None, |segment| {
                painted += 1;
                assert_eq!(segment.id, None);
                assert_eq!(segment.x, 0. ..width);
            });
            assert_eq!(painted, 1);
            assert_eq!(layout.hit(0, count as f64 - 0.5), Some(count as u64 - 1));
        }
        Ok(())
    }

    #[test]
    fn overview_lod_preserves_gaps_and_skips_individually_visible_frames() {
        let mut row = FlameRow::default();
        row.push(Span {
            index: 0,
            left: 0.,
            right: 1.,
        });
        row.push(Span {
            index: 1,
            left: 1.,
            right: 2.,
        });
        assert_eq!(row.overview(&(0. ..2.), 100.), None);
        assert_eq!(row.overview(&(0. ..10.), 2.), None);
        for index in 2..10 {
            row.push(Span {
                index,
                left: index as f64,
                right: index as f64 + 1.,
            });
        }
        assert_eq!(row.overview(&(0. ..10.), 2.), Some(0. ..2.));
        row.push(Span {
            index: 10,
            left: 11.,
            right: 12.,
        });
        assert_eq!(row.overview(&(0. ..12.), 2.), None);
    }

    #[test]
    fn sequential_index_falls_back_and_checks_duplicates() -> Result<(), LayoutError> {
        let mut index = Locations::Sequential(SequentialLocations::default());
        index.insert(0, 0, 1)?;
        index.insert(u64::MAX, 2, 3)?;
        assert_eq!(index.get(0), Some((0, 1)));
        assert_eq!(index.get(u64::MAX), Some((2, 3)));
        assert_eq!(index.get(1), None);
        assert_eq!(index.insert(0, 0, 2), Err(LayoutError::DuplicateId(0)));
        Ok(())
    }

    #[test]
    fn span_blocks_preserve_coordinates_through_linear_and_explicit_storage() {
        let mut spans = Spans::default();
        let expected: Vec<_> = (0..2 * SPAN_BLOCK + 1)
            .map(|index| {
                let left = if index < SPAN_BLOCK {
                    index as f64
                } else {
                    index as f64 * 2.
                };
                Span {
                    index: index * 500,
                    left,
                    right: left + if index == SPAN_BLOCK + 10 { 0.75 } else { 1. },
                }
            })
            .collect();
        for &span in &expected {
            spans.push(span);
        }
        assert!(matches!(
            &spans.blocks[..],
            [
                SpanBlock::Linear { .. },
                SpanBlock::Explicit(_),
                SpanBlock::Linear { .. }
            ]
        ));
        for (index, span) in expected.iter().enumerate() {
            let actual = spans.get(index).expect("stored span");
            assert_eq!(actual.index, span.index);
            assert_eq!(actual.left.to_bits(), span.left.to_bits());
            assert_eq!(actual.right.to_bits(), span.right.to_bits());
        }
    }

    #[test]
    fn packed_locations_keep_wide_and_reserved_values() {
        let mut locations = SequentialLocations::default();
        for (id, (depth, offset)) in [
            (0, 10_000_000),
            (499, 19_999),
            (511, (1 << 22) - 1),
            (512, 0),
        ]
        .into_iter()
        .enumerate()
        {
            locations.push(depth, offset);
            assert_eq!(locations.get(id), Some((depth, offset)));
        }
        assert_eq!(locations.wide.len(), 2);
    }

    proptest! {
        #[test]
        fn bucket_search_matches_full_binary_search(mut ends in prop::collection::vec(0u16..512, 0..512), edge in 0u16..512) {
            ends.sort_unstable();
            let spans = ends
                .into_iter()
                .enumerate()
                .map(|(index, end)| Span { index, left: 0., right: f64::from(end) })
                .collect::<Vec<_>>();
            let edge = f64::from(edge);
            let mut indexed = Spans::default();
            for &span in &spans {
                indexed.push(span);
            }
            prop_assert_eq!(
                skip_bucket(&indexed, 0, edge),
                spans.partition_point(|span| span.right <= edge),
            );
        }
    }
}
