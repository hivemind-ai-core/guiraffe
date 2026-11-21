//! Drag-and-drop metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

pub static SORTABLE_BOARD: WidgetSpec = WidgetSpec {
    id: "sortable-board",
    krate: "dnd",
    summary: "Sortable lists or card grids with keyboard and pointer drag.",
    props: &[
        PropSpec {
            name: "columns",
            type_name: "board-columns",
            default_json: "[]",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "cards-per-row",
            type_name: "number",
            default_json: "1",
            accepted: &[],
            range: Some((1.0, u16::MAX as f64)),
            required: false,
            token_backed: false,
        },
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "reorder",
        payload_type: "board-reorder",
    }],
    state: "keyed board with local reorder state",
    theme_tokens: &[],
    a11y_role: "group",
};

pub const WIDGETS: &[&WidgetSpec] = &[&SORTABLE_BOARD];
