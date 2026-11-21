use gpui_kit::{
    AppContext, Context, Entity, IntoElement, MouseButton, ParentElement, Render, Role,
    StyleRefinement, Styled, TestAppContext, VisualTestContext, Window, div, point, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_charts::{
    Channels, ChartEvent, ChartOptions, ChartSpec, ChartView, Domain, Layer, Mark, SeriesId,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Host {
    chart: Entity<ChartView>,
    renders: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().child(
            self.chart
                .clone()
                .cached(StyleRefinement::default().w_full().h(px(400.))),
        )
    }
}

struct Root {
    host: Entity<Host>,
    unrelated: Entity<Unrelated>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(
                self.host
                    .clone()
                    .cached(StyleRefinement::default().w_full().h(px(400.))),
            )
            .child(
                self.unrelated
                    .clone()
                    .cached(StyleRefinement::default().w_full().h(px(20.))),
            )
    }
}

struct Unrelated(Rc<Cell<usize>>);

impl Render for Unrelated {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.set(self.0.get() + 1);
        div().size_full().child("Unrelated content")
    }
}

struct TestChart {
    view: Entity<ChartView>,
    host_renders: Rc<Cell<usize>>,
    unrelated_renders: Rc<Cell<usize>>,
    events: Rc<RefCell<Vec<ChartEvent>>>,
}

fn mount(cx: &mut TestAppContext) -> (TestChart, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::base::init(cx);
        guiraffe_charts::init(cx);
        gpui_kit::component::init(cx);
        guiraffe_core::apply(Default::default(), cx);
    });
    let host_renders = Rc::new(Cell::new(0));
    let unrelated_renders = Rc::new(Cell::new(0));
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut view = None;
    let (_, window) = cx.add_window_view(|window, cx| {
        let mut spec = ChartSpec::new("Revenue observations").layer(
            Layer::new(
                Mark::Line,
                vec![(0., 0.), (5., 5.), (10., 10.)],
                Channels::new(|p: &(f64, f64)| p.0, |p| p.1),
            )
            .title("Revenue"),
        );
        spec.x.domain = Domain::new(0., 10.);
        spec.y.domain = Domain::new(0., 10.);
        let chart = ChartView::new(spec.prepare(), ChartOptions::default(), cx);
        let log = events.clone();
        cx.subscribe(&chart, move |_, _, event, _| log.borrow_mut().push(*event))
            .detach();
        view = Some(chart.clone());
        let content = cx.new(|cx| Root {
            host: cx.new(|_| Host {
                chart,
                renders: host_renders.clone(),
            }),
            unrelated: cx.new(|_| Unrelated(unrelated_renders.clone())),
        });
        gpui_kit::base::Root::new(content, window, cx)
    });
    window.run_until_parked();
    (
        TestChart {
            view: view.expect("mounted view"),
            host_renders,
            unrelated_renders,
            events,
        },
        window,
    )
}

fn mouse_at(window: &mut VisualTestContext, x: f32, y: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
    window.update(|window, _| {
        let bounds = window.find("chart-canvas").bounds();
        let width = f32::from(bounds.size.width) - 95.;
        let height = f32::from(bounds.size.height) - 37.;
        bounds.origin + point(px(75. + width * x), px(12. + height * y))
    })
}

#[gpui_kit::test]
fn native_hover_tooltip_selection_and_render_counts(cx: &mut TestAppContext) {
    let (chart, window) = mount(cx);
    let builds = chart
        .view
        .read_with(window, |view, _| view.scene_build_count());
    let unrelated = chart.unrelated_renders.get();
    let host_before = chart.host_renders.get();
    let canvas_before = chart
        .view
        .read_with(window, |view, _| view.canvas_paint_count());
    let mid = mouse_at(window, 0.5, 0.5);
    window.simulate_mouse_move(mid, None, Default::default());
    window.run_until_parked();
    assert_eq!(
        chart
            .view
            .read_with(window, |view, cx| view.hovered(cx))
            .map(|id| id.key),
        Some(1)
    );
    window.update(|window, _| {
        assert_eq!(window.find("chart-canvas").role(), Some(Role::Canvas));
        assert_eq!(window.find("chart-tooltip").role(), Some(Role::Tooltip));
    });
    assert_eq!(
        chart
            .view
            .read_with(window, |view, _| view.scene_build_count()),
        builds
    );
    assert_eq!(chart.unrelated_renders.get(), unrelated);
    eprintln!(
        "hover render counts: host {} → {}, canvas {} → {}, scene {builds} → {builds}",
        host_before,
        chart.host_renders.get(),
        canvas_before,
        chart
            .view
            .read_with(window, |view, _| view.canvas_paint_count())
    );
    let stable = chart.host_renders.get();
    window.simulate_mouse_move(mid + point(px(1.), px(1.)), None, Default::default());
    window.run_until_parked();
    assert_eq!(
        chart.host_renders.get(),
        stable,
        "a stable snapped tooltip avoids a new invalidation"
    );
    window.simulate_click(mid, Default::default());
    window.run_until_parked();
    assert!(
        chart
            .events
            .borrow()
            .iter()
            .any(|e| matches!(e, ChartEvent::Selected(id) if id.key == 1))
    );
}

#[gpui_kit::test]
fn native_hover_across_points_keeps_host_cached(cx: &mut TestAppContext) {
    let (chart, window) = mount(cx);
    // Draw the root overlay registration and deliver the initial pointer event.
    if !gpui_kit::GPUI_FAST {
        window.update(|window, cx| window.draw(cx).clear(cx));
    }
    if !gpui_kit::GPUI_FAST {
        window.simulate_mouse_move(point(px(1.), px(390.)), None, Default::default());
        window.run_until_parked();
    }
    let mid = mouse_at(window, 0.5, 0.5);
    let left = mouse_at(window, 0.01, 0.99);
    let right = mouse_at(window, 0.99, 0.01);
    if !gpui_kit::GPUI_FAST {
        window.update(|window, cx| window.draw(cx).clear(cx));
    }
    let before = chart.host_renders.get();
    for (point, key) in [
        (mid, 1),
        (left, 0),
        (right, 2),
        (mid, 1),
        (left, 0),
        (right, 2),
    ] {
        window.simulate_mouse_move(point, None, Default::default());
        window.run_until_parked();
        assert_eq!(
            chart
                .view
                .read_with(window, |view, cx| view.hovered(cx))
                .map(|id| id.key),
            Some(key),
        );
        assert_eq!(chart.host_renders.get(), before, "hovered key {key}");
    }
}

#[gpui_kit::test]
fn legend_and_data_table_buttons_are_operable(cx: &mut TestAppContext) {
    let (chart, window) = mount(cx);
    let legend = window.update(|window, _| window.find_by_label("Revenue").bounds());
    window.simulate_click(legend.center(), Default::default());
    window.run_until_parked();
    let series = SeriesId::default();
    assert!(
        !chart
            .view
            .read_with(window, |view, _| view.series_visible(series))
    );
    assert!(
        chart
            .events
            .borrow()
            .contains(&ChartEvent::SeriesVisibility {
                series,
                visible: false
            })
    );
    window.update(|window, cx| {
        assert!(
            window.focused(cx).is_none(),
            "legend click keeps keyboard focus clear"
        );
        assert_eq!(window.find_by_label("Revenue").bounds(), legend);
    });
    window.update(|window, cx| {
        window.focus_next(cx);
        window.render_frame(cx);
        window.press("space", cx);
    });
    window.run_until_parked();
    assert!(
        chart
            .view
            .read_with(window, |view, _| view.series_visible(series))
    );
    window.update(|window, cx| {
        window.press("tab", cx);
        window.render_frame(cx);
        window.press("space", cx);
    });
    window.run_until_parked();
    window
        .update(|window, _| assert_eq!(window.find("chart-data-table").role(), Some(Role::Table)));
}

#[gpui_kit::test]
fn brush_emits_domain_and_wheel_navigation_keeps_anchor(cx: &mut TestAppContext) {
    let (chart, window) = mount(cx);
    let from = mouse_at(window, 0.2, 0.5);
    let to = mouse_at(window, 0.7, 0.5);
    window.simulate_mouse_down(from, MouseButton::Left, Default::default());
    window.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    window.simulate_mouse_up(to, MouseButton::Left, Default::default());
    window.run_until_parked();
    let domain = chart.view.read_with(window, |view, _| view.viewport());
    assert!((domain.min() - 2.).abs() < 1e-5);
    assert!((domain.max() - 7.).abs() < 1e-5);
    assert!(chart.events.borrow().contains(&ChartEvent::Brushed(domain)));
    let mid = mouse_at(window, 0.5, 0.5);
    window.simulate_event(gpui_kit::ScrollEvent {
        position: mid,
        delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(60.))),
        modifiers: Default::default(),
        touch_phase: gpui_kit::TouchPhase::Moved,
        source: gpui_kit::ScrollSource::Pointer,
    });
    window.run_until_parked();
    let zoom = chart.view.read_with(window, |view, _| view.viewport());
    assert!(zoom.span() < domain.span());
    assert!((zoom.min() + zoom.max() - domain.min() - domain.max()).abs() < 1e-6);
}

#[gpui_kit::test]
fn deferred_source_publishes_and_replacement_supersedes_pending_work(cx: &mut TestAppContext) {
    let (_, window) = mount(cx);
    let spec = || {
        let mut spec = ChartSpec::new("Background source").layer(Layer::new(
            Mark::Line,
            (0..100_000).collect::<Vec<i32>>(),
            Channels::new(|i| f64::from(*i), |i| f64::from(*i % 10)),
        ));
        spec.x.nice = false;
        spec
    };
    let (deferred, superseded) = window.update(|_, cx| {
        let deferred = ChartView::from_spec(spec(), ChartOptions::default(), cx);
        let superseded = ChartView::from_spec(spec(), ChartOptions::default(), cx);
        superseded.update(cx, |view, cx| {
            let mut replacement = ChartSpec::new("Replacement");
            replacement.x.domain = Domain::new(10., 20.);
            view.replace(replacement.prepare(), cx);
        });
        (deferred, superseded)
    });
    window.run_until_parked();
    window.update(|_, cx| {
        assert_eq!(
            deferred.read(cx).viewport(),
            Domain::new(0., 99_999.).expect("domain")
        );
        assert_eq!(
            superseded.read(cx).viewport(),
            Domain::new(10., 20.).expect("domain")
        );
    });
}

#[gpui_kit::test]
fn superseded_stream_batch_keeps_previous_plot_until_latest_spec_is_ready(cx: &mut TestAppContext) {
    use guiraffe_charts::Stream;

    let (chart, window) = mount(cx);
    let mut source = Stream::default();
    source.append((0..500).collect::<Vec<i32>>());
    let make_spec = |snapshot: Stream<i32>, max| {
        let mut spec = ChartSpec::new("Stream").layer(Layer::new(
            Mark::Line,
            snapshot,
            Channels::new(|value: &i32| f64::from(*value), |value| f64::from(*value)),
        ));
        spec.x.domain = Domain::new(0., max);
        spec
    };
    let first_batch = make_spec(source.clone(), 499.);
    source.append((500..1_000).collect());
    source.retain_last(500);
    let latest_batch = make_spec(source, 999.);
    window.update(|_, cx| {
        chart.view.update(cx, |view, cx| {
            let previous = view.viewport();
            view.replace_spec(first_batch, cx);
            view.replace_spec(latest_batch, cx);
            assert_eq!(view.viewport(), previous);
        });
    });
    window.run_until_parked();
    assert_eq!(
        chart.view.read_with(window, |view, _| view.viewport()),
        Domain::new(0., 999.).expect("valid stream domain"),
    );
}

#[gpui_kit::test]
fn dense_scatter_paints_batches_and_picks_unsampled_source_points(cx: &mut TestAppContext) {
    let (chart, window) = mount(cx);
    let mut spec = ChartSpec::new("Dense scatter").layer(Layer::new(
        Mark::Point,
        (0..9_000).collect::<Vec<_>>(),
        Channels::new(|_| 5., |_| 5.),
    ));
    spec.x.domain = Domain::new(0., 10.);
    spec.y.domain = Domain::new(0., 10.);
    window.update(|_, cx| {
        chart
            .view
            .update(cx, |view, cx| view.replace(spec.prepare(), cx));
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    let mid = mouse_at(window, 0.5, 0.5);
    window.simulate_mouse_move(mid, None, Default::default());
    window.run_until_parked();
    assert_eq!(
        chart
            .view
            .read_with(window, |view, cx| view.hovered(cx))
            .map(|datum| datum.key),
        Some(8_999),
    );
}
