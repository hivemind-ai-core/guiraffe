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

pub const WIDGETS: &[&WidgetSpec] = &[&PASSWORD_STRENGTH];
