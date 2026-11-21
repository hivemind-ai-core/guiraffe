use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};

use guiraffe_datagrid::{
    Nav, Selection, SelectionMode,
    model::{
        clipboard,
        undo::{Change, UndoLog},
    },
    selection::RowSet,
};

use crate::{CellPos, address::parse_reference, column_name};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum NumberFormat {
    #[default]
    General,
    Number(u8),
    /// ISO date from a day count relative to 1970-01-01.
    Date,
    Text,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct CellFormat {
    pub number: NumberFormat,
    pub align: Alignment,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Cell {
    pub text: Arc<str>,
    pub format: CellFormat,
}

impl Cell {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn input(&self) -> Arc<str> {
        if self.format.number == NumberFormat::Text {
            Arc::from("")
        } else {
            self.text.clone()
        }
    }
}

type History = UndoLog<CellPos, Cell, ()>;
const MAX_BATCH: usize = 100_000;

pub struct SheetModel {
    rows: usize,
    cols: usize,
    cells: BTreeMap<CellPos, Cell>,
    occupied_rows: HashMap<usize, RowSet>,
    occupied_cols: HashMap<usize, RowSet>,
    history: History,
    pub selection: Selection,
    pub read_only: bool,
}

impl SheetModel {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self::from_cells(rows, cols, [])
    }

    pub fn from_cells(
        rows: usize,
        cols: usize,
        cells: impl IntoIterator<Item = (CellPos, Cell)>,
    ) -> Self {
        let mut model = Self {
            rows,
            cols,
            cells: BTreeMap::new(),
            occupied_rows: HashMap::new(),
            occupied_cols: HashMap::new(),
            history: History::default(),
            selection: Selection::new(SelectionMode::Range),
            read_only: false,
        };
        for (pos, cell) in cells {
            if model.contains(pos) {
                model.write(pos, cell);
            }
        }
        model
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn cells(&self) -> impl Iterator<Item = (CellPos, &Cell)> {
        self.cells.iter().map(|(&pos, cell)| (pos, cell))
    }

    pub fn cell(&self, pos: CellPos) -> Option<&Cell> {
        self.cells.get(&pos)
    }

    pub fn contains(&self, pos: CellPos) -> bool {
        pos.row < self.rows && pos.col < self.cols
    }

    fn write(&mut self, pos: CellPos, cell: Cell) {
        let occupied = !cell.text.is_empty();
        let was_occupied = self.cell(pos).is_some_and(|old| !old.text.is_empty());
        if occupied != was_occupied {
            Self::update_occupancy(&mut self.occupied_rows, pos.row, pos.col, occupied);
            Self::update_occupancy(&mut self.occupied_cols, pos.col, pos.row, occupied);
        }
        if cell == Cell::default() {
            self.cells.remove(&pos);
        } else {
            self.cells.insert(pos, cell);
        }
    }

    fn update_occupancy(
        lanes: &mut HashMap<usize, RowSet>,
        lane: usize,
        at: usize,
        occupied: bool,
    ) {
        if occupied {
            lanes.entry(lane).or_default().insert(at..at + 1);
        } else if let Some(range) = lanes.get_mut(&lane) {
            range.remove(at);
            if range.is_empty() {
                lanes.remove(&lane);
            }
        }
    }

    /// Apply a single transaction and return changed coordinates for formula invalidation.
    pub fn edit(&mut self, writes: impl IntoIterator<Item = (CellPos, Cell)>) -> Vec<CellPos> {
        if self.read_only {
            return Vec::new();
        }
        let mut changes = Vec::new();
        for (pos, new) in writes {
            if !self.contains(pos) {
                continue;
            }
            let old = self.cell(pos).cloned().unwrap_or_default();
            if old != new {
                self.write(pos, new.clone());
                changes.push(Change { key: pos, old, new });
            }
        }
        let positions = changes.iter().map(|change| change.key).collect();
        self.history.push((), changes);
        positions
    }

    pub fn undo(&mut self) -> Vec<CellPos> {
        self.restore(false)
    }

    pub fn redo(&mut self) -> Vec<CellPos> {
        self.restore(true)
    }

    fn restore(&mut self, redo: bool) -> Vec<CellPos> {
        if self.read_only {
            return Vec::new();
        }
        let history = if redo {
            self.history.redo()
        } else {
            self.history.undo()
        };
        let Some(history) = history else {
            return Vec::new();
        };
        let mut steps: Vec<_> = history
            .changes
            .iter()
            .map(|change| {
                (
                    change.key,
                    if redo {
                        change.new.clone()
                    } else {
                        change.old.clone()
                    },
                )
            })
            .collect();
        if !redo {
            steps.reverse();
        }
        let positions = steps.iter().map(|(pos, _)| *pos).collect();
        for (pos, cell) in steps {
            self.write(pos, cell);
        }
        positions
    }

    pub fn select(&mut self, pos: CellPos, extend: bool) {
        if self.contains(pos) {
            self.selection.click(pos, extend, false);
        }
    }

    pub fn navigate(&mut self, nav: Nav, extend: bool, jump: bool, page: usize) {
        let from = self.selection.cursor.unwrap_or_default();
        let dest = if jump {
            self.jump(from, nav)
                .unwrap_or_else(|| nav.apply(from, self.rows, self.cols, page))
        } else {
            nav.apply(from, self.rows, self.cols, page)
        };
        self.select(dest, extend);
    }

    fn jump(&self, from: CellPos, nav: Nav) -> Option<CellPos> {
        let vertical = matches!(nav, Nav::Up | Nav::Down);
        let backwards = matches!(nav, Nav::Up | Nav::Left);
        if !matches!(nav, Nav::Up | Nav::Down | Nav::Left | Nav::Right) {
            return None;
        }
        let (at, limit, lane) = if vertical {
            (from.row, self.rows, self.occupied_cols.get(&from.col))
        } else {
            (from.col, self.cols, self.occupied_rows.get(&from.row))
        };
        if limit == 0 {
            return None;
        }
        let ranges = lane.map_or(&[][..], RowSet::ranges);
        let first_after = ranges.partition_point(|range| range.end <= at);
        let containing = ranges.get(first_after).filter(|range| range.contains(&at));
        let edge = if backwards {
            match containing {
                Some(range) if at > range.start => range.start,
                _ => first_after
                    .checked_sub(1)
                    .and_then(|index| ranges.get(index))
                    .map_or(0, |range| range.end - 1),
            }
        } else {
            match containing {
                Some(range) if at + 1 < range.end => range.end - 1,
                _ => ranges
                    .get(first_after + usize::from(containing.is_some()))
                    .map_or(limit - 1, |range| range.start),
            }
        };
        Some(if vertical {
            CellPos::new(edge, from.col)
        } else {
            CellPos::new(from.row, edge)
        })
    }

    pub fn copy_tsv(&self) -> Option<String> {
        let (rows, cols) = self.selection.rect()?;
        if rows.len().saturating_mul(cols.len()) > MAX_BATCH {
            return None;
        }
        let cols: Vec<_> = cols.collect();
        let mut output = String::new();
        clipboard::write_tsv(&mut output, rows, &cols, |row, col, out| {
            if let Some(cell) = self.cell(CellPos::new(row, col)) {
                out.push_str(&cell.text);
            }
        });
        Some(output)
    }

    pub fn paste_tsv(&mut self, text: &str) -> Vec<CellPos> {
        if self.read_only || text.len() > 16 * 1024 * 1024 {
            return Vec::new();
        }
        let block = clipboard::parse_tsv(text);
        let Some((rows, cols)) = clipboard::paste_target(
            block.rows(),
            block.cols(),
            self.selection.rect(),
            self.selection.cursor,
            self.rows,
            self.cols,
        ) else {
            return Vec::new();
        };
        if rows.len().saturating_mul(cols.len()) > MAX_BATCH {
            return Vec::new();
        }
        let origin = CellPos::new(rows.start, cols.start);
        let mut writes = Vec::new();
        for row in rows {
            for col in cols.clone() {
                let pos = CellPos::new(row, col);
                let mut cell = self.cell(pos).cloned().unwrap_or_default();
                if let Some(text) = block.tiled(origin, row, col) {
                    cell.text = Arc::from(text);
                    writes.push((pos, cell));
                }
            }
        }
        self.edit(writes)
    }

    pub fn clear_selection(&mut self) -> Vec<CellPos> {
        let Some((rows, cols)) = self.selection.rect() else {
            return Vec::new();
        };
        let writes: Vec<_> = self
            .cells
            .range(CellPos::new(rows.start, 0)..CellPos::new(rows.end, 0))
            .filter(|(pos, cell)| cols.contains(&pos.col) && !cell.text.is_empty())
            .map(|(&pos, cell)| {
                (
                    pos,
                    Cell {
                        text: Arc::from(""),
                        format: cell.format,
                    },
                )
            })
            .collect();
        self.edit(writes)
    }

    /// Copy a rectangle into an adjacent extension. With series enabled, two numeric seeds
    /// define the step in each lane. Formula references translate by the target displacement.
    pub fn fill(
        &mut self,
        rows: Range<usize>,
        cols: Range<usize>,
        target: CellPos,
        series: bool,
    ) -> Vec<CellPos> {
        if self.read_only
            || rows.is_empty()
            || cols.is_empty()
            || !self.contains(target)
            || rows.end > self.rows
            || cols.end > self.cols
        {
            return Vec::new();
        }
        let target_rows = rows.start.min(target.row)..rows.end.max(target.row + 1);
        let target_cols = cols.start.min(target.col)..cols.end.max(target.col + 1);
        if target_rows.len().saturating_mul(target_cols.len()) > MAX_BATCH {
            return Vec::new();
        }
        let vertical = target_rows != rows;
        let axis = if vertical { &rows } else { &cols };
        let mut writes = Vec::new();
        for row in target_rows {
            for col in target_cols.clone() {
                if rows.contains(&row) && cols.contains(&col) {
                    continue;
                }
                let pos = CellPos::new(row, col);
                let tile = |n: usize, range: &Range<usize>| {
                    let span = range.len();
                    range.start + (n % span + span - range.start % span) % span
                };
                let source = CellPos::new(tile(row, &rows), tile(col, &cols));
                let mut cell = self.cell(source).cloned().unwrap_or_default();
                let seed = |n| {
                    if vertical {
                        CellPos::new(n, source.col)
                    } else {
                        CellPos::new(source.row, n)
                    }
                };
                if series
                    && axis.len() >= 2
                    && let Some(first) = self.numeric(seed(axis.start))
                    && let Some(second) = self.numeric(seed(axis.start + 1))
                {
                    let index = if vertical { row } else { col };
                    cell.text = Arc::from(
                        (first + (second - first) * (index as f64 - axis.start as f64)).to_string(),
                    );
                } else if cell.text.starts_with('=') && cell.format.number != NumberFormat::Text {
                    cell.text = Arc::from(translate(&cell.text, source, pos, self.rows, self.cols));
                }
                writes.push((pos, cell));
            }
        }
        self.edit(writes)
    }

    fn numeric(&self, pos: CellPos) -> Option<f64> {
        let number = self.cell(pos)?.text.parse::<f64>().ok()?;
        number.is_finite().then_some(number)
    }
}

fn translate(text: &str, from: CellPos, to: CellPos, rows: usize, cols: usize) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '$') {
            let len = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'$'))
                .count();
            let (token, tail) = rest.split_at(len);
            if let Some((pos, fixed_row, fixed_col)) = parse_reference(token) {
                let translate = |origin: usize, from: usize, to: usize, fixed: bool| {
                    let delta = if fixed { 0 } else { to as i128 - from as i128 };
                    usize::try_from(origin as i128 + delta).ok()
                };
                let row = translate(pos.row, from.row, to.row, fixed_row);
                let col = translate(pos.col, from.col, to.col, fixed_col);
                match (row, col) {
                    (Some(row), Some(col)) if row < rows && col < cols => {
                        use std::fmt::Write as _;
                        let _ = write!(
                            output,
                            "{}{}{}{}",
                            if fixed_col { "$" } else { "" },
                            column_name(col),
                            if fixed_row { "$" } else { "" },
                            row + 1
                        );
                    }
                    _ => output.push_str("#REF!"),
                }
            } else {
                output.push_str(token);
            }
            rest = tail;
        } else {
            output.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn fill_keeps_fixed_reference_axes() {
        let from = CellPos::new(2, 2);
        let to = CellPos::new(3, 3);
        assert_eq!(
            translate("=$A1+A$1+$A$1+A1", from, to, 10, 10),
            "=$A2+B$1+$A$1+B2"
        );
    }

    #[test]
    fn ctrl_jump_uses_contiguous_run_edges() {
        let mut sheet = SheetModel::from_cells(
            20,
            20,
            [0, 1, 2, 10, 11].map(|row| (CellPos::new(row, 0), Cell::new("42"))),
        );
        sheet.select(CellPos::default(), false);
        for (nav, row) in [
            (Nav::Down, 2),
            (Nav::Down, 10),
            (Nav::Down, 11),
            (Nav::Up, 10),
            (Nav::Up, 2),
        ] {
            sheet.navigate(nav, false, true, 1);
            assert_eq!(sheet.selection.cursor, Some(CellPos::new(row, 0)));
        }
    }

    proptest! {
        #[test]
        fn transactions_match_naive(
            transactions in proptest::collection::vec(
                proptest::collection::vec((0usize..32, 0u8..32), 1..15), 1..32
            )
        ) {
            let mut model = SheetModel::new(4, 8);
            let mut dense = [0u8; 32];
            let mut states = vec![dense];
            let assert_state = |model: &SheetModel, expected: &[u8; 32]| {
                for (index, value) in expected.iter().enumerate() {
                    let pos = CellPos::new(index / 8, index % 8);
                    prop_assert_eq!(
                        model.cell(pos).and_then(|cell| cell.text.parse::<u8>().ok()).unwrap_or(0),
                        *value
                    );
                }
                Ok(())
            };
            for batch in transactions {
                let changes = model.edit(batch.into_iter().map(|(index, value)| {
                    dense[index] = value;
                    let cell = if value == 0 { Cell::default() } else { Cell::new(value.to_string()) };
                    (CellPos::new(index / 8, index % 8), cell)
                }));
                if !changes.is_empty() {
                    states.push(dense);
                }
                assert_state(&model, &dense)?;
            }
            for state in states.iter().rev().skip(1) {
                prop_assert!(!model.undo().is_empty());
                assert_state(&model, state)?;
            }
            for state in states.iter().skip(1) {
                prop_assert!(!model.redo().is_empty());
                assert_state(&model, state)?;
            }
        }

        #[test]
        fn series_matches_linear_model(first in -100i32..100, step in -20i32..20, length in 3usize..50) {
            let a = CellPos::new(0, 0);
            let b = CellPos::new(1, 0);
            let mut model = SheetModel::from_cells(length, 1, [
                (a, Cell::new(first.to_string())), (b, Cell::new((first + step).to_string()))
            ]);
            model.fill(0..2, 0..1, CellPos::new(length - 1, 0), true);
            for row in 0..length {
                prop_assert_eq!(model.numeric(CellPos::new(row, 0)), Some(f64::from(first + step * row as i32)));
            }
            prop_assert_eq!(model.undo().len(), length - 2);
            prop_assert_eq!(model.cells().count(), 2);
            prop_assert_eq!(model.redo().len(), length - 2);
            prop_assert_eq!(model.cells().count(), length);
        }
    }
}
