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
    /// Whether the compiler accepts a JSON record as a literal value for this property.
    pub fn accepts_record(&self) -> bool {
        matches!(self.type_name, "object" | "flow-graph" | "dock-state")
    }

    fn default_value(&self) -> Value {
        serde_json::from_str(self.default_json).unwrap_or(Value::Null)
    }

    pub fn schema(&self) -> Value {
        let ty = match self.type_name {
            "ring-sections" | "integer-array" | "code-annotations" | "code-folds"
            | "tree-nodes" | "range-value" | "grid-rows" | "grid-columns" | "flame-frames"
            | "timeline-tracks" | "track-selection" | "gantt-tasks" | "board-columns"
            | "chart-layers" | "chart-axes" | "chart-labels" | "chat-messages"
            | "chat-attachments" | "chat-commands" | "calendar-presets" | "dock-panels"
            | "avatar-people" => "array",
            ty => ty,
        };
        let mut schema = match self.type_name {
            "range-value" => range_schema(),
            "calendar-value" => calendar_value_schema(),
            "flow-graph" => flow_graph_schema(),
            "dock-state" => dock_state_schema(),
            "iso-time" => json!({"type":"string","pattern":ISO_TIME}),
            "iso-date" => json!({"type":"string","pattern":ISO_DATE}),
            _ => json!({"type": ty}),
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
            "code-folds" => schema["items"] = code_fold_schema(),
            "tree-nodes" => schema["items"] = tree_node_schema(),
            "grid-rows" => schema["items"] = grid_row_schema(),
            "grid-columns" => schema["items"] = grid_column_schema(),
            "flame-frames" => schema["items"] = flame_frame_schema(),
            "timeline-tracks" => schema["items"] = timeline_track_schema(),
            "gantt-tasks" => schema["items"] = gantt_task_schema(),
            "board-columns" => schema["items"] = board_column_schema(),
            "chart-layers" => schema["items"] = chart_layer_schema(),
            "chart-axes" => schema["items"] = chart_axis_schema(),
            "chart-labels" => schema["items"] = chart_label_schema(),
            "chat-messages" => schema["items"] = chat_message_schema(),
            "calendar-presets" => schema["items"] = calendar_preset_schema(),
            "dock-panels" => schema["items"] = dock_panel_schema(),
            "avatar-people" => schema["items"] = avatar_person_schema(),
            "chat-attachments" => schema["items"] = chat_attachment_schema(),
            "chat-commands" => schema["items"] = chat_command_schema(),
            "track-selection" => {
                schema["items"] = json!({"type":"integer","minimum":0,"maximum":u32::MAX});
                schema["oneOf"] = json!([{"maxItems":0},{"minItems":2,"maxItems":2}]);
            }
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
            "data-grid-event" => grid_event_schema(self.id),
            "string-array" => json!({"type":"array","items":{"type":"string"}}),
            "tag-rejection" => json!({
                "type":"object","required":["tag","reason"],"additionalProperties":false,
                "properties":{
                    "tag":{"type":"string"},
                    "reason":{"enum":["empty","duplicate","full","too-long"]}
                }
            }),
            "flame-frame-click" => json!({
                "type":"object","required":["id","path","value"],"additionalProperties":false,
                "properties":{
                    "id":{"type":"integer","minimum":0},
                    "path":{"type":"array","items":{"type":"integer","minimum":0}},
                    "value":{"type":"number","minimum":0}
                }
            }),
            "chart-datum" | "chart-domain" | "chart-series-visibility" => {
                chart_event_schema(self.payload_type)
            }
            "board-reorder" => {
                let index = json!({"type":"integer","minimum":0});
                let indices = json!({"type":"array","items":index});
                json!({
                    "type":"object","additionalProperties":false,
                    "required":["source","destination","insertionIndex","originalIndices","lists"],
                    "properties":{
                        "source":index,"destination":index,"insertionIndex":index,
                        "originalIndices":indices,"lists":{"type":"array","items":indices}
                    }
                })
            }
            "timeline-clip-change" => json!({
                "type":"object","required":["track","clip"],"additionalProperties":false,
                "properties":{
                    "track":{"type":"integer","minimum":0},
                    "clip":{"type":"integer","minimum":0},
                    "start":{"type":"number"},
                    "length":{"type":"number","minimum":0.001},
                    "sourceOffset":{"type":"number","minimum":0}
                }
            }),
            "calendar-value" => calendar_value_schema(),
            "flow-graph" => flow_graph_schema(),
            "dock-state" => dock_state_schema(),
            "gantt-reschedule" => gantt_event_schema(false),
            "gantt-resize" => gantt_event_schema(true),
            "flow-selection" => {
                let ids = json!({"type":"array","items":{"type":"integer","minimum":0}});
                json!({"type":"object","required":["nodes","edges"],"additionalProperties":false,
                    "properties":{"nodes":ids,"edges":ids}})
            }
            "flow-viewport" => json!({
                "type":"object","required":["panX","panY","zoom"],"additionalProperties":false,
                "properties":{
                    "panX":{"type":"number"},"panY":{"type":"number"},
                    "zoom":{"type":"number","minimum":0.02,"maximum":4}
                }
            }),
            "iso-time" => json!({"type":"string","pattern":ISO_TIME}),
            "chat-submit" => json!({
                "type":"object","required":["id","text","attachments"],"additionalProperties":false,
                "properties":{
                    "id":{"type":"integer","minimum":0},"text":{"type":"string"},
                    "attachments":{"type":"array","items":chat_attachment_schema()}
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

pub fn code_fold_schema() -> Value {
    let line = json!({"type":"integer","minimum":0,"maximum":u32::MAX});
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["start","end"],
        "properties":{
            "start":line,
            "end":line,
            "collapsed":{"type":"boolean"}
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

pub fn grid_row_schema() -> Value {
    json!({"type":"array","items":{"type":["string","number","boolean","null"]}})
}

pub fn grid_column_schema() -> Value {
    let width = json!({"type":"number","minimum":1,"maximum":4096});
    let flag = json!({"type":"boolean"});
    json!({
        "type":"object","required":["id","title"],"additionalProperties":false,
        "properties":{
            "id":{"type":"string"},"title":{"type":"string"},
            "source":{"type":"integer","minimum":0},
            "width":width,"minWidth":width,"maxWidth":width,
            "align":{"enum":["auto","start","center","end"]},
            "pin":{"enum":["none","left","right"]},
            "sortable":flag,"resizable":flag,"editable":flag,"visible":flag
        }
    })
}

fn grid_event_schema(event: &str) -> Value {
    fn object(fields: &[(&str, Value)]) -> Value {
        let names: Vec<_> = fields.iter().map(|(name, _)| *name).collect();
        let fields: serde_json::Map<String, Value> = fields
            .iter()
            .map(|(name, kind)| ((*name).to_owned(), kind.clone()))
            .collect();
        json!({"type":"object","required":names,"properties":fields,"additionalProperties":false})
    }
    let index = json!({"type":"integer","minimum":0});
    let nullable_index = json!({"type":["integer","null"],"minimum":0});
    let flag = json!({"type":"boolean"});
    let text = json!({"type":"string"});
    let op = json!({"enum":["edit","paste","cut","clear","undo","redo"]});
    match event {
        "selection-change" => {
            let cell = object(&[("row", index.clone()), ("column", index.clone())]);
            object(&[
                ("cursor", json!({"oneOf":[cell,{"type":"null"}]})),
                ("selectedRows", index),
            ])
        }
        "sort-change" => {
            json!({"type":"array","items":object(&[("column",index),("direction",json!({"enum":["asc","desc"]}))])})
        }
        "cell-edit" => object(&[
            ("row", index.clone()),
            ("key", index.clone()),
            ("column", index),
            ("value", text),
        ]),
        "edit-cancel" => object(&[("row", index.clone()), ("column", index)]),
        "copy" => object(&[("cells", index), ("truncated", flag)]),
        "edit-reject" => object(&[("row", index.clone()), ("column", index), ("reason", text)]),
        "row-activate" => object(&[("row", index.clone()), ("key", index)]),
        "column-resize" => object(&[("column", index), ("width", json!({"type":"number"}))]),
        "column-reorder" => object(&[
            ("column", index.clone()),
            ("order", json!({"type":"array","items":index})),
        ]),
        "cells-change" => object(&[("op", op), ("cells", index), ("truncated", flag)]),
        "cells-reject" => object(&[
            ("op", op),
            ("rejected", index.clone()),
            ("applied", index),
            (
                "first",
                json!({"type":"array","items":object(&[
                    ("row",nullable_index.clone()),("column",nullable_index),("reason",text)
                ])}),
            ),
        ]),
        _ => json!({"type":"object"}),
    }
}

pub fn flame_frame_schema() -> Value {
    let id = json!({"type":"integer","minimum":0,"maximum":(1_u64<<53)-1});
    let size = json!({"type":"number","minimum":0});
    json!({
        "type":"object","required":["id","self","total","label"],"additionalProperties":false,
        "properties":{
            "id":id,"parent":{"type":["integer","null"],"minimum":0,"maximum":(1_u64<<53)-1},
            "self":size,"total":size,"label":{"type":"string"},"package":{"type":"string"}
        }
    })
}

pub fn timeline_track_schema() -> Value {
    json!({
        "type":"object","required":["name","clips"],"additionalProperties":false,
        "properties":{
            "name":{"type":"string"},
            "clips":{"type":"array","items":{
                "type":"object","required":["id","start","length"],"additionalProperties":false,
                "properties":{
                    "id":{"type":"integer","minimum":0,"maximum":u32::MAX-1},
                    "start":{"type":"number"},
                    "length":{"type":"number","minimum":0.001},
                    "sourceOffset":{"type":"number","minimum":0},
                    "locked":{"type":"boolean"}
                }
            }}
        }
    })
}

pub fn gantt_task_schema() -> Value {
    let id = json!({"type":"integer","minimum":0,"maximum":u32::MAX});
    json!({
        "type":"object","additionalProperties":false,
        "required":["id","name","duration"],
        "properties":{
            "id":id,
            "name":{"type":"string"},
            "duration":{"type":"number","exclusiveMinimum":0},
            "notBefore":{"type":"number"},
            "after":{"type":"array","items":id}
        }
    })
}

fn gantt_event_schema(resize: bool) -> Value {
    let mut schema = json!({
        "type":"object","additionalProperties":false,"required":["id","notBefore"],
        "properties":{
            "id":{"type":"integer","minimum":0,"maximum":u32::MAX},
            "notBefore":{"type":"number"}
        }
    });
    if resize {
        schema["required"] = json!(["id", "notBefore", "duration"]);
        schema["properties"]["duration"] = json!({"type":"number","exclusiveMinimum":0});
    }
    schema
}

pub fn board_column_schema() -> Value {
    json!({
        "type":"object","required":["title","cards"],"additionalProperties":false,
        "properties":{
            "title":{"type":"string"},
            "cards":{"type":"array","items":{
                "type":"object","required":["id","label"],"additionalProperties":false,
                "properties":{
                    "id":{"type":"integer","minimum":0,"maximum":(1_u64<<53)-1},
                    "label":{"type":"string"}
                }
            }}
        }
    })
}

pub fn chart_label_schema() -> Value {
    json!({
        "type":"object","required":["id","label"],"additionalProperties":false,
        "properties":{
            "id":{"type":"integer","minimum":0,"maximum":u32::MAX},
            "label":{"type":"string"}
        }
    })
}

pub fn chart_layer_schema() -> Value {
    let number = json!({"type":"number"});
    let index = json!({"type":"integer","minimum":0,"maximum":u32::MAX});
    json!({
        "type":"object","required":["mark","data"],"additionalProperties":false,
        "properties":{
            "mark":{"enum":["line","area","bar","point","bubble","rect","rule","text","error-bar","candlestick"]},
            "arrangement":{"enum":["overlap","group","stack","normalize"]},
            "orientation":{"enum":["vertical","horizontal"]},
            "title":{"type":"string"},"secondary":{"type":"boolean"},
            "groupLabels":{"type":"array","items":chart_label_schema()},
            "data":{"type":"array","items":{
                "type":"object","required":["x","y"],"additionalProperties":false,
                "properties":{
                    "key":{"type":"integer","minimum":0,"maximum":(1_u64<<53)-1},
                    "x":number,"y":number,"group":index,"facet":index,
                    "colorCategory":index,"colorValue":{"type":"number","minimum":0,"maximum":1},
                    "size":{"type":"number","minimum":0,"maximum":1024},
                    "x2":number,"y2":number,"low":number,"open":number,"high":number,"close":number,
                    "text":{"type":"string"}
                }
            }}
        }
    })
}

pub fn chart_axis_schema() -> Value {
    json!({
        "type":"object","required":["axis"],"additionalProperties":false,
        "properties":{
            "axis":{"enum":["x","y","right"]},
            "scale":{"enum":["linear","log","seconds","days","frames","band","ordinal"]},
            "title":{"type":"string"},"grid":{"type":"boolean"},"nice":{"type":"boolean"},
            "domain":range_schema(),
            "framesPerSecond":{"type":"number","minimum":1,"maximum":240},
            "padding":{"type":"number","minimum":0,"maximum":1},
            "labels":{"type":"array","items":{"type":"string"}}
        }
    })
}

fn chart_event_schema(payload: &str) -> Value {
    let index = json!({"type":"integer","minimum":0});
    match payload {
        "chart-datum" => json!({
            "type":"object","required":["layer","group","index","key","facet"],"additionalProperties":false,
            "properties":{"layer":index,"group":index,"index":index,"key":index,"facet":index}
        }),
        "chart-domain" => json!({
            "type":"object","required":["min","max"],"additionalProperties":false,
            "properties":{"min":{"type":"number"},"max":{"type":"number"}}
        }),
        _ => json!({
            "type":"object","required":["layer","group","visible"],"additionalProperties":false,
            "properties":{"layer":index,"group":index,"visible":{"type":"boolean"}}
        }),
    }
}

/// Derive the compiler's positional schema from a widget descriptor.
#[macro_export]
macro_rules! widget_schema {
    ($widget:expr) => {{
        const SPEC: &$crate::widget_spec::WidgetSpec = $widget;
        const PROPS: &[&str] = &{
            let mut names = [""; SPEC.props.len()];
            let mut index = 0;
            while index < names.len() {
                names[index] = SPEC.props[index].name;
                index += 1;
            }
            names
        };
        const EVENTS: &[&str] = &{
            let mut names = [""; SPEC.events.len()];
            let mut index = 0;
            while index < names.len() {
                names[index] = SPEC.events[index].id;
                index += 1;
            }
            names
        };
        &$crate::widget_spec::Schema {
            props: PROPS,
            events: EVENTS,
        }
    }};
}

pub fn chat_attachment_schema() -> Value {
    json!({
        "type":"object","required":["id","name"],"additionalProperties":false,
        "properties":{"id":{"type":"string"},"name":{"type":"string"}}
    })
}

pub fn chat_command_schema() -> Value {
    json!({
        "type":"object","required":["id","label"],"additionalProperties":false,
        "properties":{
            "id":{"type":"string"},"label":{"type":"string"},"keywords":{"type":"string"},"disabled":{"type":"boolean"}
        }
    })
}

pub fn chat_message_schema() -> Value {
    json!({
        "type":"object","required":["id","role","text"],"additionalProperties":false,
        "properties":{
            "id":{"type":"integer","minimum":0,"maximum":(1_u64<<53)-1},
            "role":{"enum":["user","assistant"]},
            "text":{"type":"string"},"timestamp":{"type":"string"},"reasoning":{"type":"string"},
            "attachments":{"type":"array","items":chat_attachment_schema()},
            "tools":{"type":"array","items":{
                "type":"object","required":["id","name","status"],"additionalProperties":false,
                "properties":{
                    "id":{"type":"string"},"name":{"type":"string"},
                    "status":{"enum":["running","succeeded","failed"]},
                    "arguments":{"type":"string"},"result":{"type":"string"}
                }
            }}
        }
    })
}

const ISO_DATE: &str = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$";
const ISO_TIME: &str = r"^([01][0-9]|2[0-3]):[0-5][0-9](:[0-5][0-9])?$";
const ISO_DATE_TIME: &str =
    r"^[0-9]{4}-[0-9]{2}-[0-9]{2}(T([01][0-9]|2[0-3]):[0-5][0-9](:[0-5][0-9])?)?$";

fn calendar_value_schema() -> Value {
    let endpoint = json!({"type":["string","null"],"pattern":ISO_DATE_TIME});
    json!({"oneOf":[endpoint,{"type":"array","minItems":2,"maxItems":2,"items":endpoint}]})
}

fn calendar_preset_schema() -> Value {
    json!({
        "type":"object","required":["label","value"],"additionalProperties":false,
        "properties":{"label":{"type":"string"},"value":calendar_value_schema()}
    })
}

pub fn flow_graph_schema() -> Value {
    let id = json!({"type":"integer","minimum":0,"maximum":(1_u64<<53)-1});
    let point = json!({
        "type":"object","required":["x","y"],"additionalProperties":false,
        "properties":{"x":{"type":"number"},"y":{"type":"number"}}
    });
    let endpoint = json!({
        "type":"object","required":["node","handle"],"additionalProperties":false,
        "properties":{"node":id,"handle":{"type":"string"}}
    });
    let marker = json!({"enum":["None","Arrow","ArrowClosed"]});
    let node = json!({
        "type":"object","required":["id","pos","size","data","node_type","selected","draggable","handles"],
        "additionalProperties":false,
        "properties":{
            "id":id,"pos":point,"size":point,"data":{},"node_type":{"type":"string"},
            "parent":{"type":["null","integer"],"minimum":0,"maximum":(1_u64<<53)-1},"selected":{"type":"boolean"},
            "draggable":{"type":"boolean"},"handles":{"type":"array","items":{
                "type":"object","required":["id","kind","side","offset"],"additionalProperties":false,
                "properties":{
                    "id":{"type":"string"},"kind":{"enum":["Source","Target"]},
                    "side":{"enum":["Left","Right","Top","Bottom"]},"offset":{"type":"number","minimum":0,"maximum":1}
                }
            }}
        }
    });
    let edge = json!({
        "type":"object","required":["id","source","target","edge_type","label","animated","marker_start","marker_end"],
        "additionalProperties":false,
        "properties":{
            "id":id,"source":endpoint,"target":endpoint,
            "edge_type":{"enum":["Bezier","SmoothStep","Step","Straight","OrthogonalHorizontal","OrthogonalVertical"]},
            "label":{"type":"string"},"animated":{"type":"boolean"},
            "marker_start":marker,"marker_end":marker
        }
    });
    json!({
        "type":"object","required":["nodes","edges"],"additionalProperties":false,
        "properties":{
            "nodes":{"type":"object","additionalProperties":node},
            "edges":{"type":"object","additionalProperties":edge},
            "edge_id_high_water":id
        }
    })
}

pub fn dock_state_schema() -> Value {
    let panel = json!({
        "type":"object","required":["panel_name","children","info"],
        "properties":{
            "panel_name":{"type":"string"},"children":{"type":"array","items":{"type":"object"}},
            "info":{"type":"object"}
        }
    });
    let dock = json!({
        "type":"object","required":["panel","placement","size","open"],
        "properties":{
            "panel":panel,"placement":{"enum":["left","right","bottom"]},
            "size":{"type":"number","minimum":0},"open":{"type":"boolean"}
        }
    });
    json!({
        "type":"object","required":["center"],"additionalProperties":false,
        "properties":{
            "version":{"type":["integer","null"],"minimum":1},"center":panel,
            "left_dock":dock,"right_dock":dock,"bottom_dock":dock
        }
    })
}

pub fn dock_panel_schema() -> Value {
    json!({
        "type":"object","required":["title"],"additionalProperties":false,
        "properties":{
            "title":{"type":"string"},"text":{"type":"string"},
            "placement":{"enum":["center","left","right","bottom"]},
            "visible":{"type":"boolean"},"closable":{"type":"boolean"},"zoomable":{"type":"boolean"}
        }
    })
}

pub fn avatar_person_schema() -> Value {
    json!({
        "type":"object","additionalProperties":false,"required":["name"],
        "properties":{
            "name":{"type":"string"},"image":{"type":"string"}
        }
    })
}
