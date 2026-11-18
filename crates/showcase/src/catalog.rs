//! The catalog: one [`Entry`] per showcased component (or tight family), grouped by crate.
//!
//! `covers` lists the public type names an entry demonstrates; the coverage test
//! (`tests/coverage.rs`) scans every crate for public `Render`/`RenderOnce` types and fails
//! when one is neither covered here nor listed in [`INTERNAL`] with a reason.

use gpui_kit::{AnyElement, AnyView, App, AppContext, Context, IntoElement, Render, Window};

/// Builds an entry's live demo.
pub type DemoFn = fn(&mut Window, &mut App) -> AnyView;

#[derive(Clone, Copy)]
pub struct Entry {
    /// Crate short name (`layout`, `components`, ...).
    pub krate: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub covers: &'static [&'static str],
    pub demo: DemoFn,
}

/// Public renderable types that are not standalone components (state entities, internal
/// hosts), with the reason they need no showcase entry of their own.
pub const INTERNAL: &[(&str, &str, &str)] = &[
    ("layout", "Col", "child of Grid; shown in the Grid entry"),
    (
        "components",
        "CardSection",
        "child of Card; shown in the Card entry",
    ),
    (
        "components",
        "ScopedButton",
        "focus-scope prototype (spec/18 §12 audit); a Kit Button with opt-in tab stop",
    ),
    (
        "core",
        "TextMenuLayer",
        "popup host of the shared text context menu; shown via CodeViewer/Markdown/Chat right-click",
    ),
    (
        "devtools",
        "DevtoolsRoot",
        "feature-gated developer overlay host, not an app component; shown in the gallery (`--features devtools`)",
    ),
    (
        "devtools",
        "DevtoolsPanel",
        "the devtools overlay's own panel leaf; snapshot-tested in guiraffe-devtools",
    ),
    (
        "mermaid",
        "Host",
        "private test host in a non-public module (`mermaid::host`)",
    ),
];

/// Every entry, in display order.
pub fn entries() -> Vec<Entry> {
    let mut v = Vec::new();
    v.extend(crate::demos::layout::entries());
    v.extend(crate::demos::components::entries());
    v.extend(crate::demos::components_more::entries());
    v.extend(crate::demos::inputs::entries());
    v.extend(crate::demos::overlays::entries());
    v.extend(crate::demos::commands::entries());
    v.extend(crate::demos::code::entries());
    v.extend(crate::demos::markdown::entries());
    v.extend(crate::demos::mermaid::entries());
    v.extend(crate::demos::chat::entries());
    v.extend(crate::demos::flow::entries());
    v.extend(crate::demos::timeline::entries());
    v.extend(crate::demos::datagrid::entries());
    v.extend(crate::demos::dnd::entries());
    v.extend(crate::demos::ui::entries());
    v.extend(crate::demos::kit::entries());
    v.extend(crate::demos::charts::entries());
    v.extend(crate::recipes::entries());
    v.extend(crate::demos::composed_charts::entries());
    v.extend(crate::demos::flame_tree::entries());
    v
}

/// Case-insensitive match on name, crate, summary or covered type.
pub fn matches(e: &Entry, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty()
        || e.name.to_lowercase().contains(&q)
        || e.krate.contains(&q)
        || e.summary.to_lowercase().contains(&q)
        || e.covers.iter().any(|c| c.to_lowercase().contains(&q))
}

/// A view that re-runs a pure element builder each render: for stateless demos.
pub struct Stateless(pub fn(&mut Window, &mut App) -> AnyElement);

impl Render for Stateless {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.0)(window, cx)
    }
}

pub fn stateless(f: fn(&mut Window, &mut App) -> AnyElement, cx: &mut App) -> AnyView {
    cx.new(|_| Stateless(f)).into()
}
