//! Metadata and positional renderer schemas for the built-in DataUi widgets.

use serde_json::{Value, json};

/// The props and events a widget kind understands, in positional order.
#[derive(Debug)]
pub struct Schema {
    pub props: &'static [&'static str],
    pub events: &'static [&'static str],
}

impl Schema {
    pub const EMPTY: Schema = Schema {
        props: &[],
        events: &[],
    };
}

pub const VERSION: &str = "1.0.0";
pub const FORMAT_VERSION: u32 = 1;

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
    TEXT_INPUT: "text-input", "gpui-kit", "Single-line text editor.", false, "controlled or uncontrolled value", "textbox", ["control_size", "kit.input", "kit.foreground"],
        ["value": "string" = "\"\"", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "string"];
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
    SELECT: "select", "gpui-kit", "Select one of a list of labels.", false, "controlled or uncontrolled value", "combobox", ["control_size", "kit.input", "kit.foreground"],
        ["value": "string" = "null", &[] as &[&str], None, false;
         "label": "string" = "\"\"", &[] as &[&str], None, false;
         "options": "array" = "[]", &[] as &[&str], None, false;
         "disabled": "boolean" = "false", &[] as &[&str], None, false;
         "placeholder": "string" = "\"\"", &[] as &[&str], None, false], ["change": "selection"];
}

/// Look up an authoring descriptor by its document kind.
pub fn lookup(kind: &str) -> Option<&'static WidgetSpec> {
    WIDGETS.iter().copied().find(|widget| widget.id == kind)
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

impl EventSpec {
    fn schema(&self) -> Value {
        match self.payload_type {
            "selection" => json!({"type":["string","null"]}),
            ty => json!({"type":ty}),
        }
    }
}

impl WidgetSpec {
    pub fn example(&self) -> Value {
        let props: serde_json::Map<String, Value> = self
            .props
            .iter()
            .filter(|p| p.required)
            .map(|p| (p.name.into(), p.default_value()))
            .collect();
        json!({"version": FORMAT_VERSION, "root": {"kind": self.id, "props": props}})
    }

    fn description(&self) -> Value {
        let props = self
            .props
            .iter()
            .map(|p| {
                json!({"name":p.name, "schema":p.schema(), "default":p.default_value(),
                "required":p.required, "tokenBacked":p.token_backed})
            })
            .collect::<Vec<_>>();
        let events = self
            .events
            .iter()
            .map(|e| json!({"id":e.id,"payloadSchema":e.schema()}))
            .collect::<Vec<_>>();
        let slots = if self.accepts_children {
            json!([{"name":"children","min":0,"max":null,"accepted":KINDS}])
        } else {
            json!([])
        };
        json!({"id":self.id, "crate":self.krate, "summary":self.summary,"props":props,
            "slots":slots,"events":events,"state":self.state,"themeTokens":self.theme_tokens,
            "a11yRole":self.a11y_role,"examples":[{"name":"minimal","document":self.example()}]})
    }

    fn node_schema(&self) -> Value {
        let props: serde_json::Map<String, Value> = self
            .props
            .iter()
            .map(|p| {
                (
                    p.name.into(),
                    json!({"anyOf":[p.schema(),{"type":"string","pattern":"^@.+"}]}),
                )
            })
            .collect();
        let bind: serde_json::Map<String, Value> = self
            .props
            .iter()
            .map(|p| (p.name.into(), json!({"type":"string","minLength":1})))
            .collect();
        let events: serde_json::Map<String, Value> = self
            .events
            .iter()
            .map(|e| (e.id.into(), json!({"type":"string","minLength":1})))
            .collect();
        let mut children = json!({"type":"array","items":{"$ref":"#/$defs/node"}});
        if !self.accepts_children {
            children["maxItems"] = json!(0);
        }
        let required = self
            .props
            .iter()
            .filter(|p| p.required)
            .map(|p| {
                json!({"anyOf":[
                    {"required":["props"], "properties":{"props":{"required":[p.name]}}},
                    {"required":["bind"], "properties":{"bind":{"required":[p.name]}}}
                ]})
            })
            .collect::<Vec<_>>();
        let mut schema = json!({"type":"object","required":["kind"],"additionalProperties":false,
            "properties":{
                "kind":{"const":self.id},
                "id":{"type":["string","null"],"minLength":1},
                "each":{"type":["string","null"]},
                "when":{"type":["string","null"]},
                "key":{"type":["string","null"]},
                "props":{"type":"object","properties":props,"additionalProperties":false},
                "bind":{"type":"object","properties":bind,"additionalProperties":false},
                "on":{"type":"object","properties":events,"additionalProperties":false},
                "children":children}});
        if !required.is_empty() {
            schema["allOf"] = json!(required);
        }
        schema
    }
}

pub fn catalogue() -> Value {
    json!({"version": VERSION, "widgets": WIDGETS.iter().map(|w| w.description()).collect::<Vec<_>>()})
}

pub fn screen_schema() -> Value {
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
        "type":"object","required":["version","root"],"additionalProperties":false,
        "properties":{"version":{"const":FORMAT_VERSION},"root":{"$ref":"#/$defs/node"}},
        "$defs":{"node":{"oneOf":WIDGETS.iter().map(|w| w.node_schema()).collect::<Vec<_>>()}}})
}

/// Render a reference for screen authors from the same metadata consumed by the CLI and showcase.
pub fn reference_markdown() -> String {
    use std::fmt::Write as _;

    let mut text = format!("# DataUi widget reference\n\nCatalogue {VERSION}\n");
    for widget in WIDGETS {
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
