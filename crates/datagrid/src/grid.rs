//! The GPUI control: [`GridHandle`] (state + API), [`DataGrid`] (the element) and the
//! retained views behind it.
//!
//! Render isolation (API conventions §6): the grid view and a pool of row views read shared state
//! from an `Rc<Core>`; every mutation notifies exactly the views whose output changed with
//! `cx.notify(entity_id)`, never through an `update` on an ancestor. Rows live in a recycled
//! pool keyed by `view_row % pool`, so scrolling by one row re-renders one row view, selecting
//! a row re-renders that row, and editing a cell re-renders only its row. The host that
//! embeds [`DataGrid`] is never invalidated by the grid.

use std::{
    cell::RefCell,
    fmt::Write as _,
    hash::{DefaultHasher, Hash, Hasher},
    num::NonZeroUsize,
    ops::Range,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
};

use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClipboardItem, Context, DispatchPhase,
    Entity, EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement,
    KeyBinding, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Render, RenderOnce, Role, ScrollEvent, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, TestSupportExt, Window, canvas,
    component::input::{
        Enter as InputEnter, Escape as InputEscape, IndentInline as InputTab, Input, InputState,
        MoveDown as InputDown, MoveLeft as InputLeft, MoveRight as InputRight, MoveUp as InputUp,
        OutdentInline as InputShiftTab,
    },
    div,
    prelude::FluentBuilder,
    px,
};
use guiraffe_core::{
    ColorName, Density, GuiraffeTheme, Rgb, Size, StateKind, StyleContext, Variant,
};

use crate::{
    columns::{Align, ColumnDef, ColumnLayout, Pin, reorder},
    model::{
        clipboard::{parse_tsv, paste_target, write_tsv},
        selection::{Advance, CellPos, Nav, Region, Selection, SelectionMode},
        undo::{Change, HistoryLimits, Transaction, UndoLog, estimate_weight},
    },
    order::{
        Filter, FilterSpec, Inverse, RowOrder, SortDir, SortKey, SortSpec, compute_order,
        toggle_sort,
    },
    source::{CellRef, EditError, GridSource, RowKey},
    window as vw,
};

/// Key context of a focused grid.
pub const CONTEXT: &str = "GrDataGrid";

gpui_kit::actions!(
    guiraffe_datagrid,
    [
        MoveUp,
        MoveDown,
        MoveLeft,
        MoveRight,
        MovePageUp,
        MovePageDown,
        MoveHome,
        MoveEnd,
        MoveFirst,
        MoveLast,
        ExtendUp,
        ExtendDown,
        ExtendLeft,
        ExtendRight,
        ExtendPageUp,
        ExtendPageDown,
        ExtendHome,
        ExtendEnd,
        ExtendFirst,
        ExtendLast,
        SelectAll,
        CopySelection,
        EditCell,
        Activate,
        Paste,
        Cut,
        ClearCells,
        Undo,
        Redo,
        TabNext,
        TabPrev,
        EnterPrev
    ]
);

/// Register key bindings (idempotent). Bindings are scoped to [`CONTEXT`]; apps may override.
pub fn init(cx: &mut App) {
    crate::data_ui::register(cx);
    if !cx.has_global::<gpui_kit::component::Theme>() {
        gpui_kit::component::init(cx);
    }
    if !cx.has_global::<GuiraffeTheme>() {
        guiraffe_core::apply(GuiraffeTheme::default(), cx);
    }
    struct Bound;
    impl gpui_kit::Global for Bound {}
    if cx.has_global::<Bound>() {
        return;
    }
    cx.set_global(Bound);
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("pageup", MovePageUp, c),
        KeyBinding::new("pagedown", MovePageDown, c),
        KeyBinding::new("home", MoveHome, c),
        KeyBinding::new("end", MoveEnd, c),
        KeyBinding::new("ctrl-home", MoveFirst, c),
        KeyBinding::new("ctrl-end", MoveLast, c),
        KeyBinding::new("shift-up", ExtendUp, c),
        KeyBinding::new("shift-down", ExtendDown, c),
        KeyBinding::new("shift-left", ExtendLeft, c),
        KeyBinding::new("shift-right", ExtendRight, c),
        KeyBinding::new("shift-pageup", ExtendPageUp, c),
        KeyBinding::new("shift-pagedown", ExtendPageDown, c),
        KeyBinding::new("shift-home", ExtendHome, c),
        KeyBinding::new("shift-end", ExtendEnd, c),
        KeyBinding::new("ctrl-shift-home", ExtendFirst, c),
        KeyBinding::new("ctrl-shift-end", ExtendLast, c),
        KeyBinding::new("ctrl-a", SelectAll, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("ctrl-c", CopySelection, c),
        KeyBinding::new("cmd-c", CopySelection, c),
        KeyBinding::new("f2", EditCell, c),
        KeyBinding::new("enter", Activate, c),
        KeyBinding::new("shift-enter", EnterPrev, c),
        KeyBinding::new("tab", TabNext, c),
        KeyBinding::new("shift-tab", TabPrev, c),
        KeyBinding::new("ctrl-v", Paste, c),
        KeyBinding::new("cmd-v", Paste, c),
        KeyBinding::new("ctrl-x", Cut, c),
        KeyBinding::new("cmd-x", Cut, c),
        KeyBinding::new("delete", ClearCells, c),
        KeyBinding::new("backspace", ClearCells, c),
        KeyBinding::new("ctrl-z", Undo, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("ctrl-shift-z", Redo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("ctrl-y", Redo, c),
    ]);
}

/// Grid behavior and look. Visual sizes default to the inherited [`StyleContext`].
#[derive(Debug, Clone, PartialEq)]
pub struct GridOptions {
    pub selection: SelectionMode,
    pub zebra: bool,
    /// Row height / font scale; `None` reads `StyleContext::control_size`.
    pub size: Option<Size>,
    /// `None` reads `StyleContext::density`.
    pub density: Option<Density>,
    /// Keyboard focus: `Some(true)` opts in, `None` follows the enclosing focus scope
    /// (API conventions §12.3). A pointer click never takes focus unless this resolves to true.
    pub focusable: Option<bool>,
    /// Extra rows built above and below the viewport.
    pub overscan: usize,
    /// Sort/filter synchronously below this many rows; larger data sorts in the background.
    pub sync_below: usize,
    /// Accessible name.
    pub label: SharedString,
}

impl Default for GridOptions {
    fn default() -> Self {
        Self {
            selection: SelectionMode::SingleRow,
            zebra: true,
            size: None,
            density: None,
            focusable: None,
            // Rows outside the viewport are clipped, and the rows that scroll
            // in are built in the frame they are needed: building rows the
            // viewport does not show is about a fifth of a frame for nothing.
            overscan: 0,
            sync_below: 50_000,
            label: "Data grid".into(),
        }
    }
}

/// Events. Payloads are small (indices, keys, shared strings).
#[derive(Debug, Clone, PartialEq)]
pub enum GridEvent {
    SelectionChanged {
        cursor: Option<CellPos>,
        selected_rows: usize,
    },
    SortChanged(Arc<[SortKey]>),
    /// A committed edit; `row` is the source row, `column` the definition index.
    CellEdited {
        row: usize,
        key: RowKey,
        column: usize,
        value: SharedString,
    },
    /// An open editor was closed without committing (Escape, another edit started, its
    /// column hidden, its row filtered out or the source replaced). Its text is dropped.
    EditCancelled {
        row: usize,
        column: usize,
    },
    /// Ctrl+C finished. `truncated` when the selection exceeded
    /// [`crate::tsv::MAX_COPY_CELLS`] and only the first rows were copied.
    Copied {
        cells: usize,
        truncated: bool,
    },
    /// Validation or the source vetoed an edit; the editor stays open.
    EditRejected {
        row: usize,
        column: usize,
        reason: SharedString,
    },
    /// Double-click or Enter on a non-editable cell. `row` is the source row.
    RowActivated {
        row: usize,
        key: RowKey,
    },
    ColumnResized {
        column: usize,
        width: f32,
    },
    ColumnReordered {
        column: usize,
        order: Arc<[usize]>,
    },
    /// A bulk operation (paste, cut, clear, undo, redo) wrote `cells` cells as one undo
    /// step. `truncated` when the target exceeded [`MAX_EDIT_CELLS`] and only the first rows
    /// were written.
    CellsChanged {
        op: EditOp,
        cells: usize,
        truncated: bool,
    },
    /// Some cells of a bulk operation were vetoed (validator or source); the rest applied.
    /// One event per operation: the total count and the first [`REJECTION_SAMPLES`] reasons.
    CellsRejected {
        op: EditOp,
        rejected: usize,
        applied: usize,
        first: Arc<[CellRejection]>,
    },
}

/// What wrote cells (one undo transaction each).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditOp {
    /// A committed editor.
    Edit,
    Paste,
    Cut,
    /// Delete / Backspace.
    Clear,
    Undo,
    Redo,
}

/// One vetoed cell, or the whole operation. `row` is the source row and `column` the
/// definition index; both are `None` when the operation was refused as a whole (source busy,
/// selection too large to cut).
#[derive(Debug, Clone, PartialEq)]
pub struct CellRejection {
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub reason: SharedString,
}

impl CellRejection {
    /// An operation-level rejection (no cell position).
    pub fn whole(reason: impl Into<SharedString>) -> Self {
        Self {
            row: None,
            column: None,
            reason: reason.into(),
        }
    }
}

/// Most cells one paste, cut or clear writes; larger targets are truncated (paste, clear)
/// or refused (cut, so a cut never copies more than it clears).
pub const MAX_EDIT_CELLS: usize = 100_000;

/// How many rejection reasons [`GridEvent::CellsRejected`] carries.
pub const REJECTION_SAMPLES: usize = 5;

/// Undo key: (source row, its `RowKey` when recorded, column definition). The key guards
/// against restoring into a different record after the source changed behind the grid.
type CellKey = (usize, RowKey, usize);

/// One cell a bulk operation writes.
struct CellWrite {
    source_row: usize,
    def: usize,
    value: String,
    /// Undo/redo only: the row must still have this key, or the write is rejected.
    expect: Option<RowKey>,
}

/// Undo weight of a grid transaction: its change records plus the text they own, counted
/// once when the transaction is recorded.
fn text_weight(t: &Transaction<CellKey, String, EditOp>) -> u64 {
    let text: usize = t.changes.iter().map(|c| c.old.len() + c.new.len()).sum();
    estimate_weight(t) + text as u64
}

/// Text that round-trips a cell through `GridSource::set_cell` for undo: the default display,
/// but full precision for floats. `false` for custom cells, which can't be restored.
fn raw_text(v: CellRef<'_>, out: &mut String) -> bool {
    match v {
        CellRef::Number(n) => {
            use std::fmt::Write as _;
            let _ = write!(out, "{n}");
            true
        }
        CellRef::Custom(_) => false,
        v => {
            v.write_display(out);
            true
        }
    }
}

/// Pointer-drag autoscroll step: how far past (or into) an edge zone `pos` is, as a signed
/// scroll delta. `lo..hi` is the scrollable band, `zone` the edge width that starts scrolling.
fn edge_delta(pos: f32, lo: f32, hi: f32, zone: f32, max: f32) -> f32 {
    if hi - lo <= 2. * zone {
        return 0.;
    }
    if pos < lo + zone {
        -(lo + zone - pos).min(max)
    } else if pos > hi - zone {
        (pos - (hi - zone)).min(max)
    } else {
        0.
    }
}

/// Emits [`GridEvent`]s; subscribe with `cx.subscribe(handle.events(), ..)`.
pub struct GridEvents;
impl EventEmitter<GridEvent> for GridEvents {}

/// What an edit validator sees.
#[derive(Debug, Clone, Copy)]
pub struct EditRequest<'a> {
    pub row: usize,
    pub key: RowKey,
    pub column: usize,
    pub value: &'a str,
}

pub type Validator = Rc<dyn Fn(&EditRequest<'_>) -> Result<(), SharedString>>;

/// Render and work counters (for tests, devtools and benchmarks).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GridStats {
    pub grid_renders: u64,
    pub header_renders: u64,
    pub row_renders: u64,
    pub cells_built: u64,
    pub visible_rows: usize,
    pub visible_cells: usize,
    pub pool: usize,
    pub order_jobs: u64,
    pub stale_jobs: u64,
    pub palette_resolutions: u64,
}

/// An open editor. Tracked by source row and column *definition*, so it survives sorting,
/// filtering, reordering and pinning; `row` is the current view row, kept in sync.
struct Edit {
    row: usize,
    def: usize,
    source_row: usize,
    input: Entity<InputState>,
    error: Option<SharedString>,
    window: AnyWindowHandle,
}

/// A commit made while a background job shares the source; applied when the job lands.
struct PendingCommit {
    source_row: usize,
    def: usize,
    key: RowKey,
    value: SharedString,
}

/// Selections up to this many cells are copied on the UI thread; larger ones off it.
pub const INLINE_COPY_CELLS: usize = 100_000;

/// Everything a copy needs, owned (`Arc` clones), so it can run on any thread.
struct CopyJob {
    source: Arc<dyn GridSource>,
    order: RowOrder,
    defs: Vec<ColumnDef>,
    rows: Vec<Range<usize>>,
}

impl CopyJob {
    fn cells(&self) -> usize {
        self.rows
            .iter()
            .map(|r| r.len())
            .sum::<usize>()
            .saturating_mul(self.defs.len())
    }

    /// `(tsv, complete)`.
    fn run(&self) -> (String, bool) {
        let n = self.source.row_count();
        let mut out = String::new();
        let cols: Vec<usize> = (0..self.defs.len()).collect();
        let complete = write_tsv(
            &mut out,
            self.rows.iter().cloned().flatten(),
            &cols,
            |r, c, buf| {
                if let (Some(src), Some(def)) = (self.order.source_row(r, n), self.defs.get(c)) {
                    def.format(self.source.cell(src, def.source), buf);
                }
            },
        );
        (out, complete)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    Resize {
        def: usize,
        start_x: f32,
        start_w: f32,
    },
    Header {
        def: usize,
        start_x: f32,
        shift: bool,
        moved: bool,
        x: f32,
    },
    Select,
    VScroll {
        start_y: f32,
        start_offset: f64,
    },
    HScroll {
        start_x: f32,
        start_offset: f64,
    },
}

struct Metrics {
    row_h: f32,
    font: f32,
    pad: f32,
}

fn metrics(size: Size, density: Density) -> Metrics {
    let base = size.control_height();
    let row_h = match density {
        Density::Compact => base - 6.,
        Density::Comfortable => base,
        Density::Spacious => base + 8.,
    };
    Metrics {
        row_h,
        font: size.font_size(),
        pad: density.space(Size::Xs),
    }
}

const SCROLLBAR: f32 = 8.;
const FALLBACK_H: f32 = 480.;
const FALLBACK_W: f32 = 800.;

struct Model {
    source: Arc<dyn GridSource>,
    defs: Vec<ColumnDef>,
    order: Vec<usize>,
    layout: ColumnLayout,
    opts: GridOptions,
    rows: RowOrder,
    count: usize,
    sort: Vec<SortKey>,
    filters: Vec<(usize, Filter)>,
    job: u64,
    busy: bool,
    cancel: Arc<AtomicBool>,
    scroll_y: f64,
    scroll_x: f64,
    bounds: Bounds<Pixels>,
    context: StyleContext,
    row_h: f32,
    font: f32,
    pad: f32,
    visible: Range<usize>,
    pool: usize,
    sel: Selection,
    edit: Option<Edit>,
    drag: Option<Drag>,
    validator: Option<Validator>,
    pending: Option<PendingCommit>,
    copying: usize,
    undo: UndoLog<CellKey, String, EditOp>,
    /// Source row count when undo history was last known valid.
    src_rows: usize,
    /// Whether any displayed column is editable (cached with the layout).
    any_editable: bool,
    /// Last pointer position (grid-relative) of a range drag, while it autoscrolls.
    autoscroll: Option<(f32, f32)>,
    autoscroll_task: bool,
    stats: GridStats,
    text: String,
    observe_cells: bool,
}

impl Model {
    fn grid_w(&self) -> f32 {
        let w = f32::from(self.bounds.size.width);
        if w > 0. { w } else { FALLBACK_W }
    }
    fn body_h(&self) -> f64 {
        let h = f32::from(self.bounds.size.height);
        let h = if h > 0. { h } else { FALLBACK_H };
        f64::from((h - self.row_h).max(0.))
    }
    fn cols(&self) -> usize {
        self.layout.display_count()
    }
    fn page(&self) -> usize {
        ((self.body_h() / f64::from(self.row_h.max(1.))).floor() as usize).max(1)
    }
    fn source_row(&self, view_row: usize) -> Option<usize> {
        self.rows.source_row(view_row, self.source.row_count())
    }
    fn def_at(&self, display: usize) -> Option<&ColumnDef> {
        self.layout
            .display(display)
            .and_then(|(_, p)| self.defs.get(p.def))
    }
    fn resolve_metrics(&mut self) -> bool {
        let size = self.opts.size.unwrap_or(self.context.control_size);
        let density = self.opts.density.unwrap_or(self.context.density);
        let m = metrics(size, density);
        let changed = m.row_h != self.row_h || m.font != self.font || m.pad != self.pad;
        self.row_h = m.row_h;
        self.font = m.font;
        self.pad = m.pad;
        changed
    }
    fn clamp_scroll(&mut self) {
        self.scroll_y = vw::clamp_offset(
            self.scroll_y,
            self.count,
            f64::from(self.row_h),
            self.body_h(),
        );
        self.scroll_x = self
            .scroll_x
            .clamp(0., self.layout.max_scroll_x(self.grid_w()));
    }
    /// Per-row selection signature: what a row view draws differently.
    fn row_sig(sel: &Selection, row: usize) -> (bool, Option<usize>, Option<Range<usize>>) {
        let rect = sel.rect().filter(|(r, _)| r.contains(&row)).map(|(_, c)| c);
        (
            sel.is_row_selected(row),
            sel.cursor.filter(|c| c.row == row).map(|c| c.col),
            rect,
        )
    }
    /// Where a header dragged to grid-relative `x` lands: `(before, indicator x)`, where
    /// `before` is the definition it is inserted before (`None` = last). Drops are only
    /// accepted within the dragged column's own band (pinning is changed explicitly), so a
    /// cross-band position yields `None` and shows no indicator.
    fn drop_slot(&self, def: usize, x: f32) -> Option<(Option<usize>, f32)> {
        let w = self.grid_w();
        let band_of = |d: usize| {
            self.layout
                .display_of(d)
                .and_then(|i| self.layout.display(i))
                .map(|(b, _)| b)
        };
        let own = band_of(def)?;
        let i = self.layout.hit(x.clamp(0., w - 1.), self.scroll_x, w)?;
        let (band, target) = self.layout.display(i)?;
        if target.def == def || band != own {
            return None;
        }
        let sx = self.layout.screen_x(i, self.scroll_x, w)?;
        if x < sx + target.width / 2. {
            Some((Some(target.def), sx))
        } else {
            // After the target: before whatever follows it in the user order.
            let pos = self.order.iter().position(|&d| d == target.def)?;
            let before = self
                .order
                .get(pos + 1)
                .copied()
                .filter(|&d| d != def)
                .or_else(|| self.order.get(pos + 2).copied());
            Some((before, sx + target.width))
        }
    }

    /// Write `value` to (source row, column definition) through `GridSource::set_cell`,
    /// returning the undo diff (`None` when the old value can't be restored as text).
    fn write_cell(
        &mut self,
        source_row: usize,
        def: usize,
        value: &str,
    ) -> Result<Option<Change<CellKey, String>>, SharedString> {
        let Some(src_col) = self.defs.get(def).map(|d| d.source) else {
            return Err(EditError::ReadOnly.to_string().into());
        };
        let mut old = String::new();
        let restorable = raw_text(self.source.cell(source_row, src_col), &mut old);
        let Some(src) = Arc::get_mut(&mut self.source) else {
            return Err(EditError::Busy.to_string().into());
        };
        src.set_cell(source_row, src_col, value)
            .map_err(|e| SharedString::from(e.to_string()))?;
        Ok(restorable.then(|| Change {
            key: (source_row, self.source.row_key(source_row), def),
            old,
            new: value.to_string(),
        }))
    }

    fn validate(&self, source_row: usize, def: usize, value: &str) -> Result<(), SharedString> {
        match &self.validator {
            Some(v) => v(&EditRequest {
                row: source_row,
                key: self.source.row_key(source_row),
                column: def,
                value,
            }),
            None => Ok(()),
        }
    }

    /// Recompute [`Model::any_editable`] (after the layout changed).
    fn refresh_editable(&mut self) {
        self.any_editable = (0..self.cols()).any(|c| self.editable_display(c).is_some());
    }

    /// Whether display column `col` is an editable column.
    fn editable_display(&self, col: usize) -> Option<usize> {
        let (_, placed) = self.layout.display(col)?;
        self.defs
            .get(placed.def)
            .filter(|d| d.editable)
            .map(|_| placed.def)
    }

    /// The editable cells of the selection (or cursor) as writes of `""`, capped at
    /// [`MAX_EDIT_CELLS`]; `(writes, cells covered, truncated)`.
    fn selected_editable(&self) -> (Vec<CellWrite>, usize, bool) {
        let Some(region) = self.sel.region() else {
            return (Vec::new(), 0, false);
        };
        let (rows, cols): (Vec<Range<usize>>, Range<usize>) = match region {
            Region::Rows(r) => (r, 0..self.cols()),
            Region::Rect(r, c) => (vec![r], c),
        };
        let defs: Vec<usize> = cols.filter_map(|c| self.editable_display(c)).collect();
        let covered = rows
            .iter()
            .map(|r| r.len())
            .sum::<usize>()
            .saturating_mul(defs.len());
        let mut out = Vec::new();
        if defs.is_empty() {
            return (out, 0, false);
        }
        'rows: for r in rows.into_iter().flatten() {
            let Some(src) = self.source_row(r) else {
                continue;
            };
            for &def in &defs {
                if out.len() >= MAX_EDIT_CELLS {
                    break 'rows;
                }
                out.push(CellWrite {
                    source_row: src,
                    def,
                    value: String::new(),
                    expect: None,
                });
            }
        }
        let truncated = covered > out.len();
        (out, covered, truncated)
    }

    fn view_row_of_slot(&self, slot: usize) -> Option<usize> {
        let pool = self.pool;
        if pool == 0 || slot >= pool {
            return None;
        }
        let first = self.visible.start;
        let r = first + (slot + pool - first % pool) % pool;
        (r < self.visible.end).then_some(r)
    }
}

pub(crate) struct Core {
    m: RefCell<Model>,
    focus: FocusHandle,
    events: Entity<GridEvents>,
    view: RefCell<Option<gpui_kit::EntityId>>,
    header_id: RefCell<Option<gpui_kit::EntityId>>,
    rows: RefCell<Vec<Entity<RowView>>>,
    shapes: Rc<RefCell<CellShapes>>,
    cell_text: RefCell<lru::LruCache<u64, SharedString>>,
    palette: RefCell<Option<(GuiraffeTheme, Palette)>>,
}

impl Drop for Core {
    fn drop(&mut self) {
        self.m.get_mut().cancel.store(true, AtomicOrdering::Relaxed);
    }
}

/// The grid control handle: cheap to clone; owns the shared state and the retained views.
/// Keep it outside your host's render state and render it with [`DataGrid::new`].
#[derive(Clone)]
pub struct GridHandle {
    core: Rc<Core>,
    view: Entity<GridView>,
}

impl GridHandle {
    /// A grid over `source` with `columns` (definition order is the initial display order).
    pub fn new(source: impl GridSource, columns: Vec<ColumnDef>, cx: &mut App) -> Self {
        Self::from_arc(Arc::new(source), columns, cx)
    }

    /// A grid over a shared source. Edits need exclusive ownership (see
    /// [`GridHandle::update_source`]).
    pub fn from_arc(source: Arc<dyn GridSource>, columns: Vec<ColumnDef>, cx: &mut App) -> Self {
        init(cx);
        let order: Vec<usize> = (0..columns.len()).collect();
        let layout = ColumnLayout::compute(&columns, &order);
        let count = source.row_count();
        let opts = GridOptions::default();
        let mut model = Model {
            source,
            defs: columns,
            order,
            layout,
            sel: Selection::new(opts.selection),
            opts,
            rows: RowOrder::Identity,
            count,
            sort: Vec::new(),
            filters: Vec::new(),
            job: 0,
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
            scroll_y: 0.,
            scroll_x: 0.,
            bounds: Bounds::default(),
            context: StyleContext::default(),
            row_h: 0.,
            font: 0.,
            pad: 0.,
            visible: 0..0,
            pool: 0,
            edit: None,
            drag: None,
            validator: None,
            pending: None,
            copying: 0,
            undo: UndoLog::default().with_weigher(text_weight),
            src_rows: count,
            any_editable: false,
            autoscroll: None,
            autoscroll_task: false,
            stats: GridStats::default(),
            text: String::with_capacity(64),
            observe_cells: false,
        };
        model.resolve_metrics();
        model.refresh_editable();
        let core = Rc::new(Core {
            m: RefCell::new(model),
            focus: cx.focus_handle(),
            events: cx.new(|_| GridEvents),
            view: RefCell::new(None),
            header_id: RefCell::new(None),
            rows: RefCell::new(Vec::new()),
            shapes: Rc::new(RefCell::new(CellShapes::new(CELL_CACHE_CAPACITY))),
            cell_text: RefCell::new(lru::LruCache::new(CELL_CACHE_CAPACITY)),
            palette: RefCell::new(None),
        });
        let header = cx.new(|_| HeaderView {
            core: Rc::downgrade(&core),
        });
        *core.header_id.borrow_mut() = Some(header.entity_id());
        let view = cx.new(|_| GridView {
            core: core.clone(),
            header,
        });
        *core.view.borrow_mut() = Some(view.entity_id());
        core.sync(true, cx);
        Self { core, view }
    }

    /// Subscribe to [`GridEvent`]s on this entity.
    pub fn events(&self) -> &Entity<GridEvents> {
        &self.core.events
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.core.focus.clone()
    }

    /// Give the grid keyboard focus.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.core.focus, cx);
    }

    pub fn options(&self) -> GridOptions {
        self.core.m.borrow().opts.clone()
    }

    pub fn set_options(&self, opts: GridOptions, cx: &mut App) {
        {
            let mut m = self.core.m.borrow_mut();
            if m.opts == opts {
                return;
            }
            if m.opts.selection != opts.selection {
                m.sel = Selection::new(opts.selection);
            }
            m.opts = opts;
            m.resolve_metrics();
            m.clamp_scroll();
        }
        self.core.sync(true, cx);
    }

    pub fn stats(&self) -> GridStats {
        self.core.m.borrow().stats
    }

    /// Rows currently shown (after filtering).
    pub fn view_row_count(&self) -> usize {
        self.core.m.borrow().count
    }

    pub fn source_row(&self, view_row: usize) -> Option<usize> {
        self.core.m.borrow().source_row(view_row)
    }

    /// Visible view rows (including overscan).
    pub fn visible_rows(&self) -> Range<usize> {
        self.core.m.borrow().visible.clone()
    }

    /// The grid's laid-out bounds in window coordinates (last frame).
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.core.m.borrow().bounds
    }

    pub fn row_height(&self) -> f32 {
        self.core.m.borrow().row_h
    }

    pub fn scroll_offset(&self) -> (f64, f64) {
        let m = self.core.m.borrow();
        (m.scroll_x, m.scroll_y)
    }

    /// Scroll to an absolute offset (clamped).
    pub fn set_scroll(&self, x: f64, y: f64, cx: &mut App) {
        self.core.set_scroll(x, y, cx);
    }

    /// Scroll so view row `row` is visible.
    pub fn scroll_to_row(&self, row: usize, cx: &mut App) {
        let (x, y) = {
            let m = self.core.m.borrow();
            let y = vw::scroll_into_view(m.scroll_y, row, f64::from(m.row_h), m.body_h(), m.count);
            (m.scroll_x, y)
        };
        self.core.set_scroll(x, y, cx);
    }

    pub fn columns(&self) -> Vec<ColumnDef> {
        self.core.m.borrow().defs.clone()
    }

    /// Definition indices in display order (including hidden ones).
    pub fn column_order(&self) -> Vec<usize> {
        self.core.m.borrow().order.clone()
    }

    pub fn layout(&self) -> ColumnLayout {
        self.core.m.borrow().layout.clone()
    }

    /// Enables per-cell geometry in the test-support inspection tree.
    #[doc(hidden)]
    pub fn observe_cells_for_test(&self, cx: &mut App) {
        self.core.m.borrow_mut().observe_cells = true;
        self.core.sync(true, cx);
    }

    /// Replace all column definitions (resets order, sort and filters).
    pub fn set_columns(&self, defs: Vec<ColumnDef>, cx: &mut App) {
        self.core.drop_edit(cx);
        {
            let mut m = self.core.m.borrow_mut();
            m.order = (0..defs.len()).collect();
            m.defs = defs;
            m.undo.clear();
            m.sort.clear();
            m.filters.clear();
        }
        self.core.relayout(cx);
        self.core.refresh_order(cx);
    }

    pub fn set_column_width(&self, def: usize, width: f32, cx: &mut App) {
        let changed = {
            let mut m = self.core.m.borrow_mut();
            match m.defs.get_mut(def) {
                Some(d) => {
                    let w = d.clamp_width(width);
                    let changed = w != d.width;
                    d.width = w;
                    changed
                }
                None => false,
            }
        };
        if changed {
            self.core.resize_columns(cx);
        }
    }

    pub fn set_column_visible(&self, def: usize, visible: bool, cx: &mut App) {
        self.core.edit_def(def, |d| d.visible = visible, cx);
    }

    pub fn set_column_pin(&self, def: usize, pin: Pin, cx: &mut App) {
        self.core.edit_def(def, |d| d.pin = pin, cx);
    }

    /// Move definition `def` before `before` (last when `None`).
    pub fn move_column(&self, def: usize, before: Option<usize>, cx: &mut App) {
        self.core.move_column(def, before, cx);
    }

    pub fn sort(&self) -> Vec<SortKey> {
        self.core.m.borrow().sort.clone()
    }

    /// Replace the sort keys. Large data sorts on the background executor; the view keeps the
    /// old order until the new permutation is ready.
    pub fn set_sort(&self, keys: Vec<SortKey>, cx: &mut App) {
        self.core.set_sort(keys, cx);
    }

    /// Header-click cycling on `def` (asc → desc → none); `multi` keeps other keys.
    pub fn toggle_sort(&self, def: usize, multi: bool, cx: &mut App) {
        let mut keys = self.sort();
        toggle_sort(&mut keys, def, multi);
        self.core.set_sort(keys, cx);
    }

    /// Set or clear the predicate for column `def`.
    pub fn set_filter(&self, def: usize, filter: Option<Filter>, cx: &mut App) {
        {
            let mut m = self.core.m.borrow_mut();
            m.filters.retain(|(d, _)| *d != def);
            if let Some(f) = filter {
                m.filters.push((def, f));
            }
        }
        self.core.refresh_order(cx);
    }

    /// Whether a background sort/filter is running.
    pub fn is_busy(&self) -> bool {
        self.core.m.borrow().busy
    }

    pub fn selection(&self) -> Selection {
        self.core.m.borrow().sel.clone()
    }

    /// Click-equivalent selection of `pos` (view coordinates).
    pub fn select(&self, pos: CellPos, shift: bool, ctrl: bool, cx: &mut App) {
        self.core
            .change_selection(cx, |s, _| s.click(pos, shift, ctrl));
    }

    pub fn clear_selection(&self, cx: &mut App) {
        self.core.change_selection(cx, |s, _| s.clear());
    }

    /// Install a validator that can veto edits.
    pub fn set_validator(
        &self,
        f: impl Fn(&EditRequest<'_>) -> Result<(), SharedString> + 'static,
    ) {
        self.core.m.borrow_mut().validator = Some(Rc::new(f));
    }

    /// Swap the data source (resets the view order and re-runs sort/filter).
    pub fn set_source(&self, source: Arc<dyn GridSource>, cx: &mut App) {
        self.core.drop_edit(cx);
        {
            let mut m = self.core.m.borrow_mut();
            m.source = source;
            m.src_rows = m.source.row_count();
            m.undo.clear();
        }
        self.core.refresh_order(cx);
    }

    /// Mutate the source in place (needs exclusive ownership: no background job and no
    /// outside `Arc` clone). Returns `None` when shared. Call
    /// [`GridHandle::rows_changed`] or [`GridHandle::data_changed`] afterwards.
    ///
    /// Undo history survives value changes. Undo entries remember each row's `RowKey`, so a
    /// restore never lands in a different record: rows whose key changed are rejected. If
    /// rows were inserted or removed, [`GridHandle::data_changed`] clears the history.
    pub fn update_source<R>(&self, f: impl FnOnce(&mut dyn GridSource) -> R) -> Option<R> {
        let mut m = self.core.m.borrow_mut();
        Arc::get_mut(&mut m.source).map(f)
    }

    /// Source rows in `rows` changed in place: re-render only visible rows showing them.
    pub fn rows_changed(&self, rows: Range<usize>, cx: &mut App) {
        let slots: Vec<usize> = {
            let m = self.core.m.borrow();
            m.visible
                .clone()
                .filter(|&v| m.source_row(v).is_some_and(|s| rows.contains(&s)))
                .map(|v| vw::slot_of(v, m.pool))
                .collect()
        };
        for s in slots {
            self.core.notify_slot(s, cx);
        }
    }

    /// Anything may have changed (row count, values): re-run sort/filter and re-render.
    ///
    /// When the source's row count changed since the grid last saw it, undo history is
    /// cleared (row indices no longer identify the same records). Same-count changes keep
    /// it; undo then skips rows whose `RowKey` differs from the one recorded.
    pub fn data_changed(&self, cx: &mut App) {
        {
            let mut m = self.core.m.borrow_mut();
            let n = m.source.row_count();
            if n != m.src_rows {
                m.undo.clear();
                m.src_rows = n;
            }
        }
        self.core.refresh_order(cx);
    }

    /// Open the editor on view row `row`, display column `col` (if editable).
    pub fn begin_edit(&self, row: usize, col: usize, window: &mut Window, cx: &mut App) -> bool {
        self.core
            .begin_edit(CellPos::new(row, col), None, window, cx)
    }

    pub fn commit_edit(&self, window: &mut Window, cx: &mut App) -> bool {
        self.core.commit_edit(window, cx)
    }

    pub fn cancel_edit(&self, window: &mut Window, cx: &mut App) {
        self.core.cancel_edit(window, cx);
    }

    pub fn is_editing(&self) -> bool {
        self.core.m.borrow().edit.is_some()
    }

    /// Where the open editor is now (view row, display column); follows sorting, filtering
    /// and column moves.
    pub fn editing_cell(&self) -> Option<CellPos> {
        let m = self.core.m.borrow();
        let e = m.edit.as_ref()?;
        Some(CellPos::new(e.row, m.layout.display_of(e.def)?))
    }

    /// The open editor's input (for tests and custom editors).
    pub fn editor(&self) -> Option<Entity<InputState>> {
        self.core.m.borrow().edit.as_ref().map(|e| e.input.clone())
    }

    /// Copy the selection as TSV (also Ctrl+C). Returns the text.
    pub fn copy(&self, cx: &mut App) -> Option<String> {
        self.core.copy(cx)
    }

    /// Paste TSV or a plain value into the selection (Ctrl+V pastes the clipboard). Returns
    /// the number of cells written.
    ///
    /// The block lands at the selection's top-left cell, or is tiled over the selected range
    /// when the range is larger than one cell and the block divides it (see
    /// [`crate::model::clipboard::paste_target`]). It is clipped to the existing rows and
    /// columns; non-editable columns are skipped. Each cell goes through the validator and
    /// [`GridSource::set_cell`]; vetoed cells are reported in one
    /// [`GridEvent::CellsRejected`] and the rest still apply, as one undo step.
    pub fn paste_text(&self, text: &str, cx: &mut App) -> usize {
        self.core.paste_text(text, cx)
    }

    /// Paste the clipboard's text (Ctrl+V).
    pub fn paste(&self, cx: &mut App) -> usize {
        self.core.paste(cx)
    }

    /// Copy the selection, then clear its editable cells (Ctrl+X), as one undo step.
    pub fn cut(&self, cx: &mut App) -> usize {
        self.core.cut(cx)
    }

    /// Clear the selection's editable cells (Delete / Backspace), as one undo step.
    pub fn clear_cells(&self, cx: &mut App) -> usize {
        self.core.clear_cells(EditOp::Clear, cx)
    }

    /// Undo the last edit, paste, cut or clear (Ctrl+Z). Returns whether anything was undone.
    ///
    /// Old values are restored as text through `GridSource::set_cell` (floats at full
    /// precision). Cells that held a `CellRef::Custom` value have no text form, so they are
    /// not recorded and an undo leaves them as written.
    pub fn undo(&self, cx: &mut App) -> bool {
        self.core.undo(false, cx)
    }

    /// Redo (Ctrl+Shift+Z / Ctrl+Y).
    pub fn redo(&self, cx: &mut App) -> bool {
        self.core.undo(true, cx)
    }

    pub fn can_undo(&self) -> bool {
        self.core.m.borrow().undo.can_undo()
    }

    /// Bound the undo history by step count and, optionally, approximate bytes (each
    /// transaction weighs its change records plus the old and new text). The oldest steps
    /// go first; the default keeps 200 steps.
    pub fn set_undo_limits(&self, limits: impl Into<HistoryLimits>) {
        self.core.m.borrow_mut().undo.set_limits(limits);
    }

    pub fn undo_limits(&self) -> HistoryLimits {
        self.core.m.borrow().undo.limits()
    }

    pub fn can_redo(&self) -> bool {
        self.core.m.borrow().undo.can_redo()
    }

    /// Forget undo history (call after changing the source behind the grid's back).
    pub fn clear_history(&self) {
        self.core.m.borrow_mut().undo.clear();
    }

    /// Row view entity currently showing view row `row`, if visible (for render tests).
    pub fn row_view_id(&self, row: usize) -> Option<gpui_kit::EntityId> {
        let m = self.core.m.borrow();
        if !m.visible.contains(&row) {
            return None;
        }
        let slot = vw::slot_of(row, m.pool);
        self.core.rows.borrow().get(slot).map(|e| e.entity_id())
    }

    /// Render count of the row view currently showing view row `row`.
    pub fn row_renders(&self, row: usize, cx: &App) -> Option<u64> {
        let m = self.core.m.borrow();
        if !m.visible.contains(&row) {
            return None;
        }
        let slot = vw::slot_of(row, m.pool);
        self.core
            .rows
            .borrow()
            .get(slot)
            .map(|e| e.read(cx).renders)
    }
}

impl Core {
    fn emit(&self, e: GridEvent, cx: &mut App) {
        self.events.update(cx, |_, cx| cx.emit(e));
    }

    fn notify_grid(&self, cx: &mut App) {
        if let Some(id) = *self.view.borrow() {
            cx.notify(id);
        }
    }

    fn notify_header(&self, cx: &mut App) {
        if let Some(id) = *self.header_id.borrow() {
            cx.notify(id);
        }
    }

    fn notify_slot(&self, slot: usize, cx: &mut App) {
        let id = self.rows.borrow().get(slot).map(|e| e.entity_id());
        if let Some(id) = id {
            cx.notify(id);
        }
    }

    /// Recompute the visible window and pool; notify the grid and every row view whose row
    /// changed (all of them when `all`).
    fn sync(self: &Rc<Self>, all: bool, cx: &mut App) {
        if all {
            self.notify_header(cx);
        }
        let (old, visible, pool, all) = {
            let mut m = self.m.borrow_mut();
            m.clamp_scroll();
            let row_h = f64::from(m.row_h);
            let body = m.body_h();
            let vis = vw::visible_rows(m.scroll_y, row_h, body, m.count, m.opts.overscan);
            let pool = vw::pool_size(row_h, body, m.opts.overscan).max(vis.len());
            let old = std::mem::replace(&mut m.visible, vis.clone());
            let all = all || pool != m.pool;
            m.pool = pool;
            m.stats.pool = pool;
            m.stats.visible_rows = vis.len();
            (old, vis, pool, all)
        };
        {
            let mut rows = self.rows.borrow_mut();
            while rows.len() < pool {
                let slot = rows.len();
                let core = Rc::downgrade(self);
                rows.push(cx.new(|_| RowView {
                    core,
                    slot,
                    renders: 0,
                    labels: Default::default(),
                }));
            }
        }
        self.notify_grid(cx);
        if all {
            for slot in 0..pool {
                self.notify_slot(slot, cx);
            }
        } else {
            for row in vw::entering(old, visible) {
                self.notify_slot(vw::slot_of(row, pool), cx);
            }
        }
    }

    fn set_scroll(self: &Rc<Self>, x: f64, y: f64, cx: &mut App) {
        let (changed_x, changed_y) = {
            let mut m = self.m.borrow_mut();
            let (ox, oy) = (m.scroll_x, m.scroll_y);
            m.scroll_x = x;
            m.scroll_y = y;
            m.clamp_scroll();
            (m.scroll_x != ox, m.scroll_y != oy)
        };
        if changed_x || changed_y {
            // Horizontal scroll moves cells inside every row; vertical only re-binds slots.
            self.sync(changed_x, cx);
        }
    }

    fn resize_columns(&self, cx: &mut App) {
        let (visible, pool) = {
            let mut m = self.m.borrow_mut();
            m.layout = ColumnLayout::compute(&m.defs, &m.order);
            m.clamp_scroll();
            (m.visible.clone(), m.pool)
        };
        self.notify_header(cx);
        self.notify_grid(cx);
        for row in visible {
            self.notify_slot(vw::slot_of(row, pool), cx);
        }
    }

    fn relayout(self: &Rc<Self>, cx: &mut App) {
        let (sel_changed, hidden_edit) = {
            let mut m = self.m.borrow_mut();
            m.layout = ColumnLayout::compute(&m.defs, &m.order);
            m.refresh_editable();
            let cols = m.cols();
            let count = m.count;
            let before = m.sel.clone();
            m.sel.clamp(count, cols);
            m.clamp_scroll();
            let hidden = m
                .edit
                .as_ref()
                .is_some_and(|e| m.layout.display_of(e.def).is_none());
            (m.sel != before, hidden)
        };
        if hidden_edit {
            self.drop_edit(cx);
        }
        self.sync(true, cx);
        if sel_changed {
            self.emit_selection(cx);
        }
    }

    fn emit_selection(&self, cx: &mut App) {
        let (cursor, n) = {
            let m = self.m.borrow();
            (m.sel.cursor, m.sel.rows.len())
        };
        self.emit(
            GridEvent::SelectionChanged {
                cursor,
                selected_rows: n,
            },
            cx,
        );
    }

    fn edit_def(self: &Rc<Self>, def: usize, f: impl FnOnce(&mut ColumnDef), cx: &mut App) {
        if let Some(d) = self.m.borrow_mut().defs.get_mut(def) {
            f(d);
        }
        self.relayout(cx);
    }

    fn move_column(self: &Rc<Self>, def: usize, before: Option<usize>, cx: &mut App) {
        let order = {
            let mut m = self.m.borrow_mut();
            if !reorder(&mut m.order, def, before) {
                return;
            }
            Arc::<[usize]>::from(m.order.as_slice())
        };
        self.relayout(cx);
        self.emit(GridEvent::ColumnReordered { column: def, order }, cx);
    }

    fn set_sort(self: &Rc<Self>, keys: Vec<SortKey>, cx: &mut App) {
        let keys: Vec<SortKey> = {
            let m = self.m.borrow();
            keys.into_iter()
                .filter(|k| m.defs.get(k.column).is_some_and(|d| d.sortable))
                .collect()
        };
        {
            let mut m = self.m.borrow_mut();
            if m.sort == keys {
                return;
            }
            m.sort = keys.clone();
        }
        self.notify_header(cx);
        self.emit(GridEvent::SortChanged(keys.into()), cx);
        self.refresh_order(cx);
    }

    /// Re-run filter + sort. Small data runs inline; large data on the background executor
    /// (results from superseded jobs are dropped).
    fn refresh_order(self: &Rc<Self>, cx: &mut App) {
        let (job, src, filters, sort, cancel, inline) = {
            let mut m = self.m.borrow_mut();
            m.cancel.store(true, AtomicOrdering::Relaxed);
            m.cancel = Arc::new(AtomicBool::new(false));
            m.job += 1;
            m.stats.order_jobs += 1;
            let filters: Vec<FilterSpec> = m
                .filters
                .iter()
                .filter_map(|(d, f)| {
                    m.defs.get(*d).map(|def| FilterSpec {
                        source_col: def.source,
                        pred: f.clone(),
                    })
                })
                .collect();
            let sort: Vec<SortSpec> = m
                .sort
                .iter()
                .filter_map(|k| {
                    m.defs.get(k.column).map(|def| SortSpec {
                        source_col: def.source,
                        dir: k.dir,
                        comparator: def.comparator.clone(),
                    })
                })
                .collect();
            let inline =
                m.source.row_count() < m.opts.sync_below || (filters.is_empty() && sort.is_empty());
            (
                m.job,
                m.source.clone(),
                filters,
                sort,
                m.cancel.clone(),
                inline,
            )
        };
        if inline {
            let r = compute_order(&*src, &filters, &sort, &cancel).map(|order| {
                let inverse = order.inverse(src.row_count());
                (order, inverse)
            });
            drop(src);
            self.apply_order(job, r, cx);
            return;
        }
        self.m.borrow_mut().busy = true;
        self.notify_grid(cx);
        let core = Rc::downgrade(self);
        cx.spawn(async move |cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let order = compute_order(&*src, &filters, &sort, &cancel)?;
                    // Publication needs only lookups even when a selection was made
                    // while this job was running.
                    let inverse = order
                        .build_inverse(src.row_count(), || cancel.load(AtomicOrdering::Relaxed))?;
                    Ok((order, inverse))
                })
                .await;
            if let Some(core) = core.upgrade() {
                cx.update(|cx| core.apply_order(job, r, cx));
            }
        })
        .detach();
    }

    /// Install a new row order. Selection and an open editor follow their source rows; an
    /// editor whose row was filtered out is cancelled (with an event and refocus).
    fn apply_order(
        self: &Rc<Self>,
        job: u64,
        r: Result<(RowOrder, Inverse), crate::order::Cancelled>,
        cx: &mut App,
    ) {
        let (sel_changed, edit_gone) = {
            let mut m = self.m.borrow_mut();
            let Ok((order, inv)) = r else {
                m.stats.stale_jobs += 1;
                return;
            };
            if job != m.job {
                m.stats.stale_jobs += 1;
                return;
            }
            m.busy = false;
            let n = m.source.row_count();
            let old = std::mem::replace(&mut m.rows, order);
            m.count = m.rows.len(n);
            let needs_map = !m.sel.is_empty() || m.edit.is_some();
            let before = m.sel.clone();
            let mut edit_gone = false;
            if needs_map {
                let map = |v: usize| old.source_row(v, n).and_then(|s| inv.view_row(s));
                let all_source_rows = old == RowOrder::Identity
                    && matches!(m.sel.rows.ranges(), [range] if *range == (0..n));
                if all_source_rows {
                    // Every row in the new view was selected. Keep the range
                    // compact while remapping just the cursor and anchor.
                    m.sel.rows.clear();
                }
                m.sel = m.sel.remap(map);
                if all_source_rows {
                    let count = m.count;
                    m.sel.rows.insert(0..count);
                }
                if let Some(e) = m.edit.as_ref().map(|e| e.source_row) {
                    match inv.view_row(e) {
                        Some(row) => {
                            if let Some(e) = m.edit.as_mut() {
                                e.row = row;
                            }
                        }
                        None => edit_gone = true,
                    }
                }
            }
            let (count, cols) = (m.count, m.cols());
            m.sel.clamp(count, cols);
            (m.sel != before, edit_gone)
        };
        if edit_gone {
            self.drop_edit(cx);
        }
        self.sync(true, cx);
        if sel_changed {
            self.emit_selection(cx);
        }
        self.flush_pending(cx);
    }

    /// Apply a commit queued while a background job shared the source.
    fn flush_pending(self: &Rc<Self>, cx: &mut App) {
        let result = {
            let mut m = self.m.borrow_mut();
            if m.pending.is_none() {
                return;
            }
            if Arc::get_mut(&mut m.source).is_none() && (m.busy || m.copying > 0) {
                return; // Still shared by a job; try again when it lands.
            }
            let Some(p) = m.pending.take() else { return };
            let r = m.write_cell(p.source_row, p.def, &p.value);
            if let Ok(Some(change)) = &r {
                m.undo.push(EditOp::Edit, vec![change.clone()]);
            }
            (p, r)
        };
        let (p, r) = result;
        match r {
            Ok(_) => {
                self.notify_source_row(p.source_row, cx);
                self.emit(
                    GridEvent::CellEdited {
                        row: p.source_row,
                        key: p.key,
                        column: p.def,
                        value: p.value,
                    },
                    cx,
                );
            }
            Err(reason) => self.emit(
                GridEvent::EditRejected {
                    row: p.source_row,
                    column: p.def,
                    reason,
                },
                cx,
            ),
        }
    }

    fn notify_source_row(&self, source_row: usize, cx: &mut App) {
        let slot = {
            let m = self.m.borrow();
            m.visible
                .clone()
                .find(|&v| m.source_row(v) == Some(source_row))
                .map(|v| vw::slot_of(v, m.pool))
        };
        if let Some(s) = slot {
            self.notify_slot(s, cx);
        }
    }

    fn change_selection(self: &Rc<Self>, cx: &mut App, f: impl FnOnce(&mut Selection, &Model)) {
        {
            let mut m = self.m.borrow_mut();
            let before = m.sel.clone();
            let mut sel = std::mem::take(&mut m.sel);
            f(&mut sel, &m);
            m.sel = sel;
            if m.sel == before {
                return;
            }
            for row in m.visible.clone() {
                if Model::row_sig(&before, row) != Model::row_sig(&m.sel, row) {
                    self.notify_slot(vw::slot_of(row, m.pool), cx);
                }
            }
        }
        self.emit_selection(cx);
    }

    fn navigate(self: &Rc<Self>, nav: Nav, extend: bool, cx: &mut App) {
        let mut to = None;
        self.change_selection(cx, |s, m| {
            to = s.navigate(nav, extend, m.count, m.cols(), m.page());
        });
        let Some(to) = to.or_else(|| self.m.borrow().sel.cursor) else {
            return;
        };
        let (x, y) = {
            let m = self.m.borrow();
            (
                m.layout.scroll_into_view(to.col, m.scroll_x, m.grid_w()),
                vw::scroll_into_view(m.scroll_y, to.row, f64::from(m.row_h), m.body_h(), m.count),
            )
        };
        self.set_scroll(x, y, cx);
    }

    fn select_all(self: &Rc<Self>, cx: &mut App) {
        self.change_selection(cx, |s, m| {
            s.select_all(m.count, m.cols());
        });
    }

    /// Copy the selection as TSV. Up to [`INLINE_COPY_CELLS`] cells are written on the UI
    /// thread and returned; larger selections are serialized on the background executor
    /// (returns `None`; the clipboard is written and [`GridEvent::Copied`] emitted when done).
    /// Selections over [`crate::tsv::MAX_COPY_CELLS`] are truncated and reported.
    fn copy(self: &Rc<Self>, cx: &mut App) -> Option<String> {
        let job = {
            let m = self.m.borrow();
            let region = m.sel.region()?;
            let cols: Vec<usize> = match &region {
                Region::Rows(_) => (0..m.cols()).collect(),
                Region::Rect(_, c) => c.clone().collect(),
            };
            let defs: Vec<ColumnDef> = cols.iter().filter_map(|&d| m.def_at(d).cloned()).collect();
            let rows: Vec<Range<usize>> = match region {
                Region::Rows(r) => r,
                Region::Rect(r, _) => vec![r],
            };
            CopyJob {
                source: m.source.clone(),
                order: m.rows.clone(),
                defs,
                rows,
            }
        };
        let cells = job.cells();
        if cells <= INLINE_COPY_CELLS {
            let (text, complete) = job.run();
            self.finish_copy(text.clone(), cells, complete, cx);
            return Some(text);
        }
        self.m.borrow_mut().copying += 1;
        let core = self.clone();
        cx.spawn(async move |cx| {
            let (text, complete) = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            cx.update(|cx| {
                core.m.borrow_mut().copying -= 1;
                core.finish_copy(text, cells, complete, cx);
                core.flush_pending(cx);
            });
        })
        .detach();
        None
    }

    fn finish_copy(&self, text: String, cells: usize, complete: bool, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.emit(
            GridEvent::Copied {
                cells: if complete {
                    cells
                } else {
                    crate::tsv::MAX_COPY_CELLS.min(cells)
                },
                truncated: !complete,
            },
            cx,
        );
    }

    fn activate(self: &Rc<Self>, window: &mut Window, cx: &mut App) {
        let Some(c) = self.m.borrow().sel.cursor else {
            return;
        };
        if !self.begin_edit(c, None, window, cx) {
            self.emit_activated(c.row, cx);
        }
    }

    fn emit_activated(&self, row: usize, cx: &mut App) {
        let hit = {
            let m = self.m.borrow();
            m.source_row(row).map(|s| (s, m.source.row_key(s)))
        };
        if let Some((row, key)) = hit {
            self.emit(GridEvent::RowActivated { row, key }, cx);
        }
    }

    /// Open the editor on `pos`. `initial` replaces the content (type-to-edit); `None` edits
    /// the current text in place. The caret is at the end either way.
    fn begin_edit(
        self: &Rc<Self>,
        pos: CellPos,
        initial: Option<String>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let prepared = {
            let m = self.m.borrow();
            let Some((_, placed)) = m.layout.display(pos.col) else {
                return false;
            };
            let def_ix = placed.def;
            let Some(source_row) = m.source_row(pos.row) else {
                return false;
            };
            let Some(def) = m.defs.get(def_ix) else {
                return false;
            };
            if !def.editable {
                return false;
            }
            let text = initial.unwrap_or_else(|| {
                let mut text = String::new();
                def.format(m.source.cell(source_row, def.source), &mut text);
                text
            });
            (def_ix, source_row, text)
        };
        // Another open editor is cancelled first: its row re-renders and listeners hear it.
        self.drop_edit(cx);
        let (def, source_row, text) = prepared;
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| i.set_value(text, window, cx));
        let focus = input.read(cx).focus_handle(cx);
        let slot = {
            let mut m = self.m.borrow_mut();
            m.edit = Some(Edit {
                row: pos.row,
                def,
                source_row,
                input,
                error: None,
                window: window.window_handle(),
            });
            vw::slot_of(pos.row, m.pool)
        };
        window.focus(&focus, cx);
        self.notify_slot(slot, cx);
        true
    }

    /// Close the editor (if any) from code that has no `Window`: notify its row, emit
    /// [`GridEvent::EditCancelled`] and hand keyboard focus back to the grid (or blur), so
    /// focus never stays on a dropped Input.
    fn drop_edit(self: &Rc<Self>, cx: &mut App) {
        let (slot, row, def, window) = {
            let mut m = self.m.borrow_mut();
            let Some(e) = m.edit.take() else { return };
            (vw::slot_of(e.row, m.pool), e.source_row, e.def, e.window)
        };
        self.notify_slot(slot, cx);
        let core = self.clone();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| core.restore_focus(window, cx));
        });
        self.emit(GridEvent::EditCancelled { row, column: def }, cx);
    }

    fn restore_focus(&self, window: &mut Window, cx: &mut App) {
        if self.tab_stop() {
            window.focus(&self.focus, cx);
        } else {
            window.blur(cx);
        }
    }

    fn end_edit(self: &Rc<Self>, window: &mut Window, cx: &mut App) {
        let slot = {
            let mut m = self.m.borrow_mut();
            let Some(e) = m.edit.take() else { return };
            vw::slot_of(e.row, m.pool)
        };
        self.notify_slot(slot, cx);
        self.restore_focus(window, cx);
    }

    fn cancel_edit(self: &Rc<Self>, window: &mut Window, cx: &mut App) {
        let ids = self.m.borrow().edit.as_ref().map(|e| (e.source_row, e.def));
        self.end_edit(window, cx);
        if let Some((row, column)) = ids {
            self.emit(GridEvent::EditCancelled { row, column }, cx);
        }
    }

    fn commit_edit(self: &Rc<Self>, window: &mut Window, cx: &mut App) -> bool {
        let Some((value, def, source_row, slot)) = ({
            let m = self.m.borrow();
            m.edit.as_ref().map(|e| {
                (
                    e.input.read(cx).value(),
                    e.def,
                    e.source_row,
                    vw::slot_of(e.row, m.pool),
                )
            })
        }) else {
            return false;
        };
        enum Outcome {
            Done(RowKey),
            Queued,
            Rejected(SharedString),
        }
        let outcome = {
            let mut m = self.m.borrow_mut();
            let key = m.source.row_key(source_row);
            let veto = m.validate(source_row, def, &value).err();
            let job_running = m.busy || m.copying > 0;
            let exclusive = Arc::get_mut(&mut m.source).is_some();
            match veto {
                Some(reason) => Outcome::Rejected(reason),
                None => match exclusive {
                    true => match m.write_cell(source_row, def, &value) {
                        Ok(change) => {
                            m.undo.push(EditOp::Edit, change.into_iter().collect());
                            Outcome::Done(key)
                        }
                        Err(reason) => Outcome::Rejected(reason),
                    },
                    // A background sort/copy holds the source: apply when it lands.
                    false if job_running => {
                        m.pending = Some(PendingCommit {
                            source_row,
                            def,
                            key,
                            value: value.clone(),
                        });
                        Outcome::Queued
                    }
                    false => Outcome::Rejected(EditError::Busy.to_string().into()),
                },
            }
        };
        match outcome {
            Outcome::Done(key) => {
                self.end_edit(window, cx);
                self.emit(
                    GridEvent::CellEdited {
                        row: source_row,
                        key,
                        column: def,
                        value,
                    },
                    cx,
                );
                true
            }
            Outcome::Queued => {
                self.end_edit(window, cx);
                true
            }
            Outcome::Rejected(reason) => {
                if let Some(e) = self.m.borrow_mut().edit.as_mut() {
                    e.error = Some(reason.clone());
                }
                self.notify_slot(slot, cx);
                self.emit(
                    GridEvent::EditRejected {
                        row: source_row,
                        column: def,
                        reason,
                    },
                    cx,
                );
                false
            }
        }
    }

    fn tab_stop(&self) -> bool {
        let m = self.m.borrow();
        m.opts.focusable.unwrap_or(m.context.keyboard_nav)
    }

    // ------------------------------------------------------------------ record editing

    fn is_editing(&self) -> bool {
        self.m.borrow().edit.is_some()
    }

    /// Cell and range modes: a visible cell cursor that typing and Tab act on.
    fn cell_mode(&self) -> bool {
        matches!(
            self.m.borrow().sel.mode,
            SelectionMode::Cell | SelectionMode::Range
        )
    }

    /// Move the cursor one step (Enter/Tab rules); `false` at the grid's end.
    fn advance(self: &Rc<Self>, adv: Advance, cx: &mut App) -> bool {
        let to = {
            let m = self.m.borrow();
            let Some(from) = m.sel.cursor else {
                return false;
            };
            adv.apply(from, m.count, m.cols())
        };
        let Some(to) = to else { return false };
        self.change_selection(cx, |s, _| s.click(to, false, false));
        self.scroll_cell_into_view(to, cx);
        true
    }

    fn scroll_cell_into_view(self: &Rc<Self>, to: CellPos, cx: &mut App) {
        let (x, y) = {
            let m = self.m.borrow();
            (
                m.layout.scroll_into_view(to.col, m.scroll_x, m.grid_w()),
                vw::scroll_into_view(m.scroll_y, to.row, f64::from(m.row_h), m.body_h(), m.count),
            )
        };
        self.set_scroll(x, y, cx);
    }

    /// Enter / Shift+Enter / Tab / Shift+Tab / an arrow in the editor: commit, then move.
    /// A vetoed commit keeps the editor open and does not move.
    fn commit_and_move(self: &Rc<Self>, adv: Advance, window: &mut Window, cx: &mut App) {
        if self.commit_edit(window, cx) {
            self.advance(adv, cx);
        }
    }

    /// Left/Right in the editor commit and move only with the caret at that edge of the text
    /// and no text selected; otherwise the caret moves inside the text. Returns whether it
    /// committed (so the caller stops the Input's own handling).
    fn editor_edge(self: &Rc<Self>, adv: Advance, window: &mut Window, cx: &mut App) -> bool {
        let at_edge = {
            let m = self.m.borrow();
            m.edit.as_ref().is_some_and(|e| {
                let i = e.input.read(cx);
                let sel = i.selected_range();
                sel.is_empty()
                    && match adv {
                        Advance::Left => i.cursor() == 0,
                        _ => i.cursor() >= i.value().len(),
                    }
            })
        };
        if at_edge {
            self.commit_and_move(adv, window, cx);
        }
        at_edge
    }

    /// Tab / Shift+Tab / Shift+Enter on the grid (not editing): move the cell cursor in cell
    /// and range modes. Returns `false` (let focus move on) in row modes, without editable
    /// columns, or at the grid's first/last cell, so Tab never traps keyboard focus.
    fn tab_move(self: &Rc<Self>, adv: Advance, cx: &mut App) -> bool {
        let editable = self.m.borrow().any_editable;
        if !self.cell_mode() || !editable {
            return false;
        }
        if self.m.borrow().sel.cursor.is_none() {
            self.change_selection(cx, |s, _| s.click(CellPos::new(0, 0), false, false));
            return true;
        }
        self.advance(adv, cx)
    }

    /// Type-to-edit: a printable key on a focused grid (cell/range mode, not editing, no
    /// Ctrl/Cmd) opens the cursor cell's editor with that text replacing the content.
    fn type_to_edit(self: &Rc<Self>, ev: &KeyDownEvent, window: &mut Window, cx: &mut App) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        if m.control || m.platform || m.function || self.is_editing() || !self.cell_mode() {
            return;
        }
        let Some(text) = k.key_char.as_deref() else {
            return;
        };
        if text.is_empty() || text.chars().any(char::is_control) {
            return;
        }
        let Some(pos) = self.m.borrow().sel.cursor else {
            return;
        };
        if self.begin_edit(pos, Some(text.to_string()), window, cx) {
            cx.stop_propagation();
        }
    }

    /// Ctrl+V: paste the clipboard's text.
    fn paste(self: &Rc<Self>, cx: &mut App) -> usize {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return 0;
        };
        self.paste_text(&text, cx)
    }

    /// Refuse a bulk operation up front, with one operation-level
    /// [`GridEvent::CellsRejected`], while a background sort/copy shares the source.
    fn refuse_if_busy(self: &Rc<Self>, op: EditOp, cx: &mut App) -> bool {
        let busy = Arc::get_mut(&mut self.m.borrow_mut().source).is_none();
        if busy {
            self.reject_whole(op, 0, EditError::Busy.to_string(), cx);
        }
        busy
    }

    fn reject_whole(&self, op: EditOp, rejected: usize, reason: String, cx: &mut App) {
        self.emit(
            GridEvent::CellsRejected {
                op,
                rejected,
                applied: 0,
                first: Arc::from([CellRejection::whole(reason)]),
            },
            cx,
        );
    }

    fn paste_text(self: &Rc<Self>, text: &str, cx: &mut App) -> usize {
        if self.is_editing() || self.refuse_if_busy(EditOp::Paste, cx) {
            return 0;
        }
        let block = parse_tsv(text);
        let (writes, truncated, target) = {
            let m = self.m.borrow();
            let Some((rows, cols)) = paste_target(
                block.rows(),
                block.cols(),
                m.sel.rect(),
                m.sel.cursor,
                m.count,
                m.cols(),
            ) else {
                return 0;
            };
            let origin = CellPos::new(rows.start, cols.start);
            let defs: Vec<(usize, Option<usize>)> =
                cols.clone().map(|c| (c, m.editable_display(c))).collect();
            let mut writes = Vec::new();
            let mut truncated = false;
            'rows: for r in rows.clone() {
                let Some(src) = m.source_row(r) else { continue };
                for &(c, def) in &defs {
                    // Non-editable columns are skipped: the block is clipped to them.
                    let Some(def) = def else { continue };
                    if writes.len() >= MAX_EDIT_CELLS {
                        truncated = true;
                        break 'rows;
                    }
                    let value = block.tiled(origin, r, c).unwrap_or("").to_string();
                    writes.push(CellWrite {
                        source_row: src,
                        def,
                        value,
                        expect: None,
                    });
                }
            }
            (writes, truncated, (rows, cols))
        };
        let applied = self.apply_writes(EditOp::Paste, writes, truncated, true, cx);
        // The pasted area becomes the selection (range mode), like a spreadsheet.
        let (rows, cols) = target;
        if self.m.borrow().sel.mode == SelectionMode::Range && rows.len() * cols.len() > 1 {
            self.change_selection(cx, |s, _| {
                s.anchor = Some(CellPos::new(rows.start, cols.start));
                s.cursor = Some(CellPos::new(rows.end - 1, cols.end - 1));
            });
        }
        applied
    }

    /// Ctrl+X: copy, then clear the editable cells. Refused (one rejection event) when the
    /// selection is larger than [`MAX_EDIT_CELLS`], so a cut never copies more than it clears.
    fn cut(self: &Rc<Self>, cx: &mut App) -> usize {
        if self.is_editing() || self.refuse_if_busy(EditOp::Cut, cx) {
            return 0;
        }
        let (writes, covered, truncated) = self.m.borrow().selected_editable();
        if truncated {
            let reason = format!("selection too large to cut (over {MAX_EDIT_CELLS} cells)");
            self.reject_whole(EditOp::Cut, covered, reason, cx);
            return 0;
        }
        self.copy(cx);
        self.apply_writes(EditOp::Cut, writes, false, true, cx)
    }

    /// Delete / Backspace: clear the editable cells of the selection.
    fn clear_cells(self: &Rc<Self>, op: EditOp, cx: &mut App) -> usize {
        if self.is_editing() || self.refuse_if_busy(op, cx) {
            return 0;
        }
        let (writes, _, truncated) = self.m.borrow().selected_editable();
        self.apply_writes(op, writes, truncated, true, cx)
    }

    /// Undo (or redo) one transaction. Restores bypass the validator (they put back values the
    /// source held) but still go through `set_cell`, and only into rows that still carry the
    /// recorded `RowKey` (others are rejected, never written into another record).
    ///
    /// The transaction moves to the other stack only after its writes ran: when every write
    /// is rejected it stays where it was. A partial restore (some cells rejected, reported in
    /// one event) still moves it, so the stacks always reflect the source's last good state.
    fn undo(self: &Rc<Self>, redo: bool, cx: &mut App) -> bool {
        let op = if redo { EditOp::Redo } else { EditOp::Undo };
        let ready = {
            let m = self.m.borrow();
            if redo {
                m.undo.can_redo()
            } else {
                m.undo.can_undo()
            }
        };
        if self.is_editing() || !ready || self.refuse_if_busy(op, cx) {
            return false;
        }
        let writes: Vec<CellWrite> = {
            let m = self.m.borrow();
            let t = if redo {
                m.undo.peek_redo()
            } else {
                m.undo.peek_undo()
            };
            let Some(t) = t else { return false };
            let steps: Vec<(&CellKey, &String)> = if redo {
                t.redo_steps().collect()
            } else {
                t.undo_steps().collect()
            };
            steps
                .into_iter()
                .map(|(&(source_row, key, def), v)| CellWrite {
                    source_row,
                    def,
                    value: v.clone(),
                    expect: Some(key),
                })
                .collect()
        };
        let applied = self.apply_writes(op, writes, false, false, cx);
        if applied == 0 {
            return false;
        }
        let mut m = self.m.borrow_mut();
        if redo {
            m.undo.redo();
        } else {
            m.undo.undo();
        }
        true
    }

    /// Write cells as one undo transaction (undo/redo themselves are not recorded). Vetoed
    /// cells are collected into one [`GridEvent::CellsRejected`]; the rest apply.
    fn apply_writes(
        self: &Rc<Self>,
        op: EditOp,
        writes: Vec<CellWrite>,
        truncated: bool,
        validate: bool,
        cx: &mut App,
    ) -> usize {
        if writes.is_empty() {
            return 0;
        }
        let (applied, rejected, first) = {
            let mut m = self.m.borrow_mut();
            let mut changes = Vec::with_capacity(writes.len());
            let mut applied = 0usize;
            let mut rejected = 0usize;
            let mut first = Vec::new();
            for w in writes {
                let moved = w.expect.is_some_and(|k| {
                    w.source_row >= m.source.row_count() || m.source.row_key(w.source_row) != k
                });
                let r = if moved {
                    Err(SharedString::from("the record changed since this edit"))
                } else if validate {
                    m.validate(w.source_row, w.def, &w.value)
                } else {
                    Ok(())
                }
                .and_then(|()| m.write_cell(w.source_row, w.def, &w.value));
                match r {
                    Ok(change) => {
                        applied += 1;
                        changes.extend(change);
                    }
                    Err(reason) => {
                        rejected += 1;
                        if first.len() < REJECTION_SAMPLES {
                            first.push(CellRejection {
                                row: Some(w.source_row),
                                column: Some(w.def),
                                reason,
                            });
                        }
                    }
                }
            }
            if !matches!(op, EditOp::Undo | EditOp::Redo) {
                m.undo.push(op, changes);
            }
            (applied, rejected, first)
        };
        // Bulk writes may touch any visible row: re-render them all (not a scroll path).
        self.sync(true, cx);
        if applied > 0 {
            self.emit(
                GridEvent::CellsChanged {
                    op,
                    cells: applied,
                    truncated,
                },
                cx,
            );
        }
        if rejected > 0 {
            self.emit(
                GridEvent::CellsRejected {
                    op,
                    rejected,
                    applied,
                    first: first.into(),
                },
                cx,
            );
        }
        applied
    }

    // ------------------------------------------------------------------ drag autoscroll

    /// Scroll delta for a range drag at grid-relative `(x, y)`: non-zero near or past the
    /// body's edges (and the center band's, horizontally).
    fn autoscroll_delta(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.m.borrow();
        let w = m.grid_w();
        let top = m.row_h;
        let bottom = top + m.body_h() as f32;
        let zone = (m.row_h / 2.).max(4.);
        let max = m.row_h * 2.;
        let dy = edge_delta(y, top, bottom, zone, max);
        let lo = m.layout.left_width;
        let hi = lo + m.layout.center_viewport(w);
        let dx = edge_delta(x, lo, hi, zone, max);
        (dx, dy)
    }

    /// Extend the range to the cell under grid-relative `(x, y)` (clamped into the body).
    fn drag_select_to(self: &Rc<Self>, x: f32, y: f32, cx: &mut App) {
        let pos = {
            let m = self.m.borrow();
            let body = m.body_h() as f32;
            let row = vw::row_at(
                f64::from((y - m.row_h).clamp(0., (body - 1.).max(0.))),
                m.scroll_y,
                f64::from(m.row_h),
                m.count,
            );
            let w = m.grid_w();
            let col = m.layout.hit(x.clamp(0., w - 1.), m.scroll_x, w);
            row.zip(col).map(|(r, c)| CellPos::new(r, c))
        };
        if let Some(pos) = pos {
            self.change_selection(cx, |s, _| s.click(pos, true, false));
        }
    }

    /// One autoscroll step at the last drag position; `false` when the drag ended or the
    /// pointer left the edge zones.
    fn autoscroll_step(self: &Rc<Self>, cx: &mut App) -> bool {
        let Some((x, y)) = ({
            let m = self.m.borrow();
            (m.drag == Some(Drag::Select))
                .then_some(m.autoscroll)
                .flatten()
        }) else {
            return false;
        };
        let (dx, dy) = self.autoscroll_delta(x, y);
        if dx == 0. && dy == 0. {
            return false;
        }
        let (sx, sy) = {
            let m = self.m.borrow();
            (m.scroll_x + f64::from(dx), m.scroll_y + f64::from(dy))
        };
        self.set_scroll(sx, sy, cx);
        self.drag_select_to(x, y, cx);
        true
    }

    /// A range drag moved to grid-relative `(x, y)`: extend, scroll one step when near an
    /// edge, and keep scrolling on a timer while the pointer stays there.
    fn drag_select(self: &Rc<Self>, x: f32, y: f32, cx: &mut App) {
        self.m.borrow_mut().autoscroll = Some((x, y));
        self.drag_select_to(x, y, cx);
        if !self.autoscroll_step(cx) || self.m.borrow().autoscroll_task {
            return;
        }
        self.m.borrow_mut().autoscroll_task = true;
        let core = Rc::downgrade(self);
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(30))
                    .await;
                let go = cx.update(|cx| {
                    let Some(core) = core.upgrade() else {
                        return false;
                    };
                    let go = core.autoscroll_step(cx);
                    if !go {
                        core.m.borrow_mut().autoscroll_task = false;
                    }
                    go
                });
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    // ------------------------------------------------------------------ pointer

    fn mouse_down(self: &Rc<Self>, ev: &MouseDownEvent, window: &mut Window, cx: &mut App) {
        let (p, bounds) = {
            let m = self.m.borrow();
            (ev.position - m.bounds.origin, m.bounds)
        };
        let (x, y) = (f32::from(p.x), f32::from(p.y));
        // Clicks inside the open editor belong to the Input.
        let in_editor = {
            let m = self.m.borrow();
            m.edit.as_ref().is_some_and(|e| {
                let Some(col) = m.layout.display_of(e.def) else {
                    return false;
                };
                let top = m.row_h + vw::row_top(e.row, m.scroll_y, f64::from(m.row_h));
                let sx = m
                    .layout
                    .screen_x(col, m.scroll_x, m.grid_w())
                    .unwrap_or(-1.);
                let w = m.layout.display(col).map(|(_, p)| p.width).unwrap_or(0.);
                y >= top && y < top + m.row_h && x >= sx && x < sx + w
            })
        };
        if in_editor {
            return;
        }
        if self.m.borrow().edit.is_some() && !self.commit_edit(window, cx) {
            return;
        }
        if self.tab_stop() {
            window.focus(&self.focus, cx);
        } else {
            // A pointer press never takes keyboard focus (API conventions §12.3).
            window.prevent_default();
        }
        let mut m = self.m.borrow_mut();
        let w = m.grid_w();
        if y < m.row_h {
            if let Some(i) = m.layout.resize_edge(x, m.scroll_x, w, 4.)
                && let Some((_, placed)) = m.layout.display(i)
                && m.defs.get(placed.def).is_some_and(|d| d.resizable)
            {
                m.drag = Some(Drag::Resize {
                    def: placed.def,
                    start_x: x,
                    start_w: placed.width,
                });
                return;
            }
            if let Some(i) = m.layout.hit(x, m.scroll_x, w)
                && let Some((_, placed)) = m.layout.display(i)
            {
                m.drag = Some(Drag::Header {
                    def: placed.def,
                    start_x: x,
                    shift: ev.modifiers.shift,
                    moved: false,
                    x,
                });
            }
            return;
        }
        let body_y = f64::from(y - m.row_h);
        let content_h = m.count as f64 * f64::from(m.row_h);
        // Horizontal scrollbar: along the bottom edge of the center band
        // (pinned columns never scroll), short of the vertical bar's corner.
        let center_vp = f64::from(m.layout.center_viewport(w));
        let lw = m.layout.left_width;
        let vbar = content_h > m.body_h();
        let h = f32::from(bounds.size.height);
        let h = if h > 0. { h } else { FALLBACK_H };
        let in_hbar_x = x >= lw && f64::from(x - lw) < center_vp && !(vbar && x >= w - SCROLLBAR);
        if m.layout.center_width > center_vp && y >= h - SCROLLBAR && in_hbar_x {
            let along = f64::from(x - lw);
            let thumb = scrollbar_thumb(m.scroll_x, m.layout.center_width, center_vp);
            if along >= thumb.0 && along < thumb.0 + thumb.1 {
                m.drag = Some(Drag::HScroll {
                    start_x: x,
                    start_offset: m.scroll_x,
                });
                return;
            }
            // A press on the track pages toward it.
            let page = if along < thumb.0 {
                -center_vp
            } else {
                center_vp
            };
            let (sx, sy) = (m.scroll_x + page, m.scroll_y);
            drop(m);
            self.set_scroll(sx, sy, cx);
            return;
        }
        if x >= w - SCROLLBAR && content_h > m.body_h() {
            let thumb = scrollbar_thumb(m.scroll_y, content_h, m.body_h());
            let offset = if body_y >= thumb.0 && body_y < thumb.0 + thumb.1 {
                m.scroll_y
            } else {
                // Jump so the thumb centers on the press.
                let frac = ((body_y - thumb.1 / 2.) / (m.body_h() - thumb.1).max(1.)).clamp(0., 1.);
                frac * (content_h - m.body_h())
            };
            m.drag = Some(Drag::VScroll {
                start_y: y,
                start_offset: offset,
            });
            let x = m.scroll_x;
            drop(m);
            self.set_scroll(x, offset, cx);
            return;
        }
        let row = vw::row_at(body_y, m.scroll_y, f64::from(m.row_h), m.count);
        let col = m.layout.hit(x, m.scroll_x, w);
        let (Some(row), Some(col)) = (row, col) else {
            return;
        };
        let pos = CellPos::new(row, col);
        let range_mode = m.sel.mode == SelectionMode::Range;
        if range_mode {
            m.drag = Some(Drag::Select);
        }
        drop(m);
        if range_mode {
            // Repaint so the window-level move listener (drags outside the grid) is armed.
            self.notify_grid(cx);
        }
        if ev.click_count >= 2 {
            if !self.begin_edit(pos, None, window, cx) {
                self.emit_activated(row, cx);
            }
            return;
        }
        let ctrl = ev.modifiers.control || ev.modifiers.platform;
        self.change_selection(cx, |s, _| s.click(pos, ev.modifiers.shift, ctrl));
    }

    fn mouse_move(self: &Rc<Self>, ev: &MouseMoveEvent, cx: &mut App) {
        let Some(drag) = self.m.borrow().drag else {
            return;
        };
        if ev.pressed_button != Some(MouseButton::Left) {
            self.m.borrow_mut().drag = None;
            self.notify_grid(cx);
            return;
        }
        let (x, y) = {
            let m = self.m.borrow();
            let p = ev.position - m.bounds.origin;
            (f32::from(p.x), f32::from(p.y))
        };
        match drag {
            Drag::Resize {
                def,
                start_x,
                start_w,
            } => {
                let changed = {
                    let mut m = self.m.borrow_mut();
                    match m.defs.get_mut(def) {
                        Some(d) => {
                            let w = d.clamp_width(start_w + x - start_x);
                            let c = w != d.width;
                            d.width = w;
                            c
                        }
                        None => false,
                    }
                };
                if changed {
                    self.resize_columns(cx);
                }
            }
            Drag::Header {
                def,
                start_x,
                shift,
                moved,
                ..
            } => {
                let moved = moved || (x - start_x).abs() > 4.;
                self.m.borrow_mut().drag = Some(Drag::Header {
                    def,
                    start_x,
                    shift,
                    moved,
                    x,
                });
                if moved {
                    self.notify_grid(cx);
                }
            }
            Drag::Select => self.drag_select(x, y, cx),
            Drag::VScroll {
                start_y,
                start_offset,
            } => {
                let (sx, sy) = {
                    let m = self.m.borrow();
                    let content = m.count as f64 * f64::from(m.row_h);
                    let body = m.body_h();
                    let thumb = scrollbar_thumb(start_offset, content, body);
                    let track = (body - thumb.1).max(1.);
                    let per_px = (content - body).max(0.) / track;
                    (m.scroll_x, start_offset + f64::from(y - start_y) * per_px)
                };
                self.set_scroll(sx, sy, cx);
            }
            Drag::HScroll {
                start_x,
                start_offset,
            } => {
                let (sx, sy) = {
                    let m = self.m.borrow();
                    let content = m.layout.center_width;
                    let vp = f64::from(m.layout.center_viewport(m.grid_w()));
                    let thumb = scrollbar_thumb(start_offset, content, vp);
                    let track = (vp - thumb.1).max(1.);
                    let per_px = (content - vp).max(0.) / track;
                    (start_offset + f64::from(x - start_x) * per_px, m.scroll_y)
                };
                self.set_scroll(sx, sy, cx);
            }
        }
    }

    fn mouse_up(self: &Rc<Self>, cx: &mut App) {
        let Some(drag) = self.m.borrow_mut().drag.take() else {
            return;
        };
        self.m.borrow_mut().autoscroll = None;
        match drag {
            Drag::Resize { def, .. } => {
                let width = self.m.borrow().defs.get(def).map(|d| d.width);
                if let Some(width) = width {
                    self.emit(GridEvent::ColumnResized { column: def, width }, cx);
                }
            }
            Drag::Header {
                def,
                shift,
                moved,
                x,
                ..
            } => {
                if moved {
                    let before = self.drop_target(def, x);
                    self.notify_grid(cx);
                    if let Some(before) = before {
                        self.move_column(def, before, cx);
                    }
                } else {
                    let sortable = self.m.borrow().defs.get(def).is_some_and(|d| d.sortable);
                    if sortable {
                        let mut keys = self.m.borrow().sort.clone();
                        toggle_sort(&mut keys, def, shift);
                        self.set_sort(keys, cx);
                    }
                }
            }
            Drag::Select | Drag::VScroll { .. } | Drag::HScroll { .. } => {}
        }
    }

    /// Where a dragged header lands: `Some(before)` (None = last in its band's order).
    fn drop_target(&self, def: usize, x: f32) -> Option<Option<usize>> {
        self.m.borrow().drop_slot(def, x).map(|(before, _)| before)
    }

    fn wheel(self: &Rc<Self>, ev: &ScrollEvent, cx: &mut App) {
        let (x, y) = {
            let m = self.m.borrow();
            let d = ev.delta.pixel_delta(px(m.row_h));
            let (mut dx, mut dy) = (f64::from(f32::from(d.x)), f64::from(f32::from(d.y)));
            if ev.modifiers.shift && dx == 0. {
                dx = dy;
                dy = 0.;
            }
            (m.scroll_x - dx, m.scroll_y - dy)
        };
        self.set_scroll(x, y, cx);
    }
}

/// AccessKit row index of view row `row` (1-based; the header row is 1).
pub fn aria_row_index(row: usize) -> usize {
    row + 2
}

/// Vertical scrollbar thumb `(top, height)` within a body `body` px tall.
fn scrollbar_thumb(offset: f64, content: f64, body: f64) -> (f64, f64) {
    if content <= body || content <= 0. {
        return (0., body);
    }
    let h = (body * body / content).max(24.).min(body);
    let top = offset / (content - body) * (body - h);
    (top.clamp(0., body - h), h)
}

fn hsla(c: Rgb) -> Hsla {
    c.to_hsla()
}

/// Colors shared by the grid and its recycled rows.
#[derive(Clone, Copy)]
struct Palette {
    body: Rgb,
    zebra: Rgb,
    header: Rgb,
    hover: Rgb,
    border: Rgb,
    text: Rgb,
    dimmed: Rgb,
    selected: Rgb,
    accent: Rgb,
    ring: Rgb,
    error: Rgb,
}

impl Core {
    fn palette(&self, cx: &App) -> Palette {
        let theme = GuiraffeTheme::global(cx);
        let mut cached = self.palette.borrow_mut();
        if let Some((key, palette)) = cached.as_ref()
            && key == theme
        {
            return *palette;
        }
        let palette = Palette::resolve(theme);
        self.m.borrow_mut().stats.palette_resolutions += 1;
        *cached = Some((theme.clone(), palette));
        palette
    }
}

impl Palette {
    fn resolve(t: &GuiraffeTheme) -> Self {
        let s = t.surfaces();
        let light = t.variant(None, Variant::Light);
        Self {
            body: s.body,
            zebra: s.surface,
            header: s.surface,
            hover: s.hover,
            border: s.border,
            text: s.text,
            dimmed: t
                .state(StateKind::Dimmed, Variant::Default, None)
                .foreground,
            selected: light.background.over(s.body),
            accent: t
                .state(StateKind::Selected, Variant::Filled, None)
                .background,
            ring: t.state(StateKind::FocusRing, Variant::Default, None).border,
            error: t
                .state(StateKind::Rest, Variant::Outline, Some(ColorName::Red))
                .border,
        }
    }
}

/// The grid's root view: header, row wrappers, scrollbar, pointer and keyboard handling.
pub(crate) struct GridView {
    core: Rc<Core>,
    header: Entity<HeaderView>,
}

struct HeaderView {
    core: Weak<Core>,
}

impl Render for HeaderView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(core) = self.core.upgrade() else {
            return div().into_any_element();
        };
        let pal = core.palette(cx);
        let mut m = core.m.borrow_mut();
        m.stats.header_renders += 1;
        let row_h = m.row_h;
        let w = m.grid_w();
        // Header cells: visible center columns plus pinned bands.
        let mut header = div()
            .relative()
            .size_full()
            .bg(hsla(pal.header))
            .border_b_1()
            .border_color(hsla(pal.border))
            .id("header")
            .test_support()
            .role(Role::Row)
            .aria_row_index(1);
        let header_cell = |placed: &crate::columns::Placed, x: f32| -> AnyElement {
            let def = m.defs.get(placed.def);
            let title = def.map(|d| d.title.clone()).unwrap_or_default();
            let sort = m
                .sort
                .iter()
                .enumerate()
                .find(|(_, k)| k.column == placed.def);
            let indicator: Option<SharedString> = sort.map(|(i, k)| {
                let arrow = if k.dir == SortDir::Asc { "▲" } else { "▼" };
                if m.sort.len() > 1 {
                    format!("{arrow}{}", i + 1).into()
                } else {
                    arrow.into()
                }
            });
            let resizable = def.is_some_and(|d| d.resizable);
            let end = def.is_some_and(|d| d.align == Align::End);
            div()
                .id(("colheader", placed.def))
                .test_support()
                .role(Role::ColumnHeader)
                .aria_label(title.clone())
                .aria_column_index(placed.display + 1)
                .when_some(sort, |el, (_, k)| {
                    el.aria_description(if k.dir == SortDir::Asc {
                        "sorted ascending"
                    } else {
                        "sorted descending"
                    })
                })
                .absolute()
                .top_0()
                .left(px(x))
                .w(px(placed.width))
                .h(px(row_h))
                .px(px(m.pad))
                .flex()
                .items_center()
                .gap(px(4.))
                .when(end, |el| el.justify_end())
                .overflow_hidden()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(hsla(pal.text))
                .hover(move |s| s.bg(hsla(pal.hover)))
                .when(def.is_some_and(|d| d.sortable), |el| el.cursor_pointer())
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title),
                )
                .when_some(indicator, |el, i| {
                    el.child(
                        div()
                            .text_color(hsla(pal.accent))
                            .text_size(px(m.font * 0.8))
                            .child(i),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .h_full()
                        .w(px(1.))
                        .bg(hsla(pal.border)),
                )
                .when(resizable, |el| {
                    el.child(
                        div()
                            .id(("resize", placed.def))
                            .absolute()
                            .top_0()
                            .right(px(-4.))
                            .w(px(8.))
                            .h_full()
                            .cursor_ew_resize(),
                    )
                })
                .into_any_element()
        };
        let lw = m.layout.left_width;
        let center_vp = m.layout.center_viewport(w);
        let vis_c = m.layout.visible_center(m.scroll_x, w);
        let mut center = div()
            .absolute()
            .top_0()
            .left(px(lw))
            .w(px(center_vp))
            .h_full()
            .overflow_hidden();
        for p in m.layout.center.get(vis_c).into_iter().flatten() {
            center = center.child(header_cell(p, (p.x - m.scroll_x) as f32));
        }
        header = header.child(center);
        for p in &m.layout.left {
            header = header.child(header_cell(p, p.x as f32));
        }
        let rx = w - m.layout.right_width;
        for p in &m.layout.right {
            header = header.child(header_cell(p, rx + p.x as f32));
        }

        header.into_any_element()
    }
}

macro_rules! nav_actions {
    ($el:expr, $core:expr, $( $action:ident => ($nav:expr, $ext:expr) ),* $(,)?) => {{
        let el = $el;
        $(
            let c = $core.clone();
            let el = el.on_action(move |_: &$action, _: &mut Window, cx: &mut App| c.navigate($nav, $ext, cx));
        )*
        el
    }};
}

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let core = self.core.clone();
        let ctx = StyleContext::current();
        let resync = {
            let mut m = core.m.borrow_mut();
            m.stats.grid_renders += 1;
            if m.context != ctx {
                m.context = ctx;
                m.resolve_metrics()
            } else {
                false
            }
        };
        if resync {
            let c = core.clone();
            cx.defer(move |cx| c.sync(true, cx));
        }
        let pal = core.palette(cx);
        let tab = core.tab_stop();
        let focused = core.focus.is_focused(window);
        let m = core.m.borrow();
        let row_h = m.row_h;
        let w = m.grid_w();
        let rows = core.rows.borrow();
        let row_f = f64::from(row_h);

        let lw = m.layout.left_width;
        let center_vp = m.layout.center_viewport(w);
        let header_style = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(row_h))
            .style()
            .clone();

        // Row wrappers: positioned here, content in the retained row views.
        let mut body = div()
            .absolute()
            .top(px(row_h))
            .left_0()
            .right_0()
            .bottom_0()
            .overflow_hidden();
        for r in m.visible.clone() {
            let Some(view) = rows.get(vw::slot_of(r, m.pool)) else {
                continue;
            };
            body = body.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(vw::row_top(r, m.scroll_y, row_f)))
                    .h(px(row_h))
                    .child(view.clone()),
            );
        }

        // Scrollbar (overlay; never takes layout space).
        let content_h = m.count as f64 * row_f;
        let body_h = m.body_h();
        let vbar = (content_h > body_h).then(|| {
            let (top, h) = scrollbar_thumb(m.scroll_y, content_h, body_h);
            div()
                .absolute()
                .right(px(1.))
                .top(px(row_h + top as f32))
                .w(px(SCROLLBAR - 2.))
                .h(px(h as f32))
                .rounded(px(SCROLLBAR))
                .bg(hsla(pal.dimmed))
                .opacity(0.5)
        });
        let hbar = (m.layout.center_width > f64::from(center_vp)).then(|| {
            let (left, wbar) =
                scrollbar_thumb(m.scroll_x, m.layout.center_width, f64::from(center_vp));
            div()
                .absolute()
                .bottom(px(1.))
                .left(px(lw + left as f32))
                .h(px(SCROLLBAR - 2.))
                .w(px(wbar as f32))
                .debug_selector(|| "datagrid-hscroll-thumb".into())
                .cursor_ew_resize()
                .rounded(px(SCROLLBAR))
                .bg(hsla(pal.dimmed))
                .opacity(0.5)
        });

        // Reorder drop indicator.
        let drop_x = match m.drag {
            Some(Drag::Header {
                moved: true,
                x,
                def,
                ..
            }) => m.drop_slot(def, x).map(|(_, x)| x),
            _ => None,
        };
        let busy = m.busy;
        let count = m.count;
        let cols = m.cols();
        let label = m.opts.label.clone();
        let font = m.font;
        drop(rows);
        drop(m);

        let c_record = core.clone();
        let c_paint = core.clone();
        let (c_paste, c_cut, c_clear, c_undo, c_redo) = (
            core.clone(),
            core.clone(),
            core.clone(),
            core.clone(),
            core.clone(),
        );
        let (c_tab, c_btab, c_senter, c_type) =
            (core.clone(), core.clone(), core.clone(), core.clone());
        let (c_down, c_move, c_up, c_up_out, c_wheel) = (
            core.clone(),
            core.clone(),
            core.clone(),
            core.clone(),
            core.clone(),
        );
        let (c_all, c_copy, c_edit, c_act) =
            (core.clone(), core.clone(), core.clone(), core.clone());
        let el = div()
            .id("datagrid")
            .test_support()
            .role(Role::Grid)
            .aria_label(label)
            .aria_row_count(count + 1)
            .aria_column_count(cols)
            .track_focus(&core.focus.clone().tab_stop(tab))
            .key_context(CONTEXT);
        let el = nav_actions!(el, core,
            MoveUp => (Nav::Up, false), MoveDown => (Nav::Down, false),
            MoveLeft => (Nav::Left, false), MoveRight => (Nav::Right, false),
            MovePageUp => (Nav::PageUp, false), MovePageDown => (Nav::PageDown, false),
            MoveHome => (Nav::Home, false), MoveEnd => (Nav::End, false),
            MoveFirst => (Nav::First, false), MoveLast => (Nav::Last, false),
            ExtendUp => (Nav::Up, true), ExtendDown => (Nav::Down, true),
            ExtendLeft => (Nav::Left, true), ExtendRight => (Nav::Right, true),
            ExtendPageUp => (Nav::PageUp, true), ExtendPageDown => (Nav::PageDown, true),
            ExtendHome => (Nav::Home, true), ExtendEnd => (Nav::End, true),
            ExtendFirst => (Nav::First, true), ExtendLast => (Nav::Last, true),
        );
        el.on_action(move |_: &SelectAll, _, cx| c_all.select_all(cx))
            .on_action(move |_: &CopySelection, _, cx| {
                c_copy.copy(cx);
            })
            .on_action(move |_: &EditCell, window, cx| {
                let cur = c_edit.m.borrow().sel.cursor;
                if let Some(c) = cur {
                    c_edit.begin_edit(c, None, window, cx);
                }
            })
            .on_action(move |_: &Activate, window, cx| c_act.activate(window, cx))
            .on_action(move |_: &Paste, _, cx| {
                c_paste.paste(cx);
            })
            .on_action(move |_: &Cut, _, cx| {
                c_cut.cut(cx);
            })
            .on_action(move |_: &ClearCells, _, cx| {
                c_clear.clear_cells(EditOp::Clear, cx);
            })
            .on_action(move |_: &Undo, _, cx| {
                c_undo.undo(false, cx);
            })
            .on_action(move |_: &Redo, _, cx| {
                c_redo.undo(true, cx);
            })
            .on_action(move |_: &TabNext, _, cx| {
                if !c_tab.tab_move(Advance::Right, cx) {
                    cx.propagate();
                }
            })
            .on_action(move |_: &TabPrev, _, cx| {
                if !c_btab.tab_move(Advance::Left, cx) {
                    cx.propagate();
                }
            })
            .on_action(move |_: &EnterPrev, _, cx| {
                if !c_senter.tab_move(Advance::Up, cx) {
                    cx.propagate();
                }
            })
            .on_key_down(move |ev: &KeyDownEvent, window, cx| c_type.type_to_edit(ev, window, cx))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(hsla(pal.body))
            .text_color(hsla(pal.text))
            .text_size(px(font))
            .border_1()
            .border_color(hsla(pal.border))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let old_size = {
                            let mut m = c_record.m.borrow_mut();
                            let old_size = m.bounds.size;
                            m.bounds = bounds;
                            old_size
                        };
                        if old_size != bounds.size {
                            let c = c_record.clone();
                            cx.defer(move |cx| c.sync(old_size.width != bounds.size.width, cx));
                        }
                    },
                    move |_, _, window, _| {
                        // A range drag keeps extending (and autoscrolling) with the pointer
                        // outside the grid; inside, the element's own listener handles it.
                        if c_paint.m.borrow().drag != Some(Drag::Select) {
                            return;
                        }
                        let c = c_paint.clone();
                        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                            let outside = !c.m.borrow().bounds.contains(&ev.position);
                            if phase == DispatchPhase::Bubble && outside {
                                c.mouse_move(ev, cx);
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(body)
            .child(self.header.clone().cached(header_style))
            .children(vbar)
            .children(hbar)
            .when_some(drop_x, |el, x| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(x - 1.))
                        .w(px(2.))
                        .h(px(row_h))
                        .bg(hsla(pal.accent)),
                )
            })
            .when(busy, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(px(row_h))
                        .left_0()
                        .right_0()
                        .h(px(2.))
                        .bg(hsla(pal.accent)),
                )
            })
            .when(focused && tab, |el| {
                guiraffe_core::focus_ring(el, window.last_input_was_keyboard(), px(0.), pal.ring)
            })
            .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
                c_down.mouse_down(ev, window, cx)
            })
            .on_mouse_move(move |ev, _, cx| c_move.mouse_move(ev, cx))
            .on_mouse_up(MouseButton::Left, move |_: &MouseUpEvent, _, cx| {
                c_up.mouse_up(cx)
            })
            .on_mouse_up_out(MouseButton::Left, move |_: &MouseUpEvent, _, cx| {
                c_up_out.mouse_up(cx)
            })
            .on_scroll(move |ev, _, cx| {
                c_wheel.wheel(ev, cx);
                cx.stop_propagation();
            })
    }
}

const CELL_CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::MIN.saturating_add(1023);

type CellShapeKey = (u64, u32, u32, gpui_kit::Font);
struct CellShape {
    text: SharedString,
    layout: Arc<gpui_kit::LineLayout>,
    len: usize,
}
type CellShapes = lru::LruCache<CellShapeKey, CellShape>;

struct CellLabel {
    text: SharedString,
    hash: u64,
    x: Pixels,
    width: Pixels,
    align: Align,
    background: Option<Hsla>,
}

fn paint_cell_labels(
    labels: &[CellLabel],
    shapes: &mut CellShapes,
    bounds: Bounds<Pixels>,
    pad: Pixels,
    row_height: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line_height = window.pixel_snap(style.line_height_in_pixels(window.rem_size()));
    let font = style.font();
    let text_system = window.text_system().clone();
    for label in labels {
        if let Some(background) = label.background {
            window.paint_quad(gpui_kit::fill(
                Bounds::new(
                    bounds.origin + gpui_kit::point(label.x, px(0.)),
                    gpui_kit::size(label.width, row_height),
                ),
                background,
            ));
        }
        if label.text.is_empty() {
            continue;
        }
        let width = (label.width - pad * 2.).max(px(0.));
        let key = (
            label.hash,
            f32::from(width).to_bits(),
            f32::from(font_size).to_bits(),
            font.clone(),
        );
        if !shapes
            .get(&key)
            .is_some_and(|shape| shape.text == label.text)
        {
            let mut run = style.to_run(label.text.len());
            let mut layout = text_system.layout_line(
                label.text.as_ref(),
                font_size,
                std::slice::from_ref(&run),
                None,
            );
            if layout.width > width {
                let (text, _) = cx
                    .text_system()
                    .line_wrapper(style.font(), font_size)
                    .truncate_line(
                        label.text.clone(),
                        width,
                        "…",
                        std::slice::from_ref(&run),
                        gpui_kit::TruncateFrom::End,
                    );
                run.len = text.len();
                layout = text_system.layout_line(
                    text.as_ref(),
                    font_size,
                    std::slice::from_ref(&run),
                    None,
                );
            }
            shapes.put(
                key.clone(),
                CellShape {
                    text: label.text.clone(),
                    layout,
                    len: run.len,
                },
            );
        }
        let Some(shape) = shapes.peek(&key) else {
            continue;
        };
        let layout = &shape.layout;
        let measured_width = layout.width.ceil().min(width);
        let align_offset = match label.align {
            Align::End => width - measured_width,
            Align::Center => (width - measured_width) / 2.,
            _ => px(0.),
        };
        let origin = window.pixel_snap_point(
            bounds.origin
                + gpui_kit::point(
                    label.x + pad + align_offset,
                    (row_height - line_height) / 2.,
                ),
        );
        let decorations = [gpui_kit::DecorationRun {
            len: shape.len as u32,
            color: style.color,
            background_color: style.background_color,
            underline: style.underline,
            strikethrough: style.strikethrough,
        }];
        if style.background_color.is_some() {
            let _ = layout.paint_background(
                origin,
                line_height,
                style.text_align,
                Some(measured_width),
                &decorations,
                window,
                cx,
            );
        }
        let _ = layout.paint(
            origin,
            line_height,
            style.text_align,
            Some(measured_width),
            &decorations,
            window,
            cx,
        );
    }
}

/// One recycled row view; shows whichever visible view row maps to its slot.
pub(crate) struct RowView {
    core: Weak<Core>,
    slot: usize,
    renders: u64,
    labels: Rc<RefCell<[Vec<CellLabel>; 3]>>,
}

impl Render for RowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let Some(core) = self.core.upgrade() else {
            return div().into_any_element();
        };
        let pal = core.palette(cx);
        let mut m = core.m.borrow_mut();
        m.stats.row_renders += 1;
        let Some(row) = m.view_row_of_slot(self.slot) else {
            return div().into_any_element();
        };
        let Some(src_row) = m.source_row(row) else {
            return div().into_any_element();
        };
        let w = m.grid_w();
        let row_h = m.row_h;
        let selected = m.sel.is_row_selected(row);
        let cursor = m.sel.cursor.filter(|c| c.row == row).map(|c| c.col);
        let rect_cols = m
            .sel
            .rect()
            .filter(|(r, _)| r.contains(&row))
            .map(|(_, c)| c);
        let zebra = m.opts.zebra && row % 2 == 1;
        let cell_mode = matches!(m.sel.mode, SelectionMode::Cell | SelectionMode::Range);
        let base = if selected {
            pal.selected
        } else if zebra {
            pal.zebra
        } else {
            pal.body
        };
        let vis_c = m.layout.visible_center(m.scroll_x, w);
        let lw = m.layout.left_width;
        let center_vp = m.layout.center_viewport(w);
        let rx = w - m.layout.right_width;
        let editing = m
            .edit
            .as_ref()
            .filter(|e| e.row == row)
            .map(|e| (e.def, e.input.clone(), e.error.clone()));
        let mut text = std::mem::take(&mut m.text);
        text.clear();
        let _ = write!(text, "Row {}", row + 1);
        let row_label = SharedString::from(text.as_str());
        let semantic_cells = window.is_a11y_active() || m.observe_cells;
        let mut cells = 0usize;
        let weak_core = self.core.clone();
        let mut cell_text = core.cell_text.borrow_mut();
        let mut cell = |p: &crate::columns::Placed,
                        x: f32,
                        m: &Model,
                        labels: &mut Vec<CellLabel>|
         -> Option<AnyElement> {
            cells += 1;
            let def = m.defs.get(p.def);
            let value = def.map_or(CellRef::Empty, |d| m.source.cell(src_row, d.source));
            text.clear();
            if let Some(d) = def {
                d.format_preview(value, &mut text);
            }
            let end = match def.map_or(Align::Auto, |d| d.align) {
                Align::Auto => value.is_numeric(),
                Align::End => true,
                _ => false,
            };
            let center = def.is_some_and(|d| d.align == Align::Center);
            let in_rect = rect_cols.as_ref().is_some_and(|c| c.contains(&p.display));
            let is_cursor = cursor == Some(p.display);
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            let hash = hasher.finish();
            let label = match cell_text.get(&hash) {
                Some(label) if label.as_ref() == text => label.clone(),
                _ => {
                    let label = SharedString::from(text.as_str());
                    cell_text.put(hash, label.clone());
                    label
                }
            };
            let multiline = label.contains('\n');
            let editor = editing.as_ref().filter(|e| e.0 == p.def);
            let painted_cell = !semantic_cells && !is_cursor && editor.is_none() && !multiline;
            if editor.is_none()
                && !multiline
                && (!label.is_empty() || painted_cell && in_rect && cell_mode)
            {
                labels.push(CellLabel {
                    text: label.clone(),
                    hash,
                    x: px(x),
                    width: px(p.width),
                    align: if end {
                        Align::End
                    } else if center {
                        Align::Center
                    } else {
                        Align::Start
                    },
                    background: (painted_cell && in_rect && cell_mode).then(|| hsla(pal.selected)),
                });
            }
            if painted_cell {
                return None;
            }

            // The cell cursor (selected or editing): a 2px outline over the gridlines. It
            // extends 1px sideways over the column separators; vertically it stays inside the
            // row (rows clip) and covers the row's own bottom gridline, so all four edges are
            // equally thick and nothing doubles.
            let cursor_box = |color: Hsla| {
                div()
                    .absolute()
                    .top(px(0.))
                    .bottom(px(0.))
                    .left(px(-1.))
                    .right(px(-1.))
                    .border_2()
                    .border_color(color)
            };
            let el = div()
                .id(("cell", p.display))
                .test_support()
                .role(Role::GridCell)
                .aria_label(label.clone())
                .aria_column_index(p.display + 1)
                .aria_selected(selected || in_rect)
                .absolute()
                .top_0()
                .left(px(x))
                .w(px(p.width))
                .h(px(row_h))
                .when(in_rect && cell_mode, |el| el.bg(hsla(pal.selected)));
            if let Some((_, input, error)) = editor {
                let core = weak_core.clone();
                let editor = el
                    .px(px(m.pad))
                    .flex()
                    .items_center()
                    .when(end, |el| el.justify_end())
                    .when(center, |el| el.justify_center())
                    .child(
                        div()
                            .id("editor")
                            .test_support()
                            .aria_label(SharedString::from(format!(
                                "Edit {}",
                                def.map(|d| d.title.as_ref()).unwrap_or("")
                            )))
                            .size_full()
                            .flex()
                            .items_center()
                            // Kit's Input propagates Enter/Escape after handling them:
                            // Enter commits and moves down, Shift+Enter up.
                            .on_action({
                                let core = core.clone();
                                move |a: &InputEnter, window, cx| {
                                    if let Some(core) = core.upgrade() {
                                        let adv = if a.shift { Advance::Up } else { Advance::Down };
                                        core.commit_and_move(adv, window, cx);
                                    }
                                }
                            })
                            // Tab/arrows are taken before the Input sees them (capture):
                            // Tab/Shift+Tab and Up/Down always commit and move; Left/Right
                            // only with the caret at that edge of the text.
                            .capture_action({
                                let core = core.clone();
                                move |_: &InputTab, window, cx| {
                                    if let Some(core) = core.upgrade() {
                                        cx.stop_propagation();
                                        core.commit_and_move(Advance::Right, window, cx);
                                    }
                                }
                            })
                            .capture_action({
                                let core = core.clone();
                                move |_: &InputShiftTab, window, cx| {
                                    if let Some(core) = core.upgrade() {
                                        cx.stop_propagation();
                                        core.commit_and_move(Advance::Left, window, cx);
                                    }
                                }
                            })
                            .capture_action({
                                let core = core.clone();
                                move |_: &InputUp, window, cx| {
                                    if let Some(core) = core.upgrade() {
                                        cx.stop_propagation();
                                        core.commit_and_move(Advance::Up, window, cx);
                                    }
                                }
                            })
                            .capture_action({
                                let core = core.clone();
                                move |_: &InputDown, window, cx| {
                                    if let Some(core) = core.upgrade() {
                                        cx.stop_propagation();
                                        core.commit_and_move(Advance::Down, window, cx);
                                    }
                                }
                            })
                            .capture_action({
                                let core = core.clone();
                                move |_: &InputLeft, window, cx| {
                                    if let Some(core) = core.upgrade()
                                        && core.editor_edge(Advance::Left, window, cx)
                                    {
                                        cx.stop_propagation();
                                    }
                                }
                            })
                            .capture_action({
                                let core = core.clone();
                                move |_: &InputRight, window, cx| {
                                    if let Some(core) = core.upgrade()
                                        && core.editor_edge(Advance::Right, window, cx)
                                    {
                                        cx.stop_propagation();
                                    }
                                }
                            })
                            .on_action(move |_: &InputEscape, window, cx| {
                                if let Some(core) = core.upgrade() {
                                    core.cancel_edit(window, cx);
                                }
                            })
                            .child(Input::new(input).appearance(false).h_full().w_full().p_0()),
                    )
                    .child(cursor_box(hsla(if error.is_some() {
                        pal.error
                    } else {
                        pal.accent
                    })))
                    .when_some(error.clone(), |el, e| el.aria_description(e))
                    .into_any_element();
                return Some(editor);
            }
            let el = el.when(multiline, |el| {
                el.px(px(m.pad))
                    .flex()
                    .items_center()
                    .when(end, |el| el.justify_end())
                    .when(center, |el| el.justify_center())
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label)
            });
            Some(
                el.when(is_cursor && cell_mode, |el| {
                    el.child(cursor_box(hsla(pal.accent)))
                })
                .into_any_element(),
            )
        };
        let pad = px(m.pad);
        // Independent bands preserve clipping at the pinned column boundaries.
        let text_band = |band: usize| {
            let labels = self.labels.clone();
            let shapes = core.shapes.clone();
            canvas(
                |_, _, _| {},
                move |bounds, (), window, cx| {
                    paint_cell_labels(
                        &labels.borrow()[band],
                        &mut shapes.borrow_mut(),
                        bounds,
                        pad,
                        px(row_h),
                        window,
                        cx,
                    );
                },
            )
            .absolute()
            .size_full()
        };
        let mut bands = self.labels.borrow_mut();
        let [center_labels, left_labels, right_labels] = &mut *bands;
        center_labels.clear();
        left_labels.clear();
        right_labels.clear();
        let mut center = div()
            .absolute()
            .top_0()
            .left(px(lw))
            .w(px(center_vp))
            .h_full()
            .overflow_hidden();
        for p in m.layout.center.get(vis_c).into_iter().flatten() {
            center = center.children(cell(p, (p.x - m.scroll_x) as f32, &m, center_labels));
        }
        let mut el = div()
            .id(("row", self.slot))
            .test_support()
            .role(Role::Row)
            .aria_row_index(aria_row_index(row))
            .aria_label(row_label)
            .aria_selected(selected)
            .relative()
            .size_full()
            .bg(hsla(base))
            .border_b_1()
            .border_color(hsla(pal.border))
            .when(!selected, |el| el.hover(move |s| s.bg(hsla(pal.hover))))
            .child(center.child(text_band(0)));
        if !m.layout.left.is_empty() {
            let mut band = div()
                .absolute()
                .top_0()
                .left_0()
                .w(px(lw))
                .h_full()
                .bg(hsla(base))
                .border_r_1()
                .border_color(hsla(pal.border));
            for p in &m.layout.left {
                band = band.children(cell(p, p.x as f32, &m, left_labels));
            }
            el = el.child(band.child(text_band(1)));
        }
        if !m.layout.right.is_empty() {
            let mut band = div()
                .absolute()
                .top_0()
                .left(px(rx))
                .w(px(m.layout.right_width))
                .h_full()
                .bg(hsla(base))
                .border_l_1()
                .border_color(hsla(pal.border));
            for p in &m.layout.right {
                band = band.children(cell(p, p.x as f32, &m, right_labels));
            }
            el = el.child(band.child(text_band(2)));
        }
        if selected {
            // Shape cue besides color (API conventions §12.1): an accent bar, overlaid.
            el = el.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(3.))
                    .h_full()
                    .bg(hsla(pal.accent)),
            );
        }
        m.text = text;
        m.stats.cells_built += cells as u64;
        m.stats.visible_cells = cells;
        let _ = window;
        el.into_any_element()
    }
}

/// The element: renders a [`GridHandle`]'s retained view. Rebuilding it is free.
#[derive(IntoElement)]
pub struct DataGrid {
    handle: GridHandle,
    style: StyleRefinement,
}

impl DataGrid {
    pub fn new(handle: &GridHandle) -> Self {
        Self {
            handle: handle.clone(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for DataGrid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DataGrid {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut el = div().size_full().min_h(px(0.));
        gpui_kit::Refineable::refine(el.style(), &self.style);
        el.child(self.handle.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_bounds() {
        let (t, h) = scrollbar_thumb(0., 36e6, 700.);
        assert_eq!(t, 0.);
        assert_eq!(h, 24.);
        let (t, h) = scrollbar_thumb(36e6 - 700., 36e6, 700.);
        assert!((t + h - 700.).abs() < 1e-6);
        assert_eq!(scrollbar_thumb(0., 100., 700.), (0., 700.));
    }

    #[test]
    fn density_changes_row_height() {
        let c = metrics(Size::Sm, Density::Compact).row_h;
        let n = metrics(Size::Sm, Density::Comfortable).row_h;
        let s = metrics(Size::Sm, Density::Spacious).row_h;
        assert!(c < n && n < s);
        assert!(metrics(Size::Xs, Density::Comfortable).row_h < n);
    }
}
