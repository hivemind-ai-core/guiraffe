//! Sparse spreadsheet model and virtualized control.
pub use guiraffe_datagrid::CellPos;

mod address;
pub use address::{a1, column_name, parse_a1};
mod formula;
pub use formula::{BuiltinEngine, Formula, FormulaEngine, FormulaError, FormulaValues, Reference};
