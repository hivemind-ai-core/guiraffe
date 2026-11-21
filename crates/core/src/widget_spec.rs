//! Shared metadata for declarative widget registration.

/// The props and events a widget kind understands, in positional order.
#[derive(Debug)]
pub struct Schema {
    pub props: &'static [&'static str],
    pub events: &'static [&'static str],
}

impl Schema {
    pub fn prop_index(&self, name: &str) -> Option<usize> {
        self.props.iter().position(|prop| *prop == name)
    }

    pub const EMPTY: Schema = Schema {
        props: &[],
        events: &[],
    };
}

#[derive(Clone, Copy, Debug)]
pub struct PropSpec {
    pub name: &'static str,
    pub type_name: &'static str,
    pub default_json: &'static str,
    pub accepted: &'static [&'static str],
    pub range: Option<(f64, f64)>,
    pub token_backed: bool,
    pub required: bool,
}

#[derive(Clone, Copy, Debug)]
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

use serde_json::{Value, json};

pub const FORMAT_VERSION: u32 = 1;

impl PropSpec {
    fn default_value(&self) -> Value {
        serde_json::from_str(self.default_json).unwrap_or(Value::Null)
    }

    pub fn schema(&self) -> Value {
        let ty = match self.type_name {
            "ring-sections" | "integer-array" | "code-annotations" | "tree-nodes"
            | "range-value" => "array",
            ty => ty,
        };
        let mut schema = if self.type_name == "range-value" {
            range_schema()
        } else {
            json!({"type": ty})
        };
        if !self.accepted.is_empty() && ty == "string" {
            schema["enum"] = json!(self.accepted);
        }
        if let Some((min, max)) = self.range {
            schema["minimum"] = json!(min);
            schema["maximum"] = json!(max);
        }
        match self.type_name {
            "array" => schema["items"] = json!({"type": "string"}),
            "integer-array" => schema["items"] = json!({"type":"integer","minimum":0}),
            "code-annotations" => schema["items"] = code_annotation_schema(),
            "tree-nodes" => schema["items"] = tree_node_schema(),
            "ring-sections" => {
                schema["items"] = json!({
                    "type":"object",
                    "required":["value","color"],
                    "additionalProperties":false,
                    "properties":{
                        "value":{"type":"number","minimum":0,"maximum":100},
                        "color":{"type":"string","enum":self.accepted},
                    },
                })
            }
            _ => {}
        }
        schema
    }
}

impl EventSpec {
    pub fn schema(&self) -> Value {
        match self.payload_type {
            "markdown-task-toggle" => json!({
                "type":"object",
                "required":["checked","start","end"],
                "additionalProperties":false,
                "properties":{
                    "checked":{"type":"boolean"},
                    "start":{"type":"integer","minimum":0},
                    "end":{"type":"integer","minimum":0}
                }
            }),
            "range-value" => range_schema(),
            "string-array" => json!({"type":"array","items":{"type":"string"}}),
            "tag-rejection" => json!({
                "type":"object","required":["tag","reason"],"additionalProperties":false,
                "properties":{
                    "tag":{"type":"string"},
                    "reason":{"enum":["empty","duplicate","full","too-long"]}
                }
            }),
            "tree-check" => tree_event_schema("checked", json!({"type":"boolean"})),
            "tree-reorder" => tree_event_schema("before", json!({"type":"integer","minimum":0})),
            "tree-edit" => tree_event_schema("value", tree_value_schema()),
            "selection" => json!({"type":["string","null"]}),
            "integer-array" => json!({"type":"array","items":{"type":"integer","minimum":0}}),
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

    pub fn description(&self, kinds: &[&str]) -> Value {
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
            json!([{"name":"children","min":0,"max":null,"accepted":kinds}])
        } else {
            json!([])
        };
        json!({"id":self.id, "crate":self.krate, "summary":self.summary,"props":props,
            "slots":slots,"events":events,"state":self.state,"themeTokens":self.theme_tokens,
            "a11yRole":self.a11y_role,"examples":[{"name":"minimal","document":self.example()}]})
    }

    pub fn node_schema(&self) -> Value {
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
                "themeTokens":{"type":"array","items": if self.theme_tokens.is_empty() { json!(false) } else { json!({"enum":self.theme_tokens}) }},
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

pub fn code_annotation_schema() -> Value {
    let coordinate = json!({"type":"integer","minimum":0,"maximum":(1_u64<<53)-1});
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["id","line","start","end","kind"],
        "properties":{
            "id":coordinate,
            "line":coordinate,
            "endLine":coordinate,
            "start":coordinate,
            "end":coordinate,
            "kind":{"enum":["highlight","underline","inline-hint","lens","bubble","symbol"]},
            "payload":{"type":"string"},
            "color":{"enum":["dark","gray","red","pink","grape","violet","indigo","blue","cyan","teal","green","lime","yellow","orange"]}
        }
    })
}

fn tree_value_schema() -> Value {
    json!({"type":["string","number","boolean","array"],"items":{"type":["number","string"]}})
}

fn tree_event_schema(field: &str, schema: Value) -> Value {
    json!({
        "type":"object",
        "required":["id",field],
        "additionalProperties":false,
        "properties":{"id":{"type":"integer","minimum":0},field:schema}
    })
}

pub fn tree_node_schema() -> Value {
    json!({
        "type":"object",
        "required":["id","label"],
        "additionalProperties":false,
        "properties":{
            "id":{"type":"integer","minimum":0,"maximum":(1_u64<<53)-1},
            "label":{"type":"string"},
            "depth":{"type":"integer","minimum":0,"maximum":4096},
            "unloaded":{"type":"boolean"},
            "checked":{"type":"boolean"},
            "icon":{"type":"string"},
            "badge":{"type":"string"},
            "height":{"type":"number","minimum":1,"maximum":4096},
            "property":{
                "type":"object",
                "required":["type","value"],
                "additionalProperties":false,
                "properties":{
                    "type":{"enum":["text","number","bool","enum","colour","vector","rect","flags","path"]},
                    "value":tree_value_schema(),
                    "default":tree_value_schema(),
                    "readOnly":{"type":"boolean"},
                    "choices":{"type":"array","items":{"type":"string"}},
                    "description":{"type":"string"},
                    "units":{"type":"string"},
                    "min":{"type":"number"},
                    "max":{"type":"number"},
                    "step":{"type":"number","minimum":0},
                    "mixed":{"type":"boolean"},
                    "advanced":{"type":"boolean"},
                    "multilineRows":{"type":"integer","minimum":1,"maximum":8}
                }
            }
        }
    })
}

fn range_schema() -> Value {
    json!({"type":"array","minItems":2,"maxItems":2,"items":{"type":"number"}})
}
