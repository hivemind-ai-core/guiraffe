//! Typed, pre-resolved addresses for application logic. Missing names are inert.
//!
//! ```
//! use guiraffe_ui::{Bindings, Document, Node, Registry, Template, Slot, PatchBuf, SlotTable};
//! use gpui_kit::SharedString;
//! let template = Template::compile(&Document::new(Node::new("text").bind("text", "title")), &Registry::with_builtins())?;
//! let mut bindings = Bindings::new(template.names());
//! let title: Slot<SharedString> = bindings.slot("title");
//! let mut patches = PatchBuf::default();
//! title.set(&mut patches, "Saved");
//! let mut table = SlotTable::with_defaults(template.slot_defaults());
//! table.apply(&mut patches);
//! assert_eq!(title.get(&table).as_deref(), Some("Saved"));
//! # Ok::<(), guiraffe_ui::CompileError>(())
//! ```
use crate::{Diagnostic, EventId, Names, Severity, SlotId, SlotTable, Value};
use gpui_kit::SharedString;
use std::marker::PhantomData;

pub trait SlotValue: Sized {
    fn read(value: &Value) -> Option<Self>;
}
impl SlotValue for SharedString {
    fn read(value: &Value) -> Option<Self> {
        match value {
            Value::Str(v) => Some(v.clone()),
            _ => None,
        }
    }
}
impl SlotValue for bool {
    fn read(value: &Value) -> Option<Self> {
        value.as_bool()
    }
}
impl SlotValue for f64 {
    fn read(value: &Value) -> Option<Self> {
        value.as_f64()
    }
}
impl SlotValue for Value {
    fn read(value: &Value) -> Option<Self> {
        Some(value.clone())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Slot<T> {
    id: SlotId,
    marker: PhantomData<fn() -> T>,
}
impl<T> Copy for Slot<T> {}
impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            id: SlotId(u32::MAX),
            marker: PhantomData,
        }
    }
}
impl<T> Slot<T> {
    pub fn id(self) -> SlotId {
        self.id
    }
    pub fn is_resolved(self) -> bool {
        self.id.0 != u32::MAX
    }
}
impl<T: SlotValue> Slot<T> {
    pub fn get(self, table: &SlotTable) -> Option<T> {
        self.is_resolved()
            .then(|| T::read(table.get(self.id)))
            .flatten()
    }
}
impl<T: Into<Value>> Slot<T> {
    pub fn set(self, out: &mut crate::PatchBuf, value: impl Into<T>) {
        out.set(self.id, value.into());
    }
}
impl<T> From<Slot<T>> for SlotId {
    fn from(value: Slot<T>) -> Self {
        value.id
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event(EventId);
impl Default for Event {
    fn default() -> Self {
        Self(EventId(u32::MAX))
    }
}
impl Event {
    pub fn id(self) -> EventId {
        self.0
    }
    pub fn is_resolved(self) -> bool {
        self.0.0 != u32::MAX
    }
    pub fn matches(self, event: EventId) -> bool {
        self.is_resolved() && self.0 == event
    }
}
impl From<Event> for EventId {
    fn from(value: Event) -> Self {
        value.id()
    }
}
pub struct Bindings<'a> {
    names: &'a Names,
    diagnostics: Vec<Diagnostic>,
}
impl<'a> Bindings<'a> {
    pub fn new(names: &'a Names) -> Self {
        Self {
            names,
            diagnostics: Vec::new(),
        }
    }
    fn missing(&mut self, kind: &str, name: &str) {
        self.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            path: "/bindings".into(),
            message: format!("unresolved {kind} `{name}`"),
        });
    }
    pub fn slot<T>(&mut self, name: &str) -> Slot<T> {
        match self.names.slot(name) {
            Some(id) => Slot {
                id,
                marker: PhantomData,
            },
            None => {
                self.missing("slot", name);
                Slot::default()
            }
        }
    }
    pub fn event(&mut self, name: &str) -> Event {
        match self.names.event(name) {
            Some(id) => Event(id),
            None => {
                self.missing("event", name);
                Event::default()
            }
        }
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
