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
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement, Style, Styled, WeakEntity, Window, div, point,
    prelude::FluentBuilder, px, relative, size,
};
use guiraffe_core::{
    ColorName, GuiraffeTheme, Variant,
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source,
    },
};
use guiraffe_state::{Selected, Store};

use crate::step_model::ApplyStepAction;
pub use crate::step_model::{
    RowId, SetColumns, SetGhosts, SetRows, StepAction, StepCell, StepChange, StepDoc, StepFlag,
    StepRow, step_store,
};

/// The ruler's height above the cells, and the gap between them: the cells start
/// `RULER_HEIGHT + RULER_GAP` below the grid's top. See also [`StepGrid::cells_bounds`].
pub const RULER_HEIGHT: f32 = 16.0;
pub const RULER_GAP: f32 = 4.0;

/// How an Alt-drag sets a level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LevelGesture {
    /// Relative to the level at the press; one cell height of travel spans the range.
    #[default]
    Relative,
    /// The press height inside the cell sets the level (bottom 0, top 1); dragging
    /// continues from the pointer's height.
    Absolute,
}

/// A label the ruler shows over a column.
#[derive(Debug, Clone, PartialEq)]
pub struct RulerLabel {
    pub text: SharedString,
    /// Bold and in body ink (a bar number); otherwise small and dimmed.
    pub strong: bool,
}

/// Labels for the ruler: `None` leaves a column unlabelled.
pub type RulerLabeler = Rc<dyn Fn(usize) -> Option<RulerLabel>>;

/// A left press on a cell, offered to the host before the grid's own gestures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepPress {
    pub row: RowId,
    pub column: usize,
    pub modifiers: gpui_kit::Modifiers,
    pub position: Point<Pixels>,
}

/// Return `true` when the host handled the press; the grid then does nothing with it.
pub type StepPressHook = Rc<dyn Fn(&StepPress, &mut Window, &mut App) -> bool>;

/// Whether a row sounds: not muted, and soloed whenever any row is soloed. Rows that don't
/// sound are drawn with dimmed ink.
pub fn row_audible(doc: &StepDoc, index: usize) -> bool {
    let any_solo = doc.rows().iter().any(|r| r.soloed);
    doc.rows()
        .get(index)
        .is_some_and(|r| !r.muted && (!any_solo || r.soloed))
}

/// Cell geometry along the columns, with an optional wider gap between groups. Pure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepGeometry {
    pub cell: f32,
    pub gap: f32,
    /// Columns per group (0: none).
    pub group: usize,
    /// Extra space after each group, on top of `gap`.
    pub group_gap: f32,
}

impl StepGeometry {
    fn step(&self) -> f32 {
        self.cell + self.gap
    }
    fn grouped(&self) -> bool {
        self.group > 0 && self.group_gap > 0.0
    }
    /// Left edge of column `c`.
    pub fn x(&self, c: usize) -> f32 {
        let groups = if self.grouped() { c / self.group } else { 0 };
        c as f32 * self.step() + groups as f32 * self.group_gap
    }
    /// Top edge of row `r`.
    pub fn y(&self, r: usize) -> f32 {
        r as f32 * self.step()
    }
    /// Width of `columns` cells, from the first left edge to the last right edge.
    pub fn width(&self, columns: usize) -> f32 {
        if columns == 0 {
            0.0
        } else {
            self.x(columns - 1) + self.cell
        }
    }
    /// The column whose cell holds `x`, or `None` in a gap. Unbounded on the right.
    pub fn column_at(&self, x: f32) -> Option<usize> {
        if x < 0.0 {
            return None;
        }
        let (group, base, x) = if self.grouped() {
            let span = self.group as f32 * self.step() + self.group_gap;
            let g = (x / span) as usize;
            (Some(self.group), g * self.group, x - g as f32 * span)
        } else {
            (None, 0, x)
        };
        let i = (x / self.step()) as usize;
        if group.is_some_and(|n| i >= n) || x - i as f32 * self.step() > self.cell {
            return None;
        }
        Some(base + i)
    }
    /// The nearest column to `x`, clamped to `0..columns` (for drags across gaps).
    pub fn column_near(&self, x: f32, columns: usize) -> usize {
        let last = columns.saturating_sub(1);
        (0..columns)
            .min_by(|&a, &b| {
                let da = (self.x(a) + self.cell / 2.0 - x).abs();
                let db = (self.x(b) + self.cell / 2.0 - x).abs();
                da.total_cmp(&db)
            })
            .unwrap_or(0)
            .min(last)
    }
    /// The cell under grid-local `(x, y)`: (row, column).
    pub fn cell_at(
        &self,
        (x, y): (f32, f32),
        rows: usize,
        columns: usize,
    ) -> Option<(usize, usize)> {
        if y < 0.0 {
            return None;
        }
        let r = (y / self.step()) as usize;
        let c = self.column_at(x)?;
        (r < rows && c < columns && y - r as f32 * self.step() <= self.cell).then_some((r, c))
    }
}

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
    /// The cell under the pointer changed (`None`: the pointer left the cells).
    Hover(Option<(RowId, usize)>),
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
    /// Extra space after every `group` columns (0: none). Hit-testing includes it.
    pub group_gap: f32,
    pub level_gesture: LevelGesture,
    /// `None`: a number at each group's first column.
    pub ruler_label: Option<RulerLabeler>,
    /// Offered every left press first (see [`StepPress`]).
    pub on_press: Option<StepPressHook>,
}

impl Default for StepGridOptions {
    fn default() -> Self {
        Self {
            cell: 22.0,
            gap: 2.0,
            header_width: 120.0,
            level_style: LevelStyle::Height,
            header: None,
            group_gap: 0.0,
            level_gesture: LevelGesture::Relative,
            ruler_label: None,
            on_press: None,
        }
    }
}

/// Pure geometry without group gaps: the cell under grid-local `(x, y)`.
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
    geometry: Rc<Cell<StepGeometry>>,
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
            geometry: self.geometry.get(),
            color,
        }
    }
}

struct HighlightElement {
    column: Option<usize>,
    geometry: StepGeometry,
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
        let g = this.geometry;
        let x = g.x(column);
        window.paint_quad(gpui_kit::fill(
            Bounds::new(
                bounds.origin + point(px(x - g.gap / 2.0), px(0.)),
                size(px(g.cell + g.gap), bounds.size.height),
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
    /// Offset ticks and flam bars, over the fill.
    mark: Hsla,
}

/// Every cell in one element.
struct CellsElement {
    doc: StepDoc,
    colors: CellColors,
    /// A row's ink, by row index (rows are few; reused buffer).
    inks: Rc<std::cell::RefCell<Vec<Hsla>>>,
    geometry: StepGeometry,
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
            Some(StepAction::SetOffset {
                row: id,
                column,
                offset,
            }) if id == row.id && column == c => {
                cell.offset = offset;
            }
            _ => {}
        }
        let _ = r;
        cell
    }
}

paint_element!(CellsElement, |this, bounds, window| {
    let g = this.geometry;
    let size_c = g.cell;
    let quad = |x: f32, y: f32, w: f32, h: f32, color: Hsla, window: &mut Window| {
        window.paint_quad(gpui_kit::fill(
            Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h))),
            color,
        ));
    };
    // A dashed outline, for ghost cells.
    let dashed = |x: f32, y: f32, color: Hsla, window: &mut Window| {
        let (dash, t): (f32, f32) = (3.0, 1.5);
        let mut d = 0.0;
        while d < size_c {
            let len = dash.min(size_c - d);
            quad(x + d, y, len, t, color, window);
            quad(x + d, y + size_c - t, len, t, color, window);
            quad(x, y + d, t, len, color, window);
            quad(x + size_c - t, y + d, t, len, color, window);
            d += dash * 2.0;
        }
    };
    let inks = this.inks.borrow();
    let group = this.doc.group;
    for (r, row) in this.doc.rows().iter().enumerate() {
        let y = g.y(r);
        let mut ink = inks.get(r).copied().unwrap_or(this.colors.flag);
        // Rows that don't sound (muted, or unsoloed while any row is soloed) dim their ink.
        if !row_audible(&this.doc, r) {
            ink.a *= 0.3;
        }
        let ghosts = this.doc.ghosts(row.id);
        for c in 0..this.doc.columns {
            let x = g.x(c);
            let banded = group > 0 && (c / group) % 2 == 1;
            quad(
                x,
                y,
                size_c,
                size_c,
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
                // A ghost note is a smaller, fainter hit.
                let (inset, scale) = if cell.soft {
                    (size_c * 0.2, 0.6)
                } else {
                    (0.0, 1.0)
                };
                if cell.soft {
                    fill.a *= 0.7;
                }
                let side = size_c * scale;
                match this.level_style {
                    LevelStyle::Height => {
                        let h = (side * cell.level).max(3.0);
                        quad(x + inset, y + inset + side - h, side, h, fill, window);
                    }
                    LevelStyle::Brightness => {
                        fill.a *= 0.3 + 0.7 * cell.level;
                        quad(x + inset, y + inset, side, side, fill, window);
                    }
                }
                // Probability below 1: a dotted outline.
                if cell.probability < 1.0 {
                    let mut dot = ink;
                    dot.a *= dim;
                    let n = 4;
                    for i in 0..n {
                        let t = (i as f32 + 0.5) / n as f32 * size_c - 1.0;
                        quad(x + t, y, 2.0, 2.0, dot, window);
                        quad(x + t, y + size_c - 2.0, 2.0, 2.0, dot, window);
                        quad(x, y + t, 2.0, 2.0, dot, window);
                        quad(x + size_c - 2.0, y + t, 2.0, 2.0, dot, window);
                    }
                }
                // Ratchet: stacked ticks along the left edge.
                if cell.ratchet > 1 {
                    let mut tick = this.colors.empty;
                    tick.a = dim;
                    let n = f32::from(cell.ratchet.min(8));
                    let pitch = (size_c - 4.0) / n;
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
                let mut mark = this.colors.mark;
                mark.a *= dim;
                // Micro-timing: a tick where the hit sits inside its step.
                if cell.offset != 0.0 {
                    let at = x + size_c / 2.0 + cell.offset * size_c - 1.0;
                    quad(
                        at.clamp(x, x + size_c - 2.0),
                        y + 1.0,
                        2.0,
                        size_c - 2.0,
                        mark,
                        window,
                    );
                }
                // Flam: a short bar along the top.
                if cell.flam {
                    quad(x + 2.0, y + 2.0, size_c * 0.4, 3.0, mark, window);
                }
                if cell.flag {
                    let mut flag = this.colors.flag;
                    flag.a *= dim;
                    quad(x + size_c - 6.0, y, 6.0, 6.0, flag, window);
                }
            }
            // Ghost layer: a proposed hit, as a dashed outline until applied.
            if let Some(ghost) = ghosts.and_then(|cells| cells.get(c))
                && ghost.on
                && !repeat
            {
                let mut outline = ink;
                outline.a = (outline.a * 0.9).max(0.5);
                dashed(x, y, outline, window);
            }
            if this.cursor == Some((r, c)) {
                let t = 2.0;
                let k = this.colors.cursor;
                quad(x - t, y - t, size_c + 2.0 * t, t, k, window);
                quad(x - t, y + size_c, size_c + 2.0 * t, t, k, window);
                quad(x - t, y, t, size_c, k, window);
                quad(x + size_c, y, t, size_c, k, window);
            }
        }
    }
    // Bar lines at group boundaries, centred in the gap before the group.
    if group > 0 {
        let rows = this.doc.rows().len() as f32;
        let mut c = group;
        while c < this.doc.columns {
            let x = (g.x(c - 1) + size_c + g.x(c)) / 2.0 - 0.5;
            quad(
                x,
                0.0,
                1.0,
                rows * g.step() - g.gap,
                this.colors.bar,
                window,
            );
            c += group;
        }
    }
});

/// The ruler's labels over the columns: renders only when the columns, grouping or
/// geometry change.
struct StepRuler {
    columns: usize,
    geometry: StepGeometry,
    label: Option<RulerLabeler>,
}

impl Render for StepRuler {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        let (dim, ink) = (surfaces.dimmed.to_hsla(), surfaces.text.to_hsla());
        let g = self.geometry;
        let group = g.group.max(1);
        let labels: Vec<(usize, RulerLabel)> = match &self.label {
            Some(label) => (0..self.columns)
                .filter_map(|c| Some((c, label(c)?)))
                .collect(),
            None => (0..self.columns)
                .step_by(group)
                .map(|c| {
                    let text = SharedString::from((c / group + 1).to_string());
                    (
                        c,
                        RulerLabel {
                            text,
                            strong: false,
                        },
                    )
                })
                .collect(),
        };
        div()
            .relative()
            .size_full()
            .text_color(dim)
            .children(labels.into_iter().map(|(c, label)| {
                div()
                    .absolute()
                    .left(px(g.x(c)))
                    .when(label.strong, |d| {
                        d.text_color(ink).font_weight(gpui_kit::FontWeight::BOLD)
                    })
                    .child(label.text)
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
        /// The cell's window top, for [`LevelGesture::Absolute`].
        top: Option<f32>,
    },
    /// Ctrl-drag on a lit cell: micro-timing, one cell width spanning one step.
    Offset {
        row: RowId,
        column: usize,
        from_x: f32,
        from: f32,
        offset: f32,
    },
    /// Shift-click on a lit cell: cycle its emphasis on release.
    Flag { row: RowId, column: usize },
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
    cells_bounds: Rc<Cell<Bounds<Pixels>>>,
    geometry: Rc<Cell<StepGeometry>>,
    hovered: Option<(RowId, usize)>,
    focus: FocusHandle,
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
            let options = Rc::new(options);
            let first = doc.get(cx).clone();
            let headers = cx.new(|_| StepHeaders {
                doc: first.clone(),
                options: options.clone(),
            });
            let column = Rc::new(Cell::new(None));
            let geometry = Rc::new(Cell::new(StepGeometry {
                cell: options.cell,
                gap: options.gap,
                group: first.group,
                group_gap: options.group_gap,
            }));
            let playhead = cx.new(|_| StepPlayhead {
                column: column.clone(),
                geometry: geometry.clone(),
                color: None,
            });
            let feed = StepPlayheadFeed {
                column,
                view: playhead.entity_id(),
            };
            let ruler = cx.new(|_| StepRuler {
                columns: first.columns,
                geometry: geometry.get(),
                label: options.ruler_label.clone(),
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
                cells_bounds: Rc::default(),
                geometry,
                hovered: None,
                focus: cx.focus_handle(),
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
    pub fn playhead(&self) -> StepPlayheadFeed {
        self.feed.clone()
    }

    /// The keyboard cursor as (row index, column).
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// The cells' window bounds at the last paint (for hosts that overlay or hit-test).
    pub fn cells_bounds(&self) -> Bounds<Pixels> {
        self.cells_bounds.get()
    }

    /// The current column geometry (cell, gaps, grouping).
    pub fn geometry(&self) -> StepGeometry {
        self.geometry.get()
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
        let o = self.cells_bounds.get().origin;
        let doc = self.doc(cx);
        self.geometry.get().cell_at(
            (f32::from(p.x - o.x), f32::from(p.y - o.y)),
            doc.rows().len(),
            doc.columns,
        )
    }

    /// The press the host hook sees, if `p` is on a cell.
    fn press_at(&self, e: &MouseDownEvent, cx: &App) -> Option<StepPress> {
        let (r, c) = self.local_cell(e.position, cx)?;
        let row = self.doc(cx).rows().get(r)?.id;
        Some(StepPress {
            row,
            column: c,
            modifiers: e.modifiers,
            position: e.position,
        })
    }

    /// Track the hovered cell; emits [`StepGridEvent::Hover`] when it changes.
    fn hover(&mut self, at: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let next = at.and_then(|p| {
            let (r, c) = self.local_cell(p, cx)?;
            Some((self.doc(cx).rows().get(r)?.id, c))
        });
        if next != self.hovered {
            self.hovered = next;
            cx.emit(StepGridEvent::Hover(next));
        }
    }

    /// Commit (or drop) a pending drag: on release over the cells, release elsewhere, or a
    /// move that arrives with the button already up.
    fn finish_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(action) = self.take_commit() {
            let view = cx.weak_entity();
            let batch = !matches!(
                action,
                StepAction::ToggleCell { .. } | StepAction::CycleFlag { .. }
            );
            window.defer(cx, move |window, cx| {
                route(&view, action, Source::Pointer, batch, window, cx)
            });
        }
        cx.notify();
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
        let g = self.geometry.get();
        let top = f32::from(self.cells_bounds.get().origin.y) + g.y(r);
        let from = if cell.on { cell.level } else { 0.0 };
        self.drag = Some(if e.modifiers.alt {
            let absolute = self.options.level_gesture == LevelGesture::Absolute;
            let y = f32::from(e.position.y);
            Drag::Level {
                row: row.id,
                column: c,
                from_y: y,
                from,
                level: if absolute {
                    (1.0 - (y - top) / g.cell).clamp(0.0, 1.0)
                } else {
                    from
                },
                top: absolute.then_some(top),
            }
        } else if e.modifiers.shift && cell.on {
            Drag::Flag {
                row: row.id,
                column: c,
            }
        } else if e.modifiers.control && cell.on {
            Drag::Offset {
                row: row.id,
                column: c,
                from_x: f32::from(e.position.x),
                from: cell.offset,
                offset: cell.offset,
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

    fn mouse_move(&mut self, e: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if e.pressed_button != Some(MouseButton::Left) {
            if self.drag.is_some() {
                // The button came up outside the grid (or the window).
                self.finish_drag(window, cx);
            }
            self.hover(Some(e.position), cx);
            return;
        }
        let o = self.cells_bounds.get().origin;
        let g = self.geometry.get();
        let columns = self.doc(cx).columns;
        match &mut self.drag {
            Some(Drag::Paint { to, .. }) => {
                let c = g.column_near(f32::from(e.position.x - o.x), columns);
                if *to != c {
                    *to = c;
                    cx.notify();
                }
            }
            Some(Drag::Level {
                from_y,
                from,
                level,
                top,
                ..
            }) => {
                let y = f32::from(e.position.y);
                let next = match top {
                    Some(top) => 1.0 - (y - *top) / g.cell,
                    // One cell height of travel spans the full range.
                    None => *from + (*from_y - y) / g.cell,
                }
                .clamp(0.0, 1.0);
                if next != *level {
                    *level = next;
                    cx.notify();
                }
            }
            Some(Drag::Offset {
                from_x,
                from,
                offset,
                ..
            }) => {
                let next = (*from + (f32::from(e.position.x) - *from_x) / g.cell).clamp(-0.5, 0.5);
                if next != *offset {
                    *offset = next;
                    cx.notify();
                }
            }
            Some(Drag::Flag { .. }) | None => {}
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
                top,
                ..
            } => (level != from || top.is_some()).then_some(StepAction::SetCellLevel {
                row,
                column,
                level,
            }),
            Drag::Offset {
                row,
                column,
                from,
                offset,
                ..
            } => (offset != from).then_some(StepAction::SetOffset {
                row,
                column,
                offset,
            }),
            Drag::Flag { row, column } => Some(StepAction::CycleFlag { row, column }),
        }
    }

    fn preview(&self) -> Option<StepAction> {
        match self.drag? {
            Drag::Paint { row, from, to, on } => Some(StepAction::PaintCells { row, from, to, on }),
            Drag::Level {
                row, column, level, ..
            } => Some(StepAction::SetCellLevel { row, column, level }),
            Drag::Offset {
                row,
                column,
                offset,
                ..
            } => Some(StepAction::SetOffset {
                row,
                column,
                offset,
            }),
            Drag::Flag { .. } => None,
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
                    let g = self.geometry.get();
                    let o = self.cells_bounds.get().origin;
                    cx.emit(StepGridEvent::Inspect {
                        row,
                        column: c,
                        position: o + point(px(g.x(c)), px(g.y(r))),
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
        let columns = doc.columns;
        let geometry = StepGeometry {
            cell: self.options.cell,
            gap: self.options.gap,
            group: doc.group,
            group_gap: self.options.group_gap,
        };
        self.geometry.set(geometry);
        self.ruler.update(cx, |r, cx| {
            if (r.columns, r.geometry) != (columns, geometry) {
                (r.columns, r.geometry) = (columns, geometry);
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
                    mark: s.text.to_hsla(),
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
        let width = geometry.width(doc.columns);
        let height = (geometry.y(doc.rows().len()) - o.gap).max(0.0);
        let ruler_h = RULER_HEIGHT;
        let focused = self.focus.is_focused(window);
        let cells_bounds = self.cells_bounds.clone();
        let cells = CellsElement {
            doc: doc.clone(),
            colors,
            inks: self.inks.clone(),
            geometry,
            level_style: o.level_style,
            preview: self.preview(),
            cursor: focused.then_some(self.cursor),
        };
        div()
            .id("step-grid")
            .track_focus(&self.focus)
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
            .gap(px(RULER_GAP))
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
                            .on_mouse_down(MouseButton::Left, {
                                // The host hook runs outside this grid's update, so it can
                                // dispatch to the bus.
                                let view = cx.weak_entity();
                                let hook = o.on_press.clone();
                                move |e: &MouseDownEvent, window, cx| {
                                    let Some(grid) = view.upgrade() else { return };
                                    let press = grid.read(cx).press_at(e, cx);
                                    if let (Some(hook), Some(press)) = (&hook, press)
                                        && hook(&press, window, cx)
                                    {
                                        return;
                                    }
                                    grid.update(cx, |this, cx| {
                                        this.focus.focus(window, cx);
                                        this.mouse_down(e, cx);
                                    });
                                }
                            })
                            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                                this.mouse_move(e, window, cx)
                            }))
                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                if !*hovered {
                                    this.hover(None, cx);
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseUpEvent, window, cx| {
                                    this.finish_drag(window, cx)
                                }),
                            )
                            .on_mouse_up_out(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseUpEvent, window, cx| {
                                    if this.drag.is_some() {
                                        this.finish_drag(window, cx);
                                    }
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
                                    move |b, _, _| cells_bounds.set(b),
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
    fn group_gaps_shift_columns_and_hit_tests() {
        let g = StepGeometry {
            cell: 20.0,
            gap: 4.0,
            group: 4,
            group_gap: 10.0,
        };
        assert_eq!(g.x(3), 72.0);
        assert_eq!(g.x(4), 106.0, "after the first group gap");
        assert_eq!(g.width(8), 198.0);
        assert_eq!(g.cell_at((5.0, 5.0), 2, 8), Some((0, 0)));
        assert_eq!(g.cell_at((95.0, 5.0), 2, 8), None, "inside the group gap");
        assert_eq!(g.cell_at((110.0, 30.0), 2, 8), Some((1, 4)));
        assert_eq!(g.cell_at((240.0, 5.0), 2, 8), None, "past the last column");
        assert_eq!(g.column_near(95.0, 8), 3);
        assert_eq!(g.column_near(500.0, 8), 7);
        let plain = StepGeometry {
            group_gap: 0.0,
            ..g
        };
        for x in [5.0, 26.0, 22.0, 100.0] {
            assert_eq!(
                plain.cell_at((x, 5.0), 3, 8),
                cell_at((x, 5.0), 20.0, 4.0, 3, 8),
                "{x}"
            );
        }
    }

    #[test]
    fn muted_and_unsoloed_rows_do_not_sound() {
        let mut a = StepRow::new(1, "A", 4);
        let b = StepRow::new(2, "B", 4);
        let mut c = StepRow::new(3, "C", 4);
        a.muted = true;
        let doc = StepDoc::new(4, 0, vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(
            (0..3).map(|i| row_audible(&doc, i)).collect::<Vec<_>>(),
            [false, true, true]
        );
        c.soloed = true;
        let doc = StepDoc::new(4, 0, vec![a, b, c]);
        assert_eq!(
            (0..3).map(|i| row_audible(&doc, i)).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert!(!row_audible(&doc, 9));
    }

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
