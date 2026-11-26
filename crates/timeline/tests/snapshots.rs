//! Visual goldens for the four timeline views, light and dark, on the active backend.
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use guiraffe_timeline::{
    BarCurve, BarCurveView, Channel, Curve, CurveView, DopeSheet, GanttView, Interp, Key, Task,
    TimeFormat, Track, TracksView,
};

struct Scene {
    gantt: Entity<GanttView>,
    tracks: Entity<TracksView>,
    dope: Entity<DopeSheet>,
    curve: Entity<CurveView>,
    bars: Entity<BarCurveView>,
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cell = |e: gpui_kit::AnyView| div().h(px(180.)).w_full().overflow_hidden().child(e);
        div()
            .bg(GuiraffeTheme::global(cx).surfaces().border.to_hsla())
            .size_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(cell(self.gantt.clone().into()))
            .child(cell(self.tracks.clone().into()))
            .child(cell(self.dope.clone().into()))
            .child(cell(self.curve.clone().into()))
            .child(cell(self.bars.clone().into()))
    }
}

fn task(id: u32, name: &str, nb: f64, d: f64, after: &[u32]) -> Task {
    Task {
        id,
        name: name.into(),
        not_before: 20_400.0 * 86_400.0 + nb * 86_400.0,
        duration: d * 86_400.0,
        after: after.to_vec(),
    }
}

#[test]
fn timeline_views_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(gpui_kit::component::init);
        let img = s
            .render(size(px(900.), px(920.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let gantt = GanttView::new(
                    vec![
                        task(1, "Design", 0.0, 3.0, &[]),
                        task(2, "Build", 0.0, 5.0, &[1]),
                        task(3, "Docs", 0.0, 2.0, &[1]),
                        task(4, "Release", 0.0, 1.0, &[2, 3]),
                    ],
                    TimeFormat::Days,
                    cx,
                );
                gantt.update(cx, |v, cx| v.set_locked(4, true, cx));
                let mut a = Track::default();
                a.insert(0.5, 2.0);
                a.insert(3.0, 1.5);
                let mut b = Track::default();
                b.insert(1.0, 4.0);
                let tracks = TracksView::new(
                    vec![("Drums".into(), a), ("Bass".into(), b)],
                    TimeFormat::Seconds,
                    8.0,
                    cx,
                );
                let dope = DopeSheet::new(
                    vec![Channel::group(
                        "Object summary",
                        vec![
                            Channel::with_keyframes(
                                "position.x",
                                vec![
                                    Key::new(0., 0., Interp::Constant),
                                    Key::new(0.5, 0., Interp::Bezier),
                                    Key::new(1.2, 1., Interp::Linear),
                                ],
                            ),
                            Channel::new("opacity", vec![0.2, 1.0, 1.8]),
                        ],
                    )],
                    TimeFormat::Frames(24.0),
                    2.0,
                    cx,
                );
                dope.update(cx, |v, cx| {
                    v.press(1, v.axis().x(0.5), cx);
                    v.release(cx);
                });
                let mut c = Curve::default();
                let mut key = Key::new(0.0, 0.0, Interp::Bezier);
                key.out_handle = (0.4, 0.6);
                c.set(key);
                c.set(Key::new(1.0, 1.0, Interp::Linear));
                c.set(Key::new(2.0, 0.3, Interp::Constant));
                c.set(Key::new(3.0, 0.8, Interp::Linear));
                let curve = CurveView::new(c, 0.0, 3.0, -0.2, 1.2, cx);
                curve.update(cx, |v, cx| {
                    v.select_key(0, 0, cx);
                    let mut secondary = Curve::default();
                    secondary.set(Key::new(0., 0.8, Interp::Linear));
                    secondary.set(Key::new(3., 0.2, Interp::Linear));
                    v.add_curve(
                        guiraffe_timeline::CurveSeries::new(
                            "Other",
                            secondary,
                            guiraffe_core::ColorName::Orange,
                        ),
                        cx,
                    );
                });
                let mut bar_model = BarCurve::new(32, -1., 1.);
                bar_model.bipolar = true;
                bar_model.set_steps(Some(16));
                bar_model.draw((0., -0.8), (16., 0.8));
                bar_model.draw((16., 0.8), (31., -0.4));
                let bars = BarCurveView::new(bar_model, cx);
                cx.new(|_| Scene {
                    gantt,
                    tracks,
                    dope,
                    curve,
                    bars,
                })
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "timeline/light"
        } else {
            "timeline/dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
