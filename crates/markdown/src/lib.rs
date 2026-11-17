//! Fast Markdown viewer control with optional Mermaid support.
//!
//! Two layers:
//!
//! - [`document`] (headings, fences, task items) and [`streaming`] (append
//!   classification, settled-prefix planning, render coalescing) are pure and
//!   unit tested without a window.
//! - [`MarkdownState`] and [`MarkdownView`] are the GPUI viewer. Rendering is
//!   the Kit's `TextView`, so GFM tables, task lists, footnotes, images,
//!   selection and syntax highlighting come with it; this crate adds the
//!   document-level analysis, incremental streaming and, behind the `mermaid`
//!   feature, diagrams drawn where their fences are.
//!
//! ```no_run
//! use gpui_kit::*;
//! use guiraffe_markdown::{MarkdownState, MarkdownView};
//!
//! fn reply(cx: &mut App) -> impl IntoElement {
//!     let state = cx.new(|cx| MarkdownState::new("# Hello", cx));
//!     MarkdownView::new("reply").state(&state).on_link_click(|url, _, _| {
//!         println!("{url}");
//!     })
//! }
//! ```

pub mod document;
pub mod streaming;
pub mod view;

pub use document::{
    Document, Fence, Heading, MAX_HEADING_DEPTH, MERMAID_LANG, TaskItem, apply_task_toggle,
    slugify, stable_hash, table_of_contents,
};
pub use streaming::{
    Change, MAX_COALESCE, MIN_COALESCE, classify, coalesce_delay, settled_prefix,
    unfinished_mermaid_fence,
};
pub use view::{DEFAULT_SCROLL_MIN_HEIGHT, MarkdownState, MarkdownView};

#[cfg(feature = "mermaid")]
pub use view::{EMBEDDED_DIAGRAM_HEIGHT, MERMAID_NODE};

#[cfg(test)]
mod tests;
