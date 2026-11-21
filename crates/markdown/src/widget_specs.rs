//! Markdown metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

const fn prop(name: &'static str, type_name: &'static str, default_json: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name,
        default_json,
        accepted: &[],
        range: if matches!(type_name.as_bytes(), b"number") {
            Some((0.0, f32::MAX as f64))
        } else {
            None
        },
        token_backed: false,
        required: false,
    }
}

pub static MARKDOWN: WidgetSpec = WidgetSpec {
    id: "markdown",
    krate: "markdown",
    summary: "Renders Markdown from a literal or bound source string.",
    props: &[
        prop("source", "string", "\"\""),
        prop("selectable", "boolean", "true"),
        prop("scrollable", "boolean", "false"),
        prop("min-height", "number", "200"),
        prop("max-height", "number", "null"),
        prop("code-copy", "boolean", "true"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "link-click",
            payload_type: "string",
        },
        EventSpec {
            id: "task-toggle",
            payload_type: "markdown-task-toggle",
        },
    ],
    state: "keyed Markdown state driven by source",
    theme_tokens: &[],
    a11y_role: "document",
};

pub const WIDGETS: &[&WidgetSpec] = &[&MARKDOWN];
