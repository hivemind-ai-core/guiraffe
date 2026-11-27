//! StepGrid: a fixed-cell matrix editor (step sequencer, pad grid).
//!
//! Rows × columns of fixed-size cells, painted in one pass by one element: cell state never
//! changes layout. Row headers are host slots aligned to the rows, in their own view, so a cell
//! edit doesn't render them. The current-column highlight is fed like a meter
//! ([`StepPlayheadFeed`]): the host writes a column and publishes, and only the highlight
//! repaints. The model ([`StepDoc`]) is store state keyed by [`RowId`]; edits are
//! [`StepAction`]s on the action bus, and a drag-paint commits one `paint_cells` in a
//! `dispatch_batch`, so it is one undo step.

use std::{cell::Cell, hash::Hasher, rc::Rc};

use gpui_kit::{
    AnyElement, App, AppContext, Bounds, Context, Element, ElementId, Entity, EntityId,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, InspectorElementId,
    InteractiveElement, IntoElement, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, SharedString, Style,
    Styled, WeakEntity, Window, div, point, px, relative, size,
};
use guiraffe_core::{
    ColorName, FocusIndicator, FocusPolicy, GuiraffeTheme, ResolvedFocus, Variant,
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source,
    },
};
use guiraffe_state::{Selected, Store};

use crate::step_model::ApplyStepAction;
pub use crate::step_model::{
    RowId, SetRows, StepAction, StepCell, StepChange, StepDoc, StepRow, step_store,
};

/// How a cell's level reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LevelStyle {
    /// The fill rises with the level.
    #[default]
    Height,
    /// The fill gets brighter with the level.
    Brightness,
}

/// Emitted per applied user action, and when the user asks to inspect a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepGridEvent {
    Changed(StepChange),
    /// Right-click, or Enter on the cursor: the host opens its inspector at `position`.
    Inspect {
        row: RowId,
        column: usize,
        position: Point<Pixels>,
    },
}

/// One row as a header renderer sees it.
pub struct StepRowHeader {
    pub index: usize,
    pub id: RowId,
    pub name: SharedString,
    pub color: ColorName,
    pub length: usize,
    pub muted: bool,
    pub soloed: bool,
}

/// Renders one row header; the grid sizes and aligns it.
pub type StepHeaderRenderer = Rc<dyn Fn(&StepRowHeader, &mut Window, &mut App) -> AnyElement>;

/// Geometry and look.
#[derive(Clone)]
pub struct StepGridOptions {
    /// Cell side and gap in pixels.
    pub cell: f32,
    pub gap: f32,
    pub header_width: f32,
    pub level_style: LevelStyle,
    /// `None`: the built-in header (colour chip, name, length).
    pub header: Option<StepHeaderRenderer>,
    /// How the grid takes focus, over the app default ([`guiraffe_core::set_app_focus_policy`]).
    /// Built in: a press focuses, a Tab stop only inside a `focus_scope`, the keyboard cursor
    /// shows whenever focused. Pointer edits and bus actions work with pointer focus off.
    pub focus: FocusPolicy,
}

impl Default for StepGridOptions {
    fn default() -> Self {
        Self {
            cell: 22.0,
            gap: 2.0,
            header_width: 120.0,
            level_style: LevelStyle::Height,
            header: None,
            focus: FocusPolicy::INHERIT,
        }
    }
}

/// Pure geometry: the cell under grid-local `(x, y)`.
pub fn cell_at(
    (x, y): (f32, f32),
    cell: f32,
    gap: f32,
    rows: usize,
    columns: usize,
) -> Option<(usize, usize)> {
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let step = cell + gap;
    let (c, r) = ((x / step) as usize, (y / step) as usize);
    (r < rows && c < columns && x - c as f32 * step <= cell && y - r as f32 * step <= cell)
        .then_some((r, c))
}

/// The host's handle for the current-column highlight.
#[derive(Clone)]
pub struct StepPlayheadFeed {
    column: Rc<Cell<Option<usize>>>,
    view: EntityId,
}

impl StepPlayheadFeed {
    /// Set the current column (`None` hides the highlight).
    pub fn set(&self, column: Option<usize>) {
        self.column.set(column);
    }

    /// Repaint the highlight, and only the highlight.
    pub fn publish(&self, cx: &mut App) {
        cx.notify(self.view);
    }
}

/// The highlight view: one translucent column over the ruler and the cells.
struct StepPlayhead {
    column: Rc<Cell<Option<usize>>>,
    cell: f32,
    gap: f32,
    color: Option<(GuiraffeTheme, Hsla)>,
}

impl Render for StepPlayhead {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let color = match &self.color {
            Some((seen, c)) if seen == theme => *c,
            _ => {
                let mut c = theme.variant(None, Variant::Filled).background.to_hsla();
                c.a = 0.22;
                self.color = Some((theme.clone(), c));
                c
            }
        };
        HighlightElement {
            column: self.column.get(),
            cell: self.cell,
            gap: self.gap,
            color,
        }
    }
}

struct HighlightElement {
    column: Option<usize>,
    cell: f32,
    gap: f32,
    color: Hsla,
}

/// A fill-the-parent element with a paint function; arena-held, so rendering it allocates
/// nothing on the heap.
macro_rules! paint_element {
    ($ty:ty, |$this:ident, $bounds:ident, $window:ident| $body:block) => {
        impl IntoElement for $ty {
            type Element = Self;
            fn into_element(self) -> Self {
                self
            }
        }
        impl Element for $ty {
            type RequestLayoutState = ();
            type PrepaintState = ();
            fn id(&self) -> Option<ElementId> {
                None
            }
            fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
                None
            }
            fn request_layout(
                &mut self,
                _: Option<&GlobalElementId>,
                _: Option<&InspectorElementId>,
                window: &mut Window,
                cx: &mut App,
            ) -> (LayoutId, ()) {
                let style = Style {
                    size: size(relative(1.).into(), relative(1.).into()),
                    ..Style::default()
                };
                (window.request_layout(style, [], cx), ())
            }
            fn prepaint(
                &mut self,
                _: Option<&GlobalElementId>,
                _: Option<&InspectorElementId>,
                _: Bounds<Pixels>,
                _: &mut (),
                _: &mut Window,
                _: &mut App,
            ) {
            }
            fn paint(
                &mut self,
                _: Option<&GlobalElementId>,
                _: Option<&InspectorElementId>,
                $bounds: Bounds<Pixels>,
                _: &mut (),
                _: &mut (),
                $window: &mut Window,
                _: &mut App,
            ) {
                let $this = &*self;
                $body
            }
        }
    };
}

paint_element!(HighlightElement, |this, bounds, window| {
    if let Some(column) = this.column {
        let x = column as f32 * (this.cell + this.gap);
        window.paint_quad(gpui_kit::fill(
            Bounds::new(
                bounds.origin + point(px(x - this.gap / 2.0), px(0.)),
                size(px(this.cell + this.gap), bounds.size.height),
            ),
            this.color,
        ));
    }
});

/// Theme colours for the cells, resolved once per theme.
#[derive(Clone, Copy)]
struct CellColors {
    empty: Hsla,
    band: Hsla,
    bar: Hsla,
    flag: Hsla,
    cursor: Hsla,
}

/// Every cell in one element.
struct CellsElement {
    doc: StepDoc,
    colors: CellColors,
    /// A row's ink, by row index (rows are few; reused buffer).
    inks: Rc<std::cell::RefCell<Vec<Hsla>>>,
    cell: f32,
    gap: f32,
    level_style: LevelStyle,
    preview: Option<StepAction>,
    cursor: Option<(usize, usize)>,
}

impl CellsElement {
    /// The cell as the drag preview would leave it.
    fn shown(&self, row: &StepRow, r: usize, c: usize) -> StepCell {
        let mut cell = row.cell(c);
        match self.preview {
            Some(StepAction::PaintCells {
                row: id,
                from,
                to,
                on,
            }) if id == row.id && c >= from.min(to) && c <= from.max(to) && c < row.length => {
                cell.on = on;
            }
            Some(StepAction::SetCellLevel {
                row: id,
                column,
                level,
            }) if id == row.id && column == c => {
                cell.on = true;
                cell.level = level;
            }
            _ => {}
        }
        let _ = r;
        cell
    }
}

paint_element!(CellsElement, |this, bounds, window| {
    let step = this.cell + this.gap;
    let quad = |x: f32, y: f32, w: f32, h: f32, color: Hsla, window: &mut Window| {
        window.paint_quad(gpui_kit::fill(
            Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h))),
            color,
        ));
    };
    let inks = this.inks.borrow();
    let group = this.doc.group;
    for (r, row) in this.doc.rows().iter().enumerate() {
        let y = r as f32 * step;
        let ink = inks.get(r).copied().unwrap_or(this.colors.flag);
        for c in 0..this.doc.columns {
            let x = c as f32 * step;
            let banded = group > 0 && (c / group) % 2 == 1;
            quad(
                x,
                y,
                this.cell,
                this.cell,
                if banded {
                    this.colors.band
                } else {
                    this.colors.empty
                },
                window,
            );
            let cell = this.shown(row, r, c);
            let repeat = row.is_repeat(c);
            let dim = if repeat { 0.35 } else { 1.0 };
            if cell.on {
                let mut fill = ink;
                fill.a *= dim * (0.35 + 0.65 * cell.probability);
                match this.level_style {
                    LevelStyle::Height => {
                        let h = (this.cell * cell.level).max(3.0);
                        quad(x, y + this.cell - h, this.cell, h, fill, window);
                    }
                    LevelStyle::Brightness => {
                        fill.a *= 0.3 + 0.7 * cell.level;
                        quad(x, y, this.cell, this.cell, fill, window);
                    }
                }
                // Probability below 1: a dotted outline.
                if cell.probability < 1.0 {
                    let mut dot = ink;
                    dot.a *= dim;
                    let n = 4;
                    for i in 0..n {
                        let t = (i as f32 + 0.5) / n as f32 * this.cell - 1.0;
                        quad(x + t, y, 2.0, 2.0, dot, window);
                        quad(x + t, y + this.cell - 2.0, 2.0, 2.0, dot, window);
                        quad(x, y + t, 2.0, 2.0, dot, window);
                        quad(x + this.cell - 2.0, y + t, 2.0, 2.0, dot, window);
                    }
                }
                // Ratchet: stacked ticks along the left edge.
                if cell.ratchet > 1 {
                    let mut tick = this.colors.empty;
                    tick.a = dim;
                    let n = f32::from(cell.ratchet.min(8));
                    let pitch = (this.cell - 4.0) / n;
                    for i in 0..cell.ratchet.min(8) {
                        quad(
                            x + 2.0,
                            y + 2.0 + f32::from(i) * pitch,
                            4.0,
                            (pitch - 1.0).max(1.0),
                            tick,
                            window,
                        );
                    }
                }
                if cell.flag {
                    let mut flag = this.colors.flag;
                    flag.a *= dim;
                    quad(x + this.cell - 6.0, y, 6.0, 6.0, flag, window);
                }
            }
            if this.cursor == Some((r, c)) {
                let t = 2.0;
                let k = this.colors.cursor;
                quad(x - t, y - t, this.cell + 2.0 * t, t, k, window);
                quad(x - t, y + this.cell, this.cell + 2.0 * t, t, k, window);
                quad(x - t, y, t, this.cell, k, window);
                quad(x + this.cell, y, t, this.cell, k, window);
            }
        }
    }
    // Bar lines at group boundaries.
    if group > 0 {
        let rows = this.doc.rows().len() as f32;
        let mut c = group;
        while c < this.doc.columns {
            let x = c as f32 * step - this.gap / 2.0 - 0.5;
            quad(x, 0.0, 1.0, rows * step - this.gap, this.colors.bar, window);
            c += group;
        }
    }
});

/// The bar numbers over the columns: renders only when the column count or grouping changes.
struct StepRuler {
    columns: usize,
    group: usize,
    step: f32,
}

impl Render for StepRuler {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dim = GuiraffeTheme::global(cx).surfaces().dimmed.to_hsla();
        let group = self.group.max(1);
        div()
            .relative()
            .size_full()
            .text_color(dim)
            .children((0..self.columns).step_by(group).map(|c| {
                div()
                    .absolute()
                    .left(px(c as f32 * self.step))
                    .child(SharedString::from((c / group + 1).to_string()))
            }))
    }
}

/// The header column: re-renders only when names, colours, lengths or flags change.
struct StepHeaders {
    doc: StepDoc,
    options: Rc<StepGridOptions>,
}

impl Render for StepHeaders {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let step = self.options.cell + self.options.gap;
        let theme = GuiraffeTheme::global(cx).clone();
        let mut cells = Vec::with_capacity(self.doc.rows().len());
        for (index, row) in self.doc.rows().iter().enumerate() {
            let info = StepRowHeader {
                index,
                id: row.id,
                name: row.name.clone(),
                color: row.color,
                length: row.length,
                muted: row.muted,
                soloed: row.soloed,
            };
            let content = match &self.options.header {
                Some(render) => render(&info, window, cx),
                None => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .size_full()
                    .child(
                        div()
                            .w(px(4.))
                            .h(px(self.options.cell * 0.6))
                            .rounded(px(2.))
                            .bg(theme
                                .variant(Some(row.color), Variant::Filled)
                                .background
                                .to_hsla()),
                    )
                    .child(div().flex_1().overflow_hidden().child(row.name.clone()))
                    .child(
                        div()
                            .text_color(theme.surfaces().dimmed.to_hsla())
                            .child(SharedString::from(row.length.to_string())),
                    )
                    .into_any_element(),
            };
            cells.push(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(index as f32 * step))
                    .h(px(self.options.cell))
                    .child(content),
            );
        }
        div()
            .relative()
            .w(px(self.options.header_width))
            .h(px(self.doc.rows().len() as f32 * step))
            .text_size(px(11.))
            .children(cells)
    }
}

fn header_key(doc: &StepDoc) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in doc.rows() {
        h.write_u64(r.id.0);
        h.write(r.name.as_bytes());
        h.write_u8(r.color as u8);
        h.write_usize(r.length);
        h.write_u8(u8::from(r.muted) | (u8::from(r.soloed) << 1));
    }
    h.finish()
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Paint {
        row: RowId,
        from: usize,
        to: usize,
        on: bool,
    },
    Level {
        row: RowId,
        column: usize,
        from_y: f32,
        from: f32,
        level: f32,
    },
}

/// The grid: rows × columns of fixed cells.
pub struct StepGrid {
    store: Store<StepDoc>,
    doc: Selected<StepDoc>,
    options: Rc<StepGridOptions>,
    headers: Entity<StepHeaders>,
    header_key: u64,
    playhead: Entity<StepPlayhead>,
    ruler: Entity<StepRuler>,
    feed: StepPlayheadFeed,
    colors: Option<(GuiraffeTheme, CellColors)>,
    inks: Rc<std::cell::RefCell<Vec<Hsla>>>,
    drag: Option<Drag>,
    cursor: (usize, usize),
    cells_origin: Rc<Cell<Point<Pixels>>>,
    focus: FocusHandle,
    focus_policy: FocusPolicy,
    id: Option<ControlId>,
}

impl EventEmitter<StepGridEvent> for StepGrid {}

impl Focusable for StepGrid {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Route an input as a bus client would send it (a drag commit as a one-action batch).
fn route(
    view: &WeakEntity<StepGrid>,
    action: StepAction,
    source: Source,
    batch: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = view.upgrade() else { return };
    match entity.read(cx).id {
        Some(id) => {
            let action = action.to_action();
            if batch {
                let _ = ActionBus::dispatch_batch(
                    id,
                    std::slice::from_ref(&action),
                    source,
                    window,
                    cx,
                );
            } else {
                ActionBus::dispatch(id, &action, source, window, cx).ok();
            }
        }
        None => {
            entity.update(cx, |grid, cx| grid.perform(action, cx).ok());
        }
    }
}

impl StepGrid {
    /// A grid over the host's store (see [`step_store`]).
    pub fn new(store: Store<StepDoc>, options: StepGridOptions, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let doc = store.select(cx, |d: &StepDoc| d.clone());
            let focus_policy = options.focus;
            let options = Rc::new(options);
            let first = doc.get(cx).clone();
            let headers = cx.new(|_| StepHeaders {
                doc: first.clone(),
                options: options.clone(),
            });
            let column = Rc::new(Cell::new(None));
            let playhead = cx.new(|_| StepPlayhead {
                column: column.clone(),
                cell: options.cell,
                gap: options.gap,
                color: None,
            });
            let feed = StepPlayheadFeed {
                column,
                view: playhead.entity_id(),
            };
            let ruler = cx.new(|_| StepRuler {
                columns: first.columns,
                group: first.group,
                step: options.cell + options.gap,
            });
            Self {
                store,
                doc,
                header_key: header_key(&first),
                headers,
                playhead,
                ruler,
                feed,
                options,
                colors: None,
                inks: Rc::default(),
                drag: None,
                cursor: (0, 0),
                cells_origin: Rc::default(),
                focus: cx.focus_handle(),
                focus_policy,
                id: None,
            }
        })
    }

    /// Register on the window's action bus at `path`.
    pub fn register(view: &Entity<Self>, path: &str, window: &Window, cx: &mut App) -> ControlId {
        let id = ActionBus::register(path, view, "step-grid", Capabilities::TOGGLE, window, cx);
        view.update(cx, |g, _| g.id = Some(id));
        id
    }

    pub fn store(&self) -> &Store<StepDoc> {
        &self.store
    }

    pub fn doc<'a>(&self, cx: &'a App) -> &'a StepDoc {
        self.doc.get(cx)
    }

    /// The current-column highlight's feed.
    /// Host write: the grid's focus policy. Needs no window.
    pub fn set_focus_policy(&mut self, policy: FocusPolicy, cx: &mut Context<Self>) {
        if policy != self.focus_policy {
            self.focus_policy = policy;
            cx.notify();
        }
    }

    /// The grid's own focus policy (before the app default).
    pub fn focus_policy(&self) -> FocusPolicy {
        self.focus_policy
    }

    /// The policy in effect now: the grid's, over the app default, over the built-in.
    pub fn resolved_focus(&self, cx: &App) -> ResolvedFocus {
        self.focus_policy.resolve_in(
            ResolvedFocus {
                pointer: true,
                tab: guiraffe_core::tab_stop(None),
                indicator: FocusIndicator::Focused,
            },
            cx,
        )
    }

    pub fn playhead(&self) -> StepPlayheadFeed {
        self.feed.clone()
    }

    /// The keyboard cursor as (row index, column).
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// Apply a user action (what the bus, the pointer and the keyboard all run). Emits one
    /// [`StepGridEvent::Changed`].
    pub fn perform(
        &mut self,
        action: StepAction,
        cx: &mut Context<Self>,
    ) -> Result<Option<StepChange>, ActionError> {
        let mut next = self.doc(cx).clone();
        let Some(change) = next.apply(action) else {
            return Ok(None);
        };
        self.store.dispatch_now(ApplyStepAction(action), cx);
        cx.emit(StepGridEvent::Changed(change));
        cx.notify();
        Ok(Some(change))
    }

    fn local_cell(&self, p: Point<Pixels>, cx: &App) -> Option<(usize, usize)> {
        let o = self.cells_origin.get();
        let doc = self.doc(cx);
        cell_at(
            (f32::from(p.x - o.x), f32::from(p.y - o.y)),
            self.options.cell,
            self.options.gap,
            doc.rows().len(),
            doc.columns,
        )
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((r, c)) = self.local_cell(e.position, cx) else {
            return;
        };
        let Some(row) = self.doc(cx).rows().get(r).map(|r| (**r).clone()) else {
            return;
        };
        self.cursor = (r, c);
        if row.is_repeat(c) {
            cx.notify();
            return;
        }
        let cell = row.cell(c);
        self.drag = Some(if e.modifiers.alt {
            Drag::Level {
                row: row.id,
                column: c,
                from_y: f32::from(e.position.y),
                from: if cell.on { cell.level } else { 0.0 },
                level: if cell.on { cell.level } else { 0.0 },
            }
        } else {
            Drag::Paint {
                row: row.id,
                from: c,
                to: c,
                on: !cell.on,
            }
        });
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if e.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let o = self.cells_origin.get();
        let step = self.options.cell + self.options.gap;
        let columns = self.doc(cx).columns;
        match &mut self.drag {
            Some(Drag::Paint { to, .. }) => {
                let x = f32::from(e.position.x - o.x);
                let c = ((x / step).max(0.0) as usize).min(columns.saturating_sub(1));
                if *to != c {
                    *to = c;
                    cx.notify();
                }
            }
            Some(Drag::Level {
                from_y,
                from,
                level,
                ..
            }) => {
                // One cell height of travel spans the full range.
                let next = (*from + (*from_y - f32::from(e.position.y)) / self.options.cell)
                    .clamp(0.0, 1.0);
                if next != *level {
                    *level = next;
                    cx.notify();
                }
            }
            None => {}
        }
    }

    /// The action a finished drag commits: a click toggles; a drag paints or sets a level.
    fn take_commit(&mut self) -> Option<StepAction> {
        match self.drag.take()? {
            Drag::Paint { row, from, to, .. } if from == to => {
                Some(StepAction::ToggleCell { row, column: from })
            }
            Drag::Paint { row, from, to, on } => Some(StepAction::PaintCells { row, from, to, on }),
            Drag::Level {
                row,
                column,
                level,
                from,
                ..
            } => (level != from).then_some(StepAction::SetCellLevel { row, column, level }),
        }
    }

    fn preview(&self) -> Option<StepAction> {
        match self.drag? {
            Drag::Paint { row, from, to, on } => Some(StepAction::PaintCells { row, from, to, on }),
            Drag::Level {
                row, column, level, ..
            } => Some(StepAction::SetCellLevel { row, column, level }),
        }
    }

    fn key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) -> Option<(StepAction, bool)> {
        let doc = self.doc(cx);
        let (rows, columns) = (doc.rows().len(), doc.columns);
        let (r, c) = self.cursor;
        let id = doc.rows().get(r).map(|row| row.id);
        match e.keystroke.key.as_str() {
            "left" => self.cursor.1 = c.saturating_sub(1),
            "right" => self.cursor.1 = (c + 1).min(columns.saturating_sub(1)),
            "up" => self.cursor.0 = r.saturating_sub(1),
            "down" => self.cursor.0 = (r + 1).min(rows.saturating_sub(1)),
            "space" => return id.map(|row| (StepAction::ToggleCell { row, column: c }, false)),
            "enter" => {
                if let Some(row) = id {
                    let step = self.options.cell + self.options.gap;
                    let o = self.cells_origin.get();
                    cx.emit(StepGridEvent::Inspect {
                        row,
                        column: c,
                        position: o + point(px(c as f32 * step), px(r as f32 * step)),
                    });
                }
                return None;
            }
            _ => return None,
        }
        cx.notify();
        None
    }
}

impl Observe for StepGrid {
    type Snapshot = ControlSnapshot;
    /// The grid's size and cursor, and a page of cells: the cursor's row, as levels (0 for
    /// an off cell). A client pages through rows by moving the cursor.
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        use guiraffe_core::widget_value::Value;
        let doc = self.doc(cx);
        let (r, c) = self.cursor;
        let page: Vec<Value> = doc
            .rows()
            .get(r)
            .map(|row| {
                (0..doc.columns)
                    .map(|col| {
                        let cell = row.cell(col);
                        Value::Num(if cell.on { f64::from(cell.level) } else { 0.0 })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let field = |k: &'static str, v: Value| (SharedString::new_static(k), v);
        let record = std::collections::BTreeMap::from([
            field("rows", Value::Num(doc.rows().len() as f64)),
            field("columns", Value::Num(doc.columns as f64)),
            field(
                "row",
                Value::Num(doc.rows().get(r).map_or(0.0, |row| row.id.0 as f64)),
            ),
            field("page", Value::List(page.into())),
        ]);
        ControlSnapshot {
            value: Some(Value::Record(std::sync::Arc::new(record))),
            selected: Some(Rc::from([r, c].as_slice())),
            ..Default::default()
        }
    }
}

/// The bus outcome of a [`StepAction`].
pub struct StepOutcome(pub Option<StepChange>);

impl From<StepOutcome> for Outcome {
    fn from(o: StepOutcome) -> Self {
        o.0.map_or(Outcome::EMPTY, |c| Outcome(c.into()))
    }
}

impl Drive for StepGrid {
    type Action = StepAction;
    type Output = StepOutcome;
    fn decode(action: &Action) -> Result<StepAction, ActionError> {
        StepAction::decode(action)
    }
    fn apply(
        &mut self,
        action: StepAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<StepOutcome, ActionError> {
        self.perform(action, cx).map(StepOutcome)
    }
}

impl Render for StepGrid {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let doc = self.doc.get(cx).clone();
        // Headers render only when their fields change.
        let key = header_key(&doc);
        if key != self.header_key {
            self.header_key = key;
            let d = doc.clone();
            self.headers.update(cx, |h, cx| {
                h.doc = d;
                cx.notify();
            });
        }
        let (columns, group) = (doc.columns, doc.group);
        self.ruler.update(cx, |r, cx| {
            if (r.columns, r.group) != (columns, group) {
                (r.columns, r.group) = (columns, group);
                cx.notify();
            }
        });
        let theme = GuiraffeTheme::global(cx);
        let colors = match &self.colors {
            Some((seen, c)) if seen == theme => *c,
            _ => {
                let s = theme.surfaces();
                let mut band = s.border.to_hsla();
                band.a *= 0.55;
                let mut empty = s.border.to_hsla();
                empty.a *= 0.3;
                let c = CellColors {
                    empty,
                    band,
                    bar: s.dimmed.to_hsla(),
                    flag: theme
                        .variant(Some(ColorName::Orange), Variant::Filled)
                        .background
                        .to_hsla(),
                    cursor: theme.variant(None, Variant::Filled).background.to_hsla(),
                };
                self.colors = Some((theme.clone(), c));
                c
            }
        };
        {
            // Row inks: reuse the buffer; rows are few.
            let mut inks = self.inks.borrow_mut();
            if inks.len() != doc.rows().len() || key != self.header_key || inks.is_empty() {
                inks.clear();
                inks.extend(doc.rows().iter().map(|r| {
                    theme
                        .variant(Some(r.color), Variant::Filled)
                        .background
                        .to_hsla()
                }));
            }
        }
        let o = &*self.options;
        let step = o.cell + o.gap;
        let width = doc.columns as f32 * step - o.gap;
        let height = doc.rows().len() as f32 * step - o.gap;
        let ruler_h = 16.0;
        let policy = self.resolved_focus(cx);
        let focused = policy.indicator_visible(
            self.focus.is_focused(window),
            window.last_input_was_keyboard(),
        );
        let origin = self.cells_origin.clone();
        let cells = CellsElement {
            doc: doc.clone(),
            colors,
            inks: self.inks.clone(),
            cell: o.cell,
            gap: o.gap,
            level_style: o.level_style,
            preview: self.preview(),
            cursor: focused.then_some(self.cursor),
        };
        div()
            .id("step-grid")
            .track_focus(&self.focus.clone().tab_stop(policy.tab))
            .key_context("StepGrid")
            .text_color(theme.surfaces().text.to_hsla())
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                if let Some((action, batch)) = this.key(e, cx) {
                    let view = cx.weak_entity();
                    window.defer(cx, move |window, cx| {
                        route(&view, action, Source::Keyboard, batch, window, cx)
                    });
                }
            }))
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_size(px(10.))
            .child(
                div()
                    .flex()
                    .child(div().w(px(o.header_width)).flex_none())
                    .child(
                        // The ruler: bar numbers at each group, in its own view.
                        div().w(px(width)).h(px(ruler_h)).child(self.ruler.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .child(
                        div()
                            .w(px(o.header_width))
                            .flex_none()
                            .child(self.headers.clone()),
                    )
                    .child(
                        div()
                            .id("step-cells")
                            .debug_selector(|| "step-cells".into())
                            .relative()
                            .w(px(width))
                            .h(px(height))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                                    if this.resolved_focus(cx).pointer {
                                        this.focus.focus(window, cx);
                                    } else {
                                        // Keep focus where it is (the workspace).
                                        window.prevent_default();
                                    }
                                    this.mouse_down(e, cx);
                                }),
                            )
                            .on_mouse_move(
                                cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                                    this.mouse_move(e, cx)
                                }),
                            )
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseUpEvent, window, cx| {
                                    if let Some(action) = this.take_commit() {
                                        let view = cx.weak_entity();
                                        let batch =
                                            !matches!(action, StepAction::ToggleCell { .. });
                                        window.defer(cx, move |window, cx| {
                                            route(&view, action, Source::Pointer, batch, window, cx)
                                        });
                                    }
                                    cx.notify();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                                    if let Some((r, c)) = this.local_cell(e.position, cx)
                                        && let Some(row) = this.doc(cx).rows().get(r).map(|r| r.id)
                                    {
                                        cx.emit(StepGridEvent::Inspect {
                                            row,
                                            column: c,
                                            position: e.position,
                                        });
                                    }
                                }),
                            )
                            .child(
                                gpui_kit::canvas(
                                    move |b, _, _| origin.set(b.origin),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(cells)
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-ruler_h - 4.0))
                                    .left_0()
                                    .w(px(width))
                                    .h(px(height + ruler_h + 4.0))
                                    .child(self.playhead.clone()),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_hits_skip_gaps_and_edges() {
        assert_eq!(cell_at((5.0, 5.0), 20.0, 4.0, 3, 8), Some((0, 0)));
        assert_eq!(cell_at((26.0, 50.0), 20.0, 4.0, 3, 8), Some((2, 1)));
        assert_eq!(cell_at((22.0, 5.0), 20.0, 4.0, 3, 8), None, "the gap");
        assert_eq!(
            cell_at((5.0, 80.0), 20.0, 4.0, 3, 8),
            None,
            "past the last row"
        );
        assert_eq!(cell_at((-1.0, 5.0), 20.0, 4.0, 3, 8), None);
    }
}
