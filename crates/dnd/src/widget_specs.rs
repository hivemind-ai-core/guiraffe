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

pub const WIDGETS: &[&WidgetSpec] = &[&SORTABLE_BOARD, &DOCK];

pub static DOCK: WidgetSpec = WidgetSpec {
    id: "dock",
    krate: "dnd",
    summary: "Versioned dock with tab panels, side areas, optional native drag and layout persistence.",
    props: &[
        PropSpec {
            name: "panels",
            type_name: "dock-panels",
            default_json: "[]",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "layout",
            type_name: "dock-state",
            default_json: "null",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "version",
            type_name: "integer",
            default_json: "1",
            accepted: &[],
            range: Some((1.0, u32::MAX as f64)),
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "native-drag",
            type_name: "boolean",
            default_json: "false",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "persist-key",
            type_name: "string",
            default_json: "null",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "left-size",
            type_name: "number",
            default_json: "170",
            accepted: &[],
            range: Some((1.0, 4096.0)),
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "right-size",
            type_name: "number",
            default_json: "170",
            accepted: &[],
            range: Some((1.0, 4096.0)),
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "bottom-size",
            type_name: "number",
            default_json: "130",
            accepted: &[],
            range: Some((1.0, 4096.0)),
            required: false,
            token_backed: false,
        },
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "layout-change",
            payload_type: "dock-state",
        },
        EventSpec {
            id: "error",
            payload_type: "string",
        },
    ],
    state: "keyed dock with versioned layout and optional application layout store",
    theme_tokens: &[],
    a11y_role: "group",
};
