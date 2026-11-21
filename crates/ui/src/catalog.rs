//! Metadata and positional renderer schemas for the built-in DataUi widgets.

use serde_json::{Value, json};

pub use crate::widget_spec::{EventSpec, FORMAT_VERSION, PropSpec, Schema, WidgetSpec};

pub const VERSION: &str = "1.2.0";

use crate::widget_spec;

#[path = "../../inputs/src/widget_specs.rs"]
mod inputs;

pub fn all_widgets() -> impl Iterator<Item = &'static WidgetSpec> {
    WIDGETS.iter().chain(inputs::WIDGETS).copied()
}

const SIZE: &[&str] = &["xs", "sm", "md", "lg", "xl"];
const COLOR: &[&str] = &[
    "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal", "green",
    "lime", "yellow", "orange",
];

macro_rules! widgets {
    ($($key:ident: $id:literal, $krate:literal, $summary:literal, $children:literal,
        $state:literal, $role:literal, [$($token:literal),*],
        [$($name:literal: $ty:literal = $default:literal, $accepted:expr, $range:expr, $required:literal);*],
        [$($event:literal: $payload:literal),*];)*) => {
        $(pub(crate) static $key: (WidgetSpec, Schema) = (
            WidgetSpec {
                id: $id, krate: $krate, summary: $summary, accepts_children: $children,
                state: $state, a11y_role: $role, theme_tokens: &[$($token),*],
                props: &[$(PropSpec {name: $name, type_name: $ty, default_json: $default,
                    accepted: $accepted, range: $range, token_backed: !$accepted.is_empty(),
                    required: $required}),*],
                events: &[$(EventSpec {id: $event, payload_type: $payload}),*],
            },
            Schema { props: &[$($name),*], events: &[$($event),*] },
        );)*
        pub const KINDS: &[&str] = &[$($id),*];
        pub const WIDGETS: &[&WidgetSpec] = &[$(&$key.0),*];
    };
}

widgets! {
    STACK: "stack", "ui", "Vertical stack.", true, "stateless", "group", ["density.space"],
        ["gap": "string" = "\"sm\"", SIZE, None, false], [];
    GROUP: "group", "ui", "Wrapping horizontal group.", true, "stateless", "group", ["density.space"],
        ["gap": "string" = "\"sm\"", SIZE, None, false], [];
    GRID: "grid", "ui", "Equal-width grid rows.", true, "stateless", "group", ["density.space"],
        ["gap": "string" = "\"sm\"", SIZE, None, false;
         "columns": "number" = "2", &[] as &[&str], Some((1.0, 12.0)), false], [];
    TITLE: "title", "components", "Section heading.", false, "stateless", "heading", ["scale", "surfaces.text"],
        ["text": "string" = "\"\"", &[] as &[&str], None, true;
         "order": "number" = "3", &[] as &[&str], Some((1.0, 6.0)), false], [];
    TEXT: "text", "components", "Body text with optional truncation.", false, "stateless", "text", ["scale", "surfaces.text", "surfaces.dimmed", "variant.foreground"],
        ["text": "string" = "\"\"", &[] as &[&str], None, true;
         "dimmed": "boolean" = "false", &[] as &[&str], None, false;
         "size": "string" = "null", SIZE, None, false;
         "color": "string" = "null", COLOR, None, false;
         "single-line": "boolean" = "false", &[] as &[&str], None, false], [];
    HIGHLIGHT: "highlight", "components", "Text with matching terms highlighted.", false, "stateless", "text", ["surfaces.mark", "surfaces.text"],
        ["text": "string" = "\"\"", &[] as &[&str], None, true;
         "term": "string" = "\"\"", &[] as &[&str], None, false], [];
    PILL: "pill", "components", "Compact label.", false, "stateless", "text", ["control_size", "surfaces.track", "surfaces.text"],
        ["label": "string" = "\"\"", &[] as &[&str], None, true], [];
    CARD: "card", "components", "Surface for grouped content.", true, "stateless", "group", ["density.space", "default_radius", "recipes.card", "surfaces.body", "surfaces.text", "surfaces.border"], [], [];
    RING: "ring", "components", "Circular progress indicator.", false, "stateless", "progressbar", ["surfaces.track", "surfaces.text", "variant.background"],
        ["value": "number" = "0", &[] as &[&str], Some((0.0, 100.0)), false;
         "color": "string" = "\"blue\"", COLOR, None, false;
         "diameter": "number" = "64", &[] as &[&str], Some((1.0, f32::MAX as f64)), false;
         "label": "string" = "\"\"", &[] as &[&str], None, false], [];
    BUTTON: "button", "gpui-kit", "Clickable action.", false, "stateless", "button", ["kit.primary", "kit.primary_foreground"],
        ["label": "string" = "\"\"", &[] as &[&str], None, true], ["click": "null"];
    HUG: "hug", "ui", "Content-width container.", true, "stateless", "group", [], [], [];
    SPACER: "spacer", "ui", "Flexible empty space.", false, "stateless", "presentation", [], [], [];
    TEXT_INPUT: "text-input", "gpui-kit", "Single-line text editor.", false, "controlled or uncontrolled value", "textbox", ["control_size", "kit.input", "kit.foreground", "scale", "state.focus_ring"],
        ["value": "string" = "\"\"", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false;
         "invalid": "boolean" = "false", &[] as &[&str], None, false], ["change": "string"];
    CHECKBOX: "checkbox", "gpui-kit", "Boolean checkbox.", false, "controlled or uncontrolled value", "checkbox", ["control_size", "kit.primary", "kit.primary_foreground"],
        ["value": "boolean" = "false", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "boolean"];
    SWITCH: "switch", "gpui-kit", "Boolean switch.", false, "controlled or uncontrolled value", "switch", ["control_size", "kit.primary", "kit.switch"],
        ["value": "boolean" = "false", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "boolean"];
    SELECT: "select", "gpui-kit", "Select one of a list of labels.", false, "controlled or uncontrolled value", "combobox", ["control_size", "kit.input", "kit.foreground", "scale", "state.focus_ring"],
        ["value": "string" = "null", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false;
         "invalid": "boolean" = "false", &[] as &[&str], None, false], ["change": "selection"];
}

/// Look up an authoring descriptor by its document kind.
pub fn lookup(kind: &str) -> Option<&'static WidgetSpec> {
    all_widgets().find(|widget| widget.id == kind)
}

pub fn catalogue() -> Value {
    let kinds: Vec<_> = all_widgets().map(|widget| widget.id).collect();
    json!({"version": VERSION, "widgets": all_widgets().map(|w| w.description(&kinds)).collect::<Vec<_>>()})
}

pub fn screen_schema() -> Value {
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object","required":["version","root"],"additionalProperties":false,
        "properties":{"version":{"const":FORMAT_VERSION},"root":{"$ref":"#/$defs/node"}},
        "$defs":{"node":{"oneOf":all_widgets().map(|w| w.node_schema()).collect::<Vec<_>>()}}})
}

/// Render a reference for screen authors from the same metadata consumed by the CLI and showcase.
pub fn reference_markdown() -> String {
    use std::fmt::Write as _;

    let mut text = format!("# DataUi widget reference\n\nCatalogue {VERSION}\n");
    for widget in all_widgets() {
        let _ = write!(
            text,
            "\n## `{}`\n\n{}\n\n- Crate `{}`\n- State {}\n- Accessibility role `{}`\n",
            widget.id, widget.summary, widget.krate, widget.state, widget.a11y_role,
        );
        if widget.accepts_children {
            text.push_str("- The `children` slot accepts any number of document widgets.\n");
        }
        if !widget.theme_tokens.is_empty() {
            let _ = writeln!(text, "- Theme tokens {}", widget.theme_tokens.join(", "));
        }
        for property in widget.props {
            let required = if property.required {
                "required"
            } else {
                "optional"
            };
            let _ = writeln!(
                text,
                "- Property `{}` ({required}) accepts `{}` and defaults to `{}`.",
                property.name,
                property.schema(),
                property.default_json,
            );
        }
        for event in widget.events {
            let _ = writeln!(
                text,
                "- Event `{}` has payload `{}`.",
                event.id,
                event.schema()
            );
        }
        let example = serde_json::to_string_pretty(&widget.example()).unwrap_or_default();
        let _ = writeln!(text, "\n```json\n{example}\n```");
    }
    text
}
