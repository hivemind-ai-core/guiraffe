//! Flow widget metadata shared with the authoring catalogue generator.

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

pub static FLOW: WidgetSpec = WidgetSpec {
    id: "flow",
    krate: "flow",
    summary: "Interactive node and edge graph with editable selection and a retained viewport.",
    props: &[
        prop("graph", "flow-graph", "{\"nodes\":{},\"edges\":{}}"),
        PropSpec {
            accepted: &["dots", "lines", "cross", "none"],
            ..prop("background", "string", "\"dots\"")
        },
        prop("minimap", "boolean", "true"),
        prop("controls", "boolean", "true"),
        prop("toolbar", "boolean", "true"),
        PropSpec {
            range: Some((0.0, 4096.0)),
            ..prop("snap-grid", "number", "20")
        },
        prop("fit-on-init", "boolean", "true"),
        prop("allow-self", "boolean", "false"),
        prop("allow-parallel", "boolean", "false"),
        prop("allow-cycles", "boolean", "true"),
        prop("pan-x", "number", "0"),
        prop("pan-y", "number", "0"),
        PropSpec {
            range: Some((0.02, 4.0)),
            ..prop("zoom", "number", "1")
        },
        prop("layout", "flow-layout", "null"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "change",
            payload_type: "flow-graph",
        },
        EventSpec {
            id: "selection-change",
            payload_type: "flow-selection",
        },
        EventSpec {
            id: "viewport-change",
            payload_type: "flow-viewport",
        },
        EventSpec {
            id: "error",
            payload_type: "string",
        },
    ],
    state: "keyed document-backed graph with local undo and viewport",
    theme_tokens: &[],
    a11y_role: "group",
};

pub const WIDGETS: &[&WidgetSpec] = &[&FLOW];
