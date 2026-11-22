//! Data grid metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

const fn prop(name: &'static str, type_name: &'static str, default_json: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name,
        default_json,
        range: None,
        accepted: &[],
        required: false,
        token_backed: false,
    }
}

const fn event(id: &'static str) -> EventSpec {
    EventSpec {
        id,
        payload_type: "data-grid-event",
    }
}

pub static DATA_GRID: WidgetSpec = WidgetSpec {
    id: "data-grid",
    krate: "datagrid",
    summary: "Virtualized grid with scalar cells, editable columns, sorting and selection.",
    props: &[
        prop("rows", "grid-rows", "[]"),
        prop("columns", "grid-columns", "[]"),
        PropSpec {
            accepted: &["none", "single-row", "multi-row", "cell", "range"],
            ..prop("selection", "string", "\"single-row\"")
        },
        prop("zebra", "boolean", "true"),
        PropSpec {
            accepted: &["xs", "sm", "md", "lg", "xl"],
            token_backed: true,
            ..prop("size", "string", "null")
        },
        PropSpec {
            accepted: &["compact", "comfortable", "spacious"],
            token_backed: true,
            ..prop("density", "string", "null")
        },
        prop("focusable", "boolean", "null"),
        PropSpec {
            range: Some((0.0, 1024.0)),
            ..prop("overscan", "number", "0")
        },
        PropSpec {
            range: Some((0.0, u32::MAX as f64)),
            ..prop("sync-below", "number", "50000")
        },
        prop("label", "string", "\"Data grid\""),
    ],
    accepts_children: false,
    events: &[
        event("selection-change"),
        event("sort-change"),
        event("cell-edit"),
        event("edit-cancel"),
        event("copy"),
        event("edit-reject"),
        event("row-activate"),
        event("column-resize"),
        event("column-reorder"),
        event("cells-change"),
        event("cells-reject"),
    ],
    state: "keyed grid with local selection and edits, driven by document row snapshots",
    theme_tokens: &[],
    a11y_role: "grid",
};

pub const WIDGETS: &[&WidgetSpec] = &[&DATA_GRID];
