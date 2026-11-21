//! Mermaid metadata shared with the authoring catalogue generator.

use super::widget_spec::{PropSpec, WidgetSpec};

pub static MERMAID: WidgetSpec = WidgetSpec {
    id: "mermaid",
    krate: "mermaid",
    summary: "Interactive Mermaid diagram with pan, zoom and fit.",
    props: &[PropSpec {
        name: "source",
        type_name: "string",
        default_json: "\"flowchart LR\\n  A --> B\"",
        accepted: &[],
        range: None,
        token_backed: false,
        required: true,
    }],
    accepts_children: false,
    events: &[],
    state: "keyed diagram state driven by source",
    theme_tokens: &[],
    a11y_role: "img",
};

pub const WIDGETS: &[&WidgetSpec] = &[&MERMAID];
