//! GUIraffe data grid: a virtualized, zero-copy grid for millions of rows.
//!
//! - Data comes from a [`GridSource`] (borrowed cells, nothing copied); [`VecSource`],
//!   [`ArcSource`] and [`FnSource`] adapt common storage.
//! - Rows *and* columns are virtualized: only visible cells are built. Offsets are `f64`.
//! - Sorting/filtering produce a `u32` permutation on the background executor.
//! - Pure logic lives in [`window`], [`columns`], [`order`], [`selection`] and [`tsv`].

pub mod columns;
pub mod order;
pub mod selection;
pub mod source;
pub mod tsv;
pub mod window;

pub use columns::{Align, ColumnDef, ColumnLayout, Pin};
pub use order::{Filter, RowOrder, SortDir, SortKey};
pub use selection::{CellPos, Nav, SelectionMode};
pub use source::{ArcSource, CellRef, EditError, FnSource, GridSource, RowKey, VecSource};

mod grid;
pub use grid::{
    Activate, CONTEXT, CopySelection, DataGrid, EditCell, EditRequest, GridEvent, GridEvents,
    GridHandle, GridOptions, GridStats, GridView, RowView, SelectAll, Validator, init,
};
pub use selection::Selection;
