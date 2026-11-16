//! Stateful GPUI Controls adapters, cached per template node and keyed item.
use crate::{
    Payload, Value,
    widget::{NodeView, Registry, Schema, Widget},
};
use gpui_kit::component::{
    Disableable, Sizable,
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, div, px,
};
use guiraffe_core::{Size, StyleContext};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
pub(crate) type Change = Rc<dyn Fn(Payload, &mut Window, &mut App)>;
#[derive(Default)]
pub(crate) struct ControlCache(RefCell<HashMap<u64, HashMap<u32, Entity<Control>>>>);
impl ControlCache {
    #[cfg(feature = "test-support")]
    pub fn native_value(&self, node: u32, cx: &App) -> Option<Value> {
        let cache = self.0.borrow();
        let control = cache.get(&0)?.get(&node)?.read(cx);
        if let Some(input) = &control.input {
            Some(Value::Str(input.read(cx).value()))
        } else {
            control.select.as_ref().map(|select| {
                select
                    .read(cx)
                    .selected_value()
                    .cloned()
                    .map(Value::Str)
                    .unwrap_or_default()
            })
        }
    }
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
    pub fn remove_scope(&self, scope: u64) {
        self.0.borrow_mut().remove(&scope);
    }
}
#[derive(Debug, Clone, Copy)]
enum Kind {
    Text,
    Checkbox,
    Switch,
    Select,
}
#[derive(Clone, PartialEq)]
struct Props {
    value: Value,
    label: SharedString,
    options: Value,
    disabled: bool,
    placeholder: SharedString,
}
impl Props {
    fn from_node(n: &NodeView<'_>) -> Self {
        Self {
            value: n.prop(0).clone(),
            label: n.text(1),
            options: n.prop(2).clone(),
            disabled: n.bool(3),
            placeholder: n.text(4),
        }
    }
}
struct Control {
    kind: Kind,
    props: Props,
    change: Change,
    id: gpui_kit::ElementId,
    selector: Option<SharedString>,
    input: Option<Entity<InputState>>,
    select: Option<Entity<SelectState<Vec<SharedString>>>>,
    _subscriptions: Vec<Subscription>,
}
fn options(value: &Value) -> Vec<SharedString> {
    (0..value.list_len().unwrap_or(0))
        .filter_map(|i| value.list_get(i))
        .map(Value::display)
        .collect()
}
impl Control {
    fn new(
        kind: Kind,
        props: Props,
        change: Change,
        id: gpui_kit::ElementId,
        selector: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = Vec::new();
        let input = if matches!(kind, Kind::Text) {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder(props.placeholder.clone()));
            input.update(cx, |state, cx| {
                state.set_value(props.value.display(), window, cx)
            });
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                |this, input, event, window, cx| {
                    if matches!(event, InputEvent::Change) && !this.props.disabled {
                        (this.change)(Payload::Text(input.read(cx).value()), window, cx);
                    }
                },
            ));
            Some(input)
        } else {
            None
        };
        let select = if matches!(kind, Kind::Select) {
            let select = cx.new(|cx| SelectState::new(options(&props.options), None, window, cx));
            select.update(cx, |state, cx| {
                state.set_selected_value(&props.value.display(), window, cx)
            });
            subscriptions.push(
                cx.subscribe_in(&select, window, |this, _, event, window, cx| {
                    let SelectEvent::Confirm(value) = event;
                    if !this.props.disabled {
                        (this.change)(Payload::Selection(value.clone()), window, cx);
                    }
                }),
            );
            Some(select)
        } else {
            None
        };
        Self {
            kind,
            props,
            change,
            id,
            selector,
            input,
            select,
            _subscriptions: subscriptions,
        }
    }
    fn needs_sync(&self, props: &Props, cx: &App) -> bool {
        self.props != *props
            || self
                .input
                .as_ref()
                .is_some_and(|state| state.read(cx).value() != props.value.display())
            || self.select.as_ref().is_some_and(|state| {
                state.read(cx).selected_value().map(|s| s.as_ref()) != props.value.as_str()
            })
    }
    fn sync(&mut self, props: Props, window: &mut Window, cx: &mut Context<Self>) {
        if self.props.placeholder != props.placeholder
            && let Some(input) = &self.input
        {
            input.update(cx, |state, cx| {
                state.set_placeholder(props.placeholder.clone(), window, cx)
            });
        }
        if let Some(input) = &self.input
            && input.read(cx).value() != props.value.display()
        {
            input.update(cx, |state, cx| {
                state.set_value(props.value.display(), window, cx)
            });
        }
        if let Some(select) = &self.select {
            if props.options != self.props.options {
                select.update(cx, |state, cx| {
                    state.set_items(options(&props.options), window, cx)
                });
            }
            if select.read(cx).selected_value() != Some(&props.value.display()) {
                select.update(cx, |state, cx| {
                    state.set_selected_value(&props.value.display(), window, cx)
                });
            }
        }
        self.props = props;
        cx.notify();
    }
}
impl Render for Control {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let size = px(StyleContext::current().control_size.control_height());
        let change = self.change.clone();
        let disabled = self.props.disabled;
        let element = match self.kind {
            Kind::Checkbox => Checkbox::new(self.id.clone())
                .label(self.props.label.clone())
                .checked(self.props.value.as_bool().unwrap_or(false))
                .disabled(disabled)
                .with_size(size)
                .on_change(move |value, w, cx| {
                    if !disabled {
                        change(Payload::Bool(*value), w, cx);
                    }
                })
                .into_any_element(),
            Kind::Switch => Switch::new(self.id.clone())
                .label(self.props.label.clone())
                .checked(self.props.value.as_bool().unwrap_or(false))
                .disabled(disabled)
                .with_size(size)
                .on_change(move |value, w, cx| {
                    if !disabled {
                        change(Payload::Bool(*value), w, cx);
                    }
                })
                .into_any_element(),
            Kind::Text => self
                .input
                .as_ref()
                .map(|input| {
                    Input::new(input)
                        .disabled(disabled)
                        .with_size(size)
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
            Kind::Select => self
                .select
                .as_ref()
                .map(|state| {
                    Select::new(state)
                        .id(self.id.clone())
                        .placeholder(self.props.placeholder.clone())
                        .disabled(disabled)
                        .with_size(size)
                        .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
        };
        let wrapper = div()
            .w_full()
            .min_h(px(Size::Xs.control_height()))
            .child(element);
        #[cfg(feature = "test-support")]
        let wrapper = {
            use gpui_kit::InteractiveElement;
            let selector = self.selector.clone();
            wrapper.debug_selector(move || {
                selector
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            })
        };
        #[cfg(not(feature = "test-support"))]
        let _ = &self.selector;
        wrapper
    }
}
static CONTROL: Schema = Schema {
    props: &["value", "label", "options", "disabled", "placeholder"],
    events: &["change"],
};
struct ControlWidget(Kind);
impl Widget for ControlWidget {
    fn schema(&self) -> &'static Schema {
        &CONTROL
    }
    fn render(
        &self,
        n: &NodeView<'_>,
        _: Vec<gpui_kit::AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::AnyElement {
        let key = (n.index, n.scope);
        let props = Props::from_node(n);
        let existing = n
            .controls
            .0
            .borrow()
            .get(&key.1)
            .and_then(|scope| scope.get(&key.0))
            .cloned();
        let control = existing.unwrap_or_else(|| {
            let entity = cx.new(|cx| {
                Control::new(
                    self.0,
                    props.clone(),
                    n.change_handler(),
                    n.element_id(),
                    n.spec.id.clone(),
                    window,
                    cx,
                )
            });
            n.controls
                .0
                .borrow_mut()
                .entry(key.1)
                .or_default()
                .insert(key.0, entity.clone());
            entity
        });
        if control.read(cx).needs_sync(&props, cx) {
            control.update(cx, |control, cx| control.sync(props, window, cx));
        }
        control.into_any_element()
    }
}
pub(crate) fn register(registry: &mut Registry) {
    for (name, kind) in [
        ("text-input", Kind::Text),
        ("checkbox", Kind::Checkbox),
        ("switch", Kind::Switch),
        ("select", Kind::Select),
    ] {
        registry.register(name, ControlWidget(kind));
    }
}
