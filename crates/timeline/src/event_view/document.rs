//! Document adapter for the multi-track event timeline.

use gpui_kit::{App, AppContext, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    ColorName, Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{EventOrientation, EventTimeline, EventTimelineEvent, EventTimelineState};
use crate::widget_specs;
use crate::{EventLink, EventModel, EventModelError, EventTrack, LinkKind, TimelineEvent};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::EVENT_TIMELINE,
            schema: guiraffe_core::widget_schema!(&widget_specs::EVENT_TIMELINE),
            render: |node, _, _, _| EventTimelineDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

const MAX_SAFE: f64 = ((1_u64 << 53) - 1) as f64;

fn id(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|v| (0.0..=MAX_SAFE).contains(v) && v.fract() == 0.0)
        .map(|v| v as u64)
}

fn position(value: &Value) -> Option<i64> {
    value
        .as_f64()
        .filter(|v| (-MAX_SAFE..=MAX_SAFE).contains(v) && v.fract() == 0.0)
        .map(|v| v as i64)
}

fn color(value: &Value) -> Option<ColorName> {
    value.as_str().and_then(ColorName::from_name)
}

/// Build the model from document records. Records missing required fields are skipped;
/// inconsistent IDs (duplicates, unknown tracks or events) are reported as an error.
fn parse_model(
    tracks: &Value,
    events: &Value,
    links: &Value,
) -> Result<EventModel, EventModelError> {
    let tracks = items(tracks)
        .filter_map(|track| {
            Some(EventTrack {
                id: id(field(track, "id"))?,
                label: field(track, "label").as_str()?.to_owned(),
                color: color(field(track, "color")),
            })
        })
        .collect();
    let events = items(events)
        .filter_map(|event| {
            Some(TimelineEvent {
                id: id(field(event, "id"))?,
                track: id(field(event, "track"))?,
                position: position(field(event, "position"))?,
                label: field(event, "label").as_str()?.to_owned(),
                content: field(event, "content").as_str().map(str::to_owned),
                tags: items(field(event, "tags"))
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                color: color(field(event, "color")),
            })
        })
        .collect();
    let links = items(links)
        .filter_map(|link| {
            Some(EventLink {
                source: id(field(link, "source"))?,
                destination: id(field(link, "destination"))?,
                kind: match field(link, "kind").as_str()? {
                    "branch" => LinkKind::Branch,
                    "merge" => LinkKind::Merge,
                    _ => return None,
                },
                color: color(field(link, "color")),
            })
        })
        .collect();
    EventModel::new(tracks, events, links)
}

#[derive(Clone, PartialEq)]
struct Data {
    tracks: Value,
    events: Value,
    links: Value,
}

#[derive(Clone, PartialEq)]
struct Options {
    orientation: EventOrientation,
    color: Option<ColorName>,
    size: Option<Size>,
    label: String,
    focusable: Option<bool>,
}

#[derive(IntoElement)]
struct EventTimelineDocument {
    key: ElementId,
    data: Data,
    options: Options,
    handlers: [Option<EventHandler>; 3],
}

impl EventTimelineDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            data: Data {
                tracks: node.prop(0).clone(),
                events: node.prop(1).clone(),
                links: node.prop(2).clone(),
            },
            options: Options {
                orientation: match node.prop(3).as_str() {
                    Some("horizontal") => EventOrientation::Horizontal,
                    _ => EventOrientation::Vertical,
                },
                color: node.color(4),
                size: node.size(5),
                label: node.prop(6).as_str().unwrap_or("Event timeline").to_owned(),
                focusable: node.prop(7).as_bool(),
            },
            handlers: std::array::from_fn(|index| node.handler(index)),
        }
    }
}

struct DocumentState {
    view: Entity<EventTimelineState>,
    data: Option<Data>,
    options: Option<Options>,
    handlers: [Option<EventHandler>; 3],
    _subscription: Subscription,
}

impl RenderOnce for EventTimelineDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            // Start empty; the data is installed below like any later change.
            let view = cx.new(|cx| EventTimelineState::new(EventModel::default(), cx));
            let subscription = cx.subscribe_in(
                &view,
                window,
                |state: &mut DocumentState, _, event, window, cx| {
                    let (index, id) = match *event {
                        EventTimelineEvent::Selected(id) => (0, id),
                        EventTimelineEvent::Activated(id) => (1, id),
                    };
                    if let Some(handler) = &state.handlers[index] {
                        handler(Value::Num(id as f64), window, cx);
                    }
                },
            );
            DocumentState {
                view,
                data: None,
                options: None,
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        let (view, error) = state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            let mut error = None;
            if state.data.as_ref() != Some(&self.data) {
                match parse_model(&self.data.tracks, &self.data.events, &self.data.links) {
                    Ok(model) => state.view.update(cx, |view, cx| view.replace(model, cx)),
                    Err(problem) => {
                        error = state.handlers[2].clone().map(|h| (h, problem.to_string()))
                    }
                }
                state.data = Some(self.data);
            }
            if state.options.as_ref() != Some(&self.options) {
                let options = self.options.clone();
                state.view.update(cx, |view, cx| {
                    view.set_orientation(options.orientation, cx);
                    view.set_color(options.color, cx);
                    view.set_size(options.size, cx);
                    view.set_label(options.label, cx);
                    view.set_focusable(options.focusable, cx);
                });
                state.options = Some(self.options);
            }
            (state.view.clone(), error)
        });
        if let Some((handler, message)) = error {
            window.defer(cx, move |window, cx| {
                handler(Value::from(message), window, cx)
            });
        }
        EventTimeline::new(&view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Arc};

    fn record(fields: &[(&str, Value)]) -> Value {
        Value::Record(Arc::new(
            fields
                .iter()
                .map(|(name, value)| ((*name).into(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        ))
    }

    #[test]
    fn records_build_a_model_and_bad_ids_are_reported() {
        let tracks = Value::List(
            vec![
                record(&[("id", Value::Num(1.)), ("label", "Main".into())]),
                record(&[("id", Value::Num(2.)), ("label", "Feature".into())]),
            ]
            .into(),
        );
        let event = |id: f64, track: f64| {
            record(&[
                ("id", Value::Num(id)),
                ("track", Value::Num(track)),
                ("position", Value::Num(-3.)),
                ("label", "Commit".into()),
                ("color", "teal".into()),
            ])
        };
        let ok = Value::List(vec![event(7., 1.), event(8., 2.)].into());
        let links = Value::List(
            vec![record(&[
                ("source", Value::Num(7.)),
                ("destination", Value::Num(8.)),
                ("kind", "merge".into()),
            ])]
            .into(),
        );
        let model = parse_model(&tracks, &ok, &links);
        assert!(model.is_ok(), "{model:?}");
        let unknown = Value::List(vec![event(7., 3.)].into());
        assert!(matches!(
            parse_model(&tracks, &unknown, &Value::Null),
            Err(EventModelError::UnknownTrack(3))
        ));
        // Fractional or negative IDs never reach the model.
        assert_eq!(id(&Value::Num(1.5)), None);
        assert_eq!(id(&Value::Num(-1.)), None);
        assert_eq!(position(&Value::Num(-4.)), Some(-4));
    }
}
