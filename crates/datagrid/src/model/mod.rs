//! The grid model shared by `DataGrid` and the future `Sheet` (spec/22 "Shared model"):
//! selection and range math, clipboard encode/decode with paste tiling, and the undo
//! transaction log. Pure Rust, no GPUI dependency, so overlapping behaviour is identical
//! and testable without a window.

pub mod clipboard;
pub mod selection;
pub mod undo;
