use crate::{
    catalog::Entry,
    events::EventLog,
    knobs::{Knob, playground_with_update},
};
use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Task,
    Window, component::button::Button, div, px,
};
use guiraffe_charts::{
    Arrangement, Channels, ChartOptions, ChartSpec, ChartView, Color, Domain, Layer, Mark, Ohlc,
    Orientation, Scale, Stream,
};
use guiraffe_components::Text;
use guiraffe_core::Size;
use std::{cell::Cell, sync::Arc, time::Duration};

pub fn entries() -> [Entry; 3] {
    [
        Entry {
            krate: "charts",
            name: "Chart grammar",
            summary: "Compose marks from typed sources. Change the mark or stack, toggle a series, and drag to zoom.",
            covers: &["ChartView"],
            demo: grammar,
        },
        Entry {
            krate: "charts",
            name: "Faceted market charts",
            summary: "Compare two markets with candles and a second axis for trade volume.",
            covers: &[],
            demo: markets,
        },
        Entry {
            krate: "charts",
            name: "Streaming chart dashboard",
            summary: "Monitor two telemetry channels and inspect chart events as observations arrive.",
            covers: &[],
            demo: streaming,
        },
    ]
}

fn options(height: f32) -> ChartOptions {
    ChartOptions {
        height,
        keyboard: true,
        ..Default::default()
    }
}

#[derive(Clone, Copy)]
struct Usage {
    day: f64,
    count: f64,
    group: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct UsageOptions {
    mark: usize,
    arrangement: usize,
    grid: bool,
}

fn usage(settings: UsageOptions) -> ChartSpec {
    let arrangement = [
        Arrangement::Stack,
        Arrangement::Normalize,
        Arrangement::Overlap,
    ]
    .get(settings.arrangement)
    .copied()
    .unwrap_or_default();
    let mark = match settings.mark {
        1 => Mark::Bar(
            if arrangement == Arrangement::Overlap {
                Arrangement::Group
            } else {
                arrangement
            },
            Orientation::Vertical,
        ),
        2 => Mark::Line,
        3 => Mark::Bubble,
        _ => Mark::Area(arrangement),
    };
    let rows: Vec<_> = [
        [14., 21., 18., 28., 34., 30., 41.],
        [7., 13., 12., 19., 15., 24., 27.],
        [4., 9., 7., 12., 10., 17., 15.],
    ]
    .into_iter()
    .enumerate()
    .flat_map(|(group, data)| {
        data.into_iter().enumerate().map(move |(day, count)| Usage {
            day: day as f64,
            count,
            group: group as u32,
        })
    })
    .collect();
    let mut spec = ChartSpec::new("Requests by channel").layer(
        Layer::new(
            mark,
            rows,
            Channels::new(|row: &Usage| row.day, |row| row.count)
                .group(|row| row.group)
                .size(|row| (row.count as f32).sqrt() * 2.),
        )
        .group_label(0, "Search")
        .group_label(1, "Direct")
        .group_label(2, "Referrals"),
    );
    spec.x.scale = Scale::Band {
        labels: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            .map(str::to_owned)
            .into(),
        padding: 0.22,
    };
    spec.x.title = "Day of week".into();
    spec.x.grid = false;
    spec.y.title = if matches!(
        mark,
        Mark::Area(Arrangement::Normalize) | Mark::Bar(Arrangement::Normalize, _)
    ) {
        "Share".into()
    } else {
        "Requests".into()
    };
    spec.y.grid = settings.grid;
    spec
}

struct Grammar {
    chart: Entity<ChartView>,
    log: Entity<EventLog>,
    settings: Cell<UsageOptions>,
}

fn grammar(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Weekly requests by acquisition channel",
        &[
            Knob::new("Mark", &["Area", "Bars", "Line", "Bubbles"]),
            Knob::new(
                "Arrangement",
                &["Stacked", "Normalized", "Overlap / grouped"],
            ),
            Knob::toggle("Grid").default(1),
        ],
        |_, cx| {
            let settings = UsageOptions {
                mark: 0,
                arrangement: 0,
                grid: true,
            };
            let chart = ChartView::new(usage(settings).prepare(), options(280.), cx);
            let log = EventLog::new(cx);
            EventLog::watch_with(&log, &chart, event_text, cx);
            Grammar {
                chart,
                log,
                settings: Cell::new(settings),
            }
        },
        |demo, values, cx| {
            let settings = UsageOptions {
                mark: values.index(0),
                arrangement: values.index(1),
                grid: values.on(2),
            };
            if demo.settings.replace(settings) != settings {
                let model = usage(settings).prepare();
                demo.chart.update(cx, |view, cx| view.replace(model, cx));
            }
        },
        |demo, _, _, _| {
            div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(demo.chart.clone())
                .child(demo.log.clone())
                .into_any_element()
        },
        window,
        cx,
    )
}

fn event_text(event: &guiraffe_charts::ChartEvent) -> gpui_kit::SharedString {
    use guiraffe_charts::ChartEvent;
    match event {
        ChartEvent::Selected(id) => format!("Selected observation {}", id.key),
        ChartEvent::Brushed(domain) => format!("Brushed {:.2}–{:.2}", domain.min(), domain.max()),
        ChartEvent::ViewportChanged(domain) => {
            format!("Viewport {:.2}–{:.2}", domain.min(), domain.max())
        }
        ChartEvent::SeriesVisibility { series, visible } => {
            format!(
                "Series {} {}",
                series.group + 1,
                if *visible { "shown" } else { "hidden" }
            )
        }
    }
    .into()
}

#[derive(Clone, Copy)]
struct Trade {
    session: f64,
    market: u32,
    price: Ohlc,
    volume: f64,
}

fn markets(_: &mut Window, cx: &mut App) -> AnyView {
    let rows: Arc<[Trade]> = (0..2)
        .flat_map(|market| {
            (0..9).map(move |day| {
                let session = f64::from(day);
                let open = 51.
                    + f64::from(market) * 8.
                    + session * 0.65
                    + (session * 1.2 + f64::from(market)).sin() * 3.2;
                let close = open + (session * 2.3 + 1.).cos() * 3.;
                Trade {
                    session,
                    market,
                    price: Ohlc {
                        open,
                        close,
                        high: open.max(close) + 1.8,
                        low: open.min(close) - 1.4,
                    },
                    volume: 120. + (session * 3.9 + f64::from(market)).sin() * 40. + session * 4.,
                }
            })
        })
        .collect();
    let mut spec = ChartSpec::new("Daily price and volume across two markets")
        .layer(
            Layer::new(
                Mark::Candlestick,
                rows.clone(),
                Channels::new(|row: &Trade| row.session, |row| row.price.close)
                    .facet(|row| row.market)
                    .ohlc(|row| row.price),
            )
            .title("Price"),
        )
        .layer(
            Layer::new(
                Mark::Line,
                rows,
                Channels::new(|row: &Trade| row.session, |row| row.volume).facet(|row| row.market),
            )
            .secondary_axis(true)
            .title("Volume"),
        );
    spec.facet_labels
        .extend([(0, "North market".into()), (1, "South market".into())]);
    spec.x.domain = Domain::new(-0.5, 8.5);
    spec.x.title = "Trading session".into();
    spec.x.grid = false;
    spec.y.title = "Price ($)".into();
    spec.right.title = "Trades".into();
    ChartView::new(spec.prepare(), options(310.), cx).into()
}

#[derive(Clone, Copy)]
struct Reading {
    time: f64,
    value: f64,
    channel: u32,
}

fn readings(index: u32) -> Vec<Reading> {
    let time = f64::from(index) * 0.5;
    vec![
        Reading {
            time,
            value: 36. + (time * 1.15).sin() * 5. + (time * 0.2).cos() * 6.,
            channel: 0,
        },
        Reading {
            time,
            value: 21. + (time * 0.9 + 1.).sin() * 4. + (time * 0.4).sin() * 3.,
            channel: 1,
        },
    ]
}

fn telemetry(source: Stream<Reading>) -> ChartSpec {
    let mut spec = ChartSpec::new("Streaming CPU load by service").layer(
        Layer::new(
            Mark::Line,
            source,
            Channels::new(|row: &Reading| row.time, |row| row.value)
                .group(|row| row.channel)
                .color(|row| Color::Category(row.channel)),
        )
        .group_label(0, "API worker")
        .group_label(1, "Sync worker"),
    );
    spec.x.title = "Elapsed seconds".into();
    spec.x.grid = false;
    spec.y.title = "CPU load (%)".into();
    spec.y.domain = Domain::new(0., 60.);
    spec
}

struct Dashboard {
    chart: Entity<ChartView>,
    log: Entity<EventLog>,
    stream: Stream<Reading>,
    next: u32,
    task: Option<Task<()>>,
}

impl Dashboard {
    fn toggle(&mut self, cx: &mut Context<Self>) {
        if self.task.take().is_some() {
            cx.notify();
            return;
        }
        let executor = cx.background_executor().clone();
        self.task = Some(cx.spawn(async move |owner, cx| {
            loop {
                executor.timer(Duration::from_millis(500)).await;
                let Ok((mut snapshot, next)) =
                    owner.update(cx, |this, _| (this.stream.clone(), this.next))
                else {
                    break;
                };
                let (snapshot, chart) = executor
                    .spawn(async move {
                        snapshot.append(readings(next));
                        let chart = telemetry(snapshot.clone()).prepare();
                        (snapshot, chart)
                    })
                    .await;
                if owner
                    .update(cx, |this, cx| {
                        this.stream = snapshot;
                        this.next = next.saturating_add(1);
                        this.chart.update(cx, |view, cx| view.replace(chart, cx));
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        cx.notify();
    }
}

impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.task.is_some();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        Button::new("stream-play")
                            .label(if running {
                                "Pause stream"
                            } else {
                                "Start stream"
                            })
                            .tab_stop(true)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
                    )
                    .child(
                        Text::new(format!(
                            "{} samples · {}",
                            u64::from(self.next) * 2,
                            if running { "Live" } else { "Paused" }
                        ))
                        .size(Size::Sm),
                    ),
            )
            .child(self.chart.clone())
            .child(self.log.clone())
    }
}

fn streaming(_: &mut Window, cx: &mut App) -> AnyView {
    dashboard(cx).into()
}

fn dashboard(cx: &mut App) -> Entity<Dashboard> {
    let mut stream = Stream::default();
    let next = 30;
    stream.append((0..next).flat_map(readings).collect());
    let chart = ChartView::new(telemetry(stream.clone()).prepare(), options(300.), cx);
    let log = EventLog::new(cx);
    EventLog::watch_with(&log, &chart, event_text, cx);
    cx.new(|_| Dashboard {
        chart,
        log,
        stream,
        next,
        task: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::TestAppContext;

    #[gpui_kit::test]
    fn streaming_dashboard_publishes_and_pauses(cx: &mut TestAppContext) {
        let dashboard = cx.update(dashboard);
        let before = dashboard.read_with(cx, |state, _| state.next);
        dashboard.update(cx, |state, cx| state.toggle(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        let after = dashboard.read_with(cx, |state, _| state.next);
        assert_eq!(after, before + 1);
        dashboard.update(cx, |state, cx| state.toggle(cx));
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
        assert_eq!(dashboard.read_with(cx, |state, _| state.next), after);
    }
}
