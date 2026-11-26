use crate::{FlowError, FlowViewport as Flow, Graph, Node, NodeRenderer, Viewport, WorldPoint};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Entity, IntoElement, ParentElement, ScrollDelta,
    Window, WindowOptions, div, point, px,
};
use gpui_kit::{TestAppContext, test::TestWindowExt};
use std::{cell::Cell, rc::Rc};
fn fixture() -> Graph {
    Graph::from_parts(
        [
            Node::new(1, "Source", WorldPoint::new(80., 120.)),
            Node::new(2, "Target", WorldPoint::new(480., 120.)),
        ],
        [],
    )
    .expect("fixture")
}
#[gpui_kit::test]
fn canvas_mounts_and_controls_change_viewport(cx: &mut TestAppContext) {
    let (window, view) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .options(crate::FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
            })
        })
        .expect("window")
    });
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.find("flow");
            window.click("flow-zoom-in", cx);
        })
        .expect("update");
    assert!(view.read_with(cx, |v, _| v.viewport().zoom) > 1.);
    window
        .update(cx, |_, window, cx| window.click("flow-fit", cx))
        .expect("fit");
    assert!(view.read_with(cx, |v, _| v.viewport().valid()));
}
fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Flow>) {
    let result = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .options(crate::FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
            })
        })
        .expect("window")
    });
    result
        .0
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
        })
        .expect("frame");
    result
}
#[gpui_kit::test]
fn real_pointer_drag_snaps_and_keyboard_undo_redo(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    w.update(cx, |_, w, cx| {
        let bounds = w.find(("flow-node", 1u64)).bounds();
        let from = bounds.center();
        w.drag(from, from + point(px(60.), px(40.)), cx);
    })
    .expect("drag");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(140., 160.)
    );
    w.update(cx, |_, w, cx| w.press("ctrl-z", cx))
        .expect("undo");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(80., 120.)
    );
    w.update(cx, |_, w, cx| w.press("ctrl-shift-z", cx))
        .expect("redo");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(140., 160.)
    );
}
#[gpui_kit::test]
fn real_handle_drag_creates_edge_and_duplicate_emits_error(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    for _ in 0..2 {
        w.update(cx, |_, w, cx| {
            w.drag(point(px(240.), px(156.)), point(px(480.), px(156.)), cx)
        })
        .expect("connect");
    }
    assert_eq!(v.read_with(cx, |v, _| v.graph().edges.len()), 1);
    assert_eq!(
        v.read_with(cx, |v, _| v.last_error().cloned()),
        Some(FlowError::DuplicateConnection)
    );
}
#[gpui_kit::test]
fn keyboard_selection_delete_and_controls_restore_document(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    w.update(cx, |_, w, cx| {
        w.click(("flow-node", 1u64), cx);
        w.press("ctrl-a", cx);
        w.press("delete", cx);
    })
    .expect("delete");
    assert!(v.read_with(cx, |v, _| v.graph().nodes.is_empty()));
    w.update(cx, |_, w, cx| w.click("flow-undo", cx))
        .expect("undo");
    assert_eq!(v.read_with(cx, |v, _| v.graph().nodes.len()), 2);
}
#[gpui_kit::test]
fn wheel_pans_and_keyboard_zoom_preserves_center(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    w.update(cx, |_, w, cx| {
        w.click_at("flow", point(px(30.), px(400.)), cx);
        w.scroll("flow", ScrollDelta::Pixels(point(px(20.), px(-30.))), cx);
    })
    .expect("wheel");
    assert_eq!(
        v.read_with(cx, |v, _| v.viewport().pan),
        WorldPoint::new(20., -30.)
    );
    let before = v.read_with(cx, |v, _| v.viewport().world(v.size() * 0.5));
    w.update(cx, |_, w, cx| w.press("=", cx)).expect("zoom");
    let after = v.read_with(cx, |v, _| v.viewport().world(v.size() * 0.5));
    assert!(before.distance(after) < 0.001);
}
#[gpui_kit::test]
fn culling_unmounts_distant_nodes_and_custom_renderer_is_called(cx: &mut TestAppContext) {
    struct Renderer(Rc<Cell<usize>>);
    impl NodeRenderer for Renderer {
        fn render(&self, _: &Node, _: f32, _: &mut Window, _: &mut App) -> AnyElement {
            self.0.set(self.0.get() + 1);
            div().child("custom").into_any_element()
        }
    }
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .options(crate::FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                    .node_renderer(Renderer(observed))
            })
        })
        .expect("window")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
        w.find(("flow-node", 1u64));
    })
    .expect("frame");
    assert!(count.get() > 0);
    v.update(cx, |v, cx| {
        v.set_viewport(
            Viewport {
                pan: WorldPoint::new(-10000., -10000.),
                zoom: 1.,
            },
            cx,
        )
        .expect("camera")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        assert!(w.try_find(("flow-node", 1u64)).is_none());
    })
    .expect("cull");
}
#[gpui_kit::test]
fn native_pinch_preserves_world_point_under_pointer(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent, PinchEvent};
    let (w, v) = open(cx);
    let at = WorldPoint::new(330., 270.);
    let before = v.read_with(cx, |v, _| v.viewport().world(at));
    w.update(cx, |_, w, cx| {
        w.dispatch_event(
            PinchEvent {
                position: point(px(at.x), px(at.y)),
                delta: 0.3,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        w.render_frame(cx);
    })
    .expect("pinch");
    assert!(
        v.read_with(cx, |v, _| v.viewport().world(at))
            .distance(before)
            < 0.001
    );
    assert!(v.read_with(cx, |v, _| v.viewport().zoom) > 1.);
}
#[gpui_kit::test]
fn pan_updates_observed_node_bounds(cx: &mut TestAppContext) {
    struct Renderer(Rc<Cell<usize>>);
    impl NodeRenderer for Renderer {
        fn render(&self, _: &Node, _: f32, _: &mut Window, _: &mut App) -> AnyElement {
            self.0.set(self.0.get() + 1);
            div().child("retained").into_any_element()
        }
    }
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .options(crate::FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                    .node_renderer(Renderer(observed))
            })
        })
        .expect("window")
    });
    let before = w
        .update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.render_frame(cx);
            w.find(("flow-node", 1u64)).bounds().origin
        })
        .expect("frame");
    v.update(cx, |v, cx| {
        v.set_viewport(
            Viewport {
                pan: WorldPoint::new(10., 20.),
                zoom: 1.,
            },
            cx,
        )
        .expect("camera")
    });
    let after = w
        .update(cx, |_, w, cx| {
            w.render_frame(cx);
            w.find(("flow-node", 1u64)).bounds().origin
        })
        .expect("pan");
    assert!(count.get() > 0);
    assert_eq!(after, before + point(px(10.), px(20.)));
}

#[gpui_kit::test]
fn review_1_render_tolerates_deleted_drag_nodes(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    v.update(cx, |v, cx| {
        v.interaction
            .press(WorldPoint::new(110., 150.), false, false);
        v.interaction.motion(WorldPoint::new(150., 170.));
        v.interaction.editor.delete_selected().expect("delete");
        cx.notify();
    });
    w.update(cx, |_, w, cx| w.render_frame(cx)).expect("render");
}
#[gpui_kit::test]
fn review_16_empty_minimap_and_extreme_pinch_remain_valid(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent, PinchEvent};
    let (w, v) = open(cx);
    v.update(cx, |v, cx| {
        v.edit(
            |g| {
                g.nodes.clear();
                g.edges.clear();
                Ok(())
            },
            cx,
        )
        .expect("empty");
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        let p = w.find("flow").bounds().bottom_right() - point(px(100.), px(80.));
        w.click_at("flow", p, cx);
        for delta in [-1., -2., f32::NAN, 10000.] {
            w.dispatch_event(
                PinchEvent {
                    position: point(px(200.), px(200.)),
                    delta,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        }
    })
    .expect("events");
    assert!(v.read_with(cx, |v, _| v.viewport().valid()));
}

#[gpui_kit::test]
fn review_1_toolbar_delete_cancels_drag(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    v.update(cx, |v, cx| {
        v.interaction
            .press(WorldPoint::new(110., 150.), false, false);
        cx.notify();
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        let center = w.find("flow-delete").bounds().center();
        v.update(cx, |v, _| {
            if let Some(crate::Gesture::Move { start, .. }) = &mut v.interaction.gesture {
                *start = WorldPoint::new(center.x.as_f32(), center.y.as_f32());
            }
        });
        w.click("flow-delete", cx);
        w.render_frame(cx);
    })
    .expect("toolbar delete");
    assert!(v.read_with(cx, |v, _| v.interaction.gesture.is_none()));
    assert!(!v.read_with(cx, |v, _| v.graph().nodes.contains_key(&1)));
}
#[gpui_kit::test]
fn review_8_real_edge_click_and_keyboard_delete(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    v.update(cx, |v, cx| {
        v.edit(
            |g| {
                g.edges.insert(1, crate::core::Edge::new(1, 1, 2));
                Ok(())
            },
            cx,
        )
        .expect("edge");
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.click_at("flow", point(px(360.), px(156.)), cx);
        w.press("delete", cx);
    })
    .expect("delete");
    assert!(v.read_with(cx, |v, _| v.graph().edges.is_empty()));
    assert_eq!(v.read_with(cx, |v, _| v.graph().nodes.len()), 2);
}
#[gpui_kit::test]
fn review_3_child_paints_and_picks_above_higher_id_parent(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    v.update(cx, |v, cx| {
        v.edit(
            |g| {
                let parent = g.nodes.get_mut(&2).expect("parent");
                parent.pos = WorldPoint::new(60., 100.);
                parent.size = WorldPoint::new(400., 300.);
                g.nodes.get_mut(&1).expect("child").parent = Some(2);
                Ok(())
            },
            cx,
        )
        .expect("parent");
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.click_at("flow", point(px(120.), px(150.)), cx);
    })
    .expect("pick");
    assert!(v.read_with(cx, |v, _| v.interaction.editor.is_selected(1)));
    assert!(!v.read_with(cx, |v, _| v.interaction.editor.is_selected(2)));
    assert_eq!(v.read_with(cx, |v, _| v.ordered_ids.clone()), vec![2, 1]);
}
#[gpui_kit::test]
fn review_12_retained_node_and_edge_buffer_survive_camera_frame(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    let before = v.read_with(cx, |v, cx| {
        (
            v.nodes[&1].read(cx).revision,
            v.meshes.borrow().edge_buffer.as_ptr(),
        )
    });
    v.update(cx, |v, cx| {
        v.set_viewport(
            Viewport {
                pan: WorldPoint::new(10., 10.),
                zoom: 1.,
            },
            cx,
        )
        .expect("pan");
    });
    w.update(cx, |_, w, cx| w.render_frame(cx)).expect("frame");
    let after = v.read_with(cx, |v, cx| {
        (
            v.nodes[&1].read(cx).revision,
            v.meshes.borrow().edge_buffer.as_ptr(),
        )
    });
    assert_eq!(before, after);
}
#[gpui_kit::test]
fn review_16_control_wheel_clamps_extreme_deltas(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent, Modifiers, ScrollEvent};
    let (w, v) = open(cx);
    for (delta, expected) in [
        (100000., Viewport::MAX_ZOOM),
        (-100000., Viewport::MIN_ZOOM),
    ] {
        w.update(cx, |_, w, cx| {
            w.dispatch_event(
                ScrollEvent {
                    position: point(px(300.), px(300.)),
                    delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                    modifiers: Modifiers {
                        control: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        })
        .expect("wheel");
        assert_eq!(v.read_with(cx, |v, _| v.viewport().zoom), expected);
    }
}

#[gpui_kit::test]
fn review_12_unrelated_node_edit_does_not_render_retained_node(cx: &mut TestAppContext) {
    struct Renderer(Rc<Cell<usize>>);
    impl NodeRenderer for Renderer {
        fn render(&self, node: &Node, _: f32, _: &mut Window, _: &mut App) -> AnyElement {
            if node.id == 1 {
                self.0.set(self.0.get() + 1);
            }
            div().child("counted").into_any_element()
        }
    }
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .options(crate::FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                    .node_renderer(Renderer(observed))
            })
        })
        .expect("window")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
    })
    .expect("initial frames");
    let before = count.get();
    assert!(before > 0);
    v.update(cx, |v, cx| {
        v.edit(
            |g| {
                g.nodes.get_mut(&2).expect("node").data = "changed target".into();
                Ok(())
            },
            cx,
        )
        .expect("edit");
    });
    // render_frame deliberately refreshes every view; draw tests normal dirty-view caching.
    w.update(cx, |_, w, cx| w.draw(cx).clear(cx))
        .expect("frame");
    assert_eq!(count.get(), before);
}

#[gpui_kit::test]
fn review_16_real_self_loop_with_allow_self(cx: &mut TestAppContext) {
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| {
                Flow::new(fixture(), cx)
                    .expect("flow")
                    .options(crate::FlowOptions {
                        fit_on_init: false,
                        ..Default::default()
                    })
                    .connection_policy(crate::core::ConnectionPolicy {
                        allow_self: true,
                        ..Default::default()
                    })
            })
        })
        .expect("window")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
        w.drag(point(px(240.), px(156.)), point(px(80.), px(156.)), cx);
    })
    .expect("self connect");
    assert_eq!(v.read_with(cx, |v, _| v.graph().edges.len()), 1);
    assert_eq!(
        v.read_with(cx, |v, _| {
            let e = v.graph().edges.values().next().expect("edge");
            (e.source.node, e.target.node)
        }),
        (1, 1)
    );
}
#[gpui_kit::test]
fn review_2_real_parent_drag_moves_child_once(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    v.update(cx, |v, cx| {
        v.edit(
            |g| {
                let parent = g.nodes.get_mut(&2).expect("parent");
                parent.pos = WorldPoint::new(60., 100.);
                parent.size = WorldPoint::new(400., 300.);
                g.nodes.get_mut(&1).expect("child").parent = Some(2);
                Ok(())
            },
            cx,
        )
        .expect("parent");
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.drag(point(px(400.), px(300.)), point(px(440.), px(320.)), cx);
    })
    .expect("drag");
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&1].pos),
        WorldPoint::new(120., 140.)
    );
    assert_eq!(
        v.read_with(cx, |v, _| v.graph().nodes[&2].pos),
        WorldPoint::new(100., 120.)
    );
}

#[gpui_kit::test]
fn review2_1_noop_edit_and_idle_release_emit_no_events_or_notify(cx: &mut TestAppContext) {
    let (w, v) = open(cx);
    let events = Rc::new(Cell::new(0));
    let n = events.clone();
    let _events =
        cx.update(|cx| cx.subscribe(&v, move |_, _: &crate::FlowEvent, _| n.set(n.get() + 1)));
    let notifies = Rc::new(Cell::new(0));
    let n = notifies.clone();
    let _notifies = cx.update(|cx| cx.observe(&v, move |_, _| n.set(n.get() + 1)));
    v.update(cx, |v, cx| {
        assert!(!v.edit(|_| Ok(()), cx).expect("noop"));
    });
    w.update(cx, |_, w, cx| w.draw(cx).clear(cx)).expect("draw");
    assert_eq!(events.get(), 0);
    assert_eq!(notifies.get(), 0);
}
#[gpui_kit::test]
fn review2_first_layout_fits_all_nodes(cx: &mut TestAppContext) {
    let graph = Graph::from_parts(
        [
            Node::new(1, "left", WorldPoint::new(-500., 0.)),
            Node::new(2, "right", WorldPoint::new(3000., 0.)),
        ],
        [],
    )
    .expect("graph");
    let (w, v) = cx.update(|cx| {
        guiraffe_core::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| Flow::new(graph, cx).expect("flow"))
        })
        .expect("window")
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
        w.render_frame(cx);
    })
    .expect("layout");
    assert!(v.read_with(cx, |v, _| {
        v.graph().nodes.values().all(|n| {
            v.viewport().visible(v.size()).contains(n.pos)
                && v.viewport().visible(v.size()).contains(n.rect().max())
        })
    }));
}

#[gpui_kit::test]
fn review2_portable_host_isolation(cx: &mut TestAppContext) {
    use gpui_kit::{Context, InputEvent, Render, ScrollEvent, Styled};
    struct Sibling(Rc<Cell<usize>>);
    impl Render for Sibling {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.set(self.0.get() + 1);
            div()
        }
    }
    struct Host {
        flow: Entity<crate::Flow>,
        sibling: Entity<Sibling>,
        renders: Rc<Cell<usize>>,
    }
    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let _ = self.flow.read(cx).graph().nodes.len();
            div()
                .size_full()
                .child(self.flow.clone())
                .child(self.sibling.clone())
        }
    }
    let a = Rc::new(Cell::new(0));
    let b = Rc::new(Cell::new(0));
    let (w, flow) = cx.update(|cx| {
        guiraffe_core::init(cx);
        let flow = cx.new(|cx| crate::Flow::new(fixture(), cx).expect("flow"));
        let (w, _) = gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            let sibling = cx.new(|_| Sibling(b.clone()));
            cx.new(|_| Host {
                flow: flow.clone(),
                sibling,
                renders: a.clone(),
            })
        })
        .expect("window");
        (w, flow)
    });
    w.update(cx, |_, w, cx| {
        w.render_frame(cx);
        w.render_frame(cx);
    })
    .expect("initial");
    let before = (a.get(), b.get());
    let camera = flow.read_with(cx, |flow, _| flow.viewport());
    for _ in 0..5 {
        w.update(cx, |_, w, cx| {
            w.dispatch_event(
                ScrollEvent {
                    position: point(px(300.), px(300.)),
                    delta: ScrollDelta::Pixels(point(px(10.), px(0.))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            w.draw(cx).clear(cx);
        })
        .expect("pan");
    }
    assert_eq!(
        flow.read_with(cx, |flow, _| flow.viewport().pan),
        camera.pan + WorldPoint::new(50., 0.)
    );
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            (a.get(), b.get()),
            before,
            "pan changed unrelated host/sibling"
        );
    }
}

#[gpui_kit::test]
fn handle_centers_match_world_endpoints_at_every_zoom(cx: &mut TestAppContext) {
    let (w, view) = open(cx);
    for zoom in [0.6, 1.0, 2.0] {
        view.update(cx, |view, cx| {
            view.set_viewport(
                Viewport {
                    pan: WorldPoint::new(20., 30.),
                    zoom,
                },
                cx,
            )
        })
        .expect("set viewport");
        w.update(cx, |_, window, cx| {
            window.render_frame(cx);
            let node = window.find(("flow-node", 1u64)).bounds();
            let input = window
                .within(("flow-node", 1u64))
                .find("handle-in")
                .bounds();
            let output = window
                .within(("flow-node", 1u64))
                .find("handle-out")
                .bounds();
            assert!(
                (input.center().x - node.left()).abs() < px(0.01),
                "input must lie on node edge: {input:?} {node:?}"
            );
            assert!(
                (output.center().x - node.right()).abs() < px(0.01),
                "output must lie on node edge"
            );
            assert!(
                (input.center().y - node.center().y).abs() < px(0.01),
                "handle y must match edge route"
            );
        })
        .expect("frame");
    }
}

#[gpui_kit::test]
fn minimap_drag_continuously_updates_camera_without_moving_nodes(cx: &mut TestAppContext) {
    let (w, view) = open(cx);
    let old_graph = view.read_with(cx, |v, _| v.graph().clone());
    let before = view.read_with(cx, |v, _| v.viewport());
    let (from, to, expected) = w
        .update(cx, |_, window, _| {
            let b = window.find("flow").bounds();
            let size = WorldPoint::new(b.size.width.as_f32(), b.size.height.as_f32());
            let from = crate::paint::minimap_rect(size).center();
            let to = from + WorldPoint::new(20., 10.);
            let bounds = old_graph.bounds().expect("graph bounds");
            let world = crate::paint::minimap_world(to, size, bounds);
            (
                b.origin + point(px(from.x), px(from.y)),
                b.origin + point(px(to.x), px(to.y)),
                size * 0.5 - world * before.zoom,
            )
        })
        .expect("map");
    w.update(cx, |_, w, cx| w.drag(from, to, cx)).expect("drag");
    let after = view.read_with(cx, |v, _| v.viewport());
    assert!(
        after.pan.distance(expected) < 0.01,
        "minimap only handled press: {:?}, expected {:?}",
        after.pan,
        expected
    );
    assert_eq!(view.read_with(cx, |v, _| v.graph().clone()), old_graph);
}
/// API conventions §12: toolbar buttons are named, not tab stops outside a focus scope, and the
/// canvas is exposed as a named Canvas.
#[gpui_kit::test]
fn toolbar_and_canvas_accessibility(cx: &mut TestAppContext) {
    use gpui_kit::test::TestQueryExt;
    let (window, _) = open(cx);
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("flow").role(), Some(gpui_kit::Role::Canvas));
            assert_eq!(window.find("flow").label(), Some("Flow diagram"));
            for label in ["Zoom in", "Zoom out", "Fit to view", "Undo", "Redo"] {
                assert_eq!(
                    window.find_by_label(label).role(),
                    Some(gpui_kit::Role::Button),
                    "{label}"
                );
            }
            // No tab stops outside a focus scope: Tab never lands on a toolbar button.
            window.focus_next(cx);
            window.render_frame(cx);
            for id in [
                "flow-zoom-in",
                "flow-zoom-out",
                "flow-fit",
                "flow-undo",
                "flow-redo",
            ] {
                assert_ne!(window.find(id).focused(), Some(true), "{id} is a tab stop");
            }
        })
        .expect("update");
}

#[gpui_kit::test]
fn background_layout_previews_then_commits_one_undoable_edit(cx: &mut TestAppContext) {
    use crate::core::{AutoLayout, LayoutOptions};
    use std::time::Duration;

    let state = cx.update(|cx| {
        guiraffe_core::init(cx);
        cx.set_reduce_motion(false);
        cx.new(|cx| crate::FlowState::new(fixture(), cx).expect("state"))
    });
    let original = state.read_with(cx, |state, _| state.graph().nodes[&1].pos);
    cx.update(|cx| {
        state.update(cx, |state, cx| {
            state.arrange(
                AutoLayout::Grid { columns: 1 },
                LayoutOptions::default(),
                Duration::from_millis(64),
                cx,
            );
        })
    });
    cx.run_until_parked();
    state.read_with(cx, |state, cx| {
        assert!(state.viewport_entity().read(cx).layout_preview.is_some());
        assert_eq!(state.graph().nodes[&1].pos, original);
    });
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    cx.update(|cx| {
        state.update(cx, |state, cx| {
            assert_eq!(state.graph().nodes[&1].pos, WorldPoint::default());
            assert!(state.viewport_entity().read(cx).layout_preview.is_none());
            state.undo(cx);
            assert_eq!(state.graph().nodes[&1].pos, original);
            assert!(!state.editor.can_undo());
        })
    });
}

#[gpui_kit::test]
fn cancelling_background_layout_keeps_the_document(cx: &mut TestAppContext) {
    use crate::core::{AutoLayout, LayoutOptions};
    use std::time::Duration;

    let original = fixture();
    let state = cx.update(|cx| {
        guiraffe_core::init(cx);
        cx.new(|cx| crate::FlowState::new(original.clone(), cx).expect("state"))
    });
    cx.update(|cx| {
        state.update(cx, |state, cx| {
            state.arrange(
                AutoLayout::Grid { columns: 1 },
                LayoutOptions::default(),
                Duration::ZERO,
                cx,
            );
            state.cancel_layout(cx);
        })
    });
    cx.run_until_parked();
    state.read_with(cx, |state, _| {
        assert_eq!(state.graph(), &original);
        assert!(!state.editor.can_undo());
    });
}

/// Port anchors are where the ports are painted, at any pan and zoom (#151).
#[gpui_kit::test]
fn port_anchors_match_the_painted_ports_at_every_pan_and_zoom(cx: &mut TestAppContext) {
    let (w, view) = open(cx);
    for (pan, zoom) in [
        ((0., 0.), 1.0),
        ((20., 30.), 0.6),
        ((-140., 55.), 2.0),
        ((7.5, -3.25), 1.3),
    ] {
        view.update(cx, |view, cx| {
            view.set_viewport(
                Viewport {
                    pan: WorldPoint::new(pan.0, pan.1),
                    zoom,
                },
                cx,
            )
        })
        .expect("set viewport");
        w.update(cx, |_, window, cx| {
            window.render_frame(cx);
            let node = window.find(("flow-node", 1u64)).bounds();
            let card = view.read(cx);
            let bounds = card.node_bounds(1).expect("node bounds");
            assert!(
                (bounds.origin.x - node.origin.x).abs() < px(0.5)
                    && (bounds.origin.y - node.origin.y).abs() < px(0.5)
                    && (bounds.size.width - node.size.width).abs() < px(0.5)
                    && (bounds.size.height - node.size.height).abs() < px(0.5),
                "node bounds {bounds:?} against painted {node:?} at {pan:?} x{zoom}"
            );
            for (port, handle, side) in [
                ("in", "handle-in", crate::core::Side::Left),
                ("out", "handle-out", crate::core::Side::Right),
            ] {
                let painted = window
                    .within(("flow-node", 1u64))
                    .find(handle)
                    .bounds()
                    .center();
                let anchor = card.port_anchor(1, port).expect("port anchor");
                assert!(
                    (anchor.position.x - painted.x).abs() < px(0.5)
                        && (anchor.position.y - painted.y).abs() < px(0.5),
                    "{port} anchor {anchor:?} against painted {painted:?} at {pan:?} x{zoom}"
                );
                assert_eq!(anchor.side, side);
                assert!(anchor.visible);
            }
            assert_eq!(card.port_anchor(1, "nope"), None);
            assert_eq!(card.node_bounds(99), None);
        })
        .expect("frame");
    }
    // Panned off the canvas: the position follows, flagged not visible.
    view.update(cx, |view, cx| {
        view.set_viewport(
            Viewport {
                pan: WorldPoint::new(-5_000., 0.),
                zoom: 1.0,
            },
            cx,
        )
    })
    .expect("set viewport");
    w.update(cx, |_, window, cx| {
        window.render_frame(cx);
        let anchor = view.read(cx).port_anchor(1, "in").expect("anchor");
        assert!(!anchor.visible, "{anchor:?}");
        assert!(anchor.position.x < px(0.));
    })
    .expect("frame");
}

/// `AnchorsMoved` fires once per frame on a pan, a zoom, a node move and a layout change,
/// and not on a repaint that moves nothing (#151).
#[gpui_kit::test]
fn anchors_moved_fires_on_pan_zoom_move_and_layout_only(cx: &mut TestAppContext) {
    let (w, view) = open(cx);
    let fired = Rc::new(Cell::new(0usize));
    let _subscription = cx.update(|cx| {
        let fired = fired.clone();
        cx.subscribe(&view, move |_, _: &crate::AnchorsMoved, _| {
            fired.set(fired.get() + 1)
        })
    });
    let frame = |cx: &mut TestAppContext| {
        w.update(cx, |_, window, cx| window.render_frame(cx))
            .expect("frame");
        cx.run_until_parked();
    };
    frame(cx);
    let base = fired.get();
    let expect = |cx: &mut TestAppContext, what: &str, more: usize| {
        frame(cx);
        assert_eq!(fired.get() - base, more, "after {what}");
    };

    view.update(cx, |_, cx| cx.notify());
    expect(cx, "an unrelated repaint", 0);

    let set = |view: &Entity<Flow>, pan: (f32, f32), zoom: f32, cx: &mut TestAppContext| {
        view.update(cx, |view, cx| {
            view.set_viewport(
                Viewport {
                    pan: WorldPoint::new(pan.0, pan.1),
                    zoom,
                },
                cx,
            )
        })
        .expect("set viewport");
    };
    set(&view, (40., 10.), 1.0, cx);
    expect(cx, "a pan", 1);
    set(&view, (40., 10.), 1.5, cx);
    expect(cx, "a zoom", 2);
    view.update(cx, |view, cx| {
        view.edit(
            |graph| {
                if let Some(node) = graph.nodes.get_mut(&1) {
                    node.pos = WorldPoint::new(100., 160.);
                }
                Ok(())
            },
            cx,
        )
        .expect("move")
    });
    expect(cx, "a node move", 3);
    view.update(cx, |view, cx| {
        view.edit(
            |graph| {
                if let Some(node) = graph.nodes.get_mut(&2) {
                    node.size = WorldPoint::new(220., 90.);
                }
                Ok(())
            },
            cx,
        )
        .expect("resize")
    });
    expect(cx, "a layout change", 4);
    view.update(cx, |_, cx| cx.notify());
    expect(cx, "another unrelated repaint", 4);
}
