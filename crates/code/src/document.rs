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
/// Source bytes are shared with the caller. Only the line index, hashes and minimap summary
/// are constructed. Build large documents on a background executor before publishing them.
#[derive(Debug)]
pub struct CodeDocument {
    source: Arc<str>,
    starts: Vec<usize>,
    hashes: Vec<u64>,
    minimap: [f32; 256],
}
pub fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
impl CodeDocument {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        let source = source.into();
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| (b == b'\n').then_some(i + 1)),
        );
        let mut this = Self {
            source,
            hashes: Vec::with_capacity(starts.len()),
            starts,
            minimap: [0.; 256],
        };
        for line in 0..this.line_count() {
            let text = this.line(line).unwrap_or_default();
            let hash = content_hash(text.as_bytes());
            let density = text.trim().len().min(120) as f32 / 120.;
            this.hashes.push(hash);
            let bucket = line.saturating_mul(256) / this.line_count();
            if let Some(bin) = this.minimap.get_mut(bucket) {
                *bin = bin.max(density);
            }
        }
        this
    }
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)?;
        let mut end = self
            .starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.source.len());
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
        self.hashes.get(line).copied()
    }
    pub fn offset(&self, position: Position) -> Option<usize> {
        let line = self.line_range(position.line)?;
        let byte = line.start.checked_add(position.column)?;
        (byte <= line.end && self.source.is_char_boundary(byte)).then_some(byte)
    }
    pub fn position(&self, offset: usize) -> Option<Position> {
        if offset > self.source.len() || !self.source.is_char_boundary(offset) {
            return None;
        }
        let line = self
            .starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        Some(Position::new(
            line,
            offset.saturating_sub(*self.starts.get(line)?),
        ))
    }
    pub fn range(&self, range: SourceRange) -> Option<Range<usize>> {
        let start = self.offset(range.start)?;
        let end = self.offset(range.end)?;
        (start <= end).then_some(start..end)
    }
    pub fn minimap(&self) -> &[f32; 256] {
        &self.minimap
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
