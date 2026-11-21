//! Layered decorations for immutable source revisions.
use crate::{
    Annotation, AnnotationKind, CodeDocument, Fold, Position, PreparedAnalysis, SourceRange,
};
use guiraffe_core::ColorName;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};

/// Stable identifier scoped to a document layer.
pub type DecorationId = u64;

/// Stable layer identifier chosen by the host.
pub type DecorationLayerId = u32;

/// Which side of inserted text a range endpoint stays on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnchorBias {
    Left,
    #[default]
    Right,
}

/// Rendering order and overlap policy for a layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecorationLayerOptions {
    pub z_index: i32,
    /// When several instances of the same style cover the same span,
    /// a renderer can draw them once while preserving their individual identities.
    pub merge_identical: bool,
}

/// Diagnostic severity used by theme-aware annotation styles and navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Line treatment for diagnostics and other range annotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnderlinePattern {
    Straight,
    Wavy,
    Dotted,
    Dashed,
    Double,
}

/// Geometry for a highlighted range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HighlightShape {
    Background,
    Outline,
    Rounded,
    BracketPair,
}

/// Status drawn in one of four fixed-width annotation gutter lanes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GutterMark {
    Breakpoint,
    Bookmark,
    Error,
    Added,
    Modified,
    Deleted,
    Covered,
    Uncovered,
    PartiallyCovered,
}

/// Git metadata shared by every run attributed to this commit.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlameCommit {
    pub author: Arc<str>,
    pub short_hash: Arc<str>,
    pub message: Arc<str>,
    pub date: Arc<str>,
    pub age_days: u32,
    pub url: Option<Arc<str>>,
}

/// Display-only text placed into a source row or a lens row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InlineWidget {
    Inlay,
    Ghost,
    EndOfLineDiagnostic,
    CodeLens,
}

/// Shareable visual style and interaction payload. Intern repeated values once per document.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DecorationStyle {
    Highlight {
        shape: HighlightShape,
        color: ColorName,
    },
    Diagnostic {
        severity: Severity,
        pattern: UnderlinePattern,
        message: Arc<str>,
    },
    Gutter {
        lane: u8,
        mark: GutterMark,
    },
    Blame(Arc<BlameCommit>),
    Inline {
        widget: InlineWidget,
        label: Arc<str>,
    },
    Review {
        markdown: Arc<str>,
    },
    Unnecessary,
    Deprecated,
}

/// The input to a batched layer update. Offsets are derived from validated UTF-8 positions.
/// A style is an index returned by `Decorations::intern_style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecorationInput {
    pub id: DecorationId,
    pub range: SourceRange,
    pub style: u32,
    pub start_bias: AnchorBias,
    pub end_bias: AnchorBias,
}

/// Packed decoration record. The document owns the mapping from offsets back to positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoration {
    pub id: DecorationId,
    pub start: u32,
    pub end: u32,
    pub style: u32,
    start_bias: AnchorBias,
    end_bias: AnchorBias,
}

impl Decoration {
    fn effective_end(self) -> u64 {
        u64::from(self.end).max(u64::from(self.start) + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationError {
    /// The source or the layer's record count cannot be indexed with u32.
    Capacity,
    /// A range is reversed, outside the document or splits a UTF-8 codepoint.
    InvalidRange,
    /// An update referred to a style absent from this document.
    UnknownStyle,
}

impl std::fmt::Display for DecorationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "decoration index capacity exceeded",
            Self::InvalidRange => "invalid decoration range",
            Self::UnknownStyle => "unknown decoration style",
        })
    }
}

impl std::error::Error for DecorationError {}

/// Source revision and its ordered decoration layers. Construct large updates on a worker
/// and publish the completed snapshot with the same document revision.
#[derive(Debug)]
pub struct Decorations {
    document: Arc<CodeDocument>,
    layers: BTreeMap<DecorationLayerId, DecorationLayer>,
    styles: Vec<DecorationStyle>,
    style_ids: HashMap<DecorationStyle, u32>,
    paint_order: Vec<DecorationLayerId>,
    has_blame: bool,
    gutter_lanes: u8,
}

impl Decorations {
    pub fn new(document: Arc<CodeDocument>) -> Result<Self, DecorationError> {
        u32::try_from(document.source().len()).map_err(|_| DecorationError::Capacity)?;
        Ok(Self {
            document,
            layers: BTreeMap::new(),
            styles: Vec::new(),
            style_ids: HashMap::new(),
            paint_order: Vec::new(),
            has_blame: false,
            gutter_lanes: 0,
        })
    }

    pub fn document(&self) -> &Arc<CodeDocument> {
        &self.document
    }

    pub fn layer(&self, id: DecorationLayerId) -> Option<&DecorationLayer> {
        self.layers.get(&id)
    }

    /// Intern a style and return the stable document-local key used by batch inputs.
    pub fn intern_style(&mut self, style: DecorationStyle) -> Result<u32, DecorationError> {
        if let Some(&id) = self.style_ids.get(&style) {
            return Ok(id);
        }
        let id = u32::try_from(self.styles.len()).map_err(|_| DecorationError::Capacity)?;
        match &style {
            DecorationStyle::Blame(_) => self.has_blame = true,
            DecorationStyle::Gutter { lane, .. } => {
                self.gutter_lanes = self.gutter_lanes.max((*lane).min(3) + 1)
            }
            _ => {}
        }
        self.styles.push(style.clone());
        self.style_ids.insert(style, id);
        Ok(id)
    }

    pub fn style(&self, id: u32) -> Option<&DecorationStyle> {
        self.styles.get(id as usize)
    }

    pub(crate) fn has_blame(&self) -> bool {
        self.has_blame
    }

    pub(crate) fn gutter_lanes(&self) -> u8 {
        self.gutter_lanes
    }

    /// Visit layers in ascending z-order, breaking ties by layer identifier.
    pub fn layers(&self) -> impl Iterator<Item = (DecorationLayerId, &DecorationLayer)> {
        self.paint_order.iter().map(|&id| (id, &self.layers[&id]))
    }

    fn update_paint_order(&mut self) {
        self.paint_order.clear();
        self.paint_order.extend(self.layers.keys().copied());
        self.paint_order
            .sort_unstable_by_key(|id| (self.layers[id].options.z_index, *id));
    }

    pub fn remove_layer(&mut self, id: DecorationLayerId) {
        self.layers.remove(&id);
        self.update_paint_order();
    }

    pub fn mark_stale(&mut self, id: DecorationLayerId) {
        if let Some(layer) = self.layers.get_mut(&id) {
            layer.stale = true;
        }
    }

    /// Rebuild one layer from an atomic batch. Duplicate IDs use the last supplied record.
    /// Removing and upserting the same ID replaces that record.
    pub fn update_layer(
        &mut self,
        id: DecorationLayerId,
        options: DecorationLayerOptions,
        remove: &[DecorationId],
        upsert: impl IntoIterator<Item = DecorationInput>,
    ) -> Result<(), DecorationError> {
        let previous = self.layers.get(&id);
        let mut changes = BTreeMap::new();
        changes.extend(remove.iter().map(|&id| (id, None)));
        for input in upsert {
            if self.style(input.style).is_none() {
                return Err(DecorationError::UnknownStyle);
            }
            let range = self
                .document
                .range(input.range)
                .ok_or(DecorationError::InvalidRange)?;
            changes.insert(
                input.id,
                Some(Decoration {
                    id: input.id,
                    start: range.start as u32,
                    end: range.end as u32,
                    style: input.style,
                    start_bias: input.start_bias,
                    end_bias: input.end_bias,
                }),
            );
        }
        let unchanged = previous.into_iter().flat_map(|layer| {
            layer
                .records
                .iter()
                .copied()
                .filter(|record| !changes.contains_key(&record.id))
        });
        let mut records: Vec<_> = unchanged
            .chain(changes.values().filter_map(|record| *record))
            .collect();
        records.sort_unstable_by_key(|record| record.id);
        let layer = DecorationLayer::build(records, options, false, &self.styles, &self.document)?;
        self.layers.insert(id, layer);
        self.update_paint_order();
        Ok(())
    }

    /// Prepare the text and Markdown widgets with the viewer's existing fold and row pipeline.
    /// Publish the returned analysis together with the decoration snapshot. This scans the
    /// supplied data and is intended for a background worker.
    pub fn prepare_analysis(
        &self,
        mut legacy: Vec<Annotation>,
        folds: Vec<Fold>,
    ) -> PreparedAnalysis {
        let existing_ids: std::collections::HashSet<_> =
            legacy.iter().map(|annotation| annotation.id).collect();
        let mut available_ids = (0..=u64::MAX).filter(|id| !existing_ids.contains(id));
        for (_, layer) in self.layers() {
            for mark in layer.records() {
                let (kind, payload, color, end_of_line) = match self.style(mark.style) {
                    Some(DecorationStyle::Inline { widget, label }) => {
                        let kind = if *widget == InlineWidget::CodeLens {
                            AnnotationKind::Lens
                        } else {
                            AnnotationKind::InlineHint
                        };
                        let color = if *widget == InlineWidget::EndOfLineDiagnostic {
                            ColorName::Red
                        } else {
                            ColorName::Gray
                        };
                        (
                            kind,
                            label,
                            color,
                            *widget == InlineWidget::EndOfLineDiagnostic,
                        )
                    }
                    Some(DecorationStyle::Review { markdown }) => {
                        (AnnotationKind::Bubble, markdown, ColorName::Blue, false)
                    }
                    _ => continue,
                };
                let Some(start) = self.document.position(mark.start as usize) else {
                    continue;
                };
                let end = if end_of_line {
                    Position::new(
                        start.line,
                        self.document.line(start.line).map_or(0, str::len),
                    )
                } else {
                    self.document.position(mark.end as usize).unwrap_or(start)
                };
                let Some(id) = available_ids.next() else {
                    break;
                };
                legacy.push(Annotation {
                    id,
                    range: SourceRange::new(start, end),
                    kind,
                    payload: payload.clone(),
                    color,
                });
            }
        }
        PreparedAnalysis::new(self.document.clone(), legacy.into(), folds)
    }

    /// Move endpoints through one splice, retain stable IDs and mark the layers stale.
    /// The caller supplies the new revision that resulted from replacing `replaced`
    /// in the old revision. The inserted length is inferred from the two source lengths.
    pub fn apply_edit(
        &mut self,
        document: Arc<CodeDocument>,
        replaced: Range<usize>,
    ) -> Result<(), DecorationError> {
        let old = self.document.source();
        let new = document.source();
        u32::try_from(new.len()).map_err(|_| DecorationError::Capacity)?;
        let replaced_len = replaced
            .end
            .checked_sub(replaced.start)
            .ok_or(DecorationError::InvalidRange)?;
        if replaced.end > old.len()
            || !old.is_char_boundary(replaced.start)
            || !old.is_char_boundary(replaced.end)
        {
            return Err(DecorationError::InvalidRange);
        }
        let inserted = new
            .len()
            .checked_sub(old.len() - replaced_len)
            .ok_or(DecorationError::InvalidRange)?;
        let inserted_end = replaced.start + inserted;
        if !new.is_char_boundary(replaced.start) || !new.is_char_boundary(inserted_end) {
            return Err(DecorationError::InvalidRange);
        }
        let translate = |offset: u32, bias| {
            let offset = offset as usize;
            let offset = if offset < replaced.start {
                offset
            } else if offset > replaced.end {
                offset - replaced_len + inserted
            } else {
                match bias {
                    AnchorBias::Left => replaced.start,
                    AnchorBias::Right => inserted_end,
                }
            };
            offset as u32
        };
        let mut next = BTreeMap::new();
        for (&id, layer) in &self.layers {
            let mut records = layer.records.clone();
            for record in &mut records {
                record.start = translate(record.start, record.start_bias);
                record.end = translate(record.end, record.end_bias).max(record.start);
            }
            next.insert(
                id,
                DecorationLayer::build(records, layer.options, true, &self.styles, &document)?,
            );
        }
        self.document = document;
        self.layers = next;
        Ok(())
    }
}

/// A compact interval index. Querying borrows records directly and requires no scratch allocation.
#[derive(Debug)]
pub struct DecorationLayer {
    records: Vec<Decoration>,
    options: DecorationLayerOptions,
    stale: bool,
    nodes: Vec<IntervalNode>,
    by_start: Vec<u32>,
    by_end: Vec<u32>,
    diagnostics: Vec<u32>,
    changes: Vec<u32>,
    overview: [bool; 256],
    disjoint: bool,
    has_blame: bool,
}

const EMPTY: u32 = u32::MAX;

#[derive(Debug)]
struct IntervalNode {
    split: u32,
    first: u32,
    count: u32,
    left: u32,
    right: u32,
}

impl DecorationLayer {
    fn build(
        mut records: Vec<Decoration>,
        options: DecorationLayerOptions,
        stale: bool,
        styles: &[DecorationStyle],
        document: &CodeDocument,
    ) -> Result<Self, DecorationError> {
        let count = u32::try_from(records.len()).map_err(|_| DecorationError::Capacity)?;
        records.shrink_to_fit();
        let mut layer = Self {
            records,
            options,
            stale,
            nodes: Vec::new(),
            by_start: Vec::new(),
            by_end: Vec::new(),
            diagnostics: Vec::new(),
            changes: Vec::new(),
            overview: [false; 256],
            disjoint: false,
            has_blame: false,
        };
        let mut indices: Vec<u32> = (0..count).collect();
        indices.sort_unstable_by_key(|&i| layer.records[i as usize].start);
        for &index in &indices {
            let record = layer.records[index as usize];
            match styles.get(record.style as usize) {
                Some(DecorationStyle::Blame(_)) => layer.has_blame = true,
                Some(DecorationStyle::Diagnostic { .. }) => layer.diagnostics.push(index),
                Some(DecorationStyle::Gutter {
                    mark: GutterMark::Added | GutterMark::Modified | GutterMark::Deleted,
                    ..
                }) => layer.changes.push(index),
                _ => {}
            }
        }
        let mut bins = [0_i64; 257];
        let bin = |offset| {
            document.position(offset as usize).map_or(0, |position| {
                ((position.line as u128 * 256) / document.line_count() as u128) as usize
            })
        };
        for record in &layer.records {
            let first = bin(record.start);
            let last = bin(record.end).max(first);
            bins[first] += 1;
            bins[last + 1] -= 1;
        }
        let mut active = 0;
        for (visible, starts) in layer.overview.iter_mut().zip(bins) {
            active += starts;
            *visible = active > 0;
        }
        layer.disjoint = indices.windows(2).all(|pair| {
            let earlier = layer.records[pair[0] as usize];
            let later = layer.records[pair[1] as usize];
            earlier.effective_end() <= u64::from(later.start)
        });
        if layer.disjoint {
            layer.by_start = indices;
        } else {
            layer.by_start.reserve_exact(count as usize);
            layer.by_end.reserve_exact(count as usize);
            layer.build_node(&mut indices);
            layer.nodes.shrink_to_fit();
        }
        layer.diagnostics.shrink_to_fit();
        layer.changes.shrink_to_fit();
        Ok(layer)
    }

    fn build_node(&mut self, sorted: &mut [u32]) -> u32 {
        let Some(&median) = sorted.get(sorted.len() / 2) else {
            return EMPTY;
        };
        let split = self.records[median as usize].start;
        let right_start = sorted.partition_point(|&i| self.records[i as usize].start <= split);
        let (left_or_center, right) = sorted.split_at_mut(right_start);
        let first = self.by_start.len();
        let mut left_len = 0;
        for cursor in 0..left_or_center.len() {
            let i = left_or_center[cursor];
            if self.records[i as usize].effective_end() <= u64::from(split) {
                left_or_center[left_len] = i;
                left_len += 1;
            } else {
                self.by_start.push(i);
            }
        }

        let center = &self.by_start[first..];
        let count = center.len();
        self.by_end.extend_from_slice(center);
        self.by_end[first..]
            .sort_unstable_by_key(|&i| std::cmp::Reverse(self.records[i as usize].effective_end()));
        let index = self.nodes.len() as u32;
        self.nodes.push(IntervalNode {
            split,
            first: first as u32,
            count: count as u32,
            left: EMPTY,
            right: EMPTY,
        });
        let left = self.build_node(&mut left_or_center[..left_len]);
        let right = self.build_node(right);
        self.nodes[index as usize].left = left;
        self.nodes[index as usize].right = right;
        index
    }

    pub(crate) fn has_blame(&self) -> bool {
        self.has_blame
    }

    /// Occupied rows in a fixed 256-bin overview of the source lines.
    pub fn overview(&self) -> &[bool; 256] {
        &self.overview
    }

    pub fn options(&self) -> DecorationLayerOptions {
        self.options
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }

    pub fn records(&self) -> &[Decoration] {
        &self.records
    }

    pub fn get(&self, id: DecorationId) -> Option<&Decoration> {
        self.records
            .binary_search_by_key(&id, |record| record.id)
            .ok()
            .map(|i| &self.records[i])
    }

    pub(crate) fn adjacent(
        &self,
        offset: Option<u32>,
        reverse: bool,
        change: bool,
    ) -> Option<&Decoration> {
        let indices = if change {
            &self.changes
        } else {
            &self.diagnostics
        };
        let index = match (offset, reverse) {
            (None, false) => 0,
            (None, true) => indices.len().checked_sub(1)?,
            (Some(offset), true) => indices
                .partition_point(|&index| self.records[index as usize].start < offset)
                .checked_sub(1)?,
            (Some(offset), false) => {
                indices.partition_point(|&index| self.records[index as usize].start <= offset)
            }
        };
        indices
            .get(index)
            .map(|&index| &self.records[index as usize])
    }

    /// Visits overlapping records in O(log n + hits). Empty marks match the byte at
    /// their anchor. The query itself is half-open. Visit order is unspecified.
    pub fn query(&self, range: Range<u64>, visit: impl FnMut(&Decoration)) {
        self.query_limited(range, usize::MAX, visit);
    }

    /// Query up to `limit` intersections. The returned count equals the number
    /// of invocations, allowing a renderer to detect when its budget was reached.
    pub fn query_limited(
        &self,
        range: Range<u64>,
        limit: usize,
        mut visit: impl FnMut(&Decoration),
    ) -> usize {
        if range.start >= range.end || limit == 0 {
            return 0;
        }
        if self.disjoint {
            let first = self
                .by_start
                .partition_point(|&i| self.records[i as usize].effective_end() <= range.start);
            let mut count = 0;
            for &i in self.by_start[first..].iter().take(limit) {
                let record = &self.records[i as usize];
                if u64::from(record.start) >= range.end {
                    break;
                }
                visit(record);
                count += 1;
            }
            return count;
        }
        let mut remaining = limit;
        self.visit(0, &range, &mut remaining, &mut visit);
        limit - remaining
    }

    fn visit(
        &self,
        index: u32,
        range: &Range<u64>,
        remaining: &mut usize,
        visit: &mut impl FnMut(&Decoration),
    ) {
        let Some(node) = self.nodes.get(index as usize) else {
            return;
        };
        if *remaining == 0 {
            return;
        }
        let split = u64::from(node.split);
        let first = node.first as usize;
        let last = first + node.count as usize;
        let (indices, left, right) = if range.end <= split {
            (&self.by_start[first..last], node.left, EMPTY)
        } else if range.start > split {
            (&self.by_end[first..last], EMPTY, node.right)
        } else {
            (&self.by_start[first..last], node.left, node.right)
        };
        for &i in indices.iter().take(*remaining) {
            let record = &self.records[i as usize];
            if u64::from(record.start) >= range.end || record.effective_end() <= range.start {
                break;
            }
            visit(record);
            *remaining -= 1;
        }
        self.visit(left, range, remaining, visit);
        self.visit(right, range, remaining, visit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    fn store(document: Arc<CodeDocument>) -> Decorations {
        let mut store = Decorations::new(document).unwrap();
        assert_eq!(store.intern_style(DecorationStyle::Deprecated).unwrap(), 0);
        store
    }

    fn mark(id: u64, start: usize, end: usize) -> DecorationInput {
        DecorationInput {
            id,
            range: SourceRange::new(Position::new(0, start), Position::new(0, end)),
            style: 0,
            start_bias: AnchorBias::Left,
            end_bias: AnchorBias::Right,
        }
    }

    #[test]
    fn batch_update_and_edit_preserve_identity_and_bias() {
        let source = Arc::new(CodeDocument::new("ab界d"));
        let mut decorations = store(source);
        let options = DecorationLayerOptions::default();
        decorations
            .update_layer(3, options, &[], [mark(1, 1, 2), mark(2, 5, 6)])
            .unwrap();
        assert!(
            decorations
                .update_layer(3, options, &[2], [mark(1, 3, 5)])
                .is_err()
        );
        assert!(decorations.layer(3).unwrap().get(2).is_some());
        decorations
            .apply_edit(Arc::new(CodeDocument::new("aXYb界d")), 1..1)
            .unwrap();
        let layer = decorations.layer(3).unwrap();
        assert!(layer.is_stale());
        let anchored = layer.get(1).unwrap();
        assert_eq!((anchored.start, anchored.end), (1, 4));
        let shifted = layer.get(2).unwrap();
        assert_eq!((shifted.start, shifted.end), (7, 8));
        decorations
            .update_layer(3, options, &[1], [mark(2, 4, 7)])
            .unwrap();
        assert!(!decorations.layer(3).unwrap().is_stale());
        assert!(decorations.layer(3).unwrap().get(1).is_none());
    }

    #[test]
    fn indexed_intersections_match_linear_query_at_every_boundary() {
        let mut decorations = store(Arc::new(CodeDocument::new("x".repeat(80))));
        let inputs = (0..240).map(|id| {
            let a = (id as usize * 23) % 81;
            let b = (id as usize * 71) % 81;
            mark(id, a.min(b), a.max(b))
        });
        decorations
            .update_layer(0, Default::default(), &[], inputs)
            .unwrap();
        let layer = decorations.layer(0).unwrap();
        for start in 0..82 {
            for end in start..82 {
                let mut indexed = Vec::new();
                layer.query(start..end, |hit| indexed.push(hit.id));
                indexed.sort_unstable();
                let expected: Vec<_> = layer
                    .records()
                    .iter()
                    .filter(|record| {
                        start < end
                            && u64::from(record.start) < end
                            && record.effective_end() > start
                    })
                    .map(|record| record.id)
                    .collect();
                assert_eq!(indexed, expected);
            }
        }
    }

    #[test]
    fn packed_index_reports_same_visible_records_at_two_scales() {
        assert_eq!(std::mem::size_of::<Decoration>(), 24);
        assert_eq!(std::mem::size_of::<IntervalNode>(), 20);
        for count in [10_000, 1_000_000] {
            let mut decorations = store(Arc::new(CodeDocument::new("x".repeat(count))));
            let records = (0..count).map(|i| mark(i as u64, i, i + 1));
            decorations
                .update_layer(0, Default::default(), &[], records)
                .unwrap();
            let layer = decorations.layer(0).unwrap();
            let mut hits = 0;
            layer.query(42..90, |_| hits += 1);
            assert_eq!(hits, 48);
        }
    }
}
