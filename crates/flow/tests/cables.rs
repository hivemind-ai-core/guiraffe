//! Cables between a side-panel knob and a flow port follow both ends through pan, zoom,
//! panel resize and scroll, hide when an end leaves view, and cost nothing on a frame
//! that moves nothing (#152).

use gpui_kit::{
    AnyView, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
    Point as PixelPoint, Render, ScrollHandle, StatefulInteractiveElement, StyleRefinement, Styled,
    TestAppContext, VisualTestContext, Window, div, point, px, test::TestWindowExt,
};
use guiraffe_components::controls::KnobControl;
use guiraffe_flow::{
    FlowCanvas, FlowOptions, FlowState,
    cables::{Cable, CableAnchorExt, CableEnd, CableLayer, Cables, SetCable},
    core::{Graph, Node, Point, Side, StrokeStyle, Viewport},
};
use guiraffe_state::Store;
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Host {
    flow: Entity<FlowState>,
    knob: Entity<KnobControl>,
    layer: Entity<CableLayer>,
    panel: f32,
    scroll: ScrollHandle,
    /// Where the knob box was last painted, measured apart from its anchor.
    knob_at: std::rc::Rc<std::cell::Cell<gpui_kit::Bounds<Pixels>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .child(
                div()
                    .id("panel")
                    .w(px(self.panel))
                    .h_full()
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(div().h(px(120.)))
                    .child(
                        div()
                            .id("knob-box")
                            .size(px(48.))
                            .relative()
                            .child(
                                AnyView::from(self.knob.clone())
                                    .cached(StyleRefinement::default().size(px(48.))),
                            )
                            .child({
                                let at = self.knob_at.clone();
                                gpui_kit::canvas(move |b, _, _| at.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .inset_0()
                            })
                            .cable_anchor("knob")
                            .side(Side::Right),
                    )
                    .child(div().h(px(2000.))),
            )
            .child(div().flex_1().h_full().child(FlowCanvas::new(&self.flow)))
            .child(
                AnyView::from(self.layer.clone())
                    .cached(StyleRefinement::default().absolute().size_full()),
            )
    }
}

fn graph() -> Graph {
    Graph::from_parts(
        [
            Node::new(1, "Source", Point::new(80., 120.)),
            Node::new(2, "Target", Point::new(320., 200.)),
        ],
        [],
    )
    .expect("graph")
}

fn open(
    cx: &mut TestAppContext,
    cables: usize,
) -> (Entity<Host>, Store<Cables>, &mut VisualTestContext) {
    cx.update(guiraffe_components::init);
    let store = cx.update(|cx| Store::new(Cables::new(), cx));
    let layer_store = store.clone();
    let (host, vcx) = cx.add_window_view(move |window, cx| {
        let flow = cx.new(|cx| {
            FlowState::new(graph(), cx)
                .expect("flow")
                .options(FlowOptions {
                    toolbar: false,
                    minimap: false,
                    controls: false,
                    ..FlowOptions::default()
                })
        });
        let knob = KnobControl::new().path("panel/cutoff").build(window, cx);
        let layer = cx.new(|cx| {
            let mut layer = CableLayer::new(&layer_store, cx);
            layer.add_flow("graph", &flow, cx);
            layer
        });
        Host {
            flow,
            knob,
            layer,
            panel: 200.,
            scroll: ScrollHandle::new(),
            knob_at: Default::default(),
        }
    });
    vcx.update(|_, cx| {
        for id in 0..cables as u64 {
            let to = if id == 0 {
                CableEnd::port("graph", 2, "in")
            } else {
                CableEnd::port("graph", 1, "out")
            };
            store.dispatch(
                SetCable(
                    id,
                    Cable::new(CableEnd::element("knob"), to).style(StrokeStyle::DASHED),
                ),
                cx,
            );
        }
    });
    frames(vcx);
    (host, store, vcx)
}

fn frames(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
    }
}

fn close(a: PixelPoint<Pixels>, b: PixelPoint<Pixels>) -> bool {
    (a.x - b.x).abs() < px(0.5) && (a.y - b.y).abs() < px(0.5)
}

/// Checks cable 0's ends against the painted knob and port; returns whether both show.
fn check(host: &Entity<Host>, vcx: &mut VisualTestContext, what: &str) -> bool {
    vcx.update(|window, cx| {
        let knob = host.read(cx).knob_at.get();
        let flow = host.read(cx).flow.clone();
        let port = flow
            .read(cx)
            .port_anchor(2, "in", cx)
            .expect("port anchor")
            .position;
        let layer = host.read(cx).layer.clone();
        let (from, to) = layer.read(cx).endpoints(0, window, cx).expect("cable 0");
        let knob_point = point(knob.right(), knob.center().y);
        if from.visible {
            assert!(
                close(from.position, knob_point),
                "{what}: knob end {from:?}, knob at {knob_point:?}"
            );
        }
        if to.visible {
            assert!(
                close(to.position, port),
                "{what}: port end {to:?}, port at {port:?}"
            );
        }
        assert_eq!(to.side, Some(Side::Left), "{what}");
        from.visible && to.visible
    })
}

#[gpui_kit::test]
fn cable_ends_follow_the_knob_and_the_port(cx: &mut TestAppContext) {
    let (host, _, vcx) = open(cx, 1);
    assert!(check(&host, vcx, "first frame"));

    let flow = vcx.update(|_, cx| host.read(cx).flow.clone());
    for (pan, zoom) in [((40., -20.), 1.0), ((-30., 15.), 0.7), ((10., 40.), 1.6)] {
        vcx.update(|_, cx| {
            flow.update(cx, |flow, cx| {
                flow.set_viewport(
                    Viewport {
                        pan: Point::new(pan.0, pan.1),
                        zoom,
                    },
                    cx,
                )
            })
            .expect("viewport");
        });
        frames(vcx);
        assert!(check(&host, vcx, &format!("pan {pan:?} zoom {zoom}")));
    }

    vcx.update(|_, cx| {
        host.update(cx, |host, cx| {
            host.panel = 280.;
            cx.notify();
        })
    });
    frames(vcx);
    assert!(check(&host, vcx, "panel resized"));

    let scroll = vcx.update(|_, cx| host.read(cx).scroll.clone());
    scroll.set_offset(point(px(0.), px(-60.)));
    vcx.update(|_, cx| host.update(cx, |_, cx| cx.notify()));
    frames(vcx);
    assert!(check(&host, vcx, "panel scrolled"));

    // The knob scrolled out of its panel: hidden, not clamped.
    scroll.set_offset(point(px(0.), px(-600.)));
    vcx.update(|_, cx| host.update(cx, |_, cx| cx.notify()));
    frames(vcx);
    assert!(!check(&host, vcx, "knob scrolled away"));
    scroll.set_offset(point(px(0.), px(0.)));
    vcx.update(|_, cx| host.update(cx, |_, cx| cx.notify()));
    frames(vcx);
    assert!(check(&host, vcx, "knob scrolled back"));

    // The port panned off the canvas: hidden.
    vcx.update(|_, cx| {
        flow.update(cx, |flow, cx| {
            flow.set_viewport(
                Viewport {
                    pan: Point::new(-5_000., 0.),
                    zoom: 1.,
                },
                cx,
            )
        })
        .expect("viewport");
    });
    frames(vcx);
    assert!(!check(&host, vcx, "port panned away"));
}

#[gpui_kit::test]
fn an_unknown_end_hides_its_cable(cx: &mut TestAppContext) {
    let (host, store, vcx) = open(cx, 1);
    vcx.update(|_, cx| {
        store.dispatch(
            SetCable(
                0,
                Cable::new(
                    CableEnd::element("nowhere"),
                    CableEnd::port("graph", 2, "in"),
                ),
            ),
            cx,
        )
    });
    frames(vcx);
    vcx.update(|window, cx| {
        let layer = host.read(cx).layer.clone();
        let (from, to) = layer.read(cx).endpoints(0, window, cx).expect("cable 0");
        assert!(!from.visible && to.visible);
        assert!(layer.read(cx).endpoints(9, window, cx).is_none());
    });
}

/// What a frame that moves nothing allocates, with `cables` cables.
fn still_frame(cx: &mut TestAppContext, cables: usize) -> u64 {
    let (host, _, vcx) = open(cx, cables);
    let knob = vcx.update(|_, cx| host.read(cx).knob.clone());
    let frame = |vcx: &mut VisualTestContext| {
        // The knob draws again in place, as when its value changes; `render_frame` would
        // refresh every view.
        count_allocations(|| {
            vcx.update(|_, cx| knob.update(cx, |_, cx| cx.notify()));
            vcx.run_until_parked();
        })
        .1
        .allocations
    };
    frame(vcx);
    frame(vcx);
    let first = frame(vcx);
    let second = frame(vcx);
    assert_eq!(first, second, "an unchanged frame's allocations vary");
    first
}

/// On upstream a frame that moves nothing reuses the cached layer: cables cost nothing.
/// gpui-fast draws a window holding a flow canvas in full rather than patching it, and a
/// full frame builds cached views again, so there the layer draws again at a cost
/// proportional to the cables (#153).
#[gpui_kit::test]
fn a_frame_that_moves_nothing_costs_the_same_for_any_number_of_cables(cx: &mut TestAppContext) {
    let one = still_frame(cx, 1);
    let many = still_frame(cx, 40);
    if gpui_kit::GPUI_FAST {
        let per_cable = (many.saturating_sub(one)) / 39;
        assert!(
            per_cable <= 40,
            "a cable costs {per_cable} allocations on a full frame ({one} with one, {many} with forty)"
        );
    } else {
        assert_eq!(
            one, many,
            "cables cost {one} with one, {many} with forty on a still frame"
        );
    }
}
