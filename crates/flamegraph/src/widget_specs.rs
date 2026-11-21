//! Flame graph metadata shared with the authoring catalogue generator.

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

pub static FLAME_GRAPH: WidgetSpec = WidgetSpec {
    id: "flame-graph",
    krate: "flamegraph",
    summary: "Flame or icicle profile with search, zoom and frame activation.",
    props: &[
        prop("frames", "flame-frames", "[]"),
        PropSpec {
            accepted: &["flame", "icicle"],
            ..prop("orientation", "string", "\"flame\"")
        },
        PropSpec {
            accepted: &["name", "package", "heat"],
            ..prop("colour-scheme", "string", "\"name\"")
        },
        prop("search", "string", "\"\""),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "frame-click",
        payload_type: "flame-frame-click",
    }],
    state: "keyed viewport and indexed frame data",
    theme_tokens: &[],
    a11y_role: "group",
};

pub const WIDGETS: &[&WidgetSpec] = &[&FLAME_GRAPH];
