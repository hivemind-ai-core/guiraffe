//! The dynamic widget interface and registry (the `dyn` side of static + dynamic UI).

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

pub type Emit = Rc<dyn Fn(EventId, Value, &mut Window, &mut App)>;

/// Everything a widget needs to render one node: resolved props, rendered children, events.
pub struct NodeView<'a> {
    pub spec: &'a NodeSpec,
    pub slots: &'a SlotTable,
    pub emit: &'a Emit,
    pub index: u32,
}

impl NodeView<'_> {
    pub fn prop(&self, name: &str) -> Option<&Value> {
        self.spec
            .props
            .iter()
            .find(|(k, _)| k.as_ref() == name)
            .map(|(_, p)| match p {
                Prop::Lit(v) => v,
                Prop::Slot(id) => self.slots.get(*id),
            })
    }
    pub fn text(&self, name: &str) -> SharedString {
        self.prop(name).map(Value::display).unwrap_or_default()
    }
    pub fn bool(&self, name: &str) -> bool {
        self.prop(name).and_then(Value::as_bool).unwrap_or(false)
    }
    pub fn num(&self, name: &str) -> Option<f64> {
        self.prop(name).and_then(Value::as_f64)
    }
    pub fn size(&self, name: &str) -> Option<Size> {
        match self.prop(name)?.as_str()? {
            "xs" => Some(Size::Xs),
            "sm" => Some(Size::Sm),
            "md" => Some(Size::Md),
            "lg" => Some(Size::Lg),
            "xl" => Some(Size::Xl),
            _ => None,
        }
    }
    pub fn color(&self, name: &str) -> Option<ColorName> {
        ColorName::from_name(self.prop(name)?.as_str()?)
    }
    /// A click-style handler for widget event `event`, if the document wires it.
    pub fn handler(&self, event: &str) -> Option<impl Fn(&mut Window, &mut App) + 'static> {
        let id = self
            .spec
            .events
            .iter()
            .find(|(e, _)| e.as_ref() == event)?
            .1;
        let emit = self.emit.clone();
        Some(move |w: &mut Window, cx: &mut App| emit(id, Value::Null, w, cx))
    }
    pub fn element_id(&self) -> gpui_kit::ElementId {
        match &self.spec.id {
            Some(id) => gpui_kit::ElementId::Name(id.clone()),
            None => gpui_kit::ElementId::Integer(self.index as u64),
        }
    }
}

/// A widget kind usable from documents. Implementations are created once per kind and shared;
/// `Send + Sync` so templates can be compiled off the UI thread (spec/15).
pub trait Widget: Send + Sync + 'static {
    fn render(
        &self,
        node: &NodeView<'_>,
        children: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement;
}

impl<F> Widget for F
where
    F: Fn(&NodeView<'_>, Vec<AnyElement>, &mut Window, &mut App) -> AnyElement + Send + Sync + 'static,
{
    fn render(
        &self,
        node: &NodeView<'_>,
        children: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self(node, children, window, cx)
    }
}

/// Rendered for unknown kinds: visible, never a panic.
pub struct Placeholder;

impl Widget for Placeholder {
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
            .p(px(4.))
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
