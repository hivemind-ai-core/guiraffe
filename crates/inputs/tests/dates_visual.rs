//! Date & time picker goldens in light and dark: closed states, and open popovers (range
//! with presets, date-time with its time field).
use chrono::{NaiveDate, NaiveTime};
use gpui_kit::{
    AnyView, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_inputs::dates::*;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
}
fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default()
}
const TODAY: (i32, u32, u32) = (2026, 3, 11);
fn today() -> NaiveDate {
    d(TODAY.0, TODAY.1, TODAY.2)
}

struct Scene(Vec<AnyView>, f32);

impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .children(self.0.iter().map(|v| div().w(px(self.1)).child(v.clone())))
    }
}

fn snap(
    name: &str,
    w: f32,
    h: f32,
    width: f32,
    build: fn(&mut Window, &mut gpui_kit::App) -> Vec<AnyView>,
) {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(move |cx| {
                guiraffe_inputs::init(cx);
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            });
        let image = s
            .render(size(px(w), px(h)), move |window, cx| {
                let views = build(window, cx);
                cx.new(|_| Scene(views, width))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let tag = format!(
            "{name}-{}",
            if scheme == Scheme::Light {
                "light"
            } else {
                "dark"
            }
        );
        assert_snapshot(&tag, &image, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

#[test]
fn pickers_closed_states() {
    snap("dates-closed", 420., 600., 300., |window, cx| {
        vec![
            DatePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Due date")
                    .description("Invoices are due on weekdays")
                    .default_value(d(2026, 3, 16))
                    .clearable(true)
                    .required(true)
            })
            .into(),
            DateRangePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Stay")
                    .placeholder("Check-in – check-out")
            })
            .into(),
            DatePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Filled, with an error")
                    .variant(InputVariant::Filled)
                    .default_value(d(2026, 3, 14))
                    .error("Pick a weekday")
            })
            .into(),
            DatePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Disabled")
                    .default_value(d(2026, 3, 2))
                    .disabled(true)
            })
            .into(),
            TimePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Alarm (12 h)")
                    .default_value(t(7, 30))
                    .clearable(true)
            })
            .into(),
            DateTimePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Meeting (24 h, German)")
                    .locale(DateLocale::de())
                    .default_value(d(2026, 3, 12).and_time(t(14, 15)))
            })
            .into(),
        ]
    });
}

#[test]
fn range_picker_open_with_presets() {
    snap("dates-range-open", 620., 470., 300., |window, cx| {
        vec![
            DateRangePicker::new(window, cx, |p| {
                let presets = DatePreset::standard_ranges(&p.locale);
                p.today(today())
                    .label("Reporting period")
                    .presets(presets)
                    .min_date(d(2026, 2, 1))
                    .exclude_date(|d| d == NaiveDate::from_ymd_opt(2026, 3, 20).unwrap_or_default())
                    .default_value(DateRange::new(d(2026, 3, 5), d(2026, 3, 11)))
                    .clearable(true)
                    .open(true)
            })
            .into(),
        ]
    });
}

#[test]
fn date_time_picker_open() {
    snap("dates-datetime-open", 420., 520., 300., |window, cx| {
        vec![
            DateTimePicker::new(window, cx, |p| {
                p.today(today())
                    .label("Departure")
                    .locale(DateLocale::en_gb())
                    .default_value(d(2026, 3, 18).and_time(t(9, 45)))
                    .open(true)
            })
            .into(),
        ]
    });
}
