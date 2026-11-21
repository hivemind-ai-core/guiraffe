//! Sparse spreadsheet model and virtualized control.
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
