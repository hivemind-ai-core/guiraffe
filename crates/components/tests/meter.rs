//! Meter: snapshots in both themes, the clip latch through a click and the bus, and decay
//! that continues on its own after the host stops feeding.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, MouseButton, ParentElement, Render, Styled,
    TestAppContext, Window, div, px, size,
};
use guiraffe_components::{ClipReset, Level, Meter, MeterOptions, meter::RESET_CLIP};
use guiraffe_core::{
    GuiraffeTheme, Scheme, apply,
    protocol::{ActionBus, Source},
};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Row(Vec<Entity<Meter>>);
impl Render for Row {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(12.))
            .flex()
            .gap(px(16.))
            .children(self.0.iter().cloned())
    }
}

/// Mono and stereo, a clipped meter, gain reduction, horizontal with LUFS.
fn wall(cx: &mut gpui_kit::App) -> Vec<Entity<Meter>> {
    let mono = Meter::new(MeterOptions::default(), cx);
    let stereo = Meter::new(MeterOptions::stereo(), cx);
    let clipped = Meter::new(MeterOptions::stereo(), cx);
    let gr = Meter::new(MeterOptions::default().gain_reduction(20.0), cx);
    let horizontal = Meter::new(MeterOptions::stereo().horizontal().with_lufs(), cx);
    let feeds: Vec<_> = [&mono, &stereo, &clipped, &gr, &horizontal]
        .iter()
        .map(|m| m.read(cx).feed())
        .collect();
    feeds[0].set(0, Level::new(-18.0, -24.0));
    feeds[1].set_stereo(Level::new(-8.0, -14.0), Level::new(-3.0, -10.0));
    feeds[2].set_stereo(Level::new(1.0, -4.0), Level::new(-2.0, -6.0));
    feeds[3].set_gain_reduction(6.0);
    feeds[4].set_stereo(Level::new(-12.0, -20.0), Level::new(-5.0, -11.0));
    feeds[4].set_lufs(Some(-14.23), cx);
    vec![mono, stereo, clipped, gr, horizontal]
}

#[test]
fn meters_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(guiraffe_components::init);
        let img = s
            .render(size(px(520.), px(220.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let meters = wall(cx);
                cx.new(|_| Row(meters))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "meter-light"
        } else {
            "meter-dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

#[cfg_attr(
    not(debug_assertions),
    ignore = "GPUI records debug bounds in debug builds"
)]
#[gpui_kit::test]
fn the_clip_latch_resets_by_click_and_by_bus_action(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let mut meters = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let wall = vec![Meter::new(MeterOptions::stereo(), cx)];
        meters = Some(wall.clone());
        Row(wall)
    });
    let meter = meters.and_then(|m| m.into_iter().next()).expect("meter");
    let resets = Rc::new(RefCell::new(0));
    let r = resets.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&meter, move |_, _: &ClipReset, _| *r.borrow_mut() += 1)
            .detach()
    });
    let feed = vcx.update(|_, cx| meter.read(cx).feed());
    let clip = |vcx: &mut gpui_kit::VisualTestContext| {
        vcx.update(|_, cx| {
            feed.set_stereo(Level::new(0.5, -3.0), Level::new(-10.0, -12.0));
            feed.publish(cx);
        });
        vcx.run_until_parked();
        assert!(
            vcx.update(|_, cx| meter.read(cx).clipped(cx)),
            "latched at 0 dBFS"
        );
    };
    clip(vcx);
    let bars = vcx.debug_bounds("meter-bars").expect("bars");
    vcx.simulate_click(bars.center(), Default::default());
    vcx.run_until_parked();
    assert!(
        !vcx.update(|_, cx| meter.read(cx).clipped(cx)),
        "a click resets"
    );
    assert_eq!(*resets.borrow(), 1);

    clip(vcx);
    let id = vcx.update(|window, cx| Meter::register(&meter, "mix/master", window, cx));
    vcx.update(|window, cx| {
        let action = guiraffe_core::protocol::Action {
            name: RESET_CLIP.into(),
            payload: guiraffe_core::widget_value::Value::Null,
        };
        assert!(ActionBus::dispatch(id, &action, Source::Shortcut, window, cx).is_ok());
    });
    assert!(
        !vcx.update(|_, cx| meter.read(cx).clipped(cx)),
        "the bus resets"
    );
    assert_eq!(*resets.borrow(), 2);
    let _ = MouseButton::Left;
}

#[gpui_kit::test]
fn decay_continues_after_the_host_stops_feeding(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let mut meters = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let wall = vec![Meter::new(MeterOptions::default(), cx)];
        meters = Some(wall.clone());
        Row(wall)
    });
    let meter = meters.and_then(|m| m.into_iter().next()).expect("meter");
    let feed = vcx.update(|_, cx| meter.read(cx).feed());
    vcx.update(|_, cx| {
        feed.set(0, Level::new(-6.0, -12.0));
        feed.publish(cx);
    });
    vcx.run_until_parked();
    feed.set(0, Level::SILENT);
    let peak = |vcx: &mut gpui_kit::VisualTestContext| {
        vcx.update(|_, cx| meter.read(cx).channel_states(cx)[0])
    };
    let start = peak(vcx);
    // No more publishes: the bars ask for animation frames themselves.
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    vcx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    vcx.run_until_parked();
    let later = peak(vcx);
    assert_eq!(start.rms, -12.0);
    assert_eq!(
        later.rms,
        f32::NEG_INFINITY,
        "the bars re-rendered on their own animation frame and read the silent cell"
    );
}
