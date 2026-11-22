//! Native control views. Model observers synchronize shared control state after patch
//! transactions, never by writing a control entity from its parent's render.
use crate::{
    Payload, Value,
    widget::{NodeView, Registry, Schema, Widget},
};
use gpui_kit::component::{
    Disableable, FocusableExt, Sizable,
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Role,
    SharedString, Styled, Subscription, Window, div, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StateKind, StyleContext, Variant};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    time::Duration,
};
pub(crate) type Change = Rc<dyn Fn(Payload, &mut Window, &mut App)>;
pub(crate) type PropsRead = Rc<dyn Fn(u32, u64) -> Option<Props>>;
type Entries = HashMap<u64, HashMap<u32, Rc<Entry>>>;
#[derive(Default)]
pub(crate) struct ControlCache {
    scopes: RefCell<Entries>,
    nodes: RefCell<BTreeMap<(u32, u64), std::rc::Weak<Entry>>>,
}
impl ControlCache {
    pub fn clear(&self) {
        self.scopes.borrow_mut().clear();
        self.nodes.borrow_mut().clear();
    }
    pub fn remove_scope(&self, scope: u64) {
        if let Some(entries) = self.scopes.borrow_mut().remove(&scope) {
            for node in entries.keys() {
                self.nodes.borrow_mut().remove(&(*node, scope));
            }
        }
    }
    pub fn schedule_scope(&self, scope: u64, cx: &mut App) {
        if let Some(entries) = self.scopes.borrow().get(&scope) {
            for entry in entries.values() {
                entry.schedule(cx);
            }
        }
    }
    pub fn schedule_node(&self, node: u32, cx: &mut App) {
        for (_, entry) in self.nodes.borrow().range((node, 0)..=(node, u64::MAX)) {
            if let Some(entry) = entry.upgrade() {
                entry.schedule(cx);
            }
        }
    }
    #[cfg(feature = "test-support")]
    pub fn native_value(&self, node: u32, scope: u64, cx: &App) -> Option<Value> {
        let cache = self.scopes.borrow();
        let state = &cache.get(&scope)?.get(&node)?.state;
        if let Some(input) = &state.input {
            Some(Value::Str(input.read(cx).value()))
        } else {
            state.select.as_ref().map(|select| {
                select
                    .read(cx)
                    .selected_value()
                    .cloned()
                    .map(Value::Str)
                    .unwrap_or_default()
            })
        }
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Text,
    Checkbox,
    Switch,
    Select,
}

#[derive(Clone, PartialEq)]
struct ToggleProps {
    size: Option<Size>,
    accessibility_label: Option<SharedString>,
    tooltip: Option<SharedString>,
    tooltip_delay: Option<Duration>,
    tab_stop: bool,
    tab_index: isize,
    focus_ring: bool,
    role: Option<Role>,
    color: Option<ColorName>,
}

impl ToggleProps {
    const DEFAULT: Self = Self {
        size: None,
        accessibility_label: None,
        tooltip: None,
        tooltip_delay: None,
        tab_stop: true,
        tab_index: 0,
        focus_ring: true,
        role: None,
        color: None,
    };

    fn from_node(n: &NodeView<'_>) -> Option<Rc<Self>> {
        let prop = |name| n.prop(n.prop_index(name));
        let text = |name| match prop(name) {
            Value::Str(text) => Some(text.clone()),
            _ => None,
        };
        let props = Self {
            size: n.size(n.prop_index("size")),
            accessibility_label: text("accessibility-label"),
            tooltip: text("tooltip"),
            tooltip_delay: prop("tooltip-show-delay-ms")
                .as_f64()
                .map(|delay| Duration::from_millis(delay.clamp(0.0, 60_000.0) as u64)),
            tab_stop: prop("tab-stop").as_bool().unwrap_or(true),
            tab_index: prop("tab-index").as_f64().unwrap_or_default() as isize,
            focus_ring: prop("focus-ring").as_bool().unwrap_or(true),
            role: match prop("role").as_str() {
                Some("checkbox") => Some(Role::CheckBox),
                Some("radio") => Some(Role::RadioButton),
                Some("switch") => Some(Role::Switch),
                _ => None,
            },
            color: n.color(n.prop_index("color")),
        };
        (props != Self::DEFAULT).then(|| Rc::new(props))
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Props {
    value: Value,
    label: SharedString,
    options: Value,
    disabled: bool,
    invalid: bool,
    placeholder: SharedString,
    toggle: Option<Rc<ToggleProps>>,
}
impl Props {
    pub(crate) fn from_node(n: &NodeView<'_>) -> Self {
        Self {
            value: n.prop(n.prop_index("value")).clone(),
            label: n.text(n.prop_index("label")),
            options: n.prop(n.prop_index("options")).clone(),
            disabled: n.bool(n.prop_index("disabled")),
            invalid: n.bool(n.prop_index("invalid")),
            placeholder: n.text(n.prop_index("placeholder")),
            toggle: match n.spec.kind.as_ref() {
                "checkbox" | "switch" => ToggleProps::from_node(n),
                _ => None,
            },
        }
    }
}
struct Entry {
    view: Entity<Control>,
    state: Rc<State>,
    reader: PropsRead,
    node: u32,
    scope: u64,
    window: AnyWindowHandle,
    scheduled: Cell<bool>,
}
impl Entry {
    fn schedule(self: &Rc<Self>, cx: &mut App) {
        if self.scheduled.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        cx.defer(move |cx| {
            let Some(entry) = weak.upgrade() else { return };
            entry.scheduled.set(false);
            let Some(props) = (entry.reader)(entry.node, entry.scope) else {
                return;
            };
            let _ = entry.window.update(cx, |_, window, cx| {
                entry.state.sync(props, entry.view.entity_id(), window, cx)
            });
        });
    }
}
struct State {
    kind: Kind,
    props: RefCell<Props>,
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
impl State {
    fn new(
        kind: Kind,
        props: Props,
        change: Change,
        id: gpui_kit::ElementId,
        selector: Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Rc<Self> {
        Rc::new_cyclic(|weak: &std::rc::Weak<State>| {
            let mut subscriptions = Vec::new();
            let handle = window.window_handle();
            let input = if matches!(kind, Kind::Text) {
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(props.placeholder.clone()));
                input.update(cx, |state, cx| {
                    state.set_value(props.value.display(), window, cx)
                });
                let target = weak.clone();
                subscriptions.push(App::subscribe(cx, &input, move |input, event, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let Some(state) = target
                        .upgrade()
                        .filter(|state| !state.props.borrow().disabled)
                    else {
                        return;
                    };
                    let value = input.read(cx).value();
                    let change = state.change.clone();
                    let _ =
                        handle.update(cx, |_, window, cx| change(Payload::Text(value), window, cx));
                }));
                Some(input)
            } else {
                None
            };
            let select = if matches!(kind, Kind::Select) {
                let select =
                    cx.new(|cx| SelectState::new(options(&props.options), None, window, cx));
                select.update(cx, |state, cx| {
                    state.set_selected_value(&props.value.display(), window, cx)
                });
                let target = weak.clone();
                subscriptions.push(App::subscribe(cx, &select, move |_, event, cx| {
                    let SelectEvent::Confirm(value) = event;
                    let Some(state) = target
                        .upgrade()
                        .filter(|state| !state.props.borrow().disabled)
                    else {
                        return;
                    };
                    let change = state.change.clone();
                    let value = value.clone();
                    let _ = handle.update(cx, |_, window, cx| {
                        change(Payload::Selection(value), window, cx)
                    });
                }));
                Some(select)
            } else {
                None
            };
            State {
                kind,
                props: RefCell::new(props),
                change,
                id,
                selector,
                input,
                select,
                _subscriptions: subscriptions,
            }
        })
    }
    fn sync(&self, props: Props, view: gpui_kit::EntityId, window: &mut Window, cx: &mut App) {
        let previous = self.props.borrow().clone();
        let mut changed = previous != props;
        if let Some(input) = &self.input {
            if previous.placeholder != props.placeholder {
                input.update(cx, |state, cx| {
                    state.set_placeholder(props.placeholder.clone(), window, cx)
                });
            }
            if input.read(cx).value() != props.value.display() {
                input.update(cx, |state, cx| {
                    state.set_value(props.value.display(), window, cx)
                });
                changed = true;
            }
        }
        if let Some(select) = &self.select {
            if previous.options != props.options {
                select.update(cx, |state, cx| {
                    state.set_items(options(&props.options), window, cx)
                });
            }
            if select.read(cx).selected_value().map(|value| value.as_ref()) != props.value.as_str()
            {
                select.update(cx, |state, cx| {
                    state.set_selected_value(&props.value.display(), window, cx)
                });
                changed = true;
            }
        }
        *self.props.borrow_mut() = props;
        if changed {
            cx.notify(view);
        }
    }
}
struct Control {
    state: Rc<State>,
}
impl Render for Control {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.state;
        let props = state.props.borrow().clone();
        let control_size = props
            .toggle
            .as_ref()
            .and_then(|toggle| toggle.size)
            .unwrap_or_else(|| StyleContext::current().control_size);
        let size = px(control_size.control_height());
        let theme = GuiraffeTheme::global(cx);
        let font_size = px(control_size.font_size() * theme.scale);
        let error_border = props.invalid.then(|| {
            theme
                .state(StateKind::FocusRing, Variant::Outline, Some(ColorName::Red))
                .border
                .to_hsla()
        });
        let change = state.change.clone();
        let disabled = props.disabled;
        let element = match state.kind {
            Kind::Checkbox => {
                let mut checkbox = Checkbox::new(state.id.clone())
                    .label(props.label)
                    .checked(props.value.as_bool().unwrap_or(false))
                    .disabled(disabled)
                    .with_size(size)
                    .on_change(move |value, w, cx| {
                        if !disabled {
                            change(Payload::Bool(*value), w, cx);
                        }
                    });
                if let Some(toggle) = props.toggle.as_deref() {
                    checkbox = checkbox
                        .tab_stop(toggle.tab_stop)
                        .tab_index(toggle.tab_index)
                        .focus_ring(toggle.focus_ring);
                    if let Some(label) = &toggle.accessibility_label {
                        checkbox = checkbox.accessibility_label(label.clone());
                    }
                    if let Some(tooltip) = &toggle.tooltip {
                        checkbox = checkbox.tooltip(tooltip.clone());
                    }
                    if let Some(delay) = toggle.tooltip_delay {
                        checkbox = checkbox.tooltip_show_delay(delay);
                    }
                    if let Some(role) = toggle.role {
                        checkbox = checkbox.role(role);
                    }
                }
                checkbox.into_any_element()
            }
            Kind::Switch => {
                let mut switch = Switch::new(state.id.clone())
                    .label(props.label)
                    .checked(props.value.as_bool().unwrap_or(false))
                    .disabled(disabled)
                    .with_size(size)
                    .on_change(move |value, w, cx| {
                        if !disabled {
                            change(Payload::Bool(*value), w, cx);
                        }
                    });
                if let Some(toggle) = props.toggle.as_deref() {
                    switch = switch
                        .tab_stop(toggle.tab_stop)
                        .tab_index(toggle.tab_index)
                        .focus_ring(toggle.focus_ring);
                    if let Some(label) = &toggle.accessibility_label {
                        switch = switch.accessibility_label(label.clone());
                    }
                    if let Some(tooltip) = &toggle.tooltip {
                        switch = switch.tooltip(tooltip.clone());
                    }
                    if let Some(delay) = toggle.tooltip_delay {
                        switch = switch.tooltip_show_delay(delay);
                    }
                    if let Some(color) = toggle.color {
                        switch = switch.color(
                            theme
                                .variant(Some(color), Variant::Filled)
                                .background
                                .to_hsla(),
                        );
                    }
                }
                switch.into_any_element()
            }
            Kind::Text => state
                .input
                .as_ref()
                .map(|input| {
                    let mut input = Input::new(input)
                        .disabled(disabled)
                        .with_size(size)
                        .text_size(font_size);
                    if let Some(border) = error_border {
                        input = input.border_color(border);
                    }
                    input.into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
            Kind::Select => state
                .select
                .as_ref()
                .map(|select| {
                    let mut select = Select::new(select)
                        .id(state.id.clone())
                        .placeholder(props.placeholder)
                        .disabled(disabled)
                        .with_size(size)
                        .text_size(font_size);
                    if let Some(border) = error_border {
                        select = select.border_color(border);
                    }
                    select.into_any_element()
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
            let selector = state.selector.clone();
            wrapper.debug_selector(move || {
                selector
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            })
        };
        #[cfg(not(feature = "test-support"))]
        let _ = &state.selector;
        wrapper
    }
}
struct ControlWidget(Kind);
impl Widget for ControlWidget {
    fn uniform_row(&self, _: &[crate::Prop]) -> bool {
        matches!(self.0, Kind::Text | Kind::Select)
    }
    fn schema(&self) -> &'static Schema {
        &self.0.spec().1
    }
    fn render(
        &self,
        n: &NodeView<'_>,
        _: Vec<gpui_kit::AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui_kit::AnyElement {
        let existing = n
            .controls
            .scopes
            .borrow()
            .get(&n.scope)
            .and_then(|entries| entries.get(&n.index))
            .cloned();
        let entry = existing.unwrap_or_else(|| {
            #[cfg(feature = "test-support")]
            let selector = n.selector();
            #[cfg(not(feature = "test-support"))]
            let selector = None;
            let state = State::new(
                self.0,
                Props::from_node(n),
                n.change_handler(),
                n.element_id(),
                selector,
                window,
                cx,
            );
            let view = cx.new(|_| Control {
                state: state.clone(),
            });
            let entry = Rc::new(Entry {
                view,
                state,
                reader: n.reader.clone(),
                node: n.index,
                scope: n.scope,
                window: window.window_handle(),
                scheduled: Cell::new(false),
            });
            n.controls
                .scopes
                .borrow_mut()
                .entry(n.scope)
                .or_default()
                .insert(n.index, entry.clone());
            n.controls
                .nodes
                .borrow_mut()
                .insert((n.index, n.scope), Rc::downgrade(&entry));
            entry
        });
        entry.view.clone().into_any_element()
    }
}
impl Kind {
    fn spec(self) -> &'static (crate::catalog::WidgetSpec, Schema) {
        use crate::catalog::{CHECKBOX, SELECT, SWITCH, TEXT_INPUT};
        match self {
            Self::Text => &TEXT_INPUT,
            Self::Checkbox => &CHECKBOX,
            Self::Switch => &SWITCH,
            Self::Select => &SELECT,
        }
    }
}

pub(crate) fn register(registry: &mut Registry) {
    for kind in [Kind::Text, Kind::Checkbox, Kind::Switch, Kind::Select] {
        registry.register(kind.spec().0.id, ControlWidget(kind));
    }
}
