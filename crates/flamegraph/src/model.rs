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

// Sequential source IDs need only a packed row/offset pair. Fall back to
// the general index as soon as an arbitrary ID or wide location appears.
enum Locations {
    Sequential(Vec<(u32, u32)>),
    Arbitrary(HashMap<FrameId, (usize, usize)>),
}

impl Locations {
    fn get(&self, id: FrameId) -> Option<(usize, usize)> {
        match self {
            Self::Sequential(locations) => locations
                .get(usize::try_from(id).ok()?)
                .map(|&(depth, offset)| (depth as usize, offset as usize)),
            Self::Arbitrary(locations) => locations.get(&id).copied(),
        }
    }

    fn insert(&mut self, id: FrameId, depth: usize, offset: usize) -> Result<(), LayoutError> {
        if let Self::Sequential(locations) = self {
            if id == locations.len() as u64
                && let (Ok(depth), Ok(offset)) = (u32::try_from(depth), u32::try_from(offset))
            {
                locations.push((depth, offset));
                return Ok(());
            }
            let mut index = HashMap::with_capacity(locations.len() + 1);
            index.extend(
                locations
                    .iter()
                    .enumerate()
                    .map(|(id, &(depth, offset))| (id as u64, (depth as usize, offset as usize))),
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

pub struct FlameLayout {
    source: Arc<dyn FrameSource>,
    rows: Vec<Vec<Span>>,
    locations: Locations,
    extent: f64,
}

impl FlameLayout {
    /// Build outside the paint path. Child totals plus self must fit inside the parent.
    pub fn new(source: Arc<dyn FrameSource>) -> Result<Self, LayoutError> {
        let mut result = Self {
            locations: Locations::Sequential(Vec::new()),
            source,
            rows: Vec::new(),
            extent: 0.,
        };
        let mut stack: Vec<(FrameId, f64, f64)> = Vec::new();
        for index in 0..result.source.len() {
            let frame = result.source.frame(index);
            if index == 0
                && frame.id == 0
                && let Locations::Sequential(locations) = &mut result.locations
            {
                locations.reserve_exact(result.source.len());
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
                result.rows.push(Vec::new());
            }
            let row = &mut result.rows[depth];
            result.locations.insert(frame.id, depth, row.len())?;
            row.push(Span { index, left, right });
            stack.push((frame.id, left, (right - frame.self_value).max(left)));
        }
        Ok(result)
    }

    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    pub fn domain(&self) -> Range<f64> {
        0.0..self.extent
    }

    fn span(&self, id: FrameId) -> Option<&Span> {
        let (depth, offset) = self.locations.get(id)?;
        self.rows.get(depth)?.get(offset)
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
        let row = self.rows.get(depth)?;
        let current = row.get(offset)?;
        let adjacent = if forward {
            self.hit(depth, current.right)?
        } else {
            let previous = row[..offset]
                .partition_point(|span| span.left < current.left)
                .checked_sub(1)?;
            let span = &row[previous];
            if span.left == span.right || span.right != current.left {
                return None;
            }
            self.source.frame(span.index).id
        };
        (self.frame(adjacent)?.parent == self.frame(id)?.parent).then_some(adjacent)
    }

    pub fn hit(&self, depth: usize, x: f64) -> Option<FrameId> {
        let row = self.rows.get(depth)?;
        let span = row.get(row.partition_point(|span| span.right <= x))?;
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
        let Some(row) = self.rows.get(depth) else {
            return;
        };
        if !width.is_finite() || width <= 0. || domain.start >= domain.end {
            return;
        }
        let scale = width / (domain.end - domain.start);
        if !scale.is_finite() || scale <= 0. {
            return;
        }
        let pixel = |x: f64| ((x - domain.start) * scale).clamp(0., width);
        let mut cursor = row.partition_point(|span| span.right <= domain.start);
        while let Some(span) = row.get(cursor).filter(|span| span.left < domain.end) {
            let x = pixel(span.left);
            let right = pixel(span.right);
            let mut end = cursor + 1;
            if right - x < 1. {
                let bucket_end = domain.start + (x.floor() + 2.) / scale;
                end += skip_bucket(&row[end..], bucket_end);
            }
            let range_end = pixel(row[end - 1].right).max((x + 1.).min(width));
            let highlighted = search.is_some_and(|search| {
                let Some(matches) = search.rows.get(depth) else {
                    return false;
                };
                matches.partition_point(|&offset| offset < cursor)
                    < matches.partition_point(|&offset| offset < end)
            });
            if range_end > x {
                visit(Segment {
                    id: (end == cursor + 1).then(|| self.source.frame(span.index).id),
                    x: x..range_end,
                    highlighted,
                });
            }
            cursor = end;
        }
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
                    row.iter()
                        .enumerate()
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

fn skip_bucket(spans: &[Span], right: f64) -> usize {
    // Adjacent pixel buckets commonly cover only a few frames. Bound the
    // search from the near end so its cost depends on the frames skipped.
    let mut low = 0;
    let mut high = 1;
    while high <= spans.len() && spans[high - 1].right <= right {
        low = high;
        high = high.saturating_mul(2);
    }
    let high = high.min(spans.len());
    low + spans[low..high].partition_point(|span| span.right <= right)
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
    fn overview_paints_one_contiguous_run_at_both_scales() -> Result<(), LayoutError> {
        for count in [10_000, 10_000_000] {
            let layout = FlameLayout::new(Arc::new(Uniform(count)))?;
            assert!(matches!(&layout.locations, Locations::Sequential(index)
                if index.len() == count && index.capacity() == count));
            let width = 1_000.;
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
    fn sequential_index_falls_back_and_checks_duplicates() -> Result<(), LayoutError> {
        let mut index = Locations::Sequential(Vec::new());
        index.insert(0, 0, 1)?;
        index.insert(u64::MAX, 2, 3)?;
        assert_eq!(index.get(0), Some((0, 1)));
        assert_eq!(index.get(u64::MAX), Some((2, 3)));
        assert_eq!(index.get(1), None);
        assert_eq!(index.insert(0, 0, 2), Err(LayoutError::DuplicateId(0)));
        Ok(())
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
            prop_assert_eq!(
                skip_bucket(&spans, edge),
                spans.partition_point(|span| span.right <= edge),
            );
        }
    }
}
