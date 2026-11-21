//! Metadata shared by the inputs renderer and the authoring catalogue generator.

use super::widget_spec::{PropSpec, WidgetSpec};

pub static PASSWORD_STRENGTH: WidgetSpec = WidgetSpec {
    id: "password-strength",
    krate: "inputs",
    summary: "Four-segment password strength meter.",
    props: &[PropSpec {
        name: "password",
        type_name: "string",
        default_json: "\"\"",
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }],
    accepts_children: false,
    events: &[],
    state: "stateless",
    theme_tokens: &[],
    a11y_role: "meter",
};

pub const WIDGETS: &[&WidgetSpec] = &[
    &PASSWORD_STRENGTH,
    &CHIP_GROUP,
    &RANGE_SLIDER,
    &DROPZONE,
    &TAGS_INPUT,
];

const fn option(
    name: &'static str,
    type_name: &'static str,
    default_json: &'static str,
) -> PropSpec {
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

pub static CHIP_GROUP: WidgetSpec = WidgetSpec {
    id: "chip-group",
    krate: "inputs",
    summary: "A row of toggleable chips. Selection is a list of zero-based option indices.",
    props: &[
        option("options", "array", "[]"),
        option("multiple", "boolean", "false"),
        option("value", "integer-array", "[]"),
        option("disabled", "boolean", "false"),
        option("focusable", "boolean", "null"),
        PropSpec {
            name: "color",
            type_name: "string",
            default_json: "null",
            accepted: &[
                "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal",
                "green", "lime", "yellow", "orange",
            ],
            range: None,
            token_backed: true,
            required: false,
        },
    ],
    accepts_children: false,
    events: &[super::widget_spec::EventSpec {
        id: "change",
        payload_type: "integer-array",
    }],
    state: "controlled or uncontrolled selection",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static RANGE_SLIDER: WidgetSpec = WidgetSpec {
    id: "range-slider",
    krate: "inputs",
    summary: "Two-thumb slider. The value is an array of low and high bounds.",
    props: &[
        option("min", "number", "0"),
        option("max", "number", "100"),
        option("step", "number", "1"),
        option("min-distance", "number", "0"),
        option("value", "range-value", "[0,100]"),
        option("label", "string", "\"Range\""),
        option("disabled", "boolean", "false"),
        option("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[super::widget_spec::EventSpec {
        id: "change",
        payload_type: "range-value",
    }],
    state: "controlled or uncontrolled range",
    theme_tokens: &[],
    a11y_role: "slider",
};

pub static DROPZONE: WidgetSpec = WidgetSpec {
    id: "dropzone",
    krate: "inputs",
    summary: "File-drop target with extension or MIME-family filters and a browse event.",
    props: &[
        option("accept", "array", "[]"),
        option("label", "string", "\"Drop files\""),
        option("disabled", "boolean", "false"),
        option("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[
        super::widget_spec::EventSpec {
            id: "accept",
            payload_type: "string-array",
        },
        super::widget_spec::EventSpec {
            id: "reject",
            payload_type: "string-array",
        },
        super::widget_spec::EventSpec {
            id: "browse",
            payload_type: "null",
        },
    ],
    state: "keyed focus and drag target",
    theme_tokens: &[],
    a11y_role: "button",
};

pub static TAGS_INPUT: WidgetSpec = WidgetSpec {
    id: "tags-input",
    krate: "inputs",
    summary: "Editable string tags with duplicate and length rules.",
    props: &[
        option("value", "array", "[]"),
        option("placeholder", "string", "\"\""),
        option("allow-duplicates", "boolean", "false"),
        option("split-chars", "string", "\",\""),
        PropSpec {
            range: Some((0.0, 65536.0)),
            ..option("max-tags", "number", "null")
        },
        PropSpec {
            range: Some((0.0, 65536.0)),
            ..option("max-len", "number", "64")
        },
    ],
    accepts_children: false,
    events: &[
        super::widget_spec::EventSpec {
            id: "change",
            payload_type: "string-array",
        },
        super::widget_spec::EventSpec {
            id: "reject",
            payload_type: "tag-rejection",
        },
    ],
    state: "controlled or uncontrolled tags",
    theme_tokens: &[],
    a11y_role: "group",
};
