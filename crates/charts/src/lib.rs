//! Composable charts with borrowed sources and canvas geometry.
//!
//! A Rust `ui!` example.
//!
//! ```ignore
//! ui! { div { {ChartView::from_spec(spec, ChartOptions::default(), cx)} } }
//! ```

mod data;
mod model;
mod paint;
mod sample;
mod scale;
mod scene;
mod view;

pub use data::*;
pub use guiraffe_timeline::TimeFormat;
pub use model::*;
pub use paint::categorical_palette;
pub use sample::lttb;
pub use scale::*;
pub use scene::Scene;
pub use view::{ChartEvent, ChartOptions, ChartView, init};

use guiraffe_core::widget_spec;
mod widget_specs;
