//! DataUi adapter for the native multi-series curve editor.

use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    ColorName,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{CurveEvent, CurveSeries, CurveView, ValueRange};
use crate::{Curve, Interp, Key, TangentMode, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: &widget_specs::CURVE_EDITOR,
            schema: guiraffe_core::widget_schema!(&widget_specs::CURVE_EDITOR),
            render: |node, _, _, _| CurveDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn field<'a>(value: &'a Value, field: &str) -> &'a Value {
    value.field(&[field.into()])
}

fn pair(value: &Value) -> Option<(f64, f64)> {
    value.list_get(0)?.as_f64().zip(value.list_get(1)?.as_f64())
}

fn index(value: f64) -> Option<usize> {
    (value.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&value)).then_some(value as usize)
}

fn parse_key(value: &Value) -> Option<Key> {
    let interpolation = field(value, "interpolation")
        .as_str()
        .and_then(Interp::from_name)
        .unwrap_or(Interp::Linear);
    let mut key = Key::new(
        field(value, "time").as_f64()?,
        field(value, "value").as_f64()?,
        interpolation,
    );
    key.in_handle = pair(field(value, "inHandle")).unwrap_or_default();
    key.out_handle = pair(field(value, "outHandle")).unwrap_or_default();
    Some(key)
}

fn parse_curves(values: &Value) -> Vec<CurveSeries> {
    let mut curves: Vec<_> = items(values)
        .filter_map(|entry| {
            let name = field(entry, "name").as_str()?;
            let mut curve = Curve::default();
            for key in items(field(entry, "keys")).filter_map(parse_key) {
                curve.set(key);
            }
            let color = field(entry, "color")
                .as_str()
                .and_then(ColorName::from_name)
                .unwrap_or(ColorName::Blue);
            let mut series = CurveSeries::new(name, curve, color);
            series.visible = field(entry, "visible").as_bool().unwrap_or(true);
            Some(series)
        })
        .collect();
    if curves.is_empty() {
        curves.push(CurveSeries::new("Curve", Curve::default(), ColorName::Blue));
    }
    curves
}

fn record<const N: usize>(fields: [(&'static str, Value); N]) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn pair_value((a, b): (f64, f64)) -> Value {
    Value::List(vec![Value::Num(a), Value::Num(b)].into())
}

fn series_value(series: &CurveSeries) -> Value {
    let keys = series
        .curve
        .keys()
        .iter()
        .map(|key| {
            let interpolation = key.interp.name();
            record([
                ("time", key.t.into()),
                ("value", key.v.into()),
                ("interpolation", interpolation.into()),
                ("inHandle", pair_value(key.in_handle)),
                ("outHandle", pair_value(key.out_handle)),
            ])
        })
        .collect::<Vec<_>>();
    record([
        ("name", series.name.clone().into()),
        ("color", series.color.name().into()),
        ("visible", series.visible.into()),
        ("keys", Value::List(keys.into())),
    ])
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fit {
    None,
    Visible,
    Normalize,
}

#[derive(IntoElement)]
struct CurveDocument {
    key: ElementId,
    curves: Value,
    bounds: [f64; 4],
    editable: bool,
    focusable: Option<bool>,
    tangent_mode: TangentMode,
    selection: Option<Option<(usize, usize)>>,
    fit: Fit,
    handlers: [Option<EventHandler>; 3],
}

impl CurveDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let number = |i, default| {
            node.prop(i)
                .as_f64()
                .filter(|n| n.is_finite())
                .unwrap_or(default)
        };
        let selection = node.prop(8);
        Self {
            key: node.state_id(),
            curves: node.prop(0).clone(),
            bounds: [number(1, 0.), number(2, 10.), number(3, 0.), number(4, 1.)],
            editable: node.prop(5).as_bool().unwrap_or(true),
            focusable: node.prop(6).as_bool(),
            tangent_mode: match node.prop(7).as_str() {
                Some("aligned") => TangentMode::Aligned,
                Some("mirrored") => TangentMode::Mirrored,
                Some("flat") => TangentMode::Flat,
                _ => TangentMode::Free,
            },
            selection: selection
                .list_len()
                .map(|_| pair(selection).and_then(|(curve, key)| index(curve).zip(index(key)))),
            fit: match node.prop(9).as_str() {
                Some("visible") => Fit::Visible,
                Some("normalize") => Fit::Normalize,
                _ => Fit::None,
            },
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct CurveDocumentState {
    view: Entity<CurveView>,
    curves: Value,
    bounds: [f64; 4],
    selection: Option<Option<(usize, usize)>>,
    fit: Fit,
    handlers: [Option<EventHandler>; 3],
    _subscription: Subscription,
}

impl RenderOnce for CurveDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let [t0, t1, lo, hi] = self.bounds;
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let view = CurveView::new(Curve::default(), t0, t1, lo, hi, cx);
            view.update(cx, |view, _| view.curves = parse_curves(&self.curves));
            let subscription = cx.subscribe_in(
                &view,
                window,
                |state: &mut CurveDocumentState, view, event, window, cx| {
                    let index = match event {
                        CurveEvent::KeyMoved { .. } => 0,
                        CurveEvent::Changed | CurveEvent::Released => 1,
                        CurveEvent::SelectionChanged => 2,
                    };
                    if let Some(handler) = state.handlers[index].clone() {
                        let view = view.read(cx);
                        let value = match *event {
                            CurveEvent::KeyMoved { index, t, v } => record([
                                ("index", Value::Num(index as f64)),
                                ("time", t.into()),
                                ("value", v.into()),
                            ]),
                            CurveEvent::Changed | CurveEvent::Released => Value::List(
                                view.curves()
                                    .iter()
                                    .map(series_value)
                                    .collect::<Vec<_>>()
                                    .into(),
                            ),
                            CurveEvent::SelectionChanged => {
                                let selection = view
                                    .selection()
                                    .map(|(curve, key)| {
                                        vec![Value::Num(curve as f64), Value::Num(key as f64)]
                                    })
                                    .unwrap_or_default();
                                Value::List(selection.into())
                            }
                        };
                        handler(value, window, cx);
                    }
                },
            );
            CurveDocumentState {
                view,
                curves: self.curves.clone(),
                bounds: self.bounds,
                selection: None,
                fit: Fit::None,
                handlers: [None, None, None],
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            state.view.update(cx, |view, cx| {
                let curves_changed = state.curves != self.curves;
                if curves_changed {
                    state.curves = self.curves;
                    view.curves = parse_curves(&state.curves);
                    view.active = 0;
                    view.selected = None;
                    view.release();
                    cx.notify();
                }
                if state.bounds != self.bounds {
                    state.bounds = self.bounds;
                    view.pending_fit = Some((t0, t1));
                    view.surface.range = Some(ValueRange::new(lo, hi, view.range().height));
                    cx.notify();
                }
                if curves_changed || state.selection != self.selection {
                    state.selection = self.selection;
                    if let Some(selection) = self.selection {
                        view.selected = selection.filter(|&(curve, key)| {
                            view.curves
                                .get(curve)
                                .is_some_and(|curve| key < curve.curve.keys().len())
                        });
                        if let Some((curve, _)) = view.selected {
                            view.active = curve;
                        }
                        cx.notify();
                    }
                }
                if state.fit != self.fit || curves_changed {
                    state.fit = self.fit;
                    if self.fit != Fit::None {
                        view.pending_fit = None;
                        view.fit_view(self.fit == Fit::Normalize, cx);
                    }
                }
                if view.editable != self.editable {
                    view.set_editable(self.editable, cx);
                }
                if view.focusable != self.focusable {
                    view.set_focusable(self.focusable, cx);
                }
                view.set_tangent_mode(self.tangent_mode);
            });
            state.view.clone()
        })
    }
}
