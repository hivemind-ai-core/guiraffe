//! Sparse spreadsheet model and virtualized GPUI control.
//!
//! The model can be prepared before mounting a view, with sheet extents independent
//! of the number of populated cells.
//!
//! ```
//! use guiraffe_sheet::{Cell, SheetModel, parse_a1};
//!
//! let mut model = SheetModel::new(1_000_000, 1_000);
//! if let Some(pos) = parse_a1("B2") {
//!     model.select(pos, false);
//!     model.edit([(pos, Cell::new("42"))]);
//!     assert_eq!(model.copy_tsv().as_deref(), Some("42"));
//! }
//! ```
pub use guiraffe_datagrid::CellPos;

mod address;
pub use address::{a1, column_name, parse_a1};
mod formula;
pub use formula::{BuiltinEngine, Formula, FormulaEngine, FormulaError, FormulaValues, Reference};
mod recalc;
pub use recalc::{Calculation, Recalculator};
mod model;
pub use model::{Alignment, Cell, CellFormat, NumberFormat, SheetModel};
mod axis;
pub use axis::Axis;
mod view;
pub use view::{Sheet, SheetState, init};
