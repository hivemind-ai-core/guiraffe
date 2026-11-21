use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

use guiraffe_datagrid::{
    Nav, Selection, SelectionMode,
    model::{
        clipboard,
        undo::{Change, UndoLog},
    },
};

use crate::{CellPos, a1, parse_a1};

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
        if self.format.number == NumberFormat::Text && self.text.starts_with('=') {
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
    transposed: BTreeSet<(usize, usize)>,
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
            transposed: BTreeSet::new(),
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
        if cell == Cell::default() {
            self.cells.remove(&pos);
            self.transposed.remove(&(pos.col, pos.row));
        } else {
            self.cells.insert(pos, cell);
            self.transposed.insert((pos.col, pos.row));
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
        let (axis, limit) = if vertical {
            (from.row, self.rows)
        } else {
            (from.col, self.cols)
        };
        let cell = |axis| {
            if vertical {
                CellPos::new(axis, from.col)
            } else {
                CellPos::new(from.row, axis)
            }
        };
        let next = if backwards {
            axis.checked_sub(1)?
        } else {
            axis.saturating_add(1)
        };
        if next >= limit {
            return Some(from);
        }
        if self.cell(from).is_some_and(|c| !c.text.is_empty())
            && self.cell(cell(next)).is_some_and(|c| !c.text.is_empty())
        {
            let mut edge = next;
            while let Some(candidate) = if backwards {
                edge.checked_sub(1)
            } else {
                edge.checked_add(1)
            } {
                if !self
                    .cell(cell(candidate))
                    .is_some_and(|c| !c.text.is_empty())
                {
                    break;
                }
                edge = candidate;
            }
            return Some(cell(edge));
        }
        let range = if backwards { 0..axis } else { next..limit };
        let mut occupied = if vertical {
            let lane = from.col;
            Box::new(
                self.transposed
                    .range((lane, range.start)..(lane, range.end))
                    .map(|&(_, row)| row),
            ) as Box<dyn DoubleEndedIterator<Item = usize>>
        } else {
            Box::new(
                self.cells
                    .range(cell(range.start)..cell(range.end))
                    .map(|(pos, _)| pos.col),
            )
        }
        .filter(|&axis| self.cell(cell(axis)).is_some_and(|c| !c.text.is_empty()));
        let edge = if backwards {
            occupied.next_back().unwrap_or(0)
        } else {
            occupied.next().unwrap_or(limit - 1)
        };
        Some(cell(edge))
    }

    pub fn copy_tsv(&self) -> Option<String> {
        let (rows, cols) = self.selection.rect()?;
        if rows.len().saturating_mul(cols.len()) > clipboard::MAX_COPY_CELLS {
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
        if ch.is_ascii_alphanumeric() || ch == '.' {
            let len = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'.')
                .count();
            let (token, tail) = rest.split_at(len);
            if let Some(pos) = parse_a1(token) {
                let row = (pos.row as i128 + to.row as i128 - from.row as i128)
                    .try_into()
                    .ok();
                let col = (pos.col as i128 + to.col as i128 - from.col as i128)
                    .try_into()
                    .ok();
                match (row, col) {
                    (Some(row), Some(col)) if row < rows && col < cols => {
                        output.push_str(&a1(CellPos::new(row, col)))
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

    proptest! {
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
