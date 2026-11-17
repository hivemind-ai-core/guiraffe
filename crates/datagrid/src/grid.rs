//! The GPUI control: [`GridHandle`] (state + API), [`DataGrid`] (the element) and the
//! retained views behind it.
//!
//! Render isolation (spec/00 §6): the grid view and a pool of row views read shared state
//! from an `Rc<Core>`; every mutation notifies exactly the views whose output changed with
//! `cx.notify(entity_id)`, never through an `update` on an ancestor. Rows live in a recycled
//! pool keyed by `view_row % pool`, so scrolling by one row re-renders one row view, selecting
//! a row re-renders that row, and editing a cell re-renders only its row. The host that
//! embeds [`DataGrid`] is never invalidated by the grid.

use std::{
    cell::RefCell,
    ops::Range,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
};

use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClipboardItem, Context, Entity,
    EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render,
    RenderOnce, Role, ScrollEvent, SharedString, StatefulInteractiveElement, StyleRefinement,
    Styled, TestSupportExt, Window, canvas,
    component::input::{Enter as InputEnter, Escape as InputEscape, Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use guiraffe_core::{
    ColorName, Density, GuiraffeTheme, Rgb, Size, StateKind, StyleContext, Variant,
};

use crate::{
    columns::{Align, ColumnDef, ColumnLayout, Pin, reorder},
    order::{Filter, FilterSpec, RowOrder, SortDir, SortKey, SortSpec, compute_order, toggle_sort},
    selection::{CellPos, Nav, Region, Selection, SelectionMode},
    source::{CellRef, EditError, GridSource, RowKey},
    tsv::write_tsv,
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
        Activate
    ]
);

/// Register key bindings (idempotent). Bindings are scoped to [`CONTEXT`]; apps may override.
pub fn init(cx: &mut App) {
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
    /// (spec/00 §12.3). A pointer click never takes focus unless this resolves to true.
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
            overscan: 3,
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
    pub row_renders: u64,
    pub cells_built: u64,
    pub visible_rows: usize,
    pub visible_cells: usize,
    pub pool: usize,
    pub order_jobs: u64,
    pub stale_jobs: u64,
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
    stats: GridStats,
    text: String,
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
    rows: RefCell<Vec<Entity<RowView>>>,
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
            stats: GridStats::default(),
            text: String::with_capacity(64),
        };
        model.resolve_metrics();
        let core = Rc::new(Core {
            m: RefCell::new(model),
            focus: cx.focus_handle(),
            events: cx.new(|_| GridEvents),
            view: RefCell::new(None),
            rows: RefCell::new(Vec::new()),
        });
        let view = cx.new(|_| GridView { core: core.clone() });
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

    /// Replace all column definitions (resets order, sort and filters).
    pub fn set_columns(&self, defs: Vec<ColumnDef>, cx: &mut App) {
        self.core.drop_edit(cx);
        {
            let mut m = self.core.m.borrow_mut();
            m.order = (0..defs.len()).collect();
            m.defs = defs;
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
            self.core.relayout(cx);
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
        }
        self.core.refresh_order(cx);
    }

    /// Mutate the source in place (needs exclusive ownership: no background job and no
    /// outside `Arc` clone). Returns `None` when shared. Call
    /// [`GridHandle::rows_changed`] or [`GridHandle::data_changed`] afterwards.
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
    pub fn data_changed(&self, cx: &mut App) {
        self.core.refresh_order(cx);
    }

    /// Open the editor on view row `row`, display column `col` (if editable).
    pub fn begin_edit(&self, row: usize, col: usize, window: &mut Window, cx: &mut App) -> bool {
        self.core.begin_edit(CellPos::new(row, col), window, cx)
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

    fn notify_slot(&self, slot: usize, cx: &mut App) {
        let id = self.rows.borrow().get(slot).map(|e| e.entity_id());
        if let Some(id) = id {
            cx.notify(id);
        }
    }

    /// Recompute the visible window and pool; notify the grid and every row view whose row
    /// changed (all of them when `all`).
    fn sync(self: &Rc<Self>, all: bool, cx: &mut App) {
        let (dirty, pool): (Vec<usize>, usize) = {
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
            let dirty = if all {
                (0..pool).collect()
            } else {
                vw::entering(old, vis)
                    .map(|r| vw::slot_of(r, pool))
                    .collect()
            };
            (dirty, pool)
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
                }));
            }
        }
        self.notify_grid(cx);
        for s in dirty {
            self.notify_slot(s, cx);
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

    fn relayout(self: &Rc<Self>, cx: &mut App) {
        let (sel_changed, hidden_edit) = {
            let mut m = self.m.borrow_mut();
            m.layout = ColumnLayout::compute(&m.defs, &m.order);
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
            let r = compute_order(&*src, &filters, &sort, &cancel);
            drop(src);
            self.apply_order(job, r, cx);
            return;
        }
        self.m.borrow_mut().busy = true;
        self.notify_grid(cx);
        let core = self.clone();
        cx.spawn(async move |cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    // `src` is dropped here, before the result reaches the UI thread, so a
                    // queued commit can take the source back.
                    compute_order(&*src, &filters, &sort, &cancel)
                })
                .await;
            cx.update(|cx| core.apply_order(job, r, cx));
        })
        .detach();
    }

    /// Install a new row order. Selection and an open editor follow their source rows; an
    /// editor whose row was filtered out is cancelled (with an event and refocus).
    fn apply_order(
        self: &Rc<Self>,
        job: u64,
        r: Result<RowOrder, crate::order::Cancelled>,
        cx: &mut App,
    ) {
        let (sel_changed, edit_gone) = {
            let mut m = self.m.borrow_mut();
            let Ok(order) = r else {
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
                let inv = m.rows.inverse(n);
                let map = |v: usize| old.source_row(v, n).and_then(|s| inv.view_row(s));
                m.sel = m.sel.remap(map);
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
            let src_col = m.defs.get(p.def).map(|d| d.source).unwrap_or(usize::MAX);
            let r = match Arc::get_mut(&mut m.source) {
                Some(src) => src
                    .set_cell(p.source_row, src_col, &p.value)
                    .map_err(|e| SharedString::from(e.to_string())),
                None => Err(SharedString::from(EditError::Busy.to_string())),
            };
            (p, r)
        };
        let (p, r) = result;
        match r {
            Ok(()) => {
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
        let dirty = {
            let mut m = self.m.borrow_mut();
            let before = m.sel.clone();
            let mut sel = std::mem::take(&mut m.sel);
            f(&mut sel, &m);
            m.sel = sel;
            if m.sel == before {
                return;
            }
            let dirty: Vec<usize> = m
                .visible
                .clone()
                .filter(|&r| Model::row_sig(&before, r) != Model::row_sig(&m.sel, r))
                .map(|r| vw::slot_of(r, m.pool))
                .collect();
            dirty
        };
        for s in dirty {
            self.notify_slot(s, cx);
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
        if !self.begin_edit(c, window, cx) {
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

    fn begin_edit(self: &Rc<Self>, pos: CellPos, window: &mut Window, cx: &mut App) -> bool {
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
            let mut text = String::new();
            def.format(m.source.cell(source_row, def.source), &mut text);
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
            let src_col = m.defs.get(def).map(|d| d.source).unwrap_or(usize::MAX);
            let veto = m.validator.as_ref().and_then(|v| {
                v(&EditRequest {
                    row: source_row,
                    key,
                    column: def,
                    value: &value,
                })
                .err()
            });
            let job_running = m.busy || m.copying > 0;
            match veto {
                Some(reason) => Outcome::Rejected(reason),
                None => match Arc::get_mut(&mut m.source) {
                    Some(src) => match src.set_cell(source_row, src_col, &value) {
                        Ok(()) => Outcome::Done(key),
                        Err(e) => Outcome::Rejected(e.to_string().into()),
                    },
                    // A background sort/copy holds the source: apply when it lands.
                    None if job_running => {
                        m.pending = Some(PendingCommit {
                            source_row,
                            def,
                            key,
                            value: value.clone(),
                        });
                        Outcome::Queued
                    }
                    None => Outcome::Rejected(EditError::Busy.to_string().into()),
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
            // A pointer press never takes keyboard focus (spec/00 §12.3).
            window.prevent_default();
        }
        let _ = bounds;
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
        if ev.click_count >= 2 {
            if !self.begin_edit(pos, window, cx) {
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
                    self.relayout(cx);
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
            Drag::Select => {
                let pos = {
                    let m = self.m.borrow();
                    let row = vw::row_at(
                        f64::from(y - m.row_h).max(0.),
                        m.scroll_y,
                        f64::from(m.row_h),
                        m.count,
                    );
                    let col = m
                        .layout
                        .hit(x.clamp(0., m.grid_w() - 1.), m.scroll_x, m.grid_w());
                    row.zip(col).map(|(r, c)| CellPos::new(r, c))
                };
                if let Some(pos) = pos {
                    self.change_selection(cx, |s, _| s.click(pos, true, false));
                }
            }
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
        }
    }

    fn mouse_up(self: &Rc<Self>, cx: &mut App) {
        let Some(drag) = self.m.borrow_mut().drag.take() else {
            return;
        };
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
            Drag::Select | Drag::VScroll { .. } => {}
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

/// Colors resolved once per render from theme tokens.
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

fn palette(cx: &App) -> Palette {
    let t = GuiraffeTheme::global(cx);
    let s = t.surfaces();
    let light = t.variant(None, Variant::Light);
    Palette {
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

/// The grid's root view: header, row wrappers, scrollbar, pointer and keyboard handling.
pub(crate) struct GridView {
    core: Rc<Core>,
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
        let pal = palette(cx);
        let tab = core.tab_stop();
        let focused = core.focus.is_focused(window);
        let m = core.m.borrow();
        let row_h = m.row_h;
        let w = m.grid_w();
        let rows = core.rows.borrow();
        let row_f = f64::from(row_h);

        // Header cells: visible center columns plus pinned bands.
        let mut header = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(row_h))
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
        for p in m.layout.center.get(vis_c.clone()).into_iter().flatten() {
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
                    c_edit.begin_edit(c, window, cx);
                }
            })
            .on_action(move |_: &Activate, window, cx| c_act.activate(window, cx))
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
                        let changed = {
                            let mut m = c_record.m.borrow_mut();
                            let changed = m.bounds.size != bounds.size;
                            m.bounds = bounds;
                            changed
                        };
                        if changed {
                            let c = c_record.clone();
                            cx.defer(move |cx| c.sync(true, cx));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(body)
            .child(header)
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

/// One recycled row view; shows whichever visible view row maps to its slot.
pub(crate) struct RowView {
    core: Weak<Core>,
    slot: usize,
    renders: u64,
}

impl Render for RowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let Some(core) = self.core.upgrade() else {
            return div().into_any_element();
        };
        let pal = palette(cx);
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
        let mut cells = 0usize;
        let weak_core = self.core.clone();
        let mut cell = |p: &crate::columns::Placed, x: f32, m: &Model| -> AnyElement {
            cells += 1;
            let def = m.defs.get(p.def);
            let value = def.map_or(CellRef::Empty, |d| m.source.cell(src_row, d.source));
            text.clear();
            if let Some(d) = def {
                d.format(value, &mut text);
            }
            let end = match def.map_or(Align::Auto, |d| d.align) {
                Align::Auto => value.is_numeric(),
                Align::End => true,
                _ => false,
            };
            let center = def.is_some_and(|d| d.align == Align::Center);
            let in_rect = rect_cols.as_ref().is_some_and(|c| c.contains(&p.display));
            let is_cursor = cursor == Some(p.display);
            let label = SharedString::from(text.clone());
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
                .px(px(m.pad))
                .flex()
                .items_center()
                .overflow_hidden()
                .when(end, |el| el.justify_end())
                .when(center, |el| el.justify_center())
                .when(in_rect && cell_mode, |el| el.bg(hsla(pal.selected)));
            if let Some((col, input, error)) = editing.as_ref().filter(|e| e.0 == p.def) {
                let _ = col;
                let core = weak_core.clone();
                return el
                    .px(px(2.))
                    .child(
                        div()
                            .id("editor")
                            .size_full()
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(hsla(if error.is_some() { pal.error } else { pal.ring }))
                            // Kit's Input propagates Enter/Escape after handling them.
                            .on_action({
                                let core = core.clone();
                                move |_: &InputEnter, window, cx| {
                                    if let Some(core) = core.upgrade() {
                                        core.commit_edit(window, cx);
                                    }
                                }
                            })
                            .on_action(move |_: &InputEscape, window, cx| {
                                if let Some(core) = core.upgrade() {
                                    core.cancel_edit(window, cx);
                                }
                            })
                            .child(Input::new(input).appearance(false).h_full()),
                    )
                    .when_some(error.clone(), |el, e| el.aria_description(e))
                    .into_any_element();
            }
            el.child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .when(is_cursor && cell_mode, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_2()
                        .border_color(hsla(pal.accent)),
                )
            })
            .into_any_element()
        };
        let mut center = div()
            .absolute()
            .top_0()
            .left(px(lw))
            .w(px(center_vp))
            .h_full()
            .overflow_hidden();
        for p in m.layout.center.get(vis_c).into_iter().flatten() {
            center = center.child(cell(p, (p.x - m.scroll_x) as f32, &m));
        }
        let mut el = div()
            .id(("row", self.slot))
            .test_support()
            .role(Role::Row)
            .aria_row_index(aria_row_index(row))
            .aria_label(SharedString::from(format!("Row {}", row + 1)))
            .aria_selected(selected)
            .relative()
            .size_full()
            .bg(hsla(base))
            .border_b_1()
            .border_color(hsla(pal.border))
            .when(!selected, |el| el.hover(move |s| s.bg(hsla(pal.hover))))
            .child(center);
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
                band = band.child(cell(p, p.x as f32, &m));
            }
            el = el.child(band);
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
                band = band.child(cell(p, p.x as f32 - 0., &m));
            }
            el = el.child(band);
        }
        if selected {
            // Shape cue besides color (spec/00 §12.1): an accent bar, overlaid.
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
