//! DataUi adapter: a [`SpotlightState`] embedded inline through [`Spotlight`].
//!
//! Document values are applied when they change, so the live palette keeps its own query,
//! selection and recently run actions between renders.

use gpui_kit::assets::IconName;
use gpui_kit::{
    App, AppContext, Context, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, Subscription, Window, div, px,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{Spotlight, SpotlightAction, SpotlightEvent, SpotlightLabels, SpotlightState};
use crate::widget_specs;

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::SPOTLIGHT,
            schema: guiraffe_core::widget_schema!(&widget_specs::SPOTLIGHT),
            render: |node, _, _, _| SpotlightDocument::from_node(node).into_any_element(),
        }],
    );
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

fn text(value: &Value, name: &str) -> Option<SharedString> {
    field(value, name)
        .as_str()
        .map(|text| SharedString::from(text.to_owned()))
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn ids(value: &Value) -> Vec<SharedString> {
    items(value)
        .filter_map(Value::as_str)
        .map(|id| SharedString::from(id.to_owned()))
        .collect()
}

fn count(value: &Value) -> Option<usize> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && v.fract() == 0.0 && *v <= u32::MAX as f64)
        .map(|v| v as usize)
}

fn pixels(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|v| v as f32)
}

/// A bundled icon by asset path (`icons/file-plus.svg`) or file stem (`file-plus`).
fn named_icon(name: &str) -> Option<IconName> {
    IconName::ALL.iter().copied().find(|icon| {
        let path = icon.path();
        path.as_ref() == name
            || path
                .rsplit('/')
                .next()
                .and_then(|file| file.strip_suffix(".svg"))
                == Some(name)
    })
}

fn parse_actions(value: &Value) -> Vec<SpotlightAction> {
    items(value)
        .filter_map(|item| {
            let mut action = SpotlightAction::new(text(item, "id")?, text(item, "title")?);
            action.description = text(item, "description");
            action.group = text(item, "group");
            action.keywords = text(item, "keywords").unwrap_or_default();
            action.icon = field(item, "icon").as_str().and_then(named_icon);
            if let Some(shortcut) = field(item, "shortcut").as_str() {
                action = action.shortcut(shortcut);
            }
            action.disabled = field(item, "disabled").as_bool().unwrap_or_default();
            Some(action)
        })
        .collect()
}

fn parse_labels(value: &Value, placeholder: Option<SharedString>) -> SpotlightLabels {
    let defaults = SpotlightLabels::default();
    SpotlightLabels {
        placeholder: placeholder.unwrap_or(defaults.placeholder),
        pinned: text(value, "pinned").unwrap_or(defaults.pinned),
        recent: text(value, "recent").unwrap_or(defaults.recent),
        empty_title: text(value, "emptyTitle").unwrap_or(defaults.empty_title),
        empty_description: text(value, "emptyDescription").unwrap_or(defaults.empty_description),
        no_results_title: text(value, "noResultsTitle").unwrap_or(defaults.no_results_title),
        no_results_description: text(value, "noResultsDescription")
            .unwrap_or(defaults.no_results_description),
        loading: text(value, "loading").unwrap_or(defaults.loading),
        name: text(value, "name").unwrap_or(defaults.name),
    }
}

/// Property values as last applied to the live palette.
#[derive(Clone, PartialEq)]
struct Props {
    actions: Value,
    pinned: Value,
    recents: Value,
    recent_limit: usize,
    limit: Option<usize>,
    labels: SpotlightLabels,
    loading: bool,
    query: Option<SharedString>,
    width: Option<f32>,
    max_list_height: Option<f32>,
}

impl Props {
    fn from_node(node: &dyn WidgetNode) -> Self {
        Self {
            actions: node.prop(0).clone(),
            pinned: node.prop(1).clone(),
            recents: node.prop(2).clone(),
            recent_limit: count(node.prop(3)).unwrap_or(5),
            limit: count(node.prop(4)),
            labels: parse_labels(node.prop(6), text_value(node.prop(5))),
            loading: node.prop(7).as_bool().unwrap_or_default(),
            query: text_value(node.prop(8)),
            width: pixels(node.prop(9)),
            max_list_height: pixels(node.prop(10)),
        }
    }
}

fn text_value(value: &Value) -> Option<SharedString> {
    value
        .as_str()
        .map(|text| SharedString::from(text.to_owned()))
}

#[derive(IntoElement)]
struct SpotlightDocument {
    key: ElementId,
    id: ElementId,
    props: Props,
    handlers: [Option<EventHandler>; 3],
}

impl SpotlightDocument {
    fn from_node(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            id: node.element_id(),
            props: Props::from_node(node),
            handlers: std::array::from_fn(|index| node.handler(index)),
        }
    }
}

struct DocumentState {
    applied: Option<Props>,
    handlers: [Option<EventHandler>; 3],
    view: Entity<SpotlightState>,
    _subscription: Subscription,
}

fn event_payload(event: &SpotlightEvent) -> Option<(usize, Value)> {
    let id = |id: &SharedString| Value::from(id.to_string());
    Some(match event {
        SpotlightEvent::Activated(action) => (0, id(action)),
        SpotlightEvent::SelectionChanged(action) => (1, action.as_ref().map_or(Value::Null, id)),
        SpotlightEvent::QueryChanged(query) => (2, id(query)),
        SpotlightEvent::Opened | SpotlightEvent::Closed => return None,
    })
}

/// Apply the document's values that changed since the last render.
fn apply(
    view: &mut SpotlightState,
    old: Option<&Props>,
    new: &Props,
    window: &mut Window,
    cx: &mut Context<SpotlightState>,
) {
    // The recent limit truncates recents, so it goes first.
    if old.is_none_or(|old| old.recent_limit != new.recent_limit) {
        view.set_recent_limit(new.recent_limit, cx);
    }
    if old.is_none_or(|old| old.actions != new.actions) {
        view.set_actions(parse_actions(&new.actions), cx);
    }
    if old.is_none_or(|old| old.pinned != new.pinned) {
        view.set_pinned(ids(&new.pinned), cx);
    }
    if old.is_none_or(|old| old.recents != new.recents) {
        view.set_recents(ids(&new.recents), cx);
    }
    if old.is_none_or(|old| old.limit != new.limit) {
        view.set_limit(new.limit, cx);
    }
    if old.is_none_or(|old| old.labels != new.labels) {
        view.set_labels(new.labels.clone(), window, cx);
    }
    if old.is_none_or(|old| old.loading != new.loading) {
        view.set_loading(new.loading, cx);
    }
    if old.is_none_or(|old| old.width != new.width) {
        view.set_width(new.width.map(px), cx);
    }
    if old.is_none_or(|old| old.max_list_height != new.max_list_height) {
        view.set_max_list_height(new.max_list_height.map(px), cx);
    }
    if old.is_none_or(|old| old.query != new.query)
        && let Some(query) = &new.query
    {
        view.set_query(query, window, cx);
    }
}

impl RenderOnce for SpotlightDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let view = cx.new(|cx| SpotlightState::new(window, cx));
            let subscription = cx.subscribe_in(
                &view,
                window,
                |state: &mut DocumentState, _, event, window, cx| {
                    if let Some((index, payload)) = event_payload(event)
                        && let Some(handler) = &state.handlers[index]
                    {
                        handler(payload, window, cx);
                    }
                },
            );
            DocumentState {
                applied: None,
                handlers: std::array::from_fn(|_| None),
                view,
                _subscription: subscription,
            }
        });
        let view = state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            if state.applied.as_ref() != Some(&self.props) {
                let old = state.applied.take();
                state.view.update(cx, |view, cx| {
                    apply(view, old.as_ref(), &self.props, window, cx)
                });
                state.applied = Some(self.props);
            }
            state.view.clone()
        });
        // The node id scopes the panel's element ids, so several palettes can share a window.
        div().id(self.id).child(Spotlight::new(&view))
    }
}
