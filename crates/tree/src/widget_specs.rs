//! Tree widget metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

const fn prop(name: &'static str, type_name: &'static str, default_json: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name,
        default_json,
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }
}

const COMMON: &[PropSpec] = &[
    prop("nodes", "tree-nodes", "[]"),
    prop("focusable", "boolean", "false"),
    PropSpec {
        accepted: &["compact", "default", "comfortable"],
        ..prop("density", "string", "\"default\"")
    },
    prop("alternating", "boolean", "false"),
    prop("grid-lines", "boolean", "false"),
    prop("indent-guides", "boolean", "false"),
    prop("group-headers", "boolean", "false"),
    prop("sticky-headers", "boolean", "false"),
    prop("expanded", "integer-array", "[]"),
];
const EVENTS: &[EventSpec] = &[
    EventSpec {
        id: "load-children",
        payload_type: "integer",
    },
    EventSpec {
        id: "select",
        payload_type: "integer",
    },
    EventSpec {
        id: "check",
        payload_type: "tree-check",
    },
    EventSpec {
        id: "reorder",
        payload_type: "tree-reorder",
    },
];

pub static TREE_LIST: WidgetSpec = WidgetSpec {
    id: "tree-list",
    krate: "tree",
    summary: "Virtualized hierarchy. Rows are listed in depth-first order with optional depth.",
    props: COMMON,
    accepts_children: false,
    events: EVENTS,
    state: "keyed tree, with optional controlled expansion",
    theme_tokens: &[],
    a11y_role: "tree",
};

pub static PROPERTY_GRID: WidgetSpec = WidgetSpec {
    id: "property-grid",
    krate: "tree",
    summary: "Editable property inspector with optional search and a persisted column divider.",
    props: &[
        COMMON[0],
        COMMON[1],
        COMMON[2],
        COMMON[3],
        COMMON[4],
        COMMON[5],
        COMMON[6],
        COMMON[7],
        COMMON[8],
        prop("searchable", "boolean", "false"),
        PropSpec {
            range: Some((0.05, 0.95)),
            ..prop("split-ratio", "number", "null")
        },
        prop("persist-key", "string", "null"),
        prop("focus-row", "boolean", "true"),
        prop("reset-button", "boolean", "true"),
        PropSpec {
            accepted: &["hidden", "icons", "labels"],
            ..prop("edit-buttons", "string", "\"hidden\"")
        },
        // Group expand/collapse animation (spec/29); null follows MotionPrefs::layout.
        prop("animate-layout", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[
        EVENTS[0],
        EVENTS[1],
        EVENTS[2],
        EVENTS[3],
        EventSpec {
            id: "edit",
            payload_type: "tree-edit",
        },
        EventSpec {
            id: "pick-path",
            payload_type: "integer",
        },
    ],
    state: "keyed inspector with editable properties and optional controlled expansion",
    theme_tokens: &[],
    a11y_role: "treegrid",
};

pub const WIDGETS: &[&WidgetSpec] = &[&TREE_LIST, &PROPERTY_GRID];
