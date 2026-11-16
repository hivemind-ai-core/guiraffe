//! The dynamic widget interface and registry (the `dyn` side of static + dynamic UI).
//!
//! Widgets declare a [`Schema`]; the compiler stores each node's props and events in schema
//! order, so rendering reads them **by position** (no string lookups at runtime), and unknown
//! props/events in a document are reported as diagnostics.

use std::{collections::HashMap, rc::Rc, sync::Arc};

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Size};

use crate::{
    slots::SlotTable,
    template::{EventId, NodeSpec, Prop},
    value::Value,
};

/// Delivers an event to the host, stamped with the template generation it was wired for.
pub(crate) type InputEmit =
    Rc<dyn Fn(Option<EventId>, Option<crate::SlotId>, crate::Payload, u32, &mut Window, &mut App)>;

pub type Emit = Rc<dyn Fn(EventId, Value, u32, &mut Window, &mut App)>;

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

/// Everything a widget needs to render one node.
pub struct NodeView<'a> {
    pub spec: &'a NodeSpec,
    pub slots: &'a SlotTable,
    pub emit: &'a Emit,
    pub generation: u32,
    pub index: u32,
    pub item: Option<&'a Value>,
    pub(crate) input_emit: &'a InputEmit,
    pub(crate) controls: &'a crate::controls::ControlCache,
    pub(crate) scope: u64,
    pub(crate) active: Option<Rc<std::cell::Cell<bool>>>,
}

static NULL: Value = Value::Null;

impl NodeView<'_> {
    /// Prop at schema position `i` (literal, or the current value of its bound slot).
    pub fn prop(&self, i: usize) -> &Value {
        match self.spec.props.get(i) {
            Some(Prop::Lit(v)) => v,
            Some(Prop::Slot(id)) => self.slots.get(*id),
            Some(Prop::Item(path)) => self.item.map(|v| v.field(path)).unwrap_or(&NULL),
            Some(Prop::Size(_) | Prop::Color(_) | Prop::Unset) | None => &NULL,
        }
    }
    pub fn text(&self, i: usize) -> SharedString {
        self.prop(i).display()
    }
    pub fn bool(&self, i: usize) -> bool {
        self.prop(i).as_bool().unwrap_or(false)
    }
    pub fn num(&self, i: usize) -> Option<f64> {
        self.prop(i).as_f64()
    }
    pub fn size(&self, i: usize) -> Option<Size> {
        match self.spec.props.get(i) {
            Some(Prop::Size(value)) => Some(*value),
            Some(Prop::Slot(id)) => self.slots.size(*id),
            _ => None,
        }
    }
    pub fn color(&self, i: usize) -> Option<ColorName> {
        match self.spec.props.get(i) {
            Some(Prop::Color(value)) => Some(*value),
            Some(Prop::Slot(id)) => self.slots.color(*id),
            _ => None,
        }
    }
    /// Handler for the widget's event at schema position `i`, if the document wires it.
    pub fn handler(&self, i: usize) -> Option<impl Fn(Value, &mut Window, &mut App) + 'static> {
        let id = (*self.spec.events.get(i)?)?;
        let active = self.active.clone();
        let (emit, generation) = (self.emit.clone(), self.generation);
        Some(move |payload: Value, w: &mut Window, cx: &mut App| {
            if active.as_ref().is_none_or(|alive| alive.get()) {
                emit(id, payload, generation, w, cx)
            }
        })
    }
    pub(crate) fn change_handler(&self) -> crate::controls::Change {
        let emit = self.input_emit.clone();
        let generation = self.generation;
        let (slot, event) = self
            .spec
            .change
            .map(|(slot, event)| (Some(slot), Some(event)))
            .unwrap_or((None, self.spec.events.first().copied().flatten()));
        let active = self.active.clone();
        Rc::new(move |payload, w, cx| {
            if active.as_ref().is_none_or(|alive| alive.get()) {
                emit(event, slot, payload, generation, w, cx)
            }
        })
    }
    pub fn element_id(&self) -> gpui_kit::ElementId {
        match &self.spec.id {
            Some(id) => gpui_kit::ElementId::Name(id.clone()),
            None => gpui_kit::ElementId::Integer(self.index as u64),
        }
    }
}

/// A widget kind usable from documents. Created once per kind and shared; `Send + Sync` so
/// templates can be compiled off the UI thread (spec/15).
pub trait Widget: Send + Sync + 'static {
    fn schema(&self) -> &'static Schema;
    fn render(
        &self,
        node: &NodeView<'_>,
        children: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement;
}

type RenderFn =
    dyn Fn(&NodeView<'_>, Vec<AnyElement>, &mut Window, &mut App) -> AnyElement + Send + Sync;

/// A widget from a schema and a render function.
pub struct FnWidget {
    schema: &'static Schema,
    render: Box<RenderFn>,
}

pub fn widget(
    schema: &'static Schema,
    render: impl Fn(&NodeView<'_>, Vec<AnyElement>, &mut Window, &mut App) -> AnyElement
    + Send
    + Sync
    + 'static,
) -> FnWidget {
    FnWidget {
        schema,
        render: Box::new(render),
    }
}

impl Widget for FnWidget {
    fn schema(&self) -> &'static Schema {
        self.schema
    }
    fn render(
        &self,
        node: &NodeView<'_>,
        children: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        (self.render)(node, children, window, cx)
    }
}

/// Rendered for unknown kinds: visible, never a panic.
pub struct Placeholder;

impl Widget for Placeholder {
    fn schema(&self) -> &'static Schema {
        &Schema::EMPTY
    }
    fn render(
        &self,
        node: &NodeView<'_>,
        children: Vec<AnyElement>,
        _: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .border_1()
            .border_dashed()
            .border_color(ColorName::Red.shade(5).to_hsla())
            .p(px(Size::Xs.spacing()))
            .text_color(s.dimmed.to_hsla())
            .child(format!("unknown widget `{}`", node.spec.kind))
            .children(children)
            .into_any_element()
    }
}

#[derive(Clone, Default)]
pub struct Registry {
    widgets: HashMap<SharedString, Arc<dyn Widget>>,
}

impl Registry {
    pub fn empty() -> Self {
        Self::default()
    }
    /// The built-in GUIraffe widgets (see [`crate::builtins`]).
    pub fn with_builtins() -> Self {
        let mut r = Self::empty();
        crate::builtins::register(&mut r);
        r
    }
    pub fn register(&mut self, kind: impl Into<SharedString>, widget: impl Widget) -> &mut Self {
        self.widgets.insert(kind.into(), Arc::new(widget));
        self
    }
    pub fn get(&self, kind: &str) -> Option<Arc<dyn Widget>> {
        self.widgets.get(kind).cloned()
    }
    pub fn kinds(&self) -> impl Iterator<Item = &str> {
        self.widgets.keys().map(|k| k.as_ref())
    }
}
