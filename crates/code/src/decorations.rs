//! Layered decorations for immutable source revisions.
use crate::{
    Annotation, AnnotationKind, CodeDocument, Fold, Position, PreparedAnalysis, SourceRange,
};
use guiraffe_core::ColorName;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
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

impl GutterMark {
    pub(crate) fn color(self) -> ColorName {
        match self {
            Self::Breakpoint | Self::Error | Self::Deleted | Self::Uncovered => ColorName::Red,
            Self::Bookmark | Self::Modified => ColorName::Blue,
            Self::Added | Self::Covered => ColorName::Green,
            Self::PartiallyCovered => ColorName::Yellow,
        }
    }
}

pub(crate) const OVERVIEW_BINS: usize = 256;
type GutterOverview = [[Option<GutterMark>; 4]; OVERVIEW_BINS];

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
    /// End-of-line error text.
    EndOfLineDiagnostic,
    /// End-of-line text coloured by diagnostic severity.
    EndOfLineDiagnosticSeverity(Severity),
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
    /// Markdown shown when the pointer hovers the annotated range.
    Review {
        markdown: Arc<str>,
    },
    /// A hosted element after the anchor line. The estimate is used until first layout.
    Block {
        estimated_rows: u16,
        payload: Arc<str>,
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
#[derive(Clone, Debug)]
pub struct Decorations {
    document: Arc<CodeDocument>,
    layers: BTreeMap<DecorationLayerId, DecorationLayer>,
    styles: HashMap<u32, (DecorationStyle, usize)>,
    next_style: Arc<AtomicU64>,
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
            styles: HashMap::new(),
            next_style: Arc::default(),
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

    /// Intern a style and return its document-local key. Once used, a style is
    /// released when its last decoration is removed. Released keys are never reused.
    pub fn intern_style(&mut self, style: DecorationStyle) -> Result<u32, DecorationError> {
        if let Some(&id) = self.style_ids.get(&style) {
            return Ok(id);
        }
        // Clones can build different successor snapshots. Share the allocator so
        // a key retired in one successor cannot alias new data in another.
        let id = self
            .next_style
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                (next <= u64::from(u32::MAX)).then_some(next + 1)
            })
            .map_err(|_| DecorationError::Capacity)? as u32;
        match &style {
            DecorationStyle::Blame(_) => self.has_blame = true,
            DecorationStyle::Gutter { lane, .. } => {
                self.gutter_lanes = self.gutter_lanes.max((*lane).min(3) + 1)
            }
            _ => {}
        }
        self.styles.insert(id, (style.clone(), 0));
        self.style_ids.insert(style, id);
        Ok(id)
    }

    pub fn style(&self, id: u32) -> Option<&DecorationStyle> {
        self.styles.get(&id).map(|(style, _)| style)
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
        if let Some(layer) = self.layers.remove(&id) {
            self.release_layer_styles(&layer);
            self.update_paint_order();
        }
    }

    fn release_layer_styles(&mut self, layer: &DecorationLayer) {
        let mut refresh_gutter = false;
        for record in layer.records() {
            let std::collections::hash_map::Entry::Occupied(mut entry) =
                self.styles.entry(record.style)
            else {
                continue;
            };
            entry.get_mut().1 -= 1;
            if entry.get().1 == 0 {
                let (style, _) = entry.remove();
                refresh_gutter |= matches!(
                    style,
                    DecorationStyle::Blame(_) | DecorationStyle::Gutter { .. }
                );
                self.style_ids.remove(&style);
            }
        }
        if refresh_gutter {
            self.has_blame = false;
            self.gutter_lanes = 0;
            for (style, _) in self.styles.values() {
                match style {
                    DecorationStyle::Blame(_) => self.has_blame = true,
                    DecorationStyle::Gutter { lane, .. } => {
                        self.gutter_lanes = self.gutter_lanes.max((*lane).min(3) + 1);
                    }
                    _ => {}
                }
            }
        }
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
        let mut records: Vec<_> = upsert
            .into_iter()
            .map(|input| {
                if self.style(input.style).is_none() {
                    return Err(DecorationError::UnknownStyle);
                }
                let range = self
                    .document
                    .range(input.range)
                    .ok_or(DecorationError::InvalidRange)?;
                Ok(Decoration {
                    id: input.id,
                    start: range.start as u32,
                    end: range.end as u32,
                    style: input.style,
                    start_bias: input.start_bias,
                    end_bias: input.end_bias,
                })
            })
            .collect::<Result<_, _>>()?;
        records.sort_by_key(|record| record.id);
        records.dedup_by(|later, earlier| {
            if later.id == earlier.id {
                *earlier = *later;
                true
            } else {
                false
            }
        });

        if let Some(previous) = self.layers.get(&id) {
            let remove: std::collections::BTreeSet<_> = remove.iter().copied().collect();
            let mut unchanged = Vec::new();
            for &record in previous.records() {
                if !remove.contains(&record.id)
                    && records
                        .binary_search_by_key(&record.id, |record| record.id)
                        .is_err()
                {
                    unchanged.push(record);
                }
            }
            records.append(&mut unchanged);
            records.sort_unstable_by_key(|record| record.id);
        }
        let layer = DecorationLayer::build(records, options, false, &self.styles, &self.document)?;
        // Acquire the new references first so a style carried over from the previous
        // layer keeps its key even when this is its only layer.
        for record in layer.records() {
            if let Some((_, references)) = self.styles.get_mut(&record.style) {
                *references += 1;
            }
        }
        if let Some(previous) = self.layers.insert(id, layer) {
            self.release_layer_styles(&previous);
        }
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
        let mut block_rows = BTreeMap::new();
        let mut block_keys = BTreeMap::new();
        for (layer_id, layer) in self.layers().filter(|(_, layer)| layer.index.has_widgets) {
            for mark in layer.records() {
                let style = self.style(mark.style);
                let (kind, payload, color, end_of_line) = match style {
                    Some(DecorationStyle::Inline { widget, label }) => {
                        let kind = if *widget == InlineWidget::CodeLens {
                            AnnotationKind::Lens
                        } else {
                            AnnotationKind::InlineHint
                        };
                        let severity = match widget {
                            InlineWidget::EndOfLineDiagnostic => Some(Severity::Error),
                            InlineWidget::EndOfLineDiagnosticSeverity(severity) => Some(*severity),
                            _ => None,
                        };
                        (
                            kind,
                            label,
                            severity.map_or(ColorName::Gray, crate::paint::severity_color),
                            severity.is_some(),
                        )
                    }
                    Some(DecorationStyle::Block { payload, .. }) => {
                        (AnnotationKind::Block, payload, ColorName::Gray, false)
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
                if let Some(DecorationStyle::Block { estimated_rows, .. }) = style {
                    block_rows.insert(id, usize::from((*estimated_rows).max(1)));
                    block_keys.insert(id, (layer_id, mark.id));
                }
                legacy.push(Annotation {
                    id,
                    range: SourceRange::new(start, end),
                    kind,
                    payload: payload.clone(),
                    color,
                });
            }
        }
        let mut prepared = PreparedAnalysis::new(self.document.clone(), legacy.into(), folds);
        prepared.rows = crate::RowMap::with_blocks(
            self.document.line_count(),
            &prepared.folds,
            &prepared.annotations,
            &block_rows,
        );
        prepared.block_rows = block_rows;
        prepared.block_keys = block_keys;
        prepared
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
            let mut records = layer.records().to_vec();
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
#[derive(Clone, Debug)]
pub struct DecorationLayer {
    options: DecorationLayerOptions,
    stale: bool,
    index: Arc<DecorationIndex>,
}

#[derive(Debug)]
struct DecorationIndex {
    records: Vec<Decoration>,
    nodes: Vec<IntervalNode>,
    by_start: Vec<u32>,
    by_end: Vec<u32>,
    diagnostics: Vec<u32>,
    changes: Vec<u32>,
    overview: [bool; OVERVIEW_BINS],
    gutter_overview: Option<Box<GutterOverview>>,
    disjoint: bool,
    has_blame: bool,
    has_widgets: bool,
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
        styles: &HashMap<u32, (DecorationStyle, usize)>,
        document: &CodeDocument,
    ) -> Result<Self, DecorationError> {
        let count = u32::try_from(records.len()).map_err(|_| DecorationError::Capacity)?;
        records.shrink_to_fit();
        let mut data = DecorationIndex {
            records,
            nodes: Vec::new(),
            by_start: Vec::new(),
            by_end: Vec::new(),
            diagnostics: Vec::new(),
            changes: Vec::new(),
            overview: [false; OVERVIEW_BINS],
            gutter_overview: None,
            disjoint: false,
            has_blame: false,
            has_widgets: false,
        };
        let mut indices: Vec<u32> = (0..count).collect();
        indices.sort_unstable_by_key(|&i| data.records[i as usize].start);
        for &index in &indices {
            let record = data.records[index as usize];
            match styles.get(&record.style).map(|(style, _)| style) {
                Some(DecorationStyle::Blame(_)) => data.has_blame = true,
                Some(DecorationStyle::Inline { .. } | DecorationStyle::Block { .. }) => {
                    data.has_widgets = true
                }
                Some(DecorationStyle::Diagnostic { .. }) => data.diagnostics.push(index),
                Some(DecorationStyle::Gutter {
                    mark: GutterMark::Added | GutterMark::Modified | GutterMark::Deleted,
                    ..
                }) => data.changes.push(index),
                _ => {}
            }
        }
        let mut bins = [0_i64; OVERVIEW_BINS + 1];
        let bin = |offset| {
            document.position(offset as usize).map_or(0, |position| {
                ((position.line as u128 * OVERVIEW_BINS as u128) / document.line_count() as u128)
                    as usize
            })
        };
        for record in &data.records {
            let first = bin(record.start);
            if let Some(DecorationStyle::Gutter { lane, mark }) =
                styles.get(&record.style).map(|(style, _)| style)
            {
                let overview = data
                    .gutter_overview
                    .get_or_insert_with(|| Box::new([[None; 4]; OVERVIEW_BINS]));
                overview[first][usize::from((*lane).min(3))] = Some(*mark);
            }
            let source = document.source();
            let mut tail = record.end.saturating_sub(1).max(record.start) as usize;
            while !source.is_char_boundary(tail) {
                tail -= 1;
            }
            let last = bin(tail as u32).max(first);
            bins[first] += 1;
            bins[last + 1] -= 1;
        }
        let mut active = 0;
        for (visible, starts) in data.overview.iter_mut().zip(bins) {
            active += starts;
            *visible = active > 0;
        }
        data.disjoint = indices.windows(2).all(|pair| {
            let earlier = data.records[pair[0] as usize];
            let later = data.records[pair[1] as usize];
            earlier.effective_end() <= u64::from(later.start)
        });
        if data.disjoint {
            data.by_start = indices;
        } else {
            data.by_start.reserve_exact(count as usize);
            data.by_end.reserve_exact(count as usize);
            data.build_node(&mut indices);
            data.nodes.shrink_to_fit();
        }
        data.diagnostics.shrink_to_fit();
        data.changes.shrink_to_fit();
        Ok(Self {
            options,
            stale,
            index: Arc::new(data),
        })
    }

    pub(crate) fn has_blame(&self) -> bool {
        self.index.has_blame
    }

    /// Occupied rows in a fixed 256-bin overview of the source lines.
    pub fn overview(&self) -> &[bool; OVERVIEW_BINS] {
        &self.index.overview
    }

    pub(crate) fn gutter_overview(&self) -> Option<&GutterOverview> {
        self.index.gutter_overview.as_deref()
    }

    pub fn options(&self) -> DecorationLayerOptions {
        self.options
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }

    pub fn records(&self) -> &[Decoration] {
        &self.index.records
    }

    pub fn get(&self, id: DecorationId) -> Option<&Decoration> {
        self.index
            .records
            .binary_search_by_key(&id, |record| record.id)
            .ok()
            .map(|i| &self.index.records[i])
    }

    pub(crate) fn adjacent(
        &self,
        offset: Option<u32>,
        reverse: bool,
        change: bool,
    ) -> Option<&Decoration> {
        let indices = if change {
            &self.index.changes
        } else {
            &self.index.diagnostics
        };
        let index = match (offset, reverse) {
            (None, false) => 0,
            (None, true) => indices.len().checked_sub(1)?,
            (Some(offset), true) => indices
                .partition_point(|&index| self.index.records[index as usize].start < offset)
                .checked_sub(1)?,
            (Some(offset), false) => {
                indices.partition_point(|&index| self.index.records[index as usize].start <= offset)
            }
        };
        indices
            .get(index)
            .map(|&index| &self.index.records[index as usize])
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
        if self.index.disjoint {
            let first = self.index.by_start.partition_point(|&i| {
                self.index.records[i as usize].effective_end() <= range.start
            });
            let mut count = 0;
            for &i in self.index.by_start[first..].iter().take(limit) {
                let record = &self.index.records[i as usize];
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
        let Some(node) = self.index.nodes.get(index as usize) else {
            return;
        };
        if *remaining == 0 {
            return;
        }
        let split = u64::from(node.split);
        let first = node.first as usize;
        let last = first + node.count as usize;
        let (indices, left, right) = if range.end <= split {
            (&self.index.by_start[first..last], node.left, EMPTY)
        } else if range.start > split {
            (&self.index.by_end[first..last], EMPTY, node.right)
        } else {
            (&self.index.by_start[first..last], node.left, node.right)
        };
        for &i in indices.iter().take(*remaining) {
            let record = &self.index.records[i as usize];
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

impl DecorationIndex {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    fn store(document: Arc<CodeDocument>) -> Decorations {
        let mut store = Decorations::new(document).expect("fixture source fits u32");
        assert_eq!(
            store
                .intern_style(DecorationStyle::Deprecated)
                .expect("first style key"),
            0
        );
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
    fn style_churn_releases_payloads_without_reusing_keys() {
        let mut decorations = store(Arc::new(CodeDocument::new("one")));
        let first = 1;
        let second = 2;
        let mut parallel = decorations.clone();
        for index in 0..10_000 {
            let style = DecorationStyle::Review {
                markdown: format!("revision {index}").into(),
            };
            let id = decorations.intern_style(style.clone()).unwrap();
            let input = DecorationInput {
                style: id,
                ..mark(1, 0, 1)
            };
            decorations
                .update_layer(first, Default::default(), &[], [input])
                .unwrap();
            decorations
                .update_layer(second, Default::default(), &[], [input])
                .unwrap();
            decorations.remove_layer(first);
            assert_eq!(decorations.style(id), Some(&style));
            decorations.remove_layer(second);
            assert!(decorations.style(id).is_none());
            assert_eq!(decorations.style_ids.len(), 1);
            assert_eq!(decorations.styles.len(), 1);
            assert!(decorations.intern_style(style).unwrap() > id);
            let next = id + 1;
            decorations
                .update_layer(
                    first,
                    Default::default(),
                    &[],
                    [DecorationInput {
                        style: next,
                        ..mark(1, 0, 1)
                    }],
                )
                .unwrap();
            decorations.remove_layer(first);
        }
        let first_original_key = 1;
        let sibling_key = parallel
            .intern_style(DecorationStyle::Review {
                markdown: "sibling".into(),
            })
            .unwrap();
        assert_ne!(sibling_key, first_original_key);
        assert!(parallel.style(first_original_key).is_none());
    }

    #[test]
    fn gutter_overview_only_places_gutter_marks_at_their_source_line_and_lane() {
        let mut decorations = store(Arc::new(CodeDocument::new("first\nsecond\nthird\nlast")));
        let bookmark = decorations
            .intern_style(DecorationStyle::Gutter {
                lane: 1,
                mark: GutterMark::Bookmark,
            })
            .expect("interned bookmark");
        let coverage = decorations
            .intern_style(DecorationStyle::Gutter {
                lane: 3,
                mark: GutterMark::Covered,
            })
            .expect("interned coverage");
        let on_line = |id, line, style| DecorationInput {
            range: SourceRange::new(Position::new(line, 0), Position::new(line, 0)),
            style,
            ..mark(id, 0, 0)
        };
        decorations
            .update_layer(0, Default::default(), &[], [mark(1, 0, 3)])
            .expect("published non-gutter layer");
        decorations
            .update_layer(
                1,
                Default::default(),
                &[],
                [on_line(2, 1, bookmark), on_line(3, 2, coverage)],
            )
            .expect("published gutter layer");

        assert!(
            decorations
                .layer(0)
                .expect("layer 0")
                .gutter_overview()
                .is_none()
        );
        let overview = decorations
            .layer(1)
            .expect("layer 1")
            .gutter_overview()
            .expect("gutter summary");
        assert_eq!(
            overview[64][1].map(GutterMark::color),
            Some(ColorName::Blue)
        );
        assert_eq!(
            overview[128][3].map(GutterMark::color),
            Some(ColorName::Green)
        );
        assert_eq!(
            overview
                .iter()
                .flatten()
                .filter(|mark| mark.is_some())
                .count(),
            2
        );
    }

    #[test]
    fn batch_update_and_edit_preserve_identity_and_bias() {
        let source = Arc::new(CodeDocument::new("ab界d"));
        let mut decorations = store(source);
        let options = DecorationLayerOptions::default();
        decorations
            .update_layer(3, options, &[], [mark(1, 1, 2), mark(2, 5, 6)])
            .expect("initial batch is valid");
        assert!(
            decorations
                .update_layer(3, options, &[2], [mark(1, 3, 5)])
                .is_err()
        );
        assert!(
            decorations
                .layer(3)
                .expect("layer 3 was published")
                .get(2)
                .is_some()
        );
        decorations
            .apply_edit(Arc::new(CodeDocument::new("aXYb界d")), 1..1)
            .expect("inserted text leaves valid offsets");
        let layer = decorations.layer(3).expect("layer 3 was published");
        let mut next = decorations.clone();
        next.mark_stale(3);
        assert!(Arc::ptr_eq(
            &next.layer(3).expect("cloned layer is present").index,
            &layer.index
        ));
        assert!(layer.is_stale());
        let anchored = layer.get(1).expect("left-biased mark remains");
        assert_eq!((anchored.start, anchored.end), (1, 4));
        let shifted = layer.get(2).expect("shifted mark remains");
        assert_eq!((shifted.start, shifted.end), (7, 8));
        decorations
            .update_layer(3, options, &[1], [mark(2, 4, 7)])
            .expect("refresh replaces mark 2");
        assert!(
            !decorations
                .layer(3)
                .expect("layer 3 was published")
                .is_stale()
        );
        assert!(
            decorations
                .layer(3)
                .expect("layer 3 was published")
                .get(1)
                .is_none()
        );
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
            .expect("all oracle input ranges are valid");
        let layer = decorations.layer(0).expect("layer 0 was published");
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
                .expect("all scale-fixture ranges are valid");
            let layer = decorations.layer(0).expect("layer 0 was published");
            let mut hits = 0;
            layer.query(42..90, |_| hits += 1);
            assert_eq!(hits, 48);
        }
    }
}
