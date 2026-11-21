//! Native stateful component metadata used by the DataUi catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

const fn prop(name: &'static str, type_name: &'static str, default_json: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name,
        default_json,
        accepted: &[],
        range: None,
        required: false,
        token_backed: false,
    }
}

const SIZE: PropSpec = PropSpec {
    accepted: &["xs", "sm", "md", "lg", "xl"],
    token_backed: true,
    ..prop("size", "string", "null")
};

pub static SEGMENTED: WidgetSpec = WidgetSpec {
    id: "segmented-control",
    krate: "components",
    summary: "A native segmented selector with a single active option.",
    props: &[
        prop("items", "array", "[]"),
        PropSpec {
            range: Some((0.0, u32::MAX as f64)),
            ..prop("value", "integer", "0")
        },
        SIZE,
        prop("full-width", "boolean", "false"),
        prop("disabled", "boolean", "false"),
        prop("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "change",
        payload_type: "integer",
    }],
    state: "keyed one-of-many selection",
    theme_tokens: &["control_size", "variant.background", "variant.foreground"],
    a11y_role: "radiogroup",
};

pub static BURGER: WidgetSpec = WidgetSpec {
    id: "burger",
    krate: "components",
    summary: "Native navigation toggle with explicit labels for its open and closed states.",
    props: &[
        prop("value", "boolean", "false"),
        prop("animate-changes", "boolean", "true"),
        SIZE,
        PropSpec {
            accepted: &[
                "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal",
                "green", "lime", "yellow", "orange",
            ],
            token_backed: true,
            ..prop("color", "string", "null")
        },
        prop("focusable", "boolean", "null"),
        prop("closed-label", "string", "\"Open navigation\""),
        prop("open-label", "string", "\"Close navigation\""),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "change",
        payload_type: "boolean",
    }],
    state: "keyed open or closed state, optionally driven by a bound boolean",
    theme_tokens: &["control_size", "variant.foreground"],
    a11y_role: "button",
};

pub static ACCORDION: WidgetSpec = WidgetSpec {
    id: "accordion",
    krate: "components",
    summary: "Disclosure sections with one or more bodies expanded.",
    props: &[
        prop("items", "disclosure-items", "[]"),
        prop("value", "integer-array", "null"),
        prop("default-open", "integer-array", "[]"),
        PropSpec {
            accepted: &["contained", "separated", "filled"],
            ..prop("variant", "string", "\"contained\"")
        },
        PropSpec {
            accepted: &["start", "end"],
            ..prop("chevron", "string", "\"end\"")
        },
        prop("multiple", "boolean", "false"),
        prop("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "change",
        payload_type: "integer-array",
    }],
    state: "native disclosure state, controlled when the value is supplied",
    theme_tokens: &["surfaces.body", "surfaces.border", "variant.foreground"],
    a11y_role: "group",
};

pub static EXPANDING_LIST: WidgetSpec = WidgetSpec {
    id: "expanding-list",
    krate: "components",
    summary: "Animated disclosure list with reserved space for one expanded card.",
    props: &[
        prop("items", "disclosure-items", "[]"),
        PropSpec {
            range: Some((0.0, u32::MAX as f64)),
            ..prop("value", "integer", "null")
        },
        PropSpec {
            range: Some((0.0, f32::MAX as f64)),
            ..prop("body-height", "number", "100")
        },
        prop("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "change",
        payload_type: "nullable-integer",
    }],
    state: "native open item and local transition progress",
    theme_tokens: &["surfaces.body", "surfaces.border", "variant.foreground"],
    a11y_role: "group",
};

pub const WIDGETS: &[&WidgetSpec] = &[&SEGMENTED, &BURGER, &ACCORDION, &EXPANDING_LIST];
