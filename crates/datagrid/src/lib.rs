//! GUIraffe data grid: a virtualized, zero-copy grid for millions of rows.
//!
//! - Data comes from a [`GridSource`] (borrowed cells, nothing copied); [`VecSource`],
//!   [`ArcSource`] and [`FnSource`] adapt common storage.
//! - Rows *and* columns are virtualized: only visible cells are built. Offsets are `f64`.
//! - Sorting/filtering produce a `u32` permutation on the background executor.
//! - Pure logic lives in [`window`], [`columns`], [`order`] and [`model`] (selection, clipboard
//!   and undo, shared with the future Sheet); [`selection`] and [`tsv`] are aliases.
//!
//! A Rust `ui!` example. Create shared entities before rendering when the host owns their state.
//!
//! ```ignore
//! ui! { DataGrid(&handle) }
//! ```

pub mod columns;
pub mod model;
pub mod order;
pub use model::undo::HistoryLimits;
pub use model::{clipboard as tsv, selection};
pub mod source;
pub mod window;

pub use columns::{Align, ColumnDef, ColumnLayout, Pin};
pub use order::{Filter, RowOrder, SortDir, SortKey};
pub use selection::{CellPos, Nav, SelectionMode};
pub use source::{ArcSource, CellRef, EditError, FnSource, GridSource, RowKey, VecSource};

mod data_ui;
mod grid;
use guiraffe_core::widget_spec;
mod widget_specs;
pub use grid::{
    Activate, CONTEXT, CellRejection, ClearCells, CopySelection, Cut, DataGrid, EditCell, EditOp,
    EditRequest, GridEvent, GridEvents, GridHandle, GridOptions, GridStats, INLINE_COPY_CELLS,
    MAX_EDIT_CELLS, Paste, REJECTION_SAMPLES, Redo, SelectAll, Undo, Validator, aria_row_index,
    init,
};
pub use selection::Selection;
