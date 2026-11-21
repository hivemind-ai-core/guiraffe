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

pub const WIDGETS: &[&WidgetSpec] = &[&PASSWORD_STRENGTH, &CHIP_GROUP];

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
