//! Immutable, shared source and line index. Positions use UTF-8 byte columns.
use std::{ops::Range, sync::Arc};

/// A position in source, independent of viewport, folding and inline lens rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}
impl Position {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}
/// Half-open source range; malformed positions are rejected, never snapped silently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SourceRange {
    pub start: Position,
    pub end: Position,
}
impl SourceRange {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}
/// Source bytes are shared with the caller. Only the line index and minimap summary
/// are constructed. Build large documents on a background executor before publishing them.
#[derive(Debug)]
pub struct CodeDocument {
    source: Arc<str>,
    starts: LineStarts,
    minimap: [f32; 256],
}
#[derive(Debug)]
enum LineStarts {
    Uniform { stride: usize, count: usize },
    Compact(Vec<u32>),
    Wide(Vec<usize>),
}

impl LineStarts {
    fn len(&self) -> usize {
        match self {
            Self::Uniform { count, .. } => *count,
            Self::Compact(starts) => starts.len(),
            Self::Wide(starts) => starts.len(),
        }
    }

    fn get(&self, line: usize) -> Option<usize> {
        match self {
            Self::Uniform { stride, count } => (line < *count).then(|| line * stride),
            Self::Compact(starts) => starts.get(line).map(|&start| start as usize),
            Self::Wide(starts) => starts.get(line).copied(),
        }
    }

    fn partition_point(&self, offset: usize) -> usize {
        match self {
            Self::Uniform { stride, count } => (offset / stride).min(count - 1) + 1,
            Self::Compact(starts) => starts.partition_point(|&start| start as usize <= offset),
            Self::Wide(starts) => starts.partition_point(|&start| start <= offset),
        }
    }
}

const INDEX_CHUNK: usize = 16 * 1024;

pub fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
impl CodeDocument {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        let mut document = Self {
            source: source.into(),
            starts: LineStarts::Compact(Vec::new()),
            minimap: [0.; 256],
        };
        document.build_index(|| false);
        document
    }

    pub(crate) fn build(source: Arc<str>, cancelled: impl Fn() -> bool) -> Option<Self> {
        let mut document = Self {
            source,
            starts: LineStarts::Compact(Vec::new()),
            minimap: [0.; 256],
        };
        document.build_index(cancelled).then_some(document)
    }

    fn build_index(&mut self, cancelled: impl Fn() -> bool) -> bool {
        let mut line_count = 1;
        let mut stride = None;
        for (index, chunk) in self.source.as_bytes().chunks(INDEX_CHUNK).enumerate() {
            if cancelled() {
                return false;
            }
            if stride.is_none() {
                stride = chunk
                    .iter()
                    .position(|&byte| byte == b'\n')
                    .map(|first| index * INDEX_CHUNK + first + 1);
            }
            line_count += chunk.iter().filter(|&&b| b == b'\n').count();
        }
        let Some(uniform) = self.uniform_stride(line_count, stride.unwrap_or(1), &cancelled) else {
            return false;
        };
        self.starts = if let Some(stride) = uniform {
            LineStarts::Uniform {
                stride,
                count: line_count,
            }
        } else if u32::try_from(self.source.len()).is_ok() {
            let Some(starts) = self.collect_starts(line_count, &cancelled, |start| start as u32)
            else {
                return false;
            };
            LineStarts::Compact(starts)
        } else {
            let Some(starts) = self.collect_starts(line_count, &cancelled, |start| start) else {
                return false;
            };
            LineStarts::Wide(starts)
        };
        let mut start = 0;
        for bucket in 0..self.minimap.len() {
            let end = ((bucket as u128 + 1) * line_count as u128)
                .div_ceil(self.minimap.len() as u128) as usize;
            let mut longest = 0;
            for line in start..end {
                if line % 256 == 0 && cancelled() {
                    return false;
                }
                let text = self.line(line).unwrap_or_default();
                if text.len() > longest {
                    longest = longest.max(text.trim().len());
                    if longest >= 120 {
                        break;
                    }
                }
            }
            self.minimap[bucket] = longest.min(120) as f32 / 120.;
            start = end;
        }
        true
    }
    fn uniform_stride(
        &self,
        line_count: usize,
        stride: usize,
        cancelled: impl Fn() -> bool,
    ) -> Option<Option<usize>> {
        // The first scan counted all newlines. If each expected separator
        // exists, no additional separators can occur between them.
        let bytes = self.source.as_bytes();
        if (line_count - 1) > bytes.len() / stride {
            return Some(None);
        }
        for line in 1..line_count {
            if line % INDEX_CHUNK == 0 && cancelled() {
                return None;
            }
            if bytes.get(line * stride - 1) != Some(&b'\n') {
                return Some(None);
            }
        }
        Some(Some(stride))
    }

    fn collect_starts<T>(
        &self,
        line_count: usize,
        cancelled: impl Fn() -> bool,
        convert: impl Fn(usize) -> T,
    ) -> Option<Vec<T>> {
        let mut starts = Vec::with_capacity(line_count);
        starts.push(convert(0));
        for (chunk_index, chunk) in self.source.as_bytes().chunks(INDEX_CHUNK).enumerate() {
            if cancelled() {
                return None;
            }
            let base = chunk_index * INDEX_CHUNK;
            starts.extend(
                chunk
                    .iter()
                    .enumerate()
                    .filter(|&(_, &byte)| byte == b'\n')
                    .map(|(index, _)| convert(base + index + 1)),
            );
        }
        Some(starts)
    }

    pub fn source(&self) -> &Arc<str> {
        &self.source
    }
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = self.starts.get(line)?;
        let mut end = self.starts.get(line + 1).unwrap_or(self.source.len());
        if end > start && self.source.as_bytes().get(end - 1) == Some(&b'\n') {
            end -= 1;
        }
        if end > start && self.source.as_bytes().get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
        Some(start..end)
    }
    pub fn line(&self, line: usize) -> Option<&str> {
        self.source.get(self.line_range(line)?)
    }
    pub fn line_hash(&self, line: usize) -> Option<u64> {
        self.line(line).map(|text| content_hash(text.as_bytes()))
    }
    pub fn offset(&self, position: Position) -> Option<usize> {
        let line = self.line_range(position.line)?;
        let byte = line.start.checked_add(position.column)?;
        (byte <= line.end && self.source.is_char_boundary(byte)).then_some(byte)
    }
    /// Offsets inside a line terminator map to the preceding line end.
    pub fn position(&self, offset: usize) -> Option<Position> {
        if offset > self.source.len() || !self.source.is_char_boundary(offset) {
            return None;
        }
        let line = self.starts.partition_point(offset).saturating_sub(1);
        let bytes = self.line_range(line)?;
        Some(Position::new(line, offset.min(bytes.end) - bytes.start))
    }
    pub fn range(&self, range: SourceRange) -> Option<Range<usize>> {
        let start = self.offset(range.start)?;
        let end = self.offset(range.end)?;
        (start <= end).then_some(start..end)
    }
    /// The source in `range`, each line prefixed with its right-aligned one-based number.
    pub fn with_line_numbers(&self, range: SourceRange) -> Option<String> {
        let bytes = self.range(range)?;
        if bytes.is_empty() {
            return None;
        }
        let text = self.source.get(bytes)?;
        let width = (range.end.line + 1).to_string().len();
        let mut out = String::with_capacity(text.len() + (width + 2) * 8);
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!("{:>width$}  {line}", range.start.line + i + 1));
        }
        Some(out)
    }
    pub fn minimap(&self) -> &[f32; 256] {
        &self.minimap
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_build_stops_after_supersession_during_line_scan() {
        let source: Arc<str> = "a\n".repeat(600).into();
        let calls = std::cell::Cell::new(0);
        let document = CodeDocument::build(source, || {
            calls.set(calls.get() + 1);
            calls.get() >= 4
        });
        assert!(document.is_none());
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn minimap_matches_per_line_density_for_sparse_and_dense_buckets() {
        for count in [1, 31, 256, 1025] {
            let source = (0..count)
                .map(|i| match i % 11 {
                    0 => "x".repeat(130),
                    1 => " ".repeat(130),
                    _ => format!("  文{}  ", "x".repeat(i % 120)),
                })
                .collect::<Vec<_>>()
                .join("\r\n");
            let doc = CodeDocument::new(source);
            let mut expected = [0_f32; 256];
            for line in 0..doc.line_count() {
                let density = doc.line(line).unwrap().trim().len().min(120) as f32 / 120.;
                let bucket = line * expected.len() / doc.line_count();
                expected[bucket] = expected[bucket].max(density);
            }
            assert_eq!(doc.minimap(), &expected);
        }
    }

    #[test]
    fn uniform_index_handles_crlf_and_a_long_final_line() {
        let doc = CodeDocument::new("a\r\nb\r\nlong final line");
        assert!(matches!(
            doc.starts,
            LineStarts::Uniform {
                stride: 3,
                count: 3
            }
        ));
        assert_eq!(doc.line(1), Some("b"));
        assert_eq!(doc.position(5), Some(Position::new(1, 1)));
        assert_eq!(doc.position(15), Some(Position::new(2, 9)));
        assert_eq!(doc.offset(Position::new(2, 9)), Some(15));
        assert_eq!(doc.line(3), None);
    }

    #[test]
    fn irregular_newlines_keep_individual_starts() {
        let doc = CodeDocument::new("a\nb\nc longer\nd");
        assert!(matches!(doc.starts, LineStarts::Compact(_)));
        assert_eq!(doc.line(2), Some("c longer"));
        assert_eq!(doc.position(13), Some(Position::new(3, 0)));
    }

    proptest::proptest! {
        #[test]
        fn positions_match_split_lines(lines in proptest::collection::vec("[a-zאב界🙂]{0,16}", 1..40)) {
            let source = lines.join("\r\n");
            let doc = CodeDocument::new(source.as_str());
            proptest::prop_assert_eq!(doc.line_count(), lines.len());
            let mut start = 0;
            for (line, text) in lines.iter().enumerate() {
                proptest::prop_assert_eq!(doc.line(line), Some(text.as_str()));
                for column in 0..=text.len() {
                    let pos = Position::new(line, column);
                    let offset = start + column;
                    if text.is_char_boundary(column) {
                        proptest::prop_assert_eq!(doc.offset(pos), Some(offset));
                        proptest::prop_assert_eq!(doc.position(offset), Some(pos));
                    } else {
                        proptest::prop_assert_eq!(doc.offset(pos), None);
                        proptest::prop_assert_eq!(doc.position(offset), None);
                    }
                }
                if line + 1 < lines.len() {
                    proptest::prop_assert_eq!(
                        doc.position(start + text.len() + 1),
                        Some(Position::new(line, text.len()))
                    );
                }
                start += text.len() + 2;
            }
        }
    }
    #[test]
    fn retained_line_index_has_no_spare_slots() {
        let doc = CodeDocument::new("x\nyz\n".repeat(10_000) + "x");
        let LineStarts::Compact(starts) = &doc.starts else {
            panic!("small files must use compact offsets");
        };
        assert_eq!(starts.capacity(), doc.line_count());
    }
    #[test]
    fn positions_at_crlf_and_terminal_cr_are_selectable() {
        let doc = CodeDocument::new("界\r\n🙂\r");
        assert_eq!(doc.position(4), Some(Position::new(0, 3)));
        assert_eq!(doc.position(10), Some(Position::new(1, 4)));
        assert_eq!(doc.position(10).and_then(|p| doc.offset(p)), Some(9));
    }
    #[test]
    fn copy_with_line_numbers_aligns_numbers() {
        let doc = CodeDocument::new(Arc::<str>::from("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\n"));
        let r = SourceRange::new(Position::new(8, 0), Position::new(9, 1));
        assert_eq!(doc.with_line_numbers(r).as_deref(), Some(" 9  i\n10  j"));
        let empty = SourceRange::new(Position::new(1, 0), Position::new(1, 0));
        assert_eq!(doc.with_line_numbers(empty), None);
    }
    #[test]
    fn shared_source_utf8_crlf_empty_and_trailing_lines() {
        let source: Arc<str> = "α\r\nlast\n".into();
        let doc = CodeDocument::new(source.clone());
        assert!(Arc::ptr_eq(doc.source(), &source));
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.line(0), Some("α"));
        assert_eq!(doc.line(2), Some(""));
        assert_eq!(doc.offset(Position::new(0, 1)), None);
        assert_eq!(doc.offset(Position::new(99, 0)), None);
        assert_eq!(doc.position(4), Some(Position::new(1, 0)));
        assert!(
            doc.range(SourceRange::new(Position::new(1, 2), Position::new(0, 0)))
                .is_none()
        );
        assert_eq!(CodeDocument::new(Arc::<str>::from("")).line_count(), 1);
    }
}
