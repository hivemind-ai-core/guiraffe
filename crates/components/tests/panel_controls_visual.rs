//! Goldens of workstation-panel controls, in light and dark: icon transport buttons with a
//! latched Record, M / S / R toggle chips, a vertical fader, and a meter and meter bank
//! that fill their containers.
//! Accept intentional changes with
//! `GUIRAFFE_UPDATE_SNAPSHOTS=1 cargo test -p guiraffe-components --test panel_controls_visual`.
#![allow(clippy::unwrap_used, reason = "test assertions")]

use gpui_kit::component::IconName;
use gpui_kit::{
    AnyView, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_components::controls::{
    ButtonControl, ButtonVariant, SliderControl, ToggleChipControl,
};
use guiraffe_components::{Level, Meter, MeterBank, MeterOptions};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Panel {
    transport: Vec<AnyView>,
    chips: Vec<AnyView>,
    fader: AnyView,
    meter: AnyView,
    bank: AnyView,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(12.))
            .flex()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(div().flex().gap(px(4.)).children(self.transport.clone()))
                    .child(div().flex().gap(px(2.)).children(self.chips.clone())),
            )
            .child(self.fader.clone())
            // Fixed containers: the meters take their length from them.
            .child(div().h(px(150.)).child(self.meter.clone()))
            .child(div().h(px(110.)).child(self.bank.clone()))
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
    snap.render(size(px(360.), px(176.)), |window, cx| {
        let transport: Vec<AnyView> = vec![
            ButtonControl::new()
                .icon(IconName::Play)
                .tooltip("Play")
                .small()
                .build(window, cx)
                .into(),
            ButtonControl::new()
                .icon(IconName::Pause)
                .tooltip("Pause")
                .small()
                .build(window, cx)
                .into(),
            ButtonControl::new()
                .label("Rec")
                .variant(ButtonVariant::Danger)
                .small()
                .toggled(true)
                .build(window, cx)
                .into(),
            ButtonControl::new()
                .label("Arm")
                .variant(ButtonVariant::Danger)
                .small()
                .toggled(false)
                .build(window, cx)
                .into(),
            ButtonControl::new()
                .icon(IconName::Undo)
                .tooltip("Undo")
                .xsmall()
                .build(window, cx)
                .into(),
        ];
        let chips: Vec<AnyView> = [
            ("M", true, ButtonVariant::Warning),
            ("S", false, ButtonVariant::Primary),
            ("R", true, ButtonVariant::Danger),
        ]
        .into_iter()
        .map(|(label, on, variant)| {
            ToggleChipControl::new()
                .label(label)
                .checked(on)
                .variant(variant)
                .build(window, cx)
                .into()
        })
        .collect();
        let fader = SliderControl::new()
            .range(0.0, 1.0)
            .step(0.01)
            .value(0.7)
            .vertical()
            .length(140.)
            .label("Volume")
            .build(window, cx)
            .into();
        let meter = Meter::new(MeterOptions::stereo().fill(), cx);
        meter
            .read(cx)
            .feed()
            .set_stereo(Level::new(-9.0, -15.0), Level::new(-4.0, -11.0));
        let bank = MeterBank::new(
            4,
            MeterOptions {
                thickness: 6.,
                ticks: Vec::new(),
                ..MeterOptions::default().fill()
            },
            cx,
        );
        for (i, feed) in MeterBank::feeds(&bank, cx).iter().enumerate() {
            feed.set(
                0,
                Level::new(-30.0 + i as f32 * 8.0, -36.0 + i as f32 * 8.0),
            );
        }
        cx.new(|_| Panel {
            transport,
            chips,
            fader,
            meter: meter.into(),
            bank: bank.into(),
        })
    })
    .unwrap()
}

#[test]
fn panel_controls_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "panel-controls-light"),
        (Scheme::Dark, "panel-controls-dark"),
    ] {
        let image = render(scheme);
        assert_snapshot(name, &image, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
