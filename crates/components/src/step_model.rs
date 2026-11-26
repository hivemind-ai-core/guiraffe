//! The `StepGrid` model: rows of fixed cells keyed by a host `RowId`, with per-row length
//! (polymeter). It lives in a [`Store`] (see [`step_store`]):
//!
//! - **Host writes** are store events ([`SetRows`], [`SetCursor`]); they never reach the bus or
//!   emit a `StepGridEvent`.
//! - **User edits** are [`StepAction`]s on the action bus (`toggle_cell`, `set_cell_level`,
//!   `paint_cells`, `set_row_length`). A drag-paint commits one `PaintCells` in a
//!   `dispatch_batch`, so it is one undo step.
//!
//! GPUI-free apart from `SharedString` and `App` for the store constructor.

use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::{App, SharedString};
use guiraffe_core::{
    ColorName,
    protocol::{Action, ActionError},
    widget_value::Value,
};
use guiraffe_state::{Event, Fx, HistoryLimits, Store};

/// A row's stable key, provided by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RowId(pub u64);

impl From<u64> for RowId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// What a cell holds. The host describes it; the grid paints it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepCell {
    pub on: bool,
    /// 0..=1: velocity or level, painted as fill height or brightness.
    pub level: f32,
    /// An accent badge.
    pub flag: bool,
    /// 0..=1: below 1 the cell is drawn dotted (a probability).
    pub probability: f32,
    /// Repeats within the step (1 = none), drawn as stacked ticks.
    pub ratchet: u8,
}

impl Default for StepCell {
    fn default() -> Self {
        Self {
            on: false,
            level: 1.0,
            flag: false,
            probability: 1.0,
            ratchet: 1,
        }
    }
}

impl StepCell {
    pub fn on() -> Self {
        Self {
            on: true,
            ..Self::default()
        }
    }

    pub fn with_level(mut self, level: f32) -> Self {
        self.level = level.clamp(0.0, 1.0);
        self
    }

    pub fn flagged(mut self) -> Self {
        self.flag = true;
        self
    }

    pub fn with_probability(mut self, p: f32) -> Self {
        self.probability = p.clamp(0.0, 1.0);
        self
    }

    pub fn with_ratchet(mut self, n: u8) -> Self {
        self.ratchet = n.clamp(1, 8);
        self
    }
}

/// One row: its key, name, colour chip, length and cells.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRow {
    pub id: RowId,
    pub name: SharedString,
    pub color: ColorName,
    /// Steps before the row repeats (polymeter); at most the grid's column count.
    pub length: usize,
    pub muted: bool,
    pub soloed: bool,
    cells: Vec<StepCell>,
}

impl StepRow {
    /// A row of `length` empty cells.
    pub fn new(id: impl Into<RowId>, name: impl Into<SharedString>, length: usize) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            color: ColorName::Blue,
            length: length.max(1),
            muted: false,
            soloed: false,
            cells: vec![StepCell::default(); length.max(1)],
        }
    }

    pub fn with_color(mut self, color: ColorName) -> Self {
        self.color = color;
        self
    }

    /// Set cell `column` (growing the row's storage if needed).
    pub fn with_cell(mut self, column: usize, cell: StepCell) -> Self {
        self.set(column, cell);
        self
    }

    /// Cells as stored (the first `length` play).
    pub fn cells(&self) -> &[StepCell] {
        &self.cells
    }

    /// The cell shown at `column`: past the row's length it repeats from the start.
    pub fn cell(&self, column: usize) -> StepCell {
        let i = column % self.length.max(1);
        self.cells.get(i).copied().unwrap_or_default()
    }

    /// Whether `column` is a repeat (past the row's length).
    pub fn is_repeat(&self, column: usize) -> bool {
        column >= self.length
    }

    fn set(&mut self, column: usize, cell: StepCell) {
        if self.cells.len() <= column {
            self.cells.resize(column + 1, StepCell::default());
        }
        self.cells[column] = cell;
    }
}

/// The grid model. Cheap to clone: rows are shared.
#[derive(Debug, Clone)]
pub struct StepDoc {
    rows: Arc<Vec<Arc<StepRow>>>,
    pub columns: usize,
    /// Columns per group (shaded bands and bar lines); 0 for none.
    pub group: usize,
}

impl PartialEq for StepDoc {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.rows, &other.rows)
            || (self.rows.len() == other.rows.len()
                && self
                    .rows
                    .iter()
                    .zip(other.rows.iter())
                    .all(|(a, b)| Arc::ptr_eq(a, b) || a == b)))
            && self.columns == other.columns
            && self.group == other.group
    }
}

/// What a [`StepAction`] changed: the bus outcome and the `StepGridEvent`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepChange {
    CellToggled {
        row: RowId,
        column: usize,
        on: bool,
    },
    CellLevelChanged {
        row: RowId,
        column: usize,
        level: f32,
    },
    CellsPainted {
        row: RowId,
        from: usize,
        to: usize,
        on: bool,
    },
    RowLengthChanged {
        row: RowId,
        length: usize,
    },
}

/// A user edit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepAction {
    ToggleCell {
        row: RowId,
        column: usize,
    },
    /// Set a cell's level (clamped to 0..=1); turns it on.
    SetCellLevel {
        row: RowId,
        column: usize,
        level: f32,
    },
    /// Turn columns `from..=to` (either order) on or off.
    PaintCells {
        row: RowId,
        from: usize,
        to: usize,
        on: bool,
    },
    SetRowLength {
        row: RowId,
        length: usize,
    },
}

impl StepDoc {
    pub fn new(columns: usize, group: usize, rows: Vec<StepRow>) -> Self {
        let mut doc = Self {
            rows: Arc::new(Vec::new()),
            columns: columns.max(1),
            group,
        };
        doc.set_rows(rows);
        doc
    }

    pub fn rows(&self) -> &[Arc<StepRow>] {
        &self.rows
    }

    pub fn index_of(&self, id: RowId) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    pub fn row(&self, id: RowId) -> Option<&StepRow> {
        self.rows.iter().find(|r| r.id == id).map(|r| &**r)
    }

    fn row_mut(&mut self, id: RowId) -> Option<&mut StepRow> {
        let i = self.index_of(id)?;
        Arc::make_mut(&mut self.rows).get_mut(i).map(Arc::make_mut)
    }

    /// Replace the rows, diffing by id: unchanged rows keep their allocation. Lengths are
    /// clamped to the column count. Later duplicates of an id are ignored.
    pub fn set_rows(&mut self, rows: Vec<StepRow>) -> bool {
        let columns = self.columns;
        let old: BTreeMap<RowId, &Arc<StepRow>> = self.rows.iter().map(|r| (r.id, r)).collect();
        let mut seen = std::collections::BTreeSet::new();
        let next: Vec<Arc<StepRow>> = rows
            .into_iter()
            .filter(|r| seen.insert(r.id))
            .map(|mut r| {
                r.length = r.length.clamp(1, columns);
                match old.get(&r.id) {
                    Some(prev) if ***prev == r => Arc::clone(prev),
                    _ => Arc::new(r),
                }
            })
            .collect();
        drop(old);
        let same = next.len() == self.rows.len()
            && next
                .iter()
                .zip(self.rows.iter())
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if !same {
            self.rows = Arc::new(next);
        }
        !same
    }

    /// Apply a user action; `None` for a no-op.
    pub fn apply(&mut self, action: StepAction) -> Option<StepChange> {
        let columns = self.columns;
        match action {
            StepAction::ToggleCell { row, column } => {
                let r = self.row(row)?;
                if column >= r.length {
                    return None;
                }
                let mut cell = r.cell(column);
                cell.on = !cell.on;
                self.row_mut(row)?.set(column, cell);
                Some(StepChange::CellToggled {
                    row,
                    column,
                    on: cell.on,
                })
            }
            StepAction::SetCellLevel { row, column, level } => {
                let r = self.row(row)?;
                if column >= r.length || !level.is_finite() {
                    return None;
                }
                let level = level.clamp(0.0, 1.0);
                let mut cell = r.cell(column);
                if cell.on && cell.level == level {
                    return None;
                }
                cell.on = true;
                cell.level = level;
                self.row_mut(row)?.set(column, cell);
                Some(StepChange::CellLevelChanged { row, column, level })
            }
            StepAction::PaintCells { row, from, to, on } => {
                let r = self.row(row)?;
                let (a, b) = (from.min(to), from.max(to).min(r.length.saturating_sub(1)));
                if a > b || a >= r.length {
                    return None;
                }
                if (a..=b).all(|c| r.cell(c).on == on) {
                    return None;
                }
                let row_mut = self.row_mut(row)?;
                for c in a..=b {
                    let mut cell = row_mut.cell(c);
                    cell.on = on;
                    row_mut.set(c, cell);
                }
                Some(StepChange::CellsPainted {
                    row,
                    from: a,
                    to: b,
                    on,
                })
            }
            StepAction::SetRowLength { row, length } => {
                let length = length.clamp(1, columns);
                if self.row(row)?.length == length {
                    return None;
                }
                self.row_mut(row)?.length = length;
                Some(StepChange::RowLengthChanged { row, length })
            }
        }
    }
}

/// Erased action names.
pub mod action_names {
    /// `{row, column}`
    pub const TOGGLE_CELL: &str = "toggle_cell";
    /// `{row, column, level}`
    pub const SET_CELL_LEVEL: &str = "set_cell_level";
    /// `{row, from, to, on}`
    pub const PAINT_CELLS: &str = "paint_cells";
    /// `{row, length}`
    pub const SET_ROW_LENGTH: &str = "set_row_length";
}

fn record(fields: &[(&'static str, Value)]) -> Value {
    Value::Record(Arc::new(
        fields
            .iter()
            .map(|(k, v)| (SharedString::new_static(k), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn num(p: &Value, name: &'static str) -> Result<f64, ActionError> {
    p.field(&[SharedString::new_static(name)])
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or(ActionError::Invalid(name))
}

fn index(p: &Value, name: &'static str) -> Result<u64, ActionError> {
    let v = num(p, name)?;
    (v >= 0.0 && v.fract() == 0.0 && v <= 9_007_199_254_740_992.0)
        .then_some(v as u64)
        .ok_or(ActionError::Invalid(name))
}

impl StepAction {
    pub fn to_action(self) -> Action {
        use action_names::*;
        let n = |v: usize| Value::Num(v as f64);
        let r = |id: RowId| Value::Num(id.0 as f64);
        let (name, payload) = match self {
            Self::ToggleCell { row, column } => (
                TOGGLE_CELL,
                record(&[("row", r(row)), ("column", n(column))]),
            ),
            Self::SetCellLevel { row, column, level } => (
                SET_CELL_LEVEL,
                record(&[
                    ("row", r(row)),
                    ("column", n(column)),
                    ("level", Value::Num(f64::from(level))),
                ]),
            ),
            Self::PaintCells { row, from, to, on } => (
                PAINT_CELLS,
                record(&[
                    ("row", r(row)),
                    ("from", n(from)),
                    ("to", n(to)),
                    ("on", Value::Bool(on)),
                ]),
            ),
            Self::SetRowLength { row, length } => (
                SET_ROW_LENGTH,
                record(&[("row", r(row)), ("length", n(length))]),
            ),
        };
        Action {
            name: SharedString::new_static(name),
            payload,
        }
    }

    pub fn decode(action: &Action) -> Result<Self, ActionError> {
        use action_names::*;
        let p = &action.payload;
        let row = || index(p, "row").map(RowId);
        let at = |name| index(p, name).map(|v| v as usize);
        Ok(match action.name.as_ref() {
            TOGGLE_CELL => Self::ToggleCell {
                row: row()?,
                column: at("column")?,
            },
            SET_CELL_LEVEL => Self::SetCellLevel {
                row: row()?,
                column: at("column")?,
                level: num(p, "level")? as f32,
            },
            PAINT_CELLS => Self::PaintCells {
                row: row()?,
                from: at("from")?,
                to: at("to")?,
                on: p
                    .field(&[SharedString::new_static("on")])
                    .as_bool()
                    .ok_or(ActionError::Invalid("on"))?,
            },
            SET_ROW_LENGTH => Self::SetRowLength {
                row: row()?,
                length: at("length")?,
            },
            _ => return Err(ActionError::Unsupported),
        })
    }
}

impl From<StepChange> for Value {
    fn from(c: StepChange) -> Self {
        let n = |v: usize| Value::Num(v as f64);
        let r = |id: RowId| Value::Num(id.0 as f64);
        let kind = |k: &'static str| ("kind", Value::Str(SharedString::new_static(k)));
        match c {
            StepChange::CellToggled { row, column, on } => record(&[
                kind("cellToggled"),
                ("row", r(row)),
                ("column", n(column)),
                ("on", Value::Bool(on)),
            ]),
            StepChange::CellLevelChanged { row, column, level } => record(&[
                kind("cellLevelChanged"),
                ("row", r(row)),
                ("column", n(column)),
                ("level", Value::Num(f64::from(level))),
            ]),
            StepChange::CellsPainted { row, from, to, on } => record(&[
                kind("cellsPainted"),
                ("row", r(row)),
                ("from", n(from)),
                ("to", n(to)),
                ("on", Value::Bool(on)),
            ]),
            StepChange::RowLengthChanged { row, length } => record(&[
                kind("rowLengthChanged"),
                ("row", r(row)),
                ("length", n(length)),
            ]),
        }
    }
}

/// A store with undo over the rows.
pub fn step_store(doc: StepDoc, undo: impl Into<HistoryLimits>, cx: &mut App) -> Store<StepDoc> {
    let store = Store::new(doc, cx);
    store.enable_undo_lens(
        undo,
        |d: &StepDoc| Arc::clone(&d.rows),
        |d: &mut StepDoc, rows| d.rows = rows,
        cx,
    );
    store
}

/// Apply a user action to the store (undoable).
#[derive(Debug, Clone, Copy)]
pub struct ApplyStepAction(pub StepAction);

impl Event<StepDoc> for ApplyStepAction {
    fn handle(self, fx: &mut Fx<'_, StepDoc>) {
        let mut next = fx.state().clone();
        if next.apply(self.0).is_some() {
            fx.set(next);
        }
    }
}

/// Host write: replace the rows, diffed by id. Never echoes.
#[derive(Debug, Clone)]
pub struct SetRows(pub Vec<StepRow>);

impl Event<StepDoc> for SetRows {
    fn handle(self, fx: &mut Fx<'_, StepDoc>) {
        let mut next = fx.state().clone();
        if next.set_rows(self.0) {
            fx.set(next);
        }
    }
    fn undoable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> StepDoc {
        StepDoc::new(
            16,
            4,
            vec![
                StepRow::new(7, "Kick", 16).with_cell(0, StepCell::on()),
                StepRow::new(9, "Hat", 12),
            ],
        )
    }

    #[test]
    fn toggles_levels_and_paints_report_one_change() {
        let mut d = doc();
        let (kick, hat) = (RowId(7), RowId(9));
        assert_eq!(
            d.apply(StepAction::ToggleCell {
                row: kick,
                column: 0
            }),
            Some(StepChange::CellToggled {
                row: kick,
                column: 0,
                on: false
            })
        );
        assert_eq!(
            d.apply(StepAction::SetCellLevel {
                row: kick,
                column: 2,
                level: 1.5
            }),
            Some(StepChange::CellLevelChanged {
                row: kick,
                column: 2,
                level: 1.0
            })
        );
        assert!(
            d.row(kick).is_some_and(|r| r.cell(2).on),
            "a level turns the cell on"
        );
        assert_eq!(
            d.apply(StepAction::PaintCells {
                row: hat,
                from: 14,
                to: 8,
                on: true
            }),
            Some(StepChange::CellsPainted {
                row: hat,
                from: 8,
                to: 11,
                on: true
            }),
            "clamped to the row's length, either direction"
        );
        assert_eq!(
            d.apply(StepAction::PaintCells {
                row: hat,
                from: 8,
                to: 11,
                on: true
            }),
            None
        );
        assert_eq!(
            d.apply(StepAction::ToggleCell {
                row: hat,
                column: 13
            }),
            None,
            "a repeat"
        );
        assert_eq!(
            d.apply(StepAction::SetRowLength {
                row: hat,
                length: 40
            }),
            Some(StepChange::RowLengthChanged {
                row: hat,
                length: 16
            })
        );
    }

    #[test]
    fn cells_past_the_length_repeat() {
        let row = StepRow::new(1, "Hat", 3).with_cell(1, StepCell::on());
        assert!(
            row.cell(4).on && row.is_repeat(4),
            "column 4 repeats column 1"
        );
        assert!(!row.cell(5).on);
    }

    #[test]
    fn set_rows_diffs_by_id() {
        let mut d = doc();
        let before = d.clone();
        let mut rows: Vec<StepRow> = d.rows().iter().map(|r| (**r).clone()).collect();
        rows.reverse();
        rows.push(StepRow::new(9, "Dup", 4));
        assert!(d.set_rows(rows));
        assert_eq!(d.rows()[0].name.as_ref(), "Hat");
        assert!(
            Arc::ptr_eq(&d.rows()[1], &before.rows()[0]),
            "unchanged rows are shared"
        );
        assert_eq!(d.rows().len(), 2, "a duplicate id is ignored");
    }

    #[test]
    fn erased_actions_round_trip() {
        for a in [
            StepAction::ToggleCell {
                row: RowId(3),
                column: 5,
            },
            StepAction::SetCellLevel {
                row: RowId(3),
                column: 5,
                level: 0.5,
            },
            StepAction::PaintCells {
                row: RowId(3),
                from: 2,
                to: 9,
                on: false,
            },
            StepAction::SetRowLength {
                row: RowId(3),
                length: 7,
            },
        ] {
            assert_eq!(StepAction::decode(&a.to_action()), Ok(a));
        }
    }
}
