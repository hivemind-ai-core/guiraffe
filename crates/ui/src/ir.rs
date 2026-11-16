//! The document IR: what Mocket (or a hand-written file) produces. Serialized as JSON.
//!
//! ```json
//! { "version": 1,
//!   "root": { "kind": "stack", "children": [
//!     { "kind": "title", "props": { "text": "Inbox" } },
//!     { "kind": "text", "bind": { "text": "status" } },
//!     { "kind": "button", "props": { "label": "Refresh" }, "on": { "click": "refresh" } } ] } }
//! ```
//! - `props`: literal values. `bind`: prop → **slot name** (view data supplied by logic).
//! - `on`: widget event → **event name** (handled by logic). Names are resolved to indices when
//!   the document is compiled, so nothing is looked up by string at runtime.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub root: Node,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub each: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bind: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub on: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            each: None,
            when: None,
            key: None,
            id: None,
            props: BTreeMap::new(),
            bind: BTreeMap::new(),
            on: BTreeMap::new(),
            children: Vec::new(),
        }
    }
    pub fn each(mut self, slot: &str) -> Self {
        self.each = Some(slot.into());
        self
    }
    pub fn when(mut self, slot: &str) -> Self {
        self.when = Some(slot.into());
        self
    }
    pub fn key(mut self, field: &str) -> Self {
        self.key = Some(field.into());
        self
    }
    pub fn prop(mut self, k: &str, v: impl Into<serde_json::Value>) -> Self {
        self.props.insert(k.into(), v.into());
        self
    }
    pub fn bind(mut self, prop: &str, slot: &str) -> Self {
        self.bind.insert(prop.into(), slot.into());
        self
    }
    pub fn on(mut self, event: &str, name: &str) -> Self {
        self.on.insert(event.into(), name.into());
        self
    }
    pub fn child(mut self, n: Node) -> Self {
        self.children.push(n);
        self
    }
}

impl Document {
    pub fn new(root: Node) -> Self {
        Self {
            version: FORMAT_VERSION,
            root,
        }
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let doc: Document = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if doc.version > FORMAT_VERSION {
            return Err(format!(
                "document format version {} is newer than supported {}",
                doc.version, FORMAT_VERSION
            ));
        }
        Ok(doc)
    }
}
