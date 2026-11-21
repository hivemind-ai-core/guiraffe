//! Chat metadata shared with the authoring catalogue generator.

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

pub static CHAT: WidgetSpec = WidgetSpec {
    id: "chat",
    krate: "chat",
    summary: "Conversation with Markdown history, tool and reasoning disclosures, and an editable composer.",
    props: &[
        prop("title", "string", "\"Workspace assistant\""),
        prop(
            "subtitle",
            "string",
            "\"A little context. A clearer next step.\"",
        ),
        prop("messages", "chat-messages", "[]"),
        prop("draft", "string", "\"\""),
        prop("attachments", "chat-attachments", "[]"),
        prop("commands", "chat-commands", "[]"),
        prop("mentions", "chat-commands", "[]"),
        prop("streaming-message", "integer", "null"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "submit",
            payload_type: "chat-submit",
        },
        EventSpec {
            id: "retry",
            payload_type: "integer",
        },
        EventSpec {
            id: "edit",
            payload_type: "integer",
        },
        EventSpec {
            id: "stop",
            payload_type: "integer",
        },
        EventSpec {
            id: "attachment-request",
            payload_type: "null",
        },
        EventSpec {
            id: "finish",
            payload_type: "integer",
        },
    ],
    state: "keyed conversation with document-supplied history and native composer state",
    theme_tokens: &[],
    a11y_role: "group",
};

pub const WIDGETS: &[&WidgetSpec] = &[&CHAT];
