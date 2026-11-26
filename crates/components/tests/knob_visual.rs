//! Goldens of the knob and number field, in light and dark: a log cutoff knob, a bipolar pan
//! knob off centre, a knob with a modulation range, and number fields in several units.
//! Accept intentional changes with
//! `GUIRAFFE_UPDATE_SNAPSHOTS=1 cargo test -p guiraffe-components --test knob_visual`.
#![allow(clippy::unwrap_used, reason = "test assertions")]

use gpui_kit::{
    AnyView, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_components::controls::{KnobControl, NumberFieldControl};
use guiraffe_components::mapping::{Curve, Unit, ValueMap};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Panel(Vec<AnyView>, Vec<AnyView>);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(div().flex().gap(px(24.)).children(self.0.clone()))
            .child(div().flex().gap(px(12.)).children(self.1.clone()))
    }
}

fn render(scheme: Scheme) -> image::RgbaImage {
    let mut snap = Snapshotter::new().unwrap().with_setup(move |cx| {
        guiraffe_components::init(cx);
        apply(
            GuiraffeTheme {
                scheme,
                ..Default::default()
            },
            cx,
        );
    });
    snap.render(size(px(360.), px(160.)), |window, cx| {
        let hz = ValueMap::new(20.0, 20_000.0)
            .curve(Curve::Log)
            .unit(Unit::Hertz)
            .default_to(1_000.0);
        let knobs: Vec<AnyView> = vec![
            KnobControl::new()
                .mapping(hz)
                .value(2_500.0)
                .label("Cutoff")
                .diameter(48.)
                .build(window, cx)
                .into(),
            KnobControl::new()
                .mapping(ValueMap::new(-1.0, 1.0).bipolar(true).decimals(2))
                .value(-0.4)
                .label("Pan")
                .diameter(48.)
                .build(window, cx)
                .into(),
            KnobControl::new()
                .mapping(ValueMap::new(0.0, 1.0).unit(Unit::Percent))
                .value(0.5)
                .modulation(Some((0.3, 0.8)))
                .label("Depth")
                .diameter(48.)
                .build(window, cx)
                .into(),
        ];
        let fields: Vec<AnyView> = vec![
            NumberFieldControl::new()
                .mapping(hz)
                .value(440.0)
                .build(window, cx)
                .into(),
            NumberFieldControl::new()
                .mapping(
                    ValueMap::new(0.001, 10.0)
                        .curve(Curve::Log)
                        .unit(Unit::Seconds),
                )
                .value(0.25)
                .build(window, cx)
                .into(),
            NumberFieldControl::new()
                .mapping(ValueMap::new(-96.0, 12.0).unit(Unit::Decibels))
                .value(-6.0)
                .build(window, cx)
                .into(),
            NumberFieldControl::new()
                .mapping(ValueMap::new(0.0, 127.0).step(1.0).unit(Unit::Note))
                .value(61.0)
                .build(window, cx)
                .into(),
        ];
        cx.new(|_| Panel(knobs, fields))
    })
    .unwrap()
}

#[test]
fn knobs_and_number_fields_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [(Scheme::Light, "knobs-light"), (Scheme::Dark, "knobs-dark")] {
        let image = render(scheme);
        assert_snapshot(name, &image, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
