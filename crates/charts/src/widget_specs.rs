//! Chart metadata shared with the authoring catalogue generator.

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

pub static CHART: WidgetSpec = WidgetSpec {
    id: "chart",
    krate: "charts",
    summary: "Layered chart with configurable scales, series, facet labels and viewport controls.",
    props: &[
        prop("summary", "string", "\"Chart\""),
        prop("layers", "chart-layers", "[]"),
        prop("axes", "chart-axes", "[]"),
        prop("facet-labels", "chart-labels", "[]"),
        PropSpec {
            range: Some((64.0, 4096.0)),
            ..prop("height", "number", "290")
        },
        prop("legend", "boolean", "true"),
        prop("crosshair", "boolean", "true"),
        prop("tooltip", "boolean", "true"),
        prop("navigation", "boolean", "true"),
        prop("keyboard", "boolean", "true"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "select",
            payload_type: "chart-datum",
        },
        EventSpec {
            id: "viewport-change",
            payload_type: "chart-domain",
        },
        EventSpec {
            id: "brush",
            payload_type: "chart-domain",
        },
        EventSpec {
            id: "series-visibility",
            payload_type: "chart-series-visibility",
        },
    ],
    state: "keyed chart with document-driven source and local viewport",
    theme_tokens: &[],
    a11y_role: "group",
};

pub const WIDGETS: &[&WidgetSpec] = &[&CHART];
