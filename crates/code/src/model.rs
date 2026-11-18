//! Presentation data and viewport-independent geometry.
use crate::{CodeDocument, Position, SourceRange};
use guiraffe_core::ColorName;
use std::{ops::Range, sync::Arc};

/// An analysis producer supplies annotations without owning GPUI views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub id: u64,
    pub range: SourceRange,
    pub kind: AnnotationKind,
    pub payload: Arc<str>,
    pub color: ColorName,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    Highlight,
    Underline,
    InlineHint,
    Lens,
    Bubble,
    Symbol,
}
/// Additional columns (blame, coverage, breakpoints, etc.) are supplied as sparse data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterColumn {
    pub title: Arc<str>,
    pub cells: std::collections::BTreeMap<usize, Arc<str>>,
    pub width_chars: usize,
}
/// Keep the first line; hide `start+1..end` when collapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub start: usize,
    pub end: usize,
    pub collapsed: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Source(usize),
    Lens(usize),
}
#[derive(Debug, Clone)]
struct Segment {
    row: usize,
    len: usize,
    source: usize,
    lens: Option<usize>,
}
/// Sparse row index: a million ordinary lines occupy one segment. Folding and lenses add
/// segments, rather than a million-element mapping rebuilt on each fold toggle.
#[derive(Debug, Clone, Default)]
pub struct RowMap {
    segments: Vec<Segment>,
    len: usize,
}
impl RowMap {
    pub fn new(lines: usize, folds: &[Fold], annotations: &[Annotation]) -> Self {
        let mut hidden: Vec<Range<usize>> = folds
            .iter()
            .filter(|f| f.collapsed && f.start < f.end)
            .map(|f| f.start.saturating_add(1).min(lines)..f.end.min(lines))
            .filter(|r| !r.is_empty())
            .collect();
        hidden.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in hidden {
            if let Some(last) = merged.last_mut()
                && r.start <= last.end
            {
                last.end = last.end.max(r.end);
                continue;
            }
            merged.push(r);
        }
        let mut points = std::collections::BTreeSet::from([0, lines]);
        for r in &merged {
            points.insert(r.start);
            points.insert(r.end);
        }
        let mut lenses = std::collections::BTreeMap::<usize, Vec<usize>>::new();
        for (id, a) in annotations
            .iter()
            .enumerate()
            .filter(|(_, a)| a.kind == AnnotationKind::Lens && a.range.start.line < lines)
        {
            points.insert(a.range.start.line);
            lenses.entry(a.range.start.line).or_default().push(id);
        }
        let points: Vec<_> = points.into_iter().collect();
        let mut map = Self::default();
        for pair in points.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            let i = merged.partition_point(|r| r.end <= start);
            if merged.get(i).is_some_and(|r| r.contains(&start)) {
                continue;
            }
            if let Some(ids) = lenses.get(&start) {
                for id in ids {
                    map.push(start, 1, Some(*id));
                }
            }
            map.push(start, end - start, None);
        }
        map
    }
    fn push(&mut self, source: usize, len: usize, lens: Option<usize>) {
        if len == 0 {
            return;
        }
        self.segments.push(Segment {
            row: self.len,
            source,
            len,
            lens,
        });
        self.len += len;
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn row(&self, index: usize) -> Option<Row> {
        if index >= self.len {
            return None;
        }
        let s = self.segments.get(
            self.segments
                .partition_point(|s| s.row <= index)
                .saturating_sub(1),
        )?;
        Some(match s.lens {
            Some(id) => Row::Lens(id),
            None => Row::Source(s.source + index - s.row),
        })
    }
    pub fn source_row(&self, line: usize) -> Option<usize> {
        let i = self
            .segments
            .partition_point(|s| s.source <= line)
            .saturating_sub(1);
        let s = self.segments.get(i)?;
        (s.lens.is_none() && line < s.source + s.len)
            .then_some(s.row + line.saturating_sub(s.source))
    }
    /// Resolve a hidden source line to the preceding visible row (its fold header).
    pub fn source_row_or_before(&self, line: usize) -> Option<usize> {
        let i = self
            .segments
            .partition_point(|s| s.source <= line)
            .checked_sub(1)?;
        let segment = self.segments.get(i)?;
        Some(segment.row + line.saturating_sub(segment.source).min(segment.len - 1))
    }
}
/// Scroll offsets in logical pixels. All external floating-point input is sanitized.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
}
impl Viewport {
    pub fn clamped(self, rows: usize, line_height: f32, height: f32) -> Self {
        let clean = |x: f64| if x.is_finite() { x.max(0.) } else { 0. };
        Self {
            x: clean(self.x),
            y: clean(self.y).min(
                (rows as f64 * clean(f64::from(line_height)) - clean(f64::from(height))).max(0.),
            ),
        }
    }
    pub fn visible(self, rows: usize, line_height: f32, height: f32) -> Range<usize> {
        if !line_height.is_finite() || line_height <= 0. || !height.is_finite() || height <= 0. {
            return 0..0;
        }
        let first = (self.y.max(0.) / f64::from(line_height)).floor() as usize;
        let count = ((height / line_height).ceil() as usize).saturating_add(1);
        first.min(rows)..first.saturating_add(count).min(rows)
    }
}
/// Indexes half-open byte intervals. Subtrees outside the query are pruned, including
/// intervals that start before the viewport and extend into it.
#[derive(Debug, Clone, Default)]
pub struct IntervalIndex {
    items: Vec<(Range<usize>, usize)>,
    max: Vec<usize>,
}
impl IntervalIndex {
    pub fn new(doc: &CodeDocument, annotations: &[Annotation]) -> Self {
        let mut items: Vec<_> = annotations
            .iter()
            .enumerate()
            .filter_map(|(i, a)| doc.range(a.range).map(|r| (r, i)))
            .collect();
        items.sort_by_key(|(r, _)| r.start);
        let mut this = Self {
            max: vec![0; items.len() * 4 + 1],
            items,
        };
        this.build(1, 0, this.items.len());
        this
    }
    fn build(&mut self, n: usize, lo: usize, hi: usize) -> usize {
        if lo == hi {
            return 0;
        }
        let end = if hi - lo == 1 {
            self.items
                .get(lo)
                .map(|(r, _)| r.end.max(r.start.saturating_add(1)))
                .unwrap_or_default()
        } else {
            let mid = (lo + hi) / 2;
            let a = self.build(n * 2, lo, mid);
            a.max(self.build(n * 2 + 1, mid, hi))
        };
        if let Some(slot) = self.max.get_mut(n) {
            *slot = end;
        }
        end
    }
    pub fn query(&self, range: Range<usize>, out: &mut Vec<usize>) -> usize {
        out.clear();
        self.visit(1, 0, self.items.len(), &range, out)
    }
    fn visit(
        &self,
        n: usize,
        lo: usize,
        hi: usize,
        r: &Range<usize>,
        out: &mut Vec<usize>,
    ) -> usize {
        if lo == hi
            || self.max.get(n).copied().unwrap_or_default() <= r.start
            || self.items.get(lo).is_none_or(|(a, _)| a.start >= r.end)
        {
            return 1;
        }
        if hi - lo == 1 {
            if let Some((_, id)) = self.items.get(lo) {
                out.push(*id);
            }
            return 1;
        }
        let mid = (lo + hi) / 2;
        1 + self.visit(n * 2, lo, mid, r, out) + self.visit(n * 2 + 1, mid, hi, r, out)
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubblePlacement {
    pub id: u64,
    pub y: f32,
    pub height: f32,
}
/// Bounded margin packing. Overflow cards are omitted; visible cards never overlap.
pub fn place_bubbles(items: &[(u64, f32, f32)], height: f32, gap: f32) -> Vec<BubblePlacement> {
    if !height.is_finite() || height <= 0. {
        return Vec::new();
    }
    let mut items: Vec<_> = items
        .iter()
        .copied()
        .filter(|(_, y, h)| y.is_finite() && h.is_finite() && *h > 0.)
        .collect();
    items.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let mut result = Vec::new();
    let mut bottom = 0.;
    for (id, y, h) in items {
        let y = y.clamp(0., (height - h).max(0.)).max(bottom);
        if y + h > height {
            continue;
        }
        result.push(BubblePlacement { id, y, height: h });
        bottom = y + h + gap.max(0.);
    }
    result
}
/// A hover callback can return occurrences and a Markdown detail card without changing source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub range: SourceRange,
    pub occurrences: Arc<[SourceRange]>,
    pub markdown: Arc<str>,
}
/// Endpoints refer to source positions, never stale screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub viewer: u64,
    pub position: Position,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    pub from: Anchor,
    pub to: Anchor,
    pub color: ColorName,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ann(line: usize, kind: AnnotationKind) -> Annotation {
        Annotation {
            id: line as u64,
            range: SourceRange::new(Position::new(line, 0), Position::new(line, 0)),
            kind,
            payload: "lens".into(),
            color: ColorName::Blue,
        }
    }
    #[test]
    fn million_lines_use_sparse_rows_and_bounded_viewport() {
        let rows = RowMap::new(1_000_000, &[], &[]);
        assert_eq!(rows.segments.len(), 1);
        assert_eq!(rows.row(999_999), Some(Row::Source(999_999)));
        assert_eq!(
            Viewport { x: 0., y: 10000. }
                .visible(rows.len(), 20., 400.)
                .len(),
            21
        );
        assert_eq!(
            Viewport {
                x: f64::NAN,
                y: f64::INFINITY
            }
            .clamped(10, 20., 400.),
            Viewport::default()
        );
    }
    #[test]
    fn folds_lenses_and_inverse_mapping() {
        let rows = RowMap::new(
            10,
            &[
                Fold {
                    start: 1,
                    end: 6,
                    collapsed: true,
                },
                Fold {
                    start: 2,
                    end: 4,
                    collapsed: true,
                },
            ],
            &[
                ann(0, AnnotationKind::Lens),
                ann(3, AnnotationKind::Lens),
                ann(7, AnnotationKind::Lens),
            ],
        );
        assert_eq!(
            (0..rows.len())
                .filter_map(|r| rows.row(r))
                .collect::<Vec<_>>(),
            vec![
                Row::Lens(0),
                Row::Source(0),
                Row::Source(1),
                Row::Source(6),
                Row::Lens(2),
                Row::Source(7),
                Row::Source(8),
                Row::Source(9)
            ]
        );
        for row in 0..rows.len() {
            if let Some(Row::Source(line)) = rows.row(row) {
                assert_eq!(rows.source_row(line), Some(row));
            }
        }
        assert_eq!(rows.source_row(3), None);
        assert_eq!(rows.source_row_or_before(3), Some(2));
    }
    #[test]
    fn interval_query_is_sublinear_and_finds_spanning_ranges() {
        let doc = CodeDocument::new(Arc::<str>::from("x\n".repeat(10000)));
        let mut anns: Vec<_> = (0..10000).map(|i| ann(i, AnnotationKind::Bubble)).collect();
        anns.push(Annotation {
            range: SourceRange::new(Position::new(0, 0), Position::new(9999, 1)),
            ..ann(0, AnnotationKind::Highlight)
        });
        let index = IntervalIndex::new(&doc, &anns);
        let mut found = Vec::new();
        let visits = index.query(18000..18001, &mut found);
        assert!(visits < 100, "{visits}");
        assert_eq!(found.len(), 2);
        assert!(found.contains(&10000));
    }
    #[test]
    fn bubbles_never_overlap_and_overflow_is_bounded() {
        let placed = place_bubbles(
            &[
                (1, 10., 30.),
                (2, 10., 30.),
                (3, 12., 30.),
                (4, f32::NAN, 2.),
            ],
            80.,
            4.,
        );
        assert_eq!(placed.len(), 2);
        assert!(placed[1].y >= placed[0].y + placed[0].height + 4.);
    }
}

#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;
    #[test]
    fn visible_rows_handles_zero_and_extreme_finite_viewports() {
        let viewport = Viewport::default();
        assert_eq!(viewport.visible(10, 20., 0.), 0..0);
        assert_eq!(viewport.visible(10, 20., f32::INFINITY), 0..0);
        assert_eq!(viewport.visible(10, f32::MIN_POSITIVE, f32::MAX), 0..10);
    }
    proptest! {
        #[test]
        fn sparse_folding_matches_a_naive_line_filter(lines in 1usize..200, raw in prop::collection::vec((0usize..200,0usize..200,any::<bool>()),0..30)) {
            let folds:Vec<_>=raw.into_iter().map(|(a,b,collapsed)|Fold{start:a.min(b),end:a.max(b),collapsed}).collect();
            let rows=RowMap::new(lines,&folds,&[]);
            let expected:Vec<_>=(0..lines).filter(|line|!folds.iter().any(|f|f.collapsed && *line>f.start && *line<f.end)).collect();
            prop_assert_eq!(rows.len(),expected.len());
            for (row,line) in expected.into_iter().enumerate(){prop_assert_eq!(rows.row(row),Some(Row::Source(line)));prop_assert_eq!(rows.source_row(line),Some(row));}
        }
        #[test]
        fn fold_updates_match_visible_source_lines(
            operations in prop::collection::vec((0usize..40, 0usize..40, any::<bool>()), 1..30),
        ) {
            const LINES: usize = 40;
            let mut folds = std::collections::BTreeMap::new();
            for (a, b, collapsed) in operations {
                let start = a.min(b);
                let end = a.max(b);
                folds.insert(start, Fold { start, end, collapsed });
                let snapshot: Vec<_> = folds.values().cloned().collect();
                let row_map = RowMap::new(LINES, &snapshot, &[]);
                let mut next_row = 0;
                for line in 0..LINES {
                    let hidden = snapshot.iter().any(|fold| {
                        fold.collapsed && fold.start < line && line < fold.end
                    });
                    if hidden {
                        prop_assert_eq!(row_map.source_row(line), None);
                    } else {
                        prop_assert_eq!(row_map.source_row(line), Some(next_row));
                        prop_assert_eq!(row_map.row(next_row), Some(Row::Source(line)));
                        next_row += 1;
                    }
                }
                prop_assert_eq!(row_map.len(), next_row);
            }
        }
        #[test]
        fn interval_tree_matches_linear_query(raw in prop::collection::vec((0usize..199,0usize..199),0..100),start in 0usize..199,width in 1usize..20) {
            let doc=CodeDocument::new(Arc::<str>::from("x\n".repeat(200)));
            let annotations:Vec<_>=raw.iter().enumerate().map(|(id,(a,b))|Annotation{id:id as u64,range:SourceRange::new(Position::new((*a).min(*b),0),Position::new((*a).max(*b),1)),kind:AnnotationKind::Highlight,payload:"".into(),color:ColorName::Blue}).collect();
            let query=start*2..(start+width)*2;let index=IntervalIndex::new(&doc,&annotations);let mut actual=Vec::new();index.query(query.clone(),&mut actual);actual.sort_unstable();
            let expected:Vec<_>=annotations.iter().enumerate().filter(|(_,a)|doc.range(a.range).is_some_and(|r|r.start<query.end && r.end>query.start)).map(|(i,_)|i).collect();
            prop_assert_eq!(actual,expected);
        }
    }
}

/// A source line plus presentation-only hints. The mapping preserves byte positions for
/// selection, symbol callbacks and traces; copied text always comes from CodeDocument.
#[derive(Debug, Clone)]
pub struct DisplayLine {
    pub text: Arc<str>,
    insertions: Vec<(usize, Range<usize>)>,
}
impl DisplayLine {
    pub fn new(source: &str, hints: &[(usize, Arc<str>)]) -> Self {
        let mut hints: Vec<_> = hints
            .iter()
            .filter(|(column, _)| *column <= source.len() && source.is_char_boundary(*column))
            .collect();
        hints.sort_by_key(|(column, _)| *column);
        let mut text = String::with_capacity(source.len());
        let mut insertions = Vec::new();
        let mut cursor = 0;
        for (column, hint) in hints {
            if let Some(chunk) = source.get(cursor..*column) {
                text.push_str(chunk);
            }
            cursor = *column;
            let start = text.len();
            let hint = hint.replace(['\n', '\r'], " ");
            let hint = hint.trim();
            let word = |c: char| c.is_alphanumeric() || c == '_';
            // Pad only where the hint would otherwise touch a word: `x: u32 = 1`, not
            // `x : u32  = 1`.
            let before = source[..*column].chars().next_back();
            if before.is_some_and(word) && hint.chars().next().is_some_and(word) {
                text.push(' ');
            }
            text.push_str(hint);
            let after = source[*column..].chars().next();
            if after.is_some_and(|c| !c.is_whitespace() && !matches!(c, ',' | ')' | ';' | ']'))
                && hint
                    .chars()
                    .next_back()
                    .is_some_and(|c| c != '(' && c != '[')
            {
                text.push(' ');
            }
            insertions.push((*column, start..text.len()));
        }
        text.push_str(source.get(cursor..).unwrap_or_default());
        Self {
            text: text.into(),
            insertions,
        }
    }
    pub fn source_to_display(&self, byte: usize, after_hints: bool) -> usize {
        byte + self
            .insertions
            .iter()
            .filter(|(source, _)| *source < byte || (after_hints && *source == byte))
            .map(|(_, range)| range.len())
            .sum::<usize>()
    }
    pub fn display_to_source(&self, byte: usize) -> usize {
        let mut inserted = 0;
        for (source, range) in &self.insertions {
            if byte < range.start {
                break;
            }
            if byte < range.end {
                return *source;
            }
            inserted += range.len();
        }
        byte.saturating_sub(inserted)
    }
    pub fn insertions(&self) -> impl Iterator<Item = &(usize, Range<usize>)> {
        self.insertions.iter()
    }
}
#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn hints_preserve_unicode_source_coordinates() {
        let source = "let α = 1;";
        let display = DisplayLine::new(source, &[(6, Arc::from(": i32"))]);
        assert_eq!(display.text.as_ref(), "let α: i32 = 1;");
        for byte in 0..=source.len() {
            if source.is_char_boundary(byte) {
                assert_eq!(
                    display.display_to_source(display.source_to_display(byte, true)),
                    byte
                );
            }
        }
        assert_eq!(display.display_to_source(8), 6);
    }
    #[test]
    fn hints_pad_without_doubling_spaces() {
        let display = |s: &str, c: usize, h: &str| DisplayLine::new(s, &[(c, Arc::from(h))]).text;
        assert_eq!(
            display("let sum = 1;", 7, ": u32").as_ref(),
            "let sum: u32 = 1;"
        );
        assert_eq!(display("f(items)", 2, "xs:").as_ref(), "f(xs: items)");
        assert_eq!(display("f(a, b)", 5, "y:").as_ref(), "f(a, y: b)");
        assert_eq!(display("x", 1, "i32").as_ref(), "x i32");
    }
}

/// A coherent analysis snapshot prepared away from the UI thread. Indexing, validation,
/// hint expansion and folded-row construction happen here; publication moves these buffers.
/// The document Arc is the admission stamp, so obsolete analysis is rejected after replacement.
pub struct PreparedAnalysis {
    pub(crate) document: Arc<CodeDocument>,
    pub(crate) annotations: Arc<[Annotation]>,
    pub(crate) index: IntervalIndex,
    pub(crate) rows: RowMap,
    pub(crate) folds: Vec<Fold>,
    pub(crate) hints: std::collections::BTreeMap<usize, DisplayLine>,
    pub(crate) ids: std::collections::BTreeMap<u64, usize>,
    pub(crate) text: Arc<[Annotation]>,
}
impl PreparedAnalysis {
    /// Intended for a host's background worker. Invalid ranges are omitted.
    pub fn new(
        document: Arc<CodeDocument>,
        annotations: Arc<[Annotation]>,
        mut folds: Vec<Fold>,
    ) -> Self {
        let annotations: Arc<[Annotation]> = if annotations
            .iter()
            .all(|a| document.range(a.range).is_some())
        {
            annotations
        } else {
            annotations
                .iter()
                .filter(|a| document.range(a.range).is_some())
                .cloned()
                .collect::<Vec<_>>()
                .into()
        };
        folds.sort_by_key(|f| f.start);
        let mut hints = std::collections::BTreeMap::<usize, Vec<(usize, Arc<str>)>>::new();
        for a in annotations
            .iter()
            .filter(|a| a.kind == AnnotationKind::InlineHint)
        {
            hints
                .entry(a.range.end.line)
                .or_default()
                .push((a.range.end.column, a.payload.clone()));
        }
        let hints = hints
            .into_iter()
            .filter_map(|(line, hints)| {
                document
                    .line(line)
                    .map(|source| (line, DisplayLine::new(source, &hints)))
            })
            .collect();
        let index = IntervalIndex::new(&document, &annotations);
        let rows = RowMap::new(document.line_count(), &folds, &annotations);
        let ids = annotations
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id, i))
            .collect();
        let text = annotations
            .iter()
            .filter(|a| {
                matches!(
                    a.kind,
                    AnnotationKind::Highlight
                        | AnnotationKind::Underline
                        | AnnotationKind::InlineHint
                        | AnnotationKind::Lens
                )
            })
            .cloned()
            .collect::<Vec<_>>()
            .into();
        Self {
            document,
            annotations,
            index,
            rows,
            folds,
            hints,
            ids,
            text,
        }
    }
}

#[cfg(test)]
mod precision_tests {
    use super::Viewport;
    #[test]
    fn subpixel_scroll_survives_near_a_million_lines() {
        let y = 900_000. * 19.25 + 0.125;
        let v = Viewport { x: 0., y }.clamped(1_000_000, 19.25, 800.);
        assert_eq!(v.y, y);
        assert_eq!(v.visible(1_000_000, 19.25, 800.).start, 900_000);
        assert_eq!((v.y + 0.25) - v.y, 0.25);
    }
}
