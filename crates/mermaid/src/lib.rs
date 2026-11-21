//! Standalone Mermaid diagram viewer control (pan/zoom, theming).
//!
//! Two layers:
//!
//! - [`engine`], [`viewport`], [`sizing`], [`source`] and [`theme`] are pure
//!   Rust (only the SVG rasterizer type crosses in): parsing, layout,
//!   rasterizing, caching, admission limits and pan/zoom math. They are
//!   unit-tested without a window.
//! - [`MermaidView`] is the GPUI element. It measures itself, asks a
//!   background task to draw and rasterize, and paints the result with pan,
//!   wheel zoom and fit. Rendering never runs on the UI thread.
//!
//! ```no_run
//! use gpui_kit::*;
//! use guiraffe_mermaid::MermaidView;
//!
//! fn diagram() -> impl IntoElement {
//!     MermaidView::new("flow").source("flowchart LR\n  A --> B")
//! }
//! ```
//! Use the same builder from `ui!`:
//!
//! ```ignore
//! ui! { MermaidView("flow") { source: "flowchart LR\n  A --> B" } }
//! ```

pub mod engine;
#[cfg(test)]
mod host;
mod preview;
pub mod sizing;
pub mod source;
#[cfg(test)]
mod tests;
pub mod theme;
pub mod view;
pub mod viewport;

pub use engine::{Drawn, Engine, Image, RenderError, Request, draw};
pub use sizing::{MAX_RASTER_EDGE, MAX_RASTER_PIXELS, dimensions_are_usable, raster_size};
pub use source::{
    CacheKey, MAX_SOURCE_BYTES, MAX_SOURCE_LINES, SourceError, admit, hash, normalize,
};
pub use theme::{DIAGRAM_FONT_SIZE, background_for, theme_for};
pub use view::{MermaidView, MermaidViewState};
pub use viewport::{MAX_ZOOM, MIN_ZOOM, Point, Viewport, ZOOM_STEP};

use guiraffe_core::widget_spec;
mod widget_specs;

/// Register MermaidView for document authoring.
pub fn init(cx: &mut gpui_kit::App) {
    if !cx.has_global::<guiraffe_core::GuiraffeTheme>() {
        guiraffe_core::init(cx);
    }
    use gpui_kit::IntoElement;
    use guiraffe_core::{
        widget_catalog::{WidgetRegistration, WidgetRegistry},
        widget_spec::Schema,
    };

    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: &Schema {
                props: &["source"],
                events: &[],
            },
            render: |node, _, _, _| {
                MermaidView::new(node.state_id())
                    .source(node.prop(0).display())
                    .into_any_element()
            },
        }],
    );
}
