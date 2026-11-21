//! Crate-owned declarative widget renderers and application registration.

use std::{collections::BTreeMap, rc::Rc};

use gpui_kit::{AnyElement, App, ElementId, Global, SharedString, Window};

use crate::{
    ColorName, Size,
    widget_spec::{Schema, WidgetSpec},
    widget_value::Value,
};

pub type EventHandler = Rc<dyn Fn(Value, &mut Window, &mut App)>;

/// Resolved properties and events for one document node.
pub trait WidgetNode {
    fn prop(&self, index: usize) -> &Value;
    fn size(&self, index: usize) -> Option<Size>;
    fn color(&self, index: usize) -> Option<ColorName>;
    fn handler(&self, index: usize) -> Option<EventHandler>;
    fn element_id(&self) -> ElementId;
}

pub type RenderWidget = fn(&dyn WidgetNode, Vec<AnyElement>, &mut Window, &mut App) -> AnyElement;

#[derive(Clone, Copy)]
pub struct WidgetRegistration {
    pub spec: &'static WidgetSpec,
    pub schema: &'static Schema,
    pub render: RenderWidget,
}

/// Widgets installed by component crates in this application.
#[derive(Default)]
pub struct WidgetRegistry(BTreeMap<SharedString, WidgetRegistration>);

impl Global for WidgetRegistry {}

impl WidgetRegistry {
    pub fn install(cx: &mut App, registrations: &[WidgetRegistration]) {
        let registry = cx.default_global::<Self>();
        for registration in registrations {
            registry
                .0
                .insert(registration.spec.id.into(), *registration);
        }
    }

    pub fn registrations(&self) -> impl Iterator<Item = WidgetRegistration> + '_ {
        self.0.values().copied()
    }
}
