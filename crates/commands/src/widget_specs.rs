//! Native command-routing metadata shared with the DataUi catalogue generator.

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

pub static COMMAND_SCOPE: WidgetSpec = WidgetSpec {
    id: "command-scope",
    krate: "commands",
    summary: "Routes keyboard commands from its children to the application registry.",
    props: &[],
    accepts_children: true,
    events: &[],
    state: "application-owned registry and native keyboard scope",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static COMMAND_MENU: WidgetSpec = WidgetSpec {
    id: "command-menu",
    krate: "commands",
    summary: "Command list backed by document entries or the application's command registry.",
    props: &[
        prop("commands", "menu-commands", "null"),
        prop("show-categories", "boolean", "true"),
        prop("show-keybindings", "boolean", "true"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "invoke",
            payload_type: "string",
        },
        EventSpec {
            id: "error",
            payload_type: "string",
        },
    ],
    state: "application-wide command registry or document-owned local registry",
    theme_tokens: &["control_size", "surfaces.text"],
    a11y_role: "group",
};

pub static COMMAND_RIBBON: WidgetSpec = WidgetSpec {
    id: "command-ribbon",
    krate: "commands",
    summary: "Keyboard-first contextual command bar over the command registry, opened by a leader key or a status-bar button.",
    props: &[
        prop("rows-per-column", "integer", "6"),
        prop("show-icons", "boolean", "true"),
        prop("show-shortcuts", "boolean", "true"),
        prop("max-columns", "integer", "8"),
        prop("row-height", "number", "null"),
        prop("open", "boolean", "false"),
        prop("choices", "object", "{}"),
        prop("label", "string", "\"Commands\""),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "opened",
            payload_type: "null",
        },
        EventSpec {
            id: "closed",
            payload_type: "null",
        },
        EventSpec {
            id: "ran",
            payload_type: "string",
        },
        EventSpec {
            id: "refused",
            payload_type: "string",
        },
    ],
    state: "per-window RibbonState over the application command registry",
    theme_tokens: &["surfaces.surface", "surfaces.border", "primary"],
    a11y_role: "menu",
};

pub const WIDGETS: &[&WidgetSpec] = &[&COMMAND_SCOPE, &COMMAND_MENU, &COMMAND_RIBBON];
