//! Compiling a [`Document`] into a [`Template`]: a flat node arena with widgets resolved from
//! the registry, props/events stored in schema order, and names resolved to indices — once,
//! at load (possibly on a background thread).

use std::{collections::HashMap, sync::Arc};

use gpui_kit::SharedString;
use guiraffe_core::{ColorName, Size};

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
    Unset,
    Lit(Value),
    Slot(SlotId),
    Item(Box<[SharedString]>),
    Size(Size),
    Color(ColorName),
}

pub struct NodeSpec {
    pub kind: SharedString,
    pub id: Option<SharedString>,
    pub widget: Arc<dyn Widget>,
    /// In the widget's schema order.
    pub props: Box<[Prop]>,
    /// In the widget's schema event order.
    pub events: Box<[Option<EventId>]>,
    /// Slots this node's own props read (non-empty ⇒ rendered as its own entity).
    pub deps: Box<[SlotId]>,
    pub each: Option<SlotId>,
    pub when: Option<SlotId>,
    pub key: Box<[SharedString]>,
    pub(crate) change: Option<(crate::payload::ChangeTarget, EventId)>,
    pub uniform_rows: bool,
    children: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// A problem found while compiling, with the node path (e.g. `/root/children/2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub path: String,
    pub message: String,
}

/// Name tables, for logic to resolve its ids once (not per event).
#[derive(Debug, Clone, Default)]
pub struct Names {
    slots: Vec<String>,
    slot_index: HashMap<String, SlotId>,
    events: Vec<String>,
    event_index: HashMap<String, EventId>,
}

impl Names {
    pub fn slot(&self, name: &str) -> Option<SlotId> {
        self.slot_index.get(name).copied()
    }
    pub fn event(&self, name: &str) -> Option<EventId> {
        self.event_index.get(name).copied()
    }
    pub fn event_name(&self, id: EventId) -> Option<&str> {
        self.events.get(id.0 as usize).map(String::as_str)
    }
    pub fn slot_names(&self) -> &[String] {
        &self.slots
    }
}

pub struct Template {
    nodes: Vec<NodeSpec>,
    child_index: Vec<u32>,
    root: u32,
    names: Names,
    diagnostics: Vec<Diagnostic>,
    slot_defaults: Vec<Value>,
    /// Node indices that read each slot (for targeted invalidation).
    slot_nodes: Vec<Vec<u32>>,
    ids: HashMap<SharedString, u32>,
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

/// Maximum nesting depth. Matches what JSON parsing accepts (serde_json's recursion limit is
/// reached at roughly 64 nested nodes), so builder- and JSON-made documents behave the same.
pub const MAX_DEPTH: usize = 60;

impl Template {
    pub fn compile(doc: &Document, registry: &Registry) -> Result<Template, CompileError> {
        if doc.version > FORMAT_VERSION {
            return Err(CompileError::UnsupportedVersion(doc.version));
        }
        let mut c = Compiler {
            registry,
            t: Template::empty(),
        };
        let root = c.node(&doc.root, "/root".into(), 0)?;
        c.t.root = root;
        Ok(c.t)
    }

    fn empty() -> Template {
        Template {
            nodes: Vec::new(),
            child_index: Vec::new(),
            root: 0,
            names: Names::default(),
            diagnostics: Vec::new(),
            slot_defaults: Vec::new(),
            slot_nodes: Vec::new(),
            ids: HashMap::new(),
        }
    }

    pub fn root(&self) -> u32 {
        self.root
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn node(&self, i: u32) -> Option<&NodeSpec> {
        self.nodes.get(i as usize)
    }
    pub fn children(&self, n: &NodeSpec) -> &[u32] {
        self.child_index
            .get(n.children.0 as usize..n.children.1 as usize)
            .unwrap_or(&[])
    }
    pub fn names(&self) -> &Names {
        &self.names
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn slot_defaults(&self) -> &[Value] {
        &self.slot_defaults
    }
    pub fn slot_nodes(&self, slot: SlotId) -> &[u32] {
        self.slot_nodes
            .get(slot.0 as usize)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    /// Node index by its document `id` (O(1)).
    pub fn find(&self, id: &str) -> Option<u32> {
        self.ids.get(id).copied()
    }
}

struct Compiler<'a> {
    registry: &'a Registry,
    t: Template,
}

impl Compiler<'_> {
    fn diag(&mut self, severity: Severity, path: &str, message: String) {
        self.t.diagnostics.push(Diagnostic {
            severity,
            path: path.to_owned(),
            message,
        });
    }

    fn slot(&mut self, name: &str) -> SlotId {
        if let Some(id) = self.t.names.slot_index.get(name) {
            return *id;
        }
        let id = SlotId(self.t.names.slots.len() as u32);
        self.t.names.slots.push(name.to_owned());
        self.t.names.slot_index.insert(name.to_owned(), id);
        self.t.slot_defaults.push(Value::Null);
        self.t.slot_nodes.push(Vec::new());
        id
    }

    fn event(&mut self, name: &str) -> EventId {
        if let Some(id) = self.t.names.event_index.get(name) {
            return *id;
        }
        let id = EventId(self.t.names.events.len() as u32);
        self.t.names.events.push(name.to_owned());
        self.t.names.event_index.insert(name.to_owned(), id);
        id
    }

    fn node(&mut self, n: &Node, path: String, depth: usize) -> Result<u32, CompileError> {
        if depth > MAX_DEPTH {
            return Err(CompileError::TooDeep {
                path,
                max: MAX_DEPTH,
            });
        }
        let index = self.t.nodes.len() as u32;
        let (widget, known): (Arc<dyn Widget>, bool) = match self.registry.get(&n.kind) {
            Some(w) => (w, true),
            None => {
                self.diag(
                    Severity::Warning,
                    &path,
                    format!("unknown widget kind `{}`", n.kind),
                );
                (Arc::new(Placeholder), false)
            }
        };
        let schema = widget.schema();
        let mut props = vec![Prop::Unset; schema.props.len()];
        let descriptor = self.registry.descriptor(&n.kind);
        for (name, v) in &n.props {
            let is_record = descriptor.is_some_and(|descriptor| {
                descriptor
                    .props
                    .iter()
                    .any(|prop| prop.name == name && prop.accepts_record())
            });
            let value = if is_record {
                Value::item_from_json(v)
            } else {
                Value::try_from_json(v)
            };
            match (schema.props.iter().position(|p| p == name), value) {
                (Some(i), Ok(v)) => props[i] = Prop::Lit(v),
                (Some(_), Err(e)) => {
                    self.diag(Severity::Warning, &path, format!("prop `{name}`: {e}"))
                }
                (None, _) if known => self.diag(
                    Severity::Warning,
                    &path,
                    format!("`{}` has no prop `{name}`", n.kind),
                ),
                (None, _) => {}
            }
        }
        let mut deps = Vec::new();
        for (name, slot) in &n.bind {
            let Some(i) = schema.props.iter().position(|p| p == name) else {
                if known {
                    self.diag(
                        Severity::Warning,
                        &path,
                        format!("`{}` has no prop `{name}` to bind", n.kind),
                    );
                }
                continue;
            };
            if let Some(path) = slot.strip_prefix("@item.") {
                props[i] = Prop::Item(path.split('.').map(|s| s.to_owned().into()).collect());
                continue;
            }
            if slot == "@item" {
                props[i] = Prop::Item(Box::new([]));
                continue;
            }
            let slot = slot.strip_prefix('@').unwrap_or(slot);
            let id = self.slot(slot);
            if let Prop::Lit(lit) = &props[i]
                && self.t.slot_defaults[id.0 as usize] == Value::Null
            {
                self.t.slot_defaults[id.0 as usize] = lit.clone();
            }
            props[i] = Prop::Slot(id);
            if !deps.contains(&id) {
                deps.push(id);
                self.t.slot_nodes[id.0 as usize].push(index);
            }
        }
        // Item references and style tokens become positional recipes at compilation.
        for (i, prop) in props.iter_mut().enumerate() {
            if let Prop::Lit(Value::Str(text)) = prop {
                if let Some(item) = text.strip_prefix("@item.") {
                    *prop = Prop::Item(item.split('.').map(|s| s.to_owned().into()).collect());
                } else if text.as_ref() == "@item" {
                    *prop = Prop::Item(Box::new([]));
                } else if let Some(name) = text.strip_prefix('@') {
                    let id = self.slot(name);
                    *prop = Prop::Slot(id);
                    if !deps.contains(&id) {
                        deps.push(id);
                        self.t.slot_nodes[id.0 as usize].push(index);
                    }
                } else if schema
                    .props
                    .get(i)
                    .is_some_and(|name| matches!(*name, "size" | "gap" | "color"))
                {
                    let name = schema.props.get(i).copied().unwrap_or_default();
                    let parsed = if name == "color" {
                        ColorName::from_name(text).map(Prop::Color)
                    } else {
                        parse_size(text).map(Prop::Size)
                    };
                    if let Some(parsed) = parsed {
                        *prop = parsed;
                    } else {
                        self.diag(
                            Severity::Warning,
                            &path,
                            format!("invalid {name} token `{text}`"),
                        );
                        *prop = Prop::Unset;
                    }
                }
            }
        }
        let each = n
            .each
            .as_ref()
            .map(|name| self.slot(name.trim_start_matches('@')));
        let when = n
            .when
            .as_ref()
            .map(|name| self.slot(name.trim_start_matches('@')));
        for id in each.into_iter().chain(when) {
            if !deps.contains(&id) {
                deps.push(id);
                self.t.slot_nodes[id.0 as usize].push(index);
            }
        }
        let mut events = vec![None; schema.events.len()];
        for (ev, name) in &n.on {
            match schema.events.iter().position(|e| e == ev) {
                Some(i) => events[i] = Some(self.event(name)),
                None if known => self.diag(
                    Severity::Warning,
                    &path,
                    format!("`{}` has no event `{ev}`", n.kind),
                ),
                None => {}
            }
        }
        let mut change = None;
        if let (Some(i), Some(ev)) = (
            schema.props.iter().position(|p| *p == "value"),
            schema.events.iter().position(|e| *e == "change"),
        ) {
            let target = match props.get(i) {
                Some(Prop::Slot(slot)) => Some(crate::payload::ChangeTarget::Slot(*slot)),
                Some(Prop::Item(path)) => {
                    Some(crate::payload::ChangeTarget::Item(Arc::from(path.as_ref())))
                }
                _ => None,
            };
            if let Some(target) = target {
                let event = events.get(ev).copied().flatten().unwrap_or_else(|| {
                    let name = match &target {
                        crate::payload::ChangeTarget::Slot(slot) => format!(
                            "{}.change",
                            self.t
                                .names
                                .slots
                                .get(slot.0 as usize)
                                .map(String::as_str)
                                .unwrap_or_default()
                        ),
                        crate::payload::ChangeTarget::Item(path) => format!(
                            "item.{}.change",
                            path.iter()
                                .map(|part| part.as_ref())
                                .collect::<Vec<_>>()
                                .join(".")
                        ),
                    };
                    self.event(&name)
                });
                events[ev] = Some(event);
                change = Some((target, event));
            }
        }
        if let Some(id) = &n.id {
            if self.t.ids.contains_key(id.as_str()) {
                self.diag(Severity::Error, &path, format!("duplicate id `{id}` (ids must be unique; they key interaction state and selectors)"));
            } else {
                self.t.ids.insert(id.clone().into(), index);
            }
        }
        self.t.nodes.push(NodeSpec {
            kind: n.kind.clone().into(),
            id: n.id.clone().map(Into::into),
            widget,
            props: props.into(),
            events: events.into(),
            deps: deps.into(),
            each,
            when,
            key: n
                .key
                .as_deref()
                .unwrap_or("id")
                .trim_start_matches("@item.")
                .split('.')
                .map(|s| s.to_owned().into())
                .collect(),
            change,
            uniform_rows: false,
            children: (0, 0),
        });
        let mut kids = Vec::with_capacity(n.children.len());
        for (i, child) in n.children.iter().enumerate() {
            kids.push(self.node(child, format!("{path}/children/{i}"), depth + 1)?);
        }
        let start = self.t.child_index.len() as u32;
        let uniform_rows = kids.len() == 1
            && kids
                .first()
                .and_then(|i| self.t.node(*i))
                .is_some_and(|child| {
                    child.each.is_none()
                        && child.when.is_none()
                        && self.t.children(child).is_empty()
                        && child.widget.uniform_row(&child.props)
                });
        self.t.child_index.extend(kids);
        let end = self.t.child_index.len() as u32;
        if let Some(node) = self.t.nodes.get_mut(index as usize) {
            node.children = (start, end);
            node.uniform_rows = uniform_rows;
        }
        Ok(index)
    }
}

pub(crate) fn parse_size(value: &str) -> Option<Size> {
    match value {
        "xs" => Some(Size::Xs),
        "sm" => Some(Size::Sm),
        "md" => Some(Size::Md),
        "lg" => Some(Size::Lg),
        "xl" => Some(Size::Xl),
        _ => None,
    }
}
