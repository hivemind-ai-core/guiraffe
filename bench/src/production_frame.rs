//! Warmed, measured viewport updates for the production-scale controls.
use anyhow::{Context as _, Result};
use gpui_kit::{
    AppContext, Context, HeadlessAppContext, IntoElement, Render, Window, WindowHandle, px, size,
};
use guiraffe_code::{CodeDocument, CodeHandle, CodeViewer};
use guiraffe_datagrid::{CellRef, ColumnDef, DataGrid, FnSource, GridHandle};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(crate) enum Component {
    Grid,
    Code,
    Calendar,
}

#[derive(Clone)]
enum Host {
    Grid(GridHandle),
    Code(CodeHandle),
    Calendar(gpui_kit::Entity<guiraffe_calendar::CalendarState>),
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Grid(grid) => DataGrid::new(grid).into_any_element(),
            Self::Code(code) => CodeViewer::new(code).into_any_element(),
            Self::Calendar(state) => guiraffe_calendar::Calendar::new(state).into_any_element(),
        }
    }
}

pub(crate) struct Frame {
    app: HeadlessAppContext,
    window: WindowHandle<Host>,
    host: Host,
    first_row: usize,
    next_row: usize,
}

impl Frame {
    pub(crate) fn new(component: Component, rows: usize) -> Result<Self> {
        let document = match component {
            Component::Grid | Component::Calendar => None,
            Component::Code => Some(Arc::new(CodeDocument::new("x\n".repeat(rows - 1) + "x"))),
        };
        let calendar = match component {
            Component::Calendar => Some(calendar_model(rows)?),
            _ => None,
        };
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut app =
            HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), || {
                Ok(None)
            });
        let mut host = None;
        let window = app.open_window(size(px(1200.), px(800.)), |_, cx| {
            if matches!(component, Component::Calendar) {
                guiraffe_calendar::init(cx);
            }
            let control = match component {
                Component::Code => Host::Code(CodeHandle::new(
                    1,
                    document.unwrap_or_else(|| panic!("code frame document missing")),
                    cx,
                )),
                Component::Calendar => {
                    let model = calendar
                        .clone()
                        .unwrap_or_else(|| panic!("calendar frame model missing"));
                    let anchor = model
                        .items()
                        .first()
                        .map(|item| match item.time {
                            guiraffe_calendar::ItemTime::Timed { start, .. } => start.date_naive(),
                            guiraffe_calendar::ItemTime::AllDay { start, .. } => start,
                        })
                        .unwrap_or_else(|| panic!("calendar fixture has no anchor"));
                    Host::Calendar(cx.new(|cx| {
                        guiraffe_calendar::CalendarState::new(
                            model,
                            anchor,
                            guiraffe_calendar::CalendarZone::Utc,
                            cx,
                        )
                    }))
                }
                Component::Grid => {
                    let cols = (0..50)
                        .map(|col| ColumnDef::new(format!("c{col}"), format!("Col {col}"), col))
                        .collect();
                    let source =
                        FnSource::new(rows, |row, col| CellRef::Int((row * 100 + col) as i64));
                    Host::Grid(GridHandle::new(source, cols, cx))
                }
            };
            host = Some(control.clone());
            cx.new(|_| control)
        })?;
        let host = host.context("frame benchmark control was not mounted")?;
        let first_row = match component {
            Component::Grid => 4_200,
            Component::Code => 42_000,
            Component::Calendar => 0,
        };
        let mut fixture = Self {
            app,
            window,
            host,
            first_row,
            next_row: first_row,
        };
        for _ in 0..8 {
            fixture.draw()?;
        }
        Ok(fixture)
    }

    pub(crate) fn draw(&mut self) -> Result<()> {
        let row = self.next_row;
        self.app
            .update_window(self.window.into(), |_, window, cx| {
                match &self.host {
                    Host::Grid(grid) => grid.scroll_to_row(row, cx),
                    Host::Code(code) => code.scroll_to_line(row, cx),
                    Host::Calendar(state) => state.update(cx, |state, cx| {
                        state.navigate(if row == self.first_row { 1 } else { -1 }, cx)
                    }),
                }
                window.draw(cx).clear(cx);
            })?;
        self.app.run_until_parked();
        // The two jumps exercise the same visible content and row-number width
        // at both source sizes and ensure each call invalidates the viewport.
        self.next_row = if row == self.first_row {
            self.first_row + 64
        } else {
            self.first_row
        };
        Ok(())
    }
}

fn calendar_model(rows: usize) -> Result<guiraffe_calendar::CalendarModel> {
    use guiraffe_calendar::{CalendarItem, CalendarModel, CalendarSource, EventId, SourceId};

    let epoch =
        chrono::DateTime::from_timestamp(1_767_225_600, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH);
    let mut items = Vec::with_capacity(rows);
    for index in 0..rows {
        let offset = index % 365;
        let start = epoch
            + chrono::Duration::days(offset as i64)
            + chrono::Duration::hours((index % 16 + 6) as i64);
        let id = EventId(index as u64 + 1);
        let source = SourceId(index as u64 % 4 + 1);
        let item = if index % 10 == 0 {
            let date = start.date_naive();
            CalendarItem::all_day(
                id,
                source,
                "All-day fixture",
                date,
                date.succ_opt().context("all-day fixture date overflow")?,
            )?
        } else {
            CalendarItem::timed(
                id,
                source,
                "Timed fixture",
                start,
                start + chrono::Duration::minutes(30),
            )?
        };
        items.push(Arc::new(item));
    }
    let sources = (1..=4)
        .map(|id| CalendarSource::new(SourceId(id), "Fixture", guiraffe_core::ColorName::Blue));
    Ok(CalendarModel::new(items, sources)?)
}
