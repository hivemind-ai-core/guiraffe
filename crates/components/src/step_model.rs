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
    /// A ghost note: a quiet hit, drawn as a smaller inset fill.
    pub soft: bool,
    /// Micro-timing in steps, -0.5..=0.5 (negative is early), drawn as a tick.
    pub offset: f32,
    /// A flam (a grace hit just before), drawn as a bar at the top.
    pub flam: bool,
}

/// A cell's emphasis, as `cycle_flag` steps through it: normal, accent, ghost note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepFlag {
    Normal,
    Accent,
    Ghost,
}

impl Default for StepCell {
    fn default() -> Self {
        Self {
            on: false,
            level: 1.0,
            flag: false,
            probability: 1.0,
            ratchet: 1,
            soft: false,
            offset: 0.0,
            flam: false,
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

    /// A ghost note (see [`StepCell::soft`]).
    pub fn ghost_note(mut self) -> Self {
        self.soft = true;
        self
    }

    /// Micro-timing in steps, clamped to -0.5..=0.5.
    pub fn with_offset(mut self, offset: f32) -> Self {
        self.offset = clamp_offset(offset);
        self
    }

    pub fn with_flam(mut self) -> Self {
        self.flam = true;
        self
    }

    /// The cell's emphasis. An accent wins over a ghost note.
    pub fn emphasis(&self) -> StepFlag {
        match (self.flag, self.soft) {
            (true, _) => StepFlag::Accent,
            (false, true) => StepFlag::Ghost,
            _ => StepFlag::Normal,
        }
    }

    fn set_emphasis(&mut self, flag: StepFlag) {
        self.flag = flag == StepFlag::Accent;
        self.soft = flag == StepFlag::Ghost;
    }
}

fn clamp_offset(offset: f32) -> f32 {
    if offset.is_finite() {
        offset.clamp(-0.5, 0.5)
    } else {
        0.0
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
    /// Draft cells per row (a proposed pattern), drawn as dashed outlines until applied.
    ghosts: Arc<BTreeMap<RowId, Arc<[StepCell]>>>,
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
            && (Arc::ptr_eq(&self.ghosts, &other.ghosts) || self.ghosts == other.ghosts)
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
    FlagCycled {
        row: RowId,
        column: usize,
        flag: StepFlag,
    },
    OffsetChanged {
        row: RowId,
        column: usize,
        offset: f32,
    },
    FlamChanged {
        row: RowId,
        column: usize,
        flam: bool,
    },
    /// The row's ghost cells became its cells, and its ghosts were cleared.
    GhostsApplied {
        row: RowId,
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
    /// Step a lit cell's emphasis: normal, accent, ghost note, normal.
    CycleFlag {
        row: RowId,
        column: usize,
    },
    /// Set a lit cell's micro-timing in steps (clamped to -0.5..=0.5).
    SetOffset {
        row: RowId,
        column: usize,
        offset: f32,
    },
    /// Set or clear a lit cell's flam.
    SetFlam {
        row: RowId,
        column: usize,
        flam: bool,
    },
    /// Replace the row's cells with its ghost cells and clear them (an "Apply").
    ApplyGhosts {
        row: RowId,
    },
}

impl StepDoc {
    pub fn new(columns: usize, group: usize, rows: Vec<StepRow>) -> Self {
        let mut doc = Self {
            rows: Arc::new(Vec::new()),
            ghosts: Arc::default(),
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

    /// A row's ghost cells, if it has any.
    pub fn ghosts(&self, row: RowId) -> Option<&[StepCell]> {
        self.ghosts.get(&row).map(|g| &**g)
    }

    /// Set (or with `None` clear) a row's ghost cells. Returns whether anything changed.
    pub fn set_ghosts(&mut self, row: RowId, cells: Option<Vec<StepCell>>) -> bool {
        let next: Option<Arc<[StepCell]>> = cells.map(Into::into);
        if self.ghosts.get(&row) == next.as_ref() {
            return false;
        }
        let ghosts = Arc::make_mut(&mut self.ghosts);
        match next {
            Some(cells) => ghosts.insert(row, cells),
            None => ghosts.remove(&row),
        };
        true
    }

    /// Change the column count and grouping (a pattern length change). Row lengths are
    /// clamped to the new count; cells past it stay stored. Returns whether anything changed.
    pub fn set_columns(&mut self, columns: usize, group: usize) -> bool {
        let columns = columns.max(1);
        if (self.columns, self.group) == (columns, group) {
            return false;
        }
        self.columns = columns;
        self.group = group;
        if self.rows.iter().any(|r| r.length > columns) {
            for row in Arc::make_mut(&mut self.rows).iter_mut() {
                if row.length > columns {
                    Arc::make_mut(row).length = columns;
                }
            }
        }
        true
    }

    /// Edit a lit, playing cell: `None` when off, a repeat, or unchanged.
    fn edit_lit(
        &mut self,
        row: RowId,
        column: usize,
        edit: impl FnOnce(&mut StepCell),
    ) -> Option<StepCell> {
        let r = self.row(row)?;
        if column >= r.length || !r.cell(column).on {
            return None;
        }
        let before = r.cell(column);
        let mut cell = before;
        edit(&mut cell);
        if cell == before {
            return None;
        }
        self.row_mut(row)?.set(column, cell);
        Some(cell)
    }

    /// Apply a user action; `None` for a no-op.
    pub fn apply(&mut self, action: StepAction) -> Option<StepChange> {
        let columns = self.columns;
        match action {
            StepAction::CycleFlag { row, column } => {
                let cell = self.edit_lit(row, column, |cell| {
                    let next = match cell.emphasis() {
                        StepFlag::Normal => StepFlag::Accent,
                        StepFlag::Accent => StepFlag::Ghost,
                        StepFlag::Ghost => StepFlag::Normal,
                    };
                    cell.set_emphasis(next);
                })?;
                Some(StepChange::FlagCycled {
                    row,
                    column,
                    flag: cell.emphasis(),
                })
            }
            StepAction::SetOffset {
                row,
                column,
                offset,
            } => {
                let offset = clamp_offset(offset);
                self.edit_lit(row, column, |cell| cell.offset = offset)?;
                Some(StepChange::OffsetChanged {
                    row,
                    column,
                    offset,
                })
            }
            StepAction::SetFlam { row, column, flam } => {
                self.edit_lit(row, column, |cell| cell.flam = flam)?;
                Some(StepChange::FlamChanged { row, column, flam })
            }
            StepAction::ApplyGhosts { row } => {
                let cells = self.ghosts.get(&row)?.clone();
                let target = self.row_mut(row)?;
                for (c, cell) in cells.iter().enumerate() {
                    target.set(c, *cell);
                }
                let length = target.length;
                for c in cells.len()..length {
                    target.set(c, StepCell::default());
                }
                Arc::make_mut(&mut self.ghosts).remove(&row);
                Some(StepChange::GhostsApplied { row })
            }
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
    /// `{row, column}`
    pub const CYCLE_FLAG: &str = "cycle_flag";
    /// `{row, column, offset}`
    pub const SET_OFFSET: &str = "set_offset";
    /// `{row, column, flam}`
    pub const SET_FLAM: &str = "set_flam";
    /// `{row}`
    pub const APPLY_GHOSTS: &str = "apply_ghosts";
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
            Self::CycleFlag { row, column } => (
                CYCLE_FLAG,
                record(&[("row", r(row)), ("column", n(column))]),
            ),
            Self::SetOffset {
                row,
                column,
                offset,
            } => (
                SET_OFFSET,
                record(&[
                    ("row", r(row)),
                    ("column", n(column)),
                    ("offset", Value::Num(f64::from(offset))),
                ]),
            ),
            Self::SetFlam { row, column, flam } => (
                SET_FLAM,
                record(&[
                    ("row", r(row)),
                    ("column", n(column)),
                    ("flam", Value::Bool(flam)),
                ]),
            ),
            Self::ApplyGhosts { row } => (APPLY_GHOSTS, record(&[("row", r(row))])),
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
            CYCLE_FLAG => Self::CycleFlag {
                row: row()?,
                column: at("column")?,
            },
            SET_OFFSET => Self::SetOffset {
                row: row()?,
                column: at("column")?,
                offset: num(p, "offset")? as f32,
            },
            SET_FLAM => Self::SetFlam {
                row: row()?,
                column: at("column")?,
                flam: p
                    .field(&[SharedString::new_static("flam")])
                    .as_bool()
                    .ok_or(ActionError::Invalid("flam"))?,
            },
            APPLY_GHOSTS => Self::ApplyGhosts { row: row()? },
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
            StepChange::FlagCycled { row, column, flag } => record(&[
                kind("flagCycled"),
                ("row", r(row)),
                ("column", n(column)),
                (
                    "flag",
                    Value::Str(SharedString::new_static(match flag {
                        StepFlag::Normal => "normal",
                        StepFlag::Accent => "accent",
                        StepFlag::Ghost => "ghost",
                    })),
                ),
            ]),
            StepChange::OffsetChanged {
                row,
                column,
                offset,
            } => record(&[
                kind("offsetChanged"),
                ("row", r(row)),
                ("column", n(column)),
                ("offset", Value::Num(f64::from(offset))),
            ]),
            StepChange::FlamChanged { row, column, flam } => record(&[
                kind("flamChanged"),
                ("row", r(row)),
                ("column", n(column)),
                ("flam", Value::Bool(flam)),
            ]),
            StepChange::GhostsApplied { row } => record(&[kind("ghostsApplied"), ("row", r(row))]),
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

/// Host write: change the column count and grouping. Never echoes; not undoable.
#[derive(Debug, Clone, Copy)]
pub struct SetColumns {
    pub columns: usize,
    pub group: usize,
}

impl Event<StepDoc> for SetColumns {
    fn handle(self, fx: &mut Fx<'_, StepDoc>) {
        let mut next = fx.state().clone();
        if next.set_columns(self.columns, self.group) {
            fx.set(next);
        }
    }
    fn undoable(&self) -> bool {
        false
    }
}

/// Host write: set (`Some`) or clear (`None`) a row's ghost cells. Never echoes.
#[derive(Debug, Clone)]
pub struct SetGhosts {
    pub row: RowId,
    pub cells: Option<Vec<StepCell>>,
}

impl Event<StepDoc> for SetGhosts {
    fn handle(self, fx: &mut Fx<'_, StepDoc>) {
        let mut next = fx.state().clone();
        if next.set_ghosts(self.row, self.cells) {
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

    #[test]
    fn flags_cycle_offsets_clamp_and_flams_toggle_on_lit_cells() {
        let mut d = doc();
        let kick = RowId(7);
        let cycle = StepAction::CycleFlag {
            row: kick,
            column: 0,
        };
        let flags: Vec<_> = (0..3)
            .filter_map(|_| match d.apply(cycle) {
                Some(StepChange::FlagCycled { flag, .. }) => Some(flag),
                _ => None,
            })
            .collect();
        assert_eq!(flags, [StepFlag::Accent, StepFlag::Ghost, StepFlag::Normal]);
        assert_eq!(
            d.apply(StepAction::CycleFlag {
                row: kick,
                column: 1
            }),
            None,
            "an off cell has no emphasis"
        );
        assert_eq!(
            d.apply(StepAction::SetOffset {
                row: kick,
                column: 0,
                offset: -2.0
            }),
            Some(StepChange::OffsetChanged {
                row: kick,
                column: 0,
                offset: -0.5
            })
        );
        assert_eq!(
            d.apply(StepAction::SetFlam {
                row: kick,
                column: 0,
                flam: true
            }),
            Some(StepChange::FlamChanged {
                row: kick,
                column: 0,
                flam: true
            })
        );
        let cell = d.row(kick).map(|r| r.cell(0)).unwrap_or_default();
        assert!(cell.flam && cell.offset == -0.5 && cell.emphasis() == StepFlag::Normal);
    }

    #[test]
    fn ghosts_and_columns_round_trip() {
        let mut d = doc();
        let hat = RowId(9);
        let draft = vec![
            StepCell::on(),
            StepCell::default(),
            StepCell::on().ghost_note(),
        ];
        assert!(d.set_ghosts(hat, Some(draft.clone())));
        assert!(!d.set_ghosts(hat, Some(draft.clone())), "unchanged");
        assert_eq!(d.ghosts(hat), Some(draft.as_slice()));
        let before = d.clone();
        assert!(d.set_ghosts(hat, None));
        assert_eq!(d.ghosts(hat), None);
        assert_ne!(d, before);
        d.set_ghosts(hat, Some(draft.clone()));
        assert_eq!(
            d.apply(StepAction::ApplyGhosts { row: hat }),
            Some(StepChange::GhostsApplied { row: hat })
        );
        let row = d
            .row(hat)
            .cloned()
            .unwrap_or_else(|| StepRow::new(0, "", 1));
        assert!(row.cell(0).on && !row.cell(1).on && row.cell(2).soft);
        assert_eq!(d.ghosts(hat), None);
        assert_eq!(d.apply(StepAction::ApplyGhosts { row: hat }), None);
        // Columns: shrink clamps lengths, growing keeps them.
        assert!(d.set_columns(8, 2));
        assert_eq!((d.columns, d.group), (8, 2));
        assert!(d.rows().iter().all(|r| r.length <= 8));
        assert!(!d.set_columns(8, 2));
        assert!(d.set_columns(32, 4));
        assert_eq!(d.row(hat).map(|r| r.length), Some(8));
        assert!(
            d.row(RowId(7)).is_some_and(|r| r.cell(0).on),
            "cells survive"
        );
    }

    #[test]
    fn new_actions_round_trip() {
        for a in [
            StepAction::CycleFlag {
                row: RowId(3),
                column: 5,
            },
            StepAction::SetOffset {
                row: RowId(3),
                column: 5,
                offset: -0.25,
            },
            StepAction::SetFlam {
                row: RowId(3),
                column: 5,
                flam: true,
            },
            StepAction::ApplyGhosts { row: RowId(3) },
        ] {
            assert_eq!(StepAction::decode(&a.to_action()), Ok(a));
        }
    }
}
