//! Shared metadata for declarative widget registration.

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

use serde_json::{Value, json};

pub const FORMAT_VERSION: u32 = 1;

impl PropSpec {
    fn default_value(&self) -> Value {
        serde_json::from_str(self.default_json).unwrap_or(Value::Null)
    }

    pub fn schema(&self) -> Value {
        let ty = match self.type_name {
            "ring-sections" => "array",
            ty => ty,
        };
        let mut schema = json!({"type": ty});
        if !self.accepted.is_empty() && ty == "string" {
            schema["enum"] = json!(self.accepted);
        }
        if let Some((min, max)) = self.range {
            schema["minimum"] = json!(min);
            schema["maximum"] = json!(max);
        }
        match self.type_name {
            "array" => schema["items"] = json!({"type": "string"}),
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
