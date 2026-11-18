//! Clipboard model: encode a selection as tab-separated values (spreadsheet-compatible
//! quoting), decode TSV / plain text back into a [`Block`], and plan where a pasted block
//! lands ([`paste_target`], tiling). No GPUI dependency; shared by the grid and the sheet.

use std::ops::Range;

use super::selection::CellPos;

/// Most cells one copy will serialize; larger selections are truncated (see [`write_tsv`]).
pub const MAX_COPY_CELLS: usize = 2_000_000;

/// Append one field, quoting it when it contains a tab, newline or quote (quotes doubled).
pub fn push_field(out: &mut String, field: &str) {
    if field.contains(['\t', '\n', '\r', '"']) {
        out.push('"');
        for ch in field.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(field);
    }
}

/// Write `rows` × `cols` as TSV. `cell(row, col, buf)` writes a cell's text into a reused
/// buffer. Stops after [`MAX_COPY_CELLS`] cells; returns whether everything was written.
pub fn write_tsv(
    out: &mut String,
    rows: impl IntoIterator<Item = usize>,
    cols: &[usize],
    mut cell: impl FnMut(usize, usize, &mut String),
) -> bool {
    let mut buf = String::new();
    let mut cells = 0usize;
    for (i, r) in rows.into_iter().enumerate() {
        if cells.saturating_add(cols.len()) > MAX_COPY_CELLS {
            return false;
        }
        if i > 0 {
            out.push('\n');
        }
        for (j, &c) in cols.iter().enumerate() {
            if j > 0 {
                out.push('\t');
            }
            buf.clear();
            cell(r, c, &mut buf);
            push_field(out, &buf);
        }
        cells += cols.len();
    }
    true
}

/// A rectangular block of decoded clipboard cells. Ragged rows have implicit empty fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    rows: Vec<Vec<String>>,
    cols: usize,
}

impl Block {
    /// A block from rows of fields; short rows are padded with empty fields.
    pub fn from_rows(mut rows: Vec<Vec<String>>) -> Self {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        if cols == 0 {
            rows.clear();
        }
        for row in &mut rows {
            while row.last().is_some_and(String::is_empty) {
                row.pop();
            }
        }
        Self { rows, cols }
    }

    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, row: usize, col: usize) -> Option<&str> {
        if col >= self.cols {
            return None;
        }
        let row = self.rows.get(row)?;
        Some(row.get(col).map_or("", String::as_str))
    }

    /// The value that lands on `(row, col)` of a target whose top-left is `origin`, tiling
    /// the block (offsets wrap modulo the block size).
    pub fn tiled(&self, origin: CellPos, row: usize, col: usize) -> Option<&str> {
        if self.is_empty() || row < origin.row || col < origin.col {
            return None;
        }
        self.get(
            (row - origin.row) % self.rows(),
            (col - origin.col) % self.cols,
        )
    }
}

/// Decode clipboard text: TSV with spreadsheet quoting (`"a\tb"`, doubled quotes) and `\n`
/// or `\r\n` row breaks. One trailing row break is ignored (spreadsheets append one), so a
/// plain single value — with or without a newline — decodes to a 1×1 block. A quote opens a
/// quoted field only at the start of a field; elsewhere it is literal.
pub fn parse_tsv(text: &str) -> Block {
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    let mut rows: Vec<Vec<String>> = vec![vec![String::new()]];
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut at_start = true;
    while let Some(ch) = chars.next() {
        let Some(row) = rows.last_mut() else { break };
        if quoted {
            let Some(field) = row.last_mut() else { break };
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            '\t' => {
                row.push(String::new());
                at_start = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                rows.push(vec![String::new()]);
                at_start = true;
            }
            c => {
                if let Some(field) = row.last_mut() {
                    field.push(c);
                }
                at_start = false;
            }
        }
    }
    Block::from_rows(rows)
}

/// Where a pasted `block_rows` × `block_cols` block lands in a `rows` × `cols` grid.
///
/// - When `selection` is larger than one cell and the block divides it exactly in both
///   directions, the block is **tiled** over the whole selection.
/// - Otherwise it is pasted once at the selection's top-left cell (or at `cursor` when there
///   is no rectangle).
///
/// The result is clipped to the grid; `None` when nothing lands.
pub fn paste_target(
    block_rows: usize,
    block_cols: usize,
    selection: Option<(Range<usize>, Range<usize>)>,
    cursor: Option<CellPos>,
    rows: usize,
    cols: usize,
) -> Option<(Range<usize>, Range<usize>)> {
    if block_rows == 0 || block_cols == 0 {
        return None;
    }
    let once = |p: CellPos| {
        (
            p.row..p.row.saturating_add(block_rows),
            p.col..p.col.saturating_add(block_cols),
        )
    };
    let (r, c) = match selection {
        Some((r, c)) if !r.is_empty() && !c.is_empty() => {
            let tiles = (r.len() > 1 || c.len() > 1)
                && r.len().is_multiple_of(block_rows)
                && c.len().is_multiple_of(block_cols);
            if tiles {
                (r, c)
            } else {
                once(CellPos::new(r.start, c.start))
            }
        }
        _ => once(cursor?),
    };
    let r = r.start..r.end.min(rows);
    let c = c.start..c.end.min(cols);
    (!r.is_empty() && !c.is_empty()).then_some((r, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rows(b: &Block) -> Vec<Vec<String>> {
        (0..b.rows())
            .map(|r| {
                (0..b.cols())
                    .map(|c| b.get(r, c).unwrap_or("!").to_string())
                    .collect()
            })
            .collect()
    }

    fn owned<const N: usize, const M: usize>(d: [[&str; M]; N]) -> Vec<Vec<String>> {
        d.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn quoting() {
        let mut s = String::new();
        let data = [["a", "b\tc"], ["say \"hi\"", "x\ny"]];
        assert!(write_tsv(&mut s, 0..2, &[0, 1], |r, c, b| b.push_str(data[r][c])));
        assert_eq!(s, "a\t\"b\tc\"\n\"say \"\"hi\"\"\"\t\"x\ny\"");
        assert_eq!(rows(&parse_tsv(&s)), owned(data));
    }

    #[test]
    fn plain_values_and_line_endings() {
        let b = parse_tsv("42");
        assert_eq!((b.rows(), b.cols(), b.get(0, 0)), (1, 1, Some("42")));
        assert_eq!(parse_tsv("42\n"), b, "one trailing newline is ignored");
        assert_eq!(
            rows(&parse_tsv("a\tb\r\nc\r\n")),
            owned([["a", "b"], ["c", ""]]),
            "CRLF; ragged rows padded"
        );
        assert_eq!(
            parse_tsv("").get(0, 0),
            Some(""),
            "empty text: one empty cell"
        );
        assert_eq!(parse_tsv("x\"y").get(0, 0), Some("x\"y"), "mid-field quote");
        assert_eq!(
            parse_tsv("\"a\r\nb\"").get(0, 0),
            Some("a\r\nb"),
            "quoted CRLF kept"
        );
    }

    #[test]
    fn tiling_and_clipping() {
        // A 1×2 block divides a 3×4 selection: tiled over all of it.
        let sel = Some((2..5, 1..5));
        assert_eq!(
            paste_target(1, 2, sel.clone(), None, 10, 10),
            Some((2..5, 1..5))
        );
        // 2 rows don't divide 3: pasted once at the top-left.
        assert_eq!(paste_target(2, 2, sel, None, 10, 10), Some((2..4, 1..3)));
        // A single-cell selection never tiles; clipped at the grid edge.
        assert_eq!(
            paste_target(3, 3, Some((8..9, 8..9)), None, 10, 10),
            Some((8..10, 8..10))
        );
        // No rectangle (row modes): the cursor anchors it.
        let cur = Some(CellPos::new(4, 0));
        assert_eq!(paste_target(1, 1, None, cur, 10, 3), Some((4..5, 0..1)));
        assert_eq!(paste_target(1, 1, None, None, 10, 3), None);
        let past = Some(CellPos::new(10, 0));
        assert_eq!(paste_target(1, 1, None, past, 10, 3), None);
        let b = parse_tsv("a\tb\nc\td");
        let o = CellPos::new(2, 1);
        assert_eq!(b.tiled(o, 2, 1), Some("a"));
        assert_eq!(b.tiled(o, 5, 4), Some("d"));
        assert_eq!(b.tiled(o, 1, 1), None);
    }

    #[test]
    fn ragged_clipboard_does_not_materialise_implicit_empty_cells() {
        let text = format!("{}x\n{}y", "\t".repeat(2_000), "x\n".repeat(2_000));
        let block = parse_tsv(&text);
        assert_eq!((block.rows(), block.cols()), (2_002, 2_001));
        assert_eq!(block.get(0, 2_000), Some("x"));
        assert_eq!(block.get(2_001, 2_000), Some(""));
        assert_eq!(block.get(2_002, 2_000), None);
        assert!(block.rows.iter().map(Vec::capacity).sum::<usize>() <= 8 * 2_002);
        assert_eq!(parse_tsv("a\tb\nc\t"), parse_tsv("a\tb\nc"));
    }

    proptest! {
        #[test]
        fn roundtrip(cells in proptest::collection::vec(proptest::collection::vec("[a-c\t\n\r\" ]{0,6}", 2..4), 1..6)) {
            let w = cells.iter().map(Vec::len).max().unwrap_or(0);
            let cells: Vec<Vec<String>> = cells
                .into_iter()
                .map(|mut r| { r.resize(w, String::new()); r })
                .collect();
            let cols: Vec<usize> = (0..w).collect();
            let mut s = String::new();
            write_tsv(&mut s, 0..cells.len(), &cols, |r, c, b| b.push_str(&cells[r][c]));
            prop_assert_eq!(rows(&parse_tsv(&s)), cells);
        }

        #[test]
        fn target_is_in_bounds_and_tiles_exactly(
            br in 1usize..4, bc in 1usize..4,
            r0 in 0usize..12, c0 in 0usize..12, h in 1usize..9, w in 1usize..9,
            rows in 1usize..15, cols in 1usize..15,
        ) {
            let sel = Some((r0..r0 + h, c0..c0 + w));
            match paste_target(br, bc, sel, None, rows, cols) {
                Some((r, c)) => {
                    prop_assert!(r.end <= rows && c.end <= cols);
                    prop_assert_eq!((r.start, c.start), (r0, c0), "anchored at top-left");
                    let tiles = (h > 1 || w > 1) && h % br == 0 && w % bc == 0;
                    let (eh, ew) = if tiles { (h, w) } else { (br, bc) };
                    prop_assert_eq!(r.len(), eh.min(rows - r0));
                    prop_assert_eq!(c.len(), ew.min(cols - c0));
                    let block = Block::from_rows(
                        (0..br).map(|i| (0..bc).map(|j| format!("{i},{j}")).collect()).collect(),
                    );
                    let o = CellPos::new(r0, c0);
                    for rr in r.clone() {
                        for cc in c.clone() {
                            let want = format!("{},{}", (rr - r0) % br, (cc - c0) % bc);
                            prop_assert_eq!(block.tiled(o, rr, cc), Some(want.as_str()));
                        }
                    }
                }
                None => prop_assert!(r0 >= rows || c0 >= cols),
            }
        }
    }
}
