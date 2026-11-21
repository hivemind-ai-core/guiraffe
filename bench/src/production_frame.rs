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
}

#[derive(Clone)]
enum Host {
    Grid(GridHandle),
    Code(CodeHandle),
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        match self {
            Self::Grid(grid) => DataGrid::new(grid).into_any_element(),
            Self::Code(code) => CodeViewer::new(code).into_any_element(),
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
            Component::Grid => None,
            Component::Code => Some(Arc::new(CodeDocument::new("x\n".repeat(rows - 1) + "x"))),
        };
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut app =
            HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), || {
                Ok(None)
            });
        let mut host = None;
        let window = app.open_window(size(px(1200.), px(800.)), |_, cx| {
            let control = if let Some(document) = document {
                Host::Code(CodeHandle::new(1, document, cx))
            } else {
                let cols = (0..50)
                    .map(|col| ColumnDef::new(format!("c{col}"), format!("Col {col}"), col))
                    .collect();
                let source = FnSource::new(rows, |row, col| CellRef::Int((row * 100 + col) as i64));
                Host::Grid(GridHandle::new(source, cols, cx))
            };
            host = Some(control.clone());
            cx.new(|_| control)
        })?;
        let host = host.context("frame benchmark control was not mounted")?;
        let first_row = match component {
            Component::Grid => 4_200,
            Component::Code => 42_000,
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
