use std::{
    cell::Cell as Local,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::{
    App, AppContext, Bounds, ClipboardItem, ContentMask, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Pixels, Point,
    Render, ScrollEvent, SharedString, Styled, Window, canvas,
    component::input::{Input, InputState},
    div, fill, point, px, size,
};
use guiraffe_core::GuiraffeTheme;
use guiraffe_datagrid::Nav;

use crate::{
    Alignment, Axis, BuiltinEngine, Cell, CellPos, FormulaEngine, FormulaError, NumberFormat,
    Recalculator, SheetModel, column_name,
};

type Values = HashMap<CellPos, Result<f64, FormulaError>>;

/// The Sheet control displays an existing state entity.
pub struct Sheet;

impl Sheet {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(state: &Entity<SheetState>) -> Entity<SheetState> {
        state.clone()
    }
}

/// Crate initialization. The control owns focus and handles navigation while focused.
pub fn init(_cx: &mut App) {}

#[derive(Clone, Copy)]
enum Drag {
    Selection,
    Fill,
    Resize {
        row: bool,
        index: usize,
        start: f64,
        size: f64,
    },
}

pub struct SheetState {
    model: SheetModel,
    rows: Axis,
    cols: Axis,
    frozen: CellPos,
    scroll: (f64, f64),
    bounds: Local<Bounds<Pixels>>,
    focus: FocusHandle,
    drag: Option<Drag>,
    fill_target: Option<CellPos>,
    editor: Option<(CellPos, Entity<InputState>)>,
    recalc: Option<Recalculator>,
    startup_error: Option<SharedString>,
    revision: u64,
    published: u64,
    values: Values,
    waiting: Option<Instant>,
    polling: bool,
}

impl SheetState {
    pub fn new(model: SheetModel, cx: &mut Context<Self>) -> Self {
        Self::with_engine(model, Arc::new(BuiltinEngine), cx)
    }

    pub fn with_engine(
        model: SheetModel,
        engine: Arc<dyn FormulaEngine>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (rows, cols) = model.dimensions();
        let (mut recalc, startup_error) = match Recalculator::new(rows, cols, engine) {
            Ok(recalc) => (Some(recalc), None),
            Err(error) => (
                None,
                Some(SharedString::from(format!(
                    "Calculation unavailable: {error}"
                ))),
            ),
        };
        let changes: Vec<_> = model
            .cells()
            .map(|(pos, cell)| (pos, cell.input()))
            .collect();
        let revision = if changes.is_empty() {
            0
        } else {
            recalc
                .as_mut()
                .and_then(|worker| worker.submit(changes))
                .unwrap_or(0)
        };
        let theme = GuiraffeTheme::global(cx);
        let unit = f64::from(theme.scale);
        let mut state = Self {
            model,
            rows: Axis::new(rows, 26. * unit),
            cols: Axis::new(cols, 112. * unit),
            frozen: CellPos::default(),
            scroll: (0., 0.),
            bounds: Local::new(Bounds::default()),
            focus: cx.focus_handle(),
            drag: None,
            fill_target: None,
            editor: None,
            recalc,
            startup_error,
            revision,
            published: 0,
            values: Values::new(),
            waiting: None,
            polling: false,
        };
        state.poll(cx);
        state
    }

    pub fn model(&self) -> &SheetModel {
        &self.model
    }

    pub fn set_read_only(&mut self, value: bool, cx: &mut Context<Self>) {
        self.model.read_only = value;
        if value {
            self.editor = None;
        }
        cx.notify();
    }

    pub fn freeze(&mut self, rows: usize, cols: usize, cx: &mut Context<Self>) {
        let (r, c) = self.model.dimensions();
        self.frozen = CellPos::new(rows.min(r), cols.min(c));
        self.scroll_by(0., 0.);
        cx.notify();
    }

    pub fn edit(
        &mut self,
        changes: impl IntoIterator<Item = (CellPos, Cell)>,
        cx: &mut Context<Self>,
    ) {
        let changed = self.model.edit(changes);
        self.submit(changed, cx);
    }

    fn submit(&mut self, changed: Vec<CellPos>, cx: &mut Context<Self>) {
        if changed.is_empty() {
            return;
        }
        let batch = changed
            .into_iter()
            .map(|pos| {
                (
                    pos,
                    self.model
                        .cell(pos)
                        .map_or_else(|| Arc::from(""), Cell::input),
                )
            })
            .collect();
        if let Some(revision) = self.recalc.as_mut().and_then(|worker| worker.submit(batch)) {
            self.revision = revision;
        }
        self.poll(cx);
        cx.notify();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        if self.revision == self.published {
            return;
        }
        self.waiting.get_or_insert_with(Instant::now);
        if self.polling {
            return;
        }
        self.polling = true;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            loop {
                timer.timer(Duration::from_millis(16)).await;
                let done = entity.update(cx, |state, cx| {
                    while let Some(calculation) =
                        state.recalc.as_ref().and_then(Recalculator::try_recv)
                    {
                        for (pos, value) in calculation.changes {
                            if let Some(value) = value {
                                state.values.insert(pos, value);
                            } else {
                                state.values.remove(&pos);
                            }
                        }
                        state.published = calculation.revision;
                    }
                    if state.published == state.revision {
                        state.waiting = None;
                        state.polling = false;
                        cx.notify();
                        true
                    } else {
                        if state
                            .waiting
                            .is_some_and(|start| start.elapsed().as_millis() < 80)
                        {
                            cx.notify();
                        }
                        false
                    }
                });
                if done.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    fn scroll_by(&mut self, x: f64, y: f64) {
        let viewport = self.bounds.get().size;
        let (rows, cols) = self.model.dimensions();
        let max_x =
            (self.cols.offset(cols) - f64::from(f32::from(viewport.width)) + self.gutter()).max(0.);
        let max_y = (self.rows.offset(rows) - f64::from(f32::from(viewport.height))
            + self.header())
        .max(0.);
        self.scroll.0 = (self.scroll.0 + x).clamp(0., max_x);
        self.scroll.1 = (self.scroll.1 + y).clamp(0., max_y);
    }

    fn gutter(&self) -> f64 {
        self.cols.size(usize::MAX) * 0.55
    }
    fn header(&self) -> f64 {
        self.rows.size(usize::MAX)
    }

    fn local(&self, p: Point<Pixels>) -> (f64, f64) {
        let p = p - self.bounds.get().origin;
        (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
    }

    fn axes_at(&self, x: f64, y: f64) -> CellPos {
        let x = x - self.gutter();
        let y = y - self.header();
        CellPos::new(
            hit_axis(&self.rows, y, self.frozen.row, self.scroll.1),
            hit_axis(&self.cols, x, self.frozen.col, self.scroll.0),
        )
    }

    fn rect(&self, pos: CellPos) -> Bounds<Pixels> {
        bounds_at(
            self.gutter() + axis_position(&self.cols, pos.col, self.frozen.col, self.scroll.0),
            self.header() + axis_position(&self.rows, pos.row, self.frozen.row, self.scroll.1),
            self.cols.size(pos.col),
            self.rows.size(pos.row),
        )
    }

    fn select_visible(&mut self) {
        let Some(pos) = self.model.selection.cursor else {
            return;
        };
        let viewport = self.bounds.get().size;
        reveal(
            &self.cols,
            pos.col,
            self.frozen.col,
            f64::from(f32::from(viewport.width)) - self.gutter(),
            &mut self.scroll.0,
        );
        reveal(
            &self.rows,
            pos.row,
            self.frozen.row,
            f64::from(f32::from(viewport.height)) - self.header(),
            &mut self.scroll.1,
        );
        self.scroll_by(0., 0.);
    }

    fn begin_edit(&mut self, initial: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.read_only {
            return;
        }
        let pos = self.model.selection.cursor.unwrap_or_default();
        if !self.model.contains(pos) {
            return;
        }
        self.model.select(pos, false);
        self.select_visible();
        let text = initial.unwrap_or_else(|| {
            self.model
                .cell(pos)
                .map_or_else(String::new, |cell| cell.text.to_string())
        });
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |input, cx| input.set_value(text, window, cx));
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.editor = Some((pos, input));
        cx.notify();
    }

    fn keyboard(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let ctrl = key.modifiers.control || key.modifiers.platform;
        let shift = key.modifiers.shift;
        if let Some((pos, input)) = &self.editor {
            let advance = match key.key.as_str() {
                "escape" => None,
                "enter" => Some(if shift {
                    guiraffe_datagrid::selection::Advance::Up
                } else {
                    guiraffe_datagrid::selection::Advance::Down
                }),
                "tab" => Some(if shift {
                    guiraffe_datagrid::selection::Advance::Left
                } else {
                    guiraffe_datagrid::selection::Advance::Right
                }),
                _ => return,
            };
            if let Some(advance) = advance {
                let pos = *pos;
                let value = input.read(cx).value();
                let mut cell = self.model.cell(pos).cloned().unwrap_or_default();
                cell.text = Arc::from(value.as_str());
                self.edit([(pos, cell)], cx);
                let (rows, cols) = self.model.dimensions();
                if let Some(next) = advance.apply(pos, rows, cols) {
                    self.model.select(next, false);
                    self.select_visible();
                }
            }
            self.editor = None;
            window.focus(&self.focus, cx);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        let nav = match key.key.as_str() {
            "up" => Some(Nav::Up),
            "down" => Some(Nav::Down),
            "left" => Some(Nav::Left),
            "right" => Some(Nav::Right),
            "home" => Some(if ctrl { Nav::First } else { Nav::Home }),
            "end" => Some(if ctrl { Nav::Last } else { Nav::End }),
            _ => None,
        };
        if let Some(nav) = nav {
            self.model.navigate(nav, shift, ctrl, 1);
            self.select_visible();
        } else {
            match (key.key.as_str(), ctrl) {
                ("enter" | "f2", false) => self.begin_edit(None, window, cx),
                ("delete" | "backspace", false) => {
                    let changed = self.model.clear_selection();
                    self.submit(changed, cx);
                }
                ("c" | "x", true) => {
                    if let Some(text) = self.model.copy_tsv() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        if key.key == "x" {
                            let changed = self.model.clear_selection();
                            self.submit(changed, cx);
                        }
                    }
                }
                ("v", true) => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let changed = self.model.paste_tsv(&text);
                        self.submit(changed, cx);
                    }
                }
                ("z" | "y", true) => {
                    let changed = if shift || key.key == "y" {
                        self.model.redo()
                    } else {
                        self.model.undo()
                    };
                    self.submit(changed, cx);
                }
                ("d" | "r", true) => {
                    if let Some((rows, cols)) = self.model.selection.rect() {
                        let target = CellPos::new(rows.end - 1, cols.end - 1);
                        let (rows, cols) = if key.key == "d" {
                            (rows.start..rows.start + 1, cols)
                        } else {
                            (rows, cols.start..cols.start + 1)
                        };
                        let changed = self.model.fill(rows, cols, target, false);
                        self.submit(changed, cx);
                    }
                }
                ("space", _) if ctrl || shift => {
                    let pos = self.model.selection.cursor.unwrap_or_default();
                    let (rows, cols) = self.model.dimensions();
                    if rows > 0 && cols > 0 {
                        self.model.select(
                            CellPos::new(
                                if ctrl { 0 } else { pos.row },
                                if shift { 0 } else { pos.col },
                            ),
                            false,
                        );
                        self.model.select(
                            CellPos::new(
                                if ctrl { rows - 1 } else { pos.row },
                                if shift { cols - 1 } else { pos.col },
                            ),
                            true,
                        );
                    }
                }
                (_, false) if !key.modifiers.alt && key.key_char.is_some() => {
                    self.begin_edit(key.key_char.clone(), window, cx);
                }
                _ => return,
            }
        }
        cx.notify();
        cx.stop_propagation();
    }

    fn pointer_down(
        &mut self,
        event: &gpui_kit::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (x, y) = self.local(event.position);
        let pos = self.axes_at(x, y);
        if !self.model.contains(pos) {
            return;
        }
        window.focus(&self.focus, cx);
        self.editor = None;
        let rect = self.rect(pos);
        if y < self.header() {
            self.drag = Some(Drag::Resize {
                row: false,
                index: pos.col,
                start: x,
                size: self.cols.size(pos.col),
            });
        } else if x < self.gutter() {
            self.drag = Some(Drag::Resize {
                row: true,
                index: pos.row,
                start: y,
                size: self.rows.size(pos.row),
            });
        } else {
            let handle = self.model.selection.rect().is_some_and(|(rows, cols)| {
                pos == CellPos::new(rows.end - 1, cols.end - 1)
                    && f64::from(f32::from(rect.right())) - x <= 8.
                    && f64::from(f32::from(rect.bottom())) - y <= 8.
            });
            if handle && !self.model.read_only {
                self.drag = Some(Drag::Fill);
                self.fill_target = Some(pos);
            } else {
                self.model.select(pos, event.modifiers.shift);
                self.drag = Some(Drag::Selection);
                if event.click_count == 2 {
                    self.begin_edit(None, window, cx);
                }
            }
        }
        cx.notify();
    }

    fn pointer_move(
        &mut self,
        event: &gpui_kit::MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (x, y) = self.local(event.position);
        let pos = self.axes_at(x, y);
        match self.drag {
            Some(Drag::Selection) => self.model.select(pos, true),
            Some(Drag::Fill) if self.model.contains(pos) => self.fill_target = Some(pos),
            Some(Drag::Resize {
                row: true,
                index,
                start,
                size,
            }) => self.rows.resize(index, size + y - start),
            Some(Drag::Resize {
                row: false,
                index,
                start,
                size,
            }) => self.cols.resize(index, size + x - start),
            _ => return,
        }
        cx.notify();
    }

    fn pointer_up(&mut self, _: &gpui_kit::MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.drag = None;
        if let Some(target) = self.fill_target.take()
            && let Some((rows, cols)) = self.model.selection.rect()
        {
            let changed = self.model.fill(rows, cols, target, true);
            self.submit(changed, cx);
        }
        cx.notify();
    }

    fn wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(self.header() as f32));
        self.scroll_by(
            -f64::from(f32::from(delta.x)),
            -f64::from(f32::from(delta.y)),
        );
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for SheetState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SheetState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let editor = self.editor.as_ref().map(|(pos, input)| {
            let bounds = self.rect(*pos);
            div()
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .child(Input::new(input).h_full().w_full())
        });
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("GrSheet")
            .capture_key_down(cx.listener(Self::keyboard))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::pointer_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::pointer_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::pointer_up))
            .on_mouse_move(cx.listener(Self::pointer_move))
            .on_scroll(cx.listener(Self::wheel))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let state = entity.read(cx);
                        state.bounds.set(bounds);
                        state.labels(bounds)
                    },
                    |_, frame, window, cx| frame.paint(window, cx),
                )
                .absolute()
                .size_full(),
            )
            .children(editor)
            .children(
                self.startup_error
                    .clone()
                    .map(|message| div().absolute().bottom_0().child(message)),
            )
    }
}

fn bounds_at(x: f64, y: f64, w: f64, h: f64) -> Bounds<Pixels> {
    Bounds::new(
        point(px(x as f32), px(y as f32)),
        size(px(w as f32), px(h as f32)),
    )
}

fn axis_position(axis: &Axis, index: usize, frozen: usize, scroll: f64) -> f64 {
    axis.offset(index) - if index < frozen { 0. } else { scroll }
}

fn hit_axis(axis: &Axis, at: f64, frozen: usize, scroll: f64) -> usize {
    axis.index(if at < axis.offset(frozen) {
        at
    } else {
        at + scroll
    })
}

fn reveal(axis: &Axis, index: usize, frozen: usize, viewport: f64, scroll: &mut f64) {
    if index < frozen {
        return;
    }
    let start = axis.offset(index);
    let end = start + axis.size(index);
    let pane = axis.offset(frozen);
    if start - *scroll < pane {
        *scroll = start - pane;
    } else if end - *scroll > viewport {
        *scroll = (end - viewport).min(start - pane);
    }
}

fn visible(
    axis: &Axis,
    frozen: usize,
    scroll: f64,
    viewport: f64,
    len: usize,
) -> impl Iterator<Item = usize> {
    let pinned = axis.index(viewport).saturating_add(1).min(frozen);
    let start = axis.index(axis.offset(frozen) + scroll).max(frozen);
    let end = axis.index(viewport + scroll).saturating_add(1).min(len);
    (0..pinned).chain(
        start..if axis.offset(frozen) < viewport {
            end
        } else {
            start
        },
    )
}

struct Label {
    rect: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    text: SharedString,
    alignment: Alignment,
    header: bool,
    selected: bool,
    cursor: bool,
    handle: bool,
}

struct Frame {
    bounds: Bounds<Pixels>,
    labels: Vec<Label>,
}

impl SheetState {
    fn labels(&self, bounds: Bounds<Pixels>) -> Frame {
        let (rows, cols) = self.model.dimensions();
        let width = f64::from(f32::from(bounds.size.width));
        let height = f64::from(f32::from(bounds.size.height));
        let gutter = self.gutter();
        let header = self.header();
        let frozen_x = gutter + self.cols.offset(self.frozen.col);
        let frozen_y = header + self.rows.offset(self.frozen.row);
        let mut frame = Frame {
            bounds,
            labels: Vec::new(),
        };
        let visible_cols: Vec<_> = visible(
            &self.cols,
            self.frozen.col,
            self.scroll.0,
            width - gutter,
            cols,
        )
        .collect();
        let cursor = self.model.selection.cursor;
        let end = self
            .model
            .selection
            .rect()
            .map(|(r, c)| CellPos::new(r.end - 1, c.end - 1));
        let mut add = |rect: Bounds<Pixels>,
                       clip: Bounds<Pixels>,
                       text,
                       alignment,
                       header,
                       selected,
                       cursor,
                       handle| {
            frame.labels.push(Label {
                rect,
                clip,
                text,
                alignment,
                header,
                selected,
                cursor,
                handle,
            });
        };
        for &col in &visible_cols {
            let mut rect = self.rect(CellPos::new(0, col));
            rect.origin.y = px(0.);
            rect.size.height = px(header as f32);
            let x = if col < self.frozen.col {
                gutter
            } else {
                frozen_x
            };
            add(
                rect,
                bounds_at(x, 0., (width - x).max(0.), header),
                column_name(col).into(),
                Alignment::Center,
                true,
                false,
                false,
                false,
            );
        }
        for row in visible(
            &self.rows,
            self.frozen.row,
            self.scroll.1,
            height - header,
            rows,
        ) {
            let mut rect = self.rect(CellPos::new(row, 0));
            rect.origin.x = px(0.);
            rect.size.width = px(gutter as f32);
            let y = if row < self.frozen.row {
                header
            } else {
                frozen_y
            };
            add(
                rect,
                bounds_at(0., y, gutter, (height - y).max(0.)),
                (row as u128 + 1).to_string().into(),
                Alignment::Center,
                true,
                false,
                false,
                false,
            );
            for &col in &visible_cols {
                let pos = CellPos::new(row, col);
                let x = if col < self.frozen.col {
                    gutter
                } else {
                    frozen_x
                };
                let (text, alignment) = self.label(pos);
                add(
                    self.rect(pos),
                    bounds_at(x, y, (width - x).max(0.), (height - y).max(0.)),
                    text,
                    alignment,
                    false,
                    self.model.selection.is_cell_selected(row, col),
                    cursor == Some(pos),
                    end == Some(pos) && !self.model.read_only,
                );
            }
        }
        frame
    }

    fn label(&self, pos: CellPos) -> (SharedString, Alignment) {
        let Some(cell) = self.model.cell(pos) else {
            return (SharedString::default(), Alignment::Left);
        };
        let alignment = cell.format.align;
        if cell.format.number == NumberFormat::Text {
            return (SharedString::from(cell.text.as_ref()), alignment);
        }
        let number = if cell.text.starts_with('=') {
            match self.values.get(&pos) {
                Some(Ok(value)) => Some(*value),
                Some(Err(error)) => return (error.to_string().into(), alignment),
                None => return ("…".into(), alignment),
            }
        } else {
            cell.text.parse::<f64>().ok().filter(|n| n.is_finite())
        };
        let text = match (number, cell.format.number) {
            (Some(value), NumberFormat::Number(decimals)) => format!(
                "{value:.precision$}",
                precision = usize::from(decimals.min(15))
            )
            .into(),
            (Some(value), NumberFormat::Date) => iso_date(value).into(),
            (Some(value), _) if cell.text.starts_with('=') => value.to_string().into(),
            _ => SharedString::from(cell.text.as_ref()),
        };
        (text, alignment)
    }
}

fn iso_date(value: f64) -> String {
    if !(-719_528. ..=2_932_896.).contains(&value) {
        return "#VALUE!".to_owned();
    }
    // Gregorian civil-from-days, shifting the year boundary to March.
    let day = value.floor() as i64 + 719_468;
    let era = day.div_euclid(146_097);
    let day_of_era = day.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = (month_index + 2) % 12 + 1;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

impl Frame {
    fn paint(self, window: &mut Window, cx: &mut App) {
        let theme = GuiraffeTheme::global(cx);
        let palette = theme.surfaces();
        let selected = GuiraffeTheme::hsla(palette.mark);
        let accent = GuiraffeTheme::hsla(theme.primary_color());
        window.paint_quad(fill(self.bounds, GuiraffeTheme::hsla(palette.body)));
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let text_system = window.text_system().clone();
        for label in self.labels {
            let rect = Bounds::new(self.bounds.origin + label.rect.origin, label.rect.size);
            let clip = Bounds::new(self.bounds.origin + label.clip.origin, label.clip.size)
                .intersect(&rect);
            if clip.size.width <= px(0.) || clip.size.height <= px(0.) {
                continue;
            }
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                let background = if label.selected {
                    selected
                } else if label.header {
                    GuiraffeTheme::hsla(palette.surface)
                } else {
                    GuiraffeTheme::hsla(palette.body)
                };
                window.paint_quad(fill(rect, background));
                let line = if label.cursor {
                    accent
                } else {
                    GuiraffeTheme::hsla(palette.border)
                };
                let right = rect.right() - px(1.);
                let bottom = rect.bottom() - px(1.);
                window.paint_quad(fill(
                    Bounds::new(point(right, rect.origin.y), size(px(1.), rect.size.height)),
                    line,
                ));
                window.paint_quad(fill(
                    Bounds::new(point(rect.origin.x, bottom), size(rect.size.width, px(1.))),
                    line,
                ));
                if label.cursor {
                    window.paint_quad(fill(
                        Bounds::new(rect.origin, size(rect.size.width, px(1.))),
                        line,
                    ));
                    window.paint_quad(fill(
                        Bounds::new(rect.origin, size(px(1.), rect.size.height)),
                        line,
                    ));
                }
                if label.handle {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(rect.right() - px(6.), rect.bottom() - px(6.)),
                            size(px(6.), px(6.)),
                        ),
                        accent,
                    ));
                }
                if label.text.is_empty() {
                    return;
                }
                let mut run = style.to_run(label.text.len());
                run.color = GuiraffeTheme::hsla(if label.header {
                    palette.dimmed
                } else {
                    palette.text
                });
                let layout =
                    text_system.layout_line(label.text.as_ref(), font_size, &[run.clone()], None);
                let room = (rect.size.width - px(12.) - layout.width).max(px(0.));
                let shift = match label.alignment {
                    Alignment::Left => px(0.),
                    Alignment::Center => room / 2.,
                    Alignment::Right => room,
                };
                let origin =
                    rect.origin + point(px(6.) + shift, (rect.size.height - line_height) / 2.);
                let decorations = [gpui_kit::DecorationRun {
                    len: run.len as u32,
                    color: run.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                let _ = layout.paint(
                    origin,
                    line_height,
                    style.text_align,
                    None,
                    &decorations,
                    window,
                    cx,
                );
            });
        }
    }
}
