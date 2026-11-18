//! Metadata and positional renderer schemas for the built-in DataUi widgets.

use serde_json::{Value, json};

use crate::Schema;

pub const VERSION: &str = "1.0.0";

#[derive(Debug)]
pub struct PropSpec {
    pub name: &'static str,
    pub type_name: &'static str,
    pub default_json: &'static str,
    pub accepted: &'static [&'static str],
    pub range: Option<(f64, f64)>,
    pub token_backed: bool,
    pub required: bool,
}

#[derive(Debug)]
pub struct EventSpec {
    pub id: &'static str,
    pub payload_type: &'static str,
}

#[derive(Debug)]
pub struct WidgetSpec {
    pub id: &'static str,
    pub krate: &'static str,
    pub summary: &'static str,
    pub props: &'static [PropSpec],
    pub accepts_children: bool,
    pub events: &'static [EventSpec],
    pub state: &'static str,
    pub theme_tokens: &'static [&'static str],
    pub a11y_role: &'static str,
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
    TITLE: "title", "components", "Section heading.", false, "stateless", "heading", ["font_size"],
        ["text": "string" = "\"\"", &[] as &[&str], None, true;
         "order": "number" = "3", &[] as &[&str], Some((1.0, 6.0)), false], [];
    TEXT: "text", "components", "Body text with optional truncation.", false, "stateless", "text", ["font_size", "foreground", "muted", "palette"],
        ["text": "string" = "\"\"", &[] as &[&str], None, true;
         "dimmed": "boolean" = "false", &[] as &[&str], None, false;
         "size": "string" = "null", SIZE, None, false;
         "color": "string" = "null", COLOR, None, false;
         "single-line": "boolean" = "false", &[] as &[&str], None, false], [];
    HIGHLIGHT: "highlight", "components", "Text with matching terms highlighted.", false, "stateless", "text", ["primary", "foreground"],
        ["text": "string" = "\"\"", &[] as &[&str], None, true;
         "term": "string" = "\"\"", &[] as &[&str], None, false], [];
    PILL: "pill", "components", "Compact label.", false, "stateless", "text", ["palette", "radius"],
        ["label": "string" = "\"\"", &[] as &[&str], None, true], [];
    CARD: "card", "components", "Surface for grouped content.", true, "stateless", "group", ["surface", "border", "radius"], [], [];
    RING: "ring", "components", "Circular progress indicator.", false, "stateless", "progressbar", ["palette"],
        ["value": "number" = "0", &[] as &[&str], Some((0.0, 100.0)), false;
         "color": "string" = "\"blue\"", COLOR, None, false;
         "diameter": "number" = "64", &[] as &[&str], Some((1.0, f32::MAX as f64)), false;
         "label": "string" = "\"\"", &[] as &[&str], None, false], [];
    BUTTON: "button", "gpui-kit", "Clickable action.", false, "stateless", "button", ["button"],
        ["label": "string" = "\"\"", &[] as &[&str], None, true], ["click": "null"];
    HUG: "hug", "ui", "Content-width container.", true, "stateless", "group", [], [], [];
    SPACER: "spacer", "ui", "Flexible empty space.", false, "stateless", "presentation", [], [], [];
    TEXT_INPUT: "text-input", "gpui-kit", "Single-line text editor.", false, "controlled or uncontrolled value", "textbox", ["input"],
        ["value": "string" = "\"\"", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "string"];
    CHECKBOX: "checkbox", "gpui-kit", "Boolean checkbox.", false, "controlled or uncontrolled value", "checkbox", ["checkbox"],
        ["value": "boolean" = "false", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "boolean"];
    SWITCH: "switch", "gpui-kit", "Boolean switch.", false, "controlled or uncontrolled value", "switch", ["switch"],
        ["value": "boolean" = "false", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "boolean"];
    SELECT: "select", "gpui-kit", "Select one of a list of labels.", false, "controlled or uncontrolled value", "combobox", ["select"],
        ["value": "string" = "null", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "string"];
}

impl PropSpec {
    fn default_value(&self) -> Value {
        serde_json::from_str(self.default_json).unwrap_or(Value::Null)
    }

    pub fn schema(&self) -> Value {
        let mut schema = json!({"type": self.type_name});
        if !self.accepted.is_empty() {
            schema["enum"] = json!(self.accepted);
        }
        if let Some((min, max)) = self.range {
            schema["minimum"] = json!(min);
            schema["maximum"] = json!(max);
        }
        if self.type_name == "array" {
            schema["items"] = json!({"type": "string"});
        }
        schema
    }
}
