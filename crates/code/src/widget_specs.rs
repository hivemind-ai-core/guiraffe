//! Code viewer metadata shared with the authoring catalogue generator.

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

pub static CODE_VIEWER: WidgetSpec = WidgetSpec {
    id: "code-viewer",
    krate: "code",
    summary: "Read-only code with optional syntax highlighting, line numbers, minimap and annotations.",
    props: &[
        PropSpec {
            required: true,
            ..prop("source", "string", "\"// Example\"")
        },
        prop("language", "string", "\"\""),
        PropSpec {
            token_backed: true,
            accepted: &["xs", "sm", "md", "lg", "xl"],
            ..prop("size", "string", "null")
        },
        prop("line-numbers", "boolean", "true"),
        prop("minimap", "boolean", "true"),
        PropSpec {
            range: Some((0.0, 1024.0)),
            ..prop("margin-chars", "number", "24")
        },
        prop("annotations", "code-annotations", "[]"),
        prop("folds", "code-folds", "[]"),
        prop("decoration-layers", "code-decoration-layers", "[]"),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "decoration",
        payload_type: "code-decoration-action",
    }],
    state: "keyed viewer state driven by source, annotations and folds",
    theme_tokens: &[],
    a11y_role: "document",
};

pub const WIDGETS: &[&WidgetSpec] = &[&CODE_VIEWER];
