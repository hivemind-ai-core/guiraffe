//! Copy a selection as tab-separated values (spreadsheet-compatible quoting).

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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn parse(tsv: &str) -> Vec<Vec<String>> {
        let mut rows = vec![vec![String::new()]];
        let mut chars = tsv.chars().peekable();
        let mut quoted = false;
        let mut at_start = true;
        while let Some(ch) = chars.next() {
            let field = rows.last_mut().and_then(|r| r.last_mut());
            let Some(field) = field else { break };
            if quoted {
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
                    if let Some(r) = rows.last_mut() {
                        r.push(String::new());
                    }
                    at_start = true;
                }
                '\n' => {
                    rows.push(vec![String::new()]);
                    at_start = true;
                }
                c => {
                    field.push(c);
                    at_start = false;
                }
            }
        }
        rows
    }

    #[test]
    fn quoting() {
        let mut s = String::new();
        let data = [["a", "b\tc"], ["say \"hi\"", "x\ny"]];
        assert!(write_tsv(&mut s, 0..2, &[0, 1], |r, c, b| b.push_str(data[r][c])));
        assert_eq!(s, "a\t\"b\tc\"\n\"say \"\"hi\"\"\"\t\"x\ny\"");
    }

    proptest! {
        #[test]
        fn roundtrip(cells in proptest::collection::vec(proptest::collection::vec("[a-c\t\n\" ]{0,6}", 3), 1..6)) {
            let mut s = String::new();
            write_tsv(&mut s, 0..cells.len(), &[0, 1, 2], |r, c, b| b.push_str(&cells[r][c]));
            prop_assert_eq!(parse(&s), cells);
        }
    }
}
