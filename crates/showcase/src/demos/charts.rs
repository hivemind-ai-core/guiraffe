//! gpui-kit charts (`component::chart`). Series colors come from the GUIraffe bridge
//! (`chart_1..5`), so they follow the primary color and scheme.

use gpui_kit::{
    AnyElement, AnyView, App, IntoElement, ParentElement, Styled, Window,
    component::{
        ActiveTheme,
        chart::{AreaChart, BarChart, LineChart, PieChart},
    },
    div, px,
};

use super::{column, example};
use crate::{
    catalog::{Entry, stateless},
    knobs::{Knob, KnobValues, playground},
};

fn entry(
    name: &'static str,
    summary: &'static str,
    demo: fn(&mut Window, &mut App) -> AnyView,
) -> Entry {
    Entry {
        krate: "charts",
        name,
        summary,
        covers: &[],
        demo,
    }
}

pub fn entries() -> Vec<Entry> {
    vec![
        entry(
            "Line chart",
            "Monthly active users; try the curve, dots and grid options.",
            line,
        ),
        entry("Bar chart", "Revenue by quarter with value labels.", bar),
        entry("Area chart", "Stacked traffic sources over a week.", area),
        entry("Pie & donut", "Browser share as a pie and as a donut.", pie),
    ]
}

const MONTHS: [(&str, f64); 12] = [
    ("Jan", 1840.),
    ("Feb", 2105.),
    ("Mar", 2390.),
    ("Apr", 2275.),
    ("May", 2810.),
    ("Jun", 3120.),
    ("Jul", 2980.),
    ("Aug", 3360.),
    ("Sep", 3710.),
    ("Oct", 3590.),
    ("Nov", 4020.),
    ("Dec", 4385.),
];

fn frame(h: f32, chart: impl IntoElement) -> AnyElement {
    div().h(px(h)).w_full().child(chart).into_any_element()
}

fn line(window: &mut Window, cx: &mut App) -> AnyView {
    playground(
        "Monthly active users, 2026",
        &[
            Knob::new("Curve", &["natural", "linear", "step"]),
            Knob::toggle("Dots").default(1),
            Knob::toggle("Grid").default(1),
        ],
        |v: &KnobValues, _, _| {
            let c = LineChart::new(MONTHS.to_vec())
                .appear(false)
                .x(|d| d.0)
                .y(|d| d.1)
                .grid(v.on(2));
            let c = match v.index(0) {
                1 => c.linear(),
                2 => c.step_after(),
                _ => c.natural(),
            };
            let c = if v.on(1) { c.dot() } else { c };
            frame(260., c)
        },
        window,
        cx,
    )
}

fn bar(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            let data = vec![
                ("Q1 2025", 42.5),
                ("Q2 2025", 51.2),
                ("Q3 2025", 48.9),
                ("Q4 2025", 63.4),
                ("Q1 2026", 58.1),
                ("Q2 2026", 71.6),
            ];
            column([example(
                "Revenue ($M)",
                frame(
                    280.,
                    BarChart::new(data)
                        .appear(false)
                        .band(|d| d.0)
                        .value(|d| d.1)
                        .label(|d| format!("{:.1}", d.1)),
                ),
            )])
        },
        cx,
    )
}

fn area(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let t = cx.theme();
            let (a, b, c) = (t.chart_2, t.chart_1, t.chart_3);
            let data = vec![
                ("Mon", 420., 180., 90.),
                ("Tue", 510., 210., 110.),
                ("Wed", 480., 260., 95.),
                ("Thu", 620., 240., 140.),
                ("Fri", 700., 300., 160.),
                ("Sat", 380., 340., 120.),
                ("Sun", 310., 290., 100.),
            ];
            column([example(
                "Visits by source: search, social, direct",
                frame(
                    260.,
                    AreaChart::new(data)
                        .appear(false)
                        .x(|d| d.0)
                        .y(|d| d.1)
                        .stroke(a)
                        .fill(a.opacity(0.25))
                        .y(|d| d.2)
                        .stroke(b)
                        .fill(b.opacity(0.25))
                        .y(|d| d.3)
                        .stroke(c)
                        .fill(c.opacity(0.25)),
                ),
            )])
        },
        cx,
    )
}

fn pie(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, cx| {
            let t = cx.theme();
            let colors = [t.chart_2, t.chart_1, t.chart_3, t.chart_4, t.chart_5];
            let data: Vec<_> = [
                ("Chrome", 64.0_f32),
                ("Safari", 19.0),
                ("Edge", 6.0),
                ("Firefox", 4.0),
                ("Other", 7.0),
            ]
            .into_iter()
            .zip(colors)
            .map(|((n, v), c)| (n, v, c))
            .collect();
            let d2 = data.clone();
            div()
                .flex()
                .gap(px(16.))
                .child(
                    div().flex_1().child(example(
                        "Pie",
                        frame(
                            240.,
                            PieChart::new(data)
                                .appear(false)
                                .value(|d| d.1)
                                .color(|d| d.2)
                                .outer_radius(100.),
                        ),
                    )),
                )
                .child(
                    div().flex_1().child(example(
                        "Donut with padding",
                        frame(
                            240.,
                            PieChart::new(d2)
                                .appear(false)
                                .value(|d| d.1)
                                .color(|d| d.2)
                                .outer_radius(100.)
                                .inner_radius(60.)
                                .pad_angle(0.03),
                        ),
                    )),
                )
                .into_any_element()
        },
        cx,
    )
}
