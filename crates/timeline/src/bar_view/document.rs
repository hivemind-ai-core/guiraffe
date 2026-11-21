//! DataUi adapter for quantized bar curve editing.

use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{BarCurve, BarCurveView, BarEvent};
use crate::{Axis, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::BAR_CURVE,
            schema: guiraffe_core::widget_schema!(&widget_specs::BAR_CURVE),
            render: |node, _, _, _| BarDocument::new(node).into_any_element(),
        }],
    );
}

#[derive(Clone, PartialEq)]
struct Bars {
    values: Value,
    minimum: f64,
    maximum: f64,
    steps: Option<usize>,
    bipolar: bool,
}

impl Bars {
    fn build(&self) -> BarCurve {
        let count = self.values.list_len().unwrap_or_default();
        let mut curve = BarCurve::new(count, self.minimum, self.maximum);
        curve.set_steps(self.steps);
        curve.bipolar = self.bipolar;
        for index in 0..count {
            if let Some(value) = self.values.list_get(index).and_then(Value::as_f64) {
                curve.set(index, value);
            }
        }
        curve
    }
}

#[derive(IntoElement)]
struct BarDocument {
    key: ElementId,
    bars: Bars,
    editable: bool,
    focusable: Option<bool>,
    handler: Option<EventHandler>,
}

impl BarDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            bars: Bars {
                values: node.prop(0).clone(),
                minimum: node.prop(1).as_f64().unwrap_or(0.),
                maximum: node.prop(2).as_f64().unwrap_or(1.),
                steps: node
                    .prop(3)
                    .as_f64()
                    .filter(|value| {
                        (1.0..=f64::from(u32::MAX)).contains(value) && value.fract() == 0.0
                    })
                    .map(|value| value as usize),
                bipolar: node.prop(4).as_bool().unwrap_or_default(),
            },
            editable: node.prop(5).as_bool().unwrap_or(true),
            focusable: node.prop(6).as_bool(),
            handler: node.handler(0),
        }
    }
}

struct BarDocumentState {
    view: Entity<BarCurveView>,
    bars: Bars,
    handler: Option<EventHandler>,
    _subscription: Subscription,
}

impl RenderOnce for BarDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let view = BarCurveView::new(self.bars.build(), cx);
            let subscription = cx.subscribe_in(
                &view,
                window,
                |state: &mut BarDocumentState, view, event: &BarEvent, window, cx| {
                    if let Some(handler) = &state.handler {
                        let range = event.changed.clone();
                        let values = view
                            .read(cx)
                            .model()
                            .values()
                            .get(range.clone())
                            .unwrap_or_default();
                        handler(
                            Value::Record(Arc::new(BTreeMap::from([
                                ("start".into(), Value::Num(range.start as f64)),
                                ("end".into(), Value::Num(range.end as f64)),
                                (
                                    "values".into(),
                                    Value::List(
                                        values
                                            .iter()
                                            .copied()
                                            .map(Value::Num)
                                            .collect::<Vec<_>>()
                                            .into(),
                                    ),
                                ),
                            ]))),
                            window,
                            cx,
                        );
                    }
                },
            );
            BarDocumentState {
                view,
                bars: self.bars.clone(),
                handler: None,
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handler = self.handler;
            state.view.update(cx, |view, cx| {
                if state.bars != self.bars {
                    state.bars = self.bars;
                    let model = state.bars.build();
                    if model.values().len() != view.model().values().len() {
                        view.surface.axis = Axis::new(
                            0.,
                            model.values().len().max(1) as f64,
                            view.surface.axis.width,
                        );
                    }
                    if &model != view.model() {
                        view.set_model(model, cx);
                    }
                }
                if view.editable != self.editable {
                    view.set_editable(self.editable, cx);
                }
                if view.focusable != self.focusable {
                    view.set_focusable(self.focusable, cx);
                }
            });
            state.view.clone()
        })
    }
}
