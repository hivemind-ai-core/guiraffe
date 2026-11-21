//! DataUi adapter for layered charts.

use std::sync::Arc;

use gpui_kit::{App, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{ChartEvent, ChartOptions, ChartView};
use crate::{
    Arrangement, AxisSpec, Channels, ChartSource, ChartSpec, Color, Domain, Layer, Mark, Ohlc,
    Orientation, Scale, TimeFormat, widget_specs,
};

pub(super) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| ChartDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

fn as_index(value: &Value) -> Option<u32> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v <= u32::MAX as f64 && v.fract() == 0.0)
        .map(|v| v as u32)
}

#[derive(Default)]
struct Datum {
    key: Option<u64>,
    x: f64,
    y: f64,
    group: u32,
    facet: u32,
    color: Option<Color>,
    size: f32,
    x2: Option<f64>,
    y2: Option<f64>,
    low: Option<f64>,
    ohlc: Option<Ohlc>,
    text: String,
}

impl Datum {
    fn parse(value: &Value) -> Option<Self> {
        let number = |name| field(value, name).as_f64().filter(|v| v.is_finite());
        let id = |name| as_index(field(value, name));
        Some(Self {
            key: number("key")
                .filter(|v| *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0)
                .map(|v| v as u64),
            x: number("x")?,
            y: number("y")?,
            group: id("group").unwrap_or(0),
            facet: id("facet").unwrap_or(0),
            color: id("colorCategory").map(Color::Category).or_else(|| {
                number("colorValue").map(|v| Color::Continuous(v.clamp(0.0, 1.0) as f32))
            }),
            size: number("size").unwrap_or(3.0).clamp(0.0, 1024.0) as f32,
            x2: number("x2"),
            y2: number("y2"),
            low: number("low"),
            ohlc: number("open")
                .zip(number("high"))
                .zip(number("low"))
                .zip(number("close"))
                .map(|(((open, high), low), close)| Ohlc {
                    open,
                    high,
                    low,
                    close,
                }),
            text: field(value, "text").as_str().unwrap_or_default().to_owned(),
        })
    }
}

struct Source(Vec<Datum>);

impl ChartSource for Source {
    type Row = Datum;
    fn len(&self) -> usize {
        self.0.len()
    }
    fn row(&self, index: usize) -> Option<&Datum> {
        self.0.get(index)
    }
    fn key(&self, index: usize) -> u64 {
        self.0
            .get(index)
            .and_then(|row| row.key)
            .unwrap_or(index as u64)
    }
}

fn axis(value: &Value) -> AxisSpec {
    let labels = || {
        items(field(value, "labels"))
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Arc<[String]>>()
    };
    let scale = match field(value, "scale").as_str() {
        Some("log") => Scale::Log,
        Some("seconds") => Scale::Time(TimeFormat::Seconds),
        Some("days") => Scale::Time(TimeFormat::Days),
        Some("frames") => Scale::Time(TimeFormat::Frames(
            field(value, "framesPerSecond")
                .as_f64()
                .unwrap_or(30.0)
                .clamp(1.0, 240.0),
        )),
        Some("band") => Scale::Band {
            labels: labels(),
            padding: field(value, "padding")
                .as_f64()
                .unwrap_or(0.1)
                .clamp(0.0, 1.0),
        },
        Some("ordinal") => Scale::Ordinal(labels()),
        _ => Scale::Linear,
    };
    let domain = field(value, "domain");
    AxisSpec {
        scale,
        domain: domain
            .list_get(0)
            .and_then(Value::as_f64)
            .zip(domain.list_get(1).and_then(Value::as_f64))
            .and_then(|(min, max)| Domain::new(min, max)),
        title: field(value, "title")
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        grid: field(value, "grid").as_bool().unwrap_or(true),
        nice: field(value, "nice").as_bool().unwrap_or(true),
    }
}

fn layer(value: &Value) -> Layer {
    let arrangement = match field(value, "arrangement").as_str() {
        Some("group") => Arrangement::Group,
        Some("stack") => Arrangement::Stack,
        Some("normalize") => Arrangement::Normalize,
        _ => Arrangement::Overlap,
    };
    let orientation = if field(value, "orientation").as_str() == Some("horizontal") {
        Orientation::Horizontal
    } else {
        Orientation::Vertical
    };
    let mark = match field(value, "mark").as_str() {
        Some("area") => Mark::Area(arrangement),
        Some("bar") => Mark::Bar(arrangement, orientation),
        Some("point") => Mark::Point,
        Some("bubble") => Mark::Bubble,
        Some("rect") => Mark::Rect,
        Some("rule") => Mark::Rule(orientation),
        Some("text") => Mark::Text,
        Some("error-bar") => Mark::ErrorBar,
        Some("candlestick") => Mark::Candlestick,
        _ => Mark::Line,
    };
    let source = Source(
        items(field(value, "data"))
            .filter_map(Datum::parse)
            .collect(),
    );
    let channels = Channels::new(|row: &Datum| row.x, |row| row.y)
        .group(|row| row.group)
        .facet(|row| row.facet)
        .size(|row| row.size)
        .text(|row| &row.text)
        .optional_fields(
            |row| row.color,
            |row| row.x2,
            |row| row.y2,
            |row| row.low,
            |row| row.ohlc,
        );
    let mut layer = Layer::new(mark, source, channels)
        .title(field(value, "title").as_str().unwrap_or_default())
        .secondary_axis(field(value, "secondary").as_bool().unwrap_or_default());
    for label in items(field(value, "groupLabels")) {
        if let Some((id, text)) = as_index(field(label, "id")).zip(field(label, "label").as_str()) {
            layer = layer.group_label(id, text);
        }
    }
    layer
}

fn spec(summary: &Value, layers: &Value, axes: &Value, facets: &Value) -> ChartSpec {
    let mut spec = ChartSpec::new(summary.as_str().unwrap_or("Chart"));
    spec.layers = items(layers).map(layer).collect();
    for value in items(axes) {
        match field(value, "axis").as_str() {
            Some("x") => spec.x = axis(value),
            Some("y") => spec.y = axis(value),
            Some("right") => spec.right = axis(value),
            _ => {}
        }
    }
    spec.facet_labels = items(facets)
        .filter_map(|label| {
            let id = as_index(field(label, "id"))?;
            Some((id, field(label, "label").as_str()?.to_owned()))
        })
        .collect();
    spec
}

fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned().into(), value))
            .collect(),
    ))
}

fn event_payload(event: &ChartEvent) -> (usize, Value) {
    let domain = |d: Domain| record([("min", Value::Num(d.min())), ("max", Value::Num(d.max()))]);
    match *event {
        ChartEvent::Selected(id) => (
            0,
            record([
                ("layer", Value::Num(id.series.layer as f64)),
                ("group", Value::Num(id.series.group.into())),
                ("index", Value::Num(id.index as f64)),
                ("key", Value::Num(id.key as f64)),
                ("facet", Value::Num(id.facet.into())),
            ]),
        ),
        ChartEvent::ViewportChanged(value) => (1, domain(value)),
        ChartEvent::Brushed(value) => (2, domain(value)),
        ChartEvent::SeriesVisibility { series, visible } => (
            3,
            record([
                ("layer", Value::Num(series.layer as f64)),
                ("group", Value::Num(series.group.into())),
                ("visible", Value::Bool(visible)),
            ]),
        ),
    }
}

#[derive(IntoElement)]
struct ChartDocument {
    key: ElementId,
    config: [Value; 4],
    options: ChartOptions,
    handlers: [Option<EventHandler>; 4],
}

impl ChartDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let flag = |index| node.prop(index).as_bool().unwrap_or(true);
        Self {
            key: node.state_id(),
            config: std::array::from_fn(|i| node.prop(i).clone()),
            options: ChartOptions {
                height: node.prop(4).as_f64().unwrap_or(290.0).clamp(64.0, 4096.0) as f32,
                legend: flag(5),
                crosshair: flag(6),
                tooltip: flag(7),
                navigation: flag(8),
                keyboard: flag(9),
            },
            handlers: std::array::from_fn(|i| node.handler(i)),
        }
    }
}

struct ChartState {
    chart: Entity<ChartView>,
    config: [Value; 4],
    handlers: [Option<EventHandler>; 4],
    _subscription: Subscription,
}

impl RenderOnce for ChartDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let [summary, layers, axes, facets] = &self.config;
            let chart = ChartView::from_spec(spec(summary, layers, axes, facets), self.options, cx);
            let subscription = cx.subscribe_in(
                &chart,
                window,
                |state: &mut ChartState, _, event, window, cx| {
                    let (index, payload) = event_payload(event);
                    if let Some(handler) = &state.handlers[index] {
                        handler(payload, window, cx);
                    }
                },
            );
            ChartState {
                chart,
                config: self.config.clone(),
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            let config = (state.config != self.config).then(|| {
                state.config = self.config;
                let [summary, layers, axes, facets] = &state.config;
                spec(summary, layers, axes, facets)
            });
            state.chart.update(cx, |chart, cx| {
                if let Some(config) = config {
                    chart.replace_spec(config, cx);
                }
                if chart.options != self.options {
                    chart.options = self.options;
                    chart.overlay.update(cx, |overlay, cx| {
                        overlay.options = self.options;
                        cx.notify();
                    });
                    cx.notify();
                }
            });
            state.chart.clone()
        })
    }
}
