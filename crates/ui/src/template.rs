//! Compiling a [`Document`] into a [`Template`]: a flat node arena with widgets resolved
//! from the registry and bindings resolved from names to indices, done once at load.

use std::{collections::HashMap, ops::Range, sync::Arc};

use gpui_kit::SharedString;

use crate::{
    ir::{Document, FORMAT_VERSION, Node},
    slots::SlotId,
    value::Value,
    widget::{Placeholder, Registry, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    Lit(Value),
    Slot(SlotId),
}

pub struct NodeSpec {
    pub kind: SharedString,
    pub id: Option<SharedString>,
    pub widget: Arc<dyn Widget>,
    pub props: Vec<(SharedString, Prop)>,
    pub events: Vec<(SharedString, EventId)>,
    /// Indices into [`Template::child_index`].
    pub children: Range<u32>,
}

/// A problem found while compiling, with the node path (e.g. `/root/children/2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub message: String,
}

/// Name tables, for logic to resolve its ids once (not per event).
#[derive(Debug, Clone, Default)]
pub struct Names {
    slots: Vec<String>,
    events: Vec<String>,
}

impl Names {
    pub fn slot(&self, name: &str) -> Option<SlotId> {
        self.slots
            .iter()
            .position(|s| s == name)
            .map(|i| SlotId(i as u32))
    }
    pub fn event(&self, name: &str) -> Option<EventId> {
        self.events
            .iter()
            .position(|s| s == name)
            .map(|i| EventId(i as u32))
    }
    pub fn event_name(&self, id: EventId) -> Option<&str> {
        self.events.get(id.0 as usize).map(String::as_str)
    }
    pub fn slot_names(&self) -> &[String] {
        &self.slots
    }
}

pub struct Template {
    pub nodes: Vec<NodeSpec>,
    pub child_index: Vec<u32>,
    pub root: u32,
    pub names: Names,
    pub diagnostics: Vec<Diagnostic>,
}

/// Fatal compile error (the document can't be used at all).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnsupportedVersion(u32),
    TooDeep { path: String, max: usize },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnsupportedVersion(v) => {
                write!(f, "unsupported document version {v} (max {FORMAT_VERSION})")
            }
            CompileError::TooDeep { path, max } => write!(f, "nesting deeper than {max} at {path}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Guards against pathological documents (stack depth during compile and render).
pub const MAX_DEPTH: usize = 256;

impl Template {
    pub fn compile(doc: &Document, registry: &Registry) -> Result<Template, CompileError> {
        if doc.version > FORMAT_VERSION {
            return Err(CompileError::UnsupportedVersion(doc.version));
        }
        let mut c = Compiler {
            registry,
            nodes: Vec::new(),
            child_index: Vec::new(),
            slots: HashMap::new(),
            names: Names::default(),
            diags: Vec::new(),
        };
        let root = c.node(&doc.root, "/root".into(), 0)?;
        Ok(Template {
            nodes: c.nodes,
            child_index: c.child_index,
            root,
            names: c.names,
            diagnostics: c.diags,
        })
    }

    pub fn node(&self, i: u32) -> Option<&NodeSpec> {
        self.nodes.get(i as usize)
    }

    pub fn children(&self, n: &NodeSpec) -> impl Iterator<Item = u32> + '_ {
        self.child_index[n.children.start as usize..n.children.end as usize]
            .iter()
            .copied()
    }

    /// Node index by its document `id`.
    pub fn find(&self, id: &str) -> Option<u32> {
        self.nodes
            .iter()
            .position(|n| n.id.as_deref() == Some(id))
            .map(|i| i as u32)
    }
}

struct Compiler<'a> {
    registry: &'a Registry,
    nodes: Vec<NodeSpec>,
    child_index: Vec<u32>,
    slots: HashMap<String, SlotId>,
    names: Names,
    diags: Vec<Diagnostic>,
}

impl Compiler<'_> {
    fn slot(&mut self, name: &str) -> SlotId {
        if let Some(id) = self.slots.get(name) {
            return *id;
        }
        let id = SlotId(self.names.slots.len() as u32);
        self.names.slots.push(name.to_owned());
        self.slots.insert(name.to_owned(), id);
        id
    }

    fn event(&mut self, name: &str) -> EventId {
        match self.names.events.iter().position(|e| e == name) {
            Some(i) => EventId(i as u32),
            None => {
                self.names.events.push(name.to_owned());
                EventId(self.names.events.len() as u32 - 1)
            }
        }
    }

    fn node(&mut self, n: &Node, path: String, depth: usize) -> Result<u32, CompileError> {
        if depth > MAX_DEPTH {
            return Err(CompileError::TooDeep {
                path,
                max: MAX_DEPTH,
            });
        }
        let widget: Arc<dyn Widget> = match self.registry.get(&n.kind) {
            Some(w) => w,
            None => {
                self.diags.push(Diagnostic {
                    path: path.clone(),
                    message: format!("unknown widget kind `{}`", n.kind),
                });
                Arc::new(Placeholder)
            }
        };
        let mut props: Vec<(SharedString, Prop)> = n
            .props
            .iter()
            .map(|(k, v)| {
                (
                    SharedString::from(k.clone()),
                    Prop::Lit(Value::from_json(v)),
                )
            })
            .collect();
        for (prop, slot) in &n.bind {
            let id = self.slot(slot);
            props.retain(|(k, _)| k.as_ref() != prop.as_str());
            props.push((prop.clone().into(), Prop::Slot(id)));
        }
        let events =
            n.on.iter()
                .map(|(ev, name)| (SharedString::from(ev.clone()), self.event(name)))
                .collect();
        let index = self.nodes.len() as u32;
        self.nodes.push(NodeSpec {
            kind: n.kind.clone().into(),
            id: n.id.clone().map(Into::into),
            widget,
            props,
            events,
            children: 0..0,
        });
        let mut kids = Vec::with_capacity(n.children.len());
        for (i, child) in n.children.iter().enumerate() {
            kids.push(self.node(child, format!("{path}/children/{i}"), depth + 1)?);
        }
        let start = self.child_index.len() as u32;
        self.child_index.extend(kids);
        let end = self.child_index.len() as u32;
        if let Some(node) = self.nodes.get_mut(index as usize) {
            node.children = start..end;
        }
        Ok(index)
    }
}
