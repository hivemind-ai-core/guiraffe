use guiraffe_flow_core::*;
use std::collections::{BTreeMap, BTreeSet};
fn graph() -> Graph {
    Graph::from_parts(
        [
            Node::new(1, "A", Point::default()),
            Node::new(2, "B", Point::new(400., 200.)),
            Node::new(3, "C", Point::new(800., 400.)),
        ],
        [Edge::new(1, 1, 2)],
    )
    .expect("fixture")
}
#[test]
fn serialization_roundtrip() {
    let g = graph();
    let json = serde_json::to_string(&g).expect("serialize");
    let result: Graph = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(g, result);
    result.validate().expect("valid");
}
#[test]
fn duplicate_ids_and_handles_rejected() {
    let n = Node::new(1, "", Point::default());
    assert_eq!(
        Graph::from_parts([n.clone(), n.clone()], []),
        Err(FlowError::DuplicateId)
    );
    let mut g = graph();
    g.nodes
        .get_mut(&1)
        .expect("node")
        .handles
        .push(n.handles[0].clone());
    assert_eq!(g.validate(), Err(FlowError::DuplicateId));
}
#[test]
fn invalid_geometry_rejected_without_mutation() {
    let mut e = Editor::new(graph()).expect("editor");
    for p in [Point::new(f32::NAN, 0.), Point::new(0., f32::INFINITY)] {
        assert_eq!(
            e.move_nodes(&BTreeMap::from([(1, p)]), None),
            Err(FlowError::InvalidGeometry)
        );
        assert_eq!(e.graph(), &graph());
        assert!(!e.can_undo());
    }
}
#[test]
fn parent_cycles_and_missing_parent_rejected() {
    let mut g = graph();
    g.nodes.get_mut(&1).expect("node").parent = Some(2);
    g.nodes.get_mut(&2).expect("node").parent = Some(1);
    assert_eq!(g.validate(), Err(FlowError::ParentCycle));
    g.nodes.get_mut(&2).expect("node").parent = Some(99);
    assert_eq!(g.validate(), Err(FlowError::MissingNode(99)));
}
#[test]
fn endpoint_validation_checks_direction_and_identity() {
    let mut g = graph();
    g.edges.get_mut(&1).expect("edge").source.handle = "in".into();
    assert_eq!(g.validate(), Err(FlowError::WrongHandleDirection));
    g.edges.get_mut(&1).expect("edge").source.handle = "missing".into();
    assert!(matches!(g.validate(), Err(FlowError::MissingHandle(_))));
}
#[test]
fn connection_policy_rejects_self_parallel_and_cycle() {
    let g = graph();
    let source = Endpoint::new(1, "out");
    assert_eq!(
        g.validate_connection(
            &source,
            &Endpoint::new(1, "in"),
            ConnectionPolicy::default()
        ),
        Err(FlowError::SelfConnection)
    );
    assert_eq!(
        g.validate_connection(
            &source,
            &Endpoint::new(2, "in"),
            ConnectionPolicy::default()
        ),
        Err(FlowError::DuplicateConnection)
    );
    assert_eq!(
        g.validate_connection(
            &Endpoint::new(2, "out"),
            &Endpoint::new(1, "in"),
            ConnectionPolicy {
                allow_cycles: false,
                ..Default::default()
            }
        ),
        Err(FlowError::Cycle)
    );
}
#[test]
fn custom_validator_is_atomic() {
    let mut e = Editor::new(graph()).expect("editor");
    let reject = |_: &Graph, _: &Endpoint, _: &Endpoint| Err(FlowError::Rejected("type".into()));
    assert!(e.connect(Edge::new(2, 2, 3), Some(&reject)).is_err());
    assert_eq!(e.graph(), &graph());
    assert!(!e.can_undo());
}
#[test]
fn history_roundtrip_and_redo_invalidation() {
    let mut e = Editor::new(graph()).expect("editor");
    let positions = BTreeMap::from([(1, Point::new(23., 37.))]);
    e.move_nodes(&positions, Some(10.)).expect("move");
    assert_eq!(e.graph().nodes[&1].pos, Point::new(20., 40.));
    e.undo().expect("undo");
    assert_eq!(e.graph(), &graph());
    e.redo().expect("redo");
    e.undo().expect("undo");
    e.add_node(Node::new(4, "D", Point::default()))
        .expect("add");
    assert!(!e.can_redo());
}
#[test]
fn failed_edit_preserves_redo() {
    let mut e = Editor::new(graph()).expect("editor");
    e.add_node(Node::new(4, "D", Point::default()))
        .expect("add");
    e.undo().expect("undo");
    assert!(e.add_node(e.graph().nodes[&1].clone()).is_err());
    assert!(e.can_redo());
}
#[test]
fn history_is_bounded_and_noops_are_free() {
    let mut e = Editor::new(graph()).expect("editor");
    e.history_limit(1);
    assert!(!e.edit(|_| Ok(())).expect("noop"));
    e.add_node(Node::new(4, "", Point::default())).expect("add");
    e.add_node(Node::new(5, "", Point::default())).expect("add");
    e.undo().expect("undo");
    assert!(!e.can_undo());
    e.history_limit(0);
    assert!(!e.can_redo());
}
#[test]
fn selection_box_toggle_and_delete_descendants() {
    let mut g = graph();
    g.nodes.get_mut(&3).expect("node").parent = Some(2);
    let mut e = Editor::new(g).expect("editor");
    e.select_box(Rect::new(400., 200., 1., 1.), false);
    assert!(e.is_selected(2));
    assert!(!e.can_undo());
    e.toggle(1);
    assert!(e.is_selected(1));
    e.toggle(1);
    e.delete_selected().expect("delete");
    assert_eq!(e.graph().nodes.len(), 1);
    assert!(e.graph().edges.is_empty());
    e.undo().expect("undo");
    assert_eq!(e.graph().nodes.len(), 3);
}
#[test]
fn nondraggable_nodes_stay_put() {
    let mut g = graph();
    g.nodes.get_mut(&1).expect("node").draggable = false;
    let mut e = Editor::new(g).expect("editor");
    assert!(
        !e.move_nodes(&BTreeMap::from([(1, Point::new(9., 9.))]), None)
            .expect("move")
    );
}
#[test]
fn viewport_roundtrip_and_anchor_invariance() {
    let mut v = Viewport {
        pan: Point::new(12., -30.),
        ..Default::default()
    };
    let p = Point::new(123., 456.);
    v.zoom_at(p, 2.);
    let world = v.world(p);
    v.zoom_at(p, 1.5);
    assert!(v.world(p).distance(world) < 0.001);
    assert!(v.world(v.screen(world)).distance(world) < 0.001);
    v.zoom_at(p, f32::NAN);
    assert!(v.valid());
    v.zoom_at(p, 10000.);
    assert_eq!(v.zoom, Viewport::MAX_ZOOM);
}
#[test]
fn fit_empty_degenerate_and_invalid_camera_input() {
    let mut v = Viewport::default();
    v.fit(
        Rect::new(-10., -20., 100., 100.),
        Point::new(800., 600.),
        20.,
    );
    assert!(
        v.screen(Point::new(40., 30.))
            .distance(Point::new(400., 300.))
            < 0.001
    );
    let old = v;
    v.fit(Rect::new(0., 0., 1., 1.), Point::new(1., 1.), 30.);
    assert_eq!(v, old);
    v.fit(Rect::default(), Point::new(800., 600.), 20.);
    assert!(v.valid());
}
#[test]
fn spatial_index_matches_bruteforce_and_reuses_buffer() {
    let entries: Vec<_> = (0..5000)
        .map(|i| {
            (
                i,
                Rect::new((i % 100) as f32 * 100., (i / 100) as f32 * 100., 50., 50.),
            )
        })
        .collect();
    let index = SpatialIndex::new(entries.clone());
    let mut result = Vec::with_capacity(5000);
    for i in 0..100 {
        let area = Rect::new(i as f32 * 37., i as f32 * 11., 700., 500.);
        let visits = index.query(area, &mut result);
        let expected: Vec<_> = entries
            .iter()
            .filter(|(_, r)| r.intersects(area))
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(result, expected);
        assert!(visits < 250);
        assert_eq!(result.capacity(), 5000);
    }
}
#[test]
fn spatial_invalid_and_empty_queries_are_empty() {
    let index = SpatialIndex::new([(1, Rect::new(f32::NAN, 0., 1., 1.))]);
    let mut result = vec![99];
    assert_eq!(index.query(Rect::default(), &mut result), 0);
    assert!(result.is_empty());
}
#[test]
fn offscreen_endpoints_keep_crossing_edge() {
    let g = Graph::from_parts(
        [
            Node::new(1, "", Point::new(-500., 0.)),
            Node::new(2, "", Point::new(1000., 0.)),
        ],
        [Edge {
            edge_type: EdgeType::Straight,
            ..Edge::new(1, 1, 2)
        }],
    )
    .expect("graph");
    let scene = Scene::new(g).expect("scene");
    let mut visible = Visible::default();
    scene.visible(Rect::new(0., 0., 500., 100.), &mut visible);
    assert!(visible.nodes.is_empty());
    assert_eq!(visible.edges, vec![1]);
}
#[test]
fn every_path_bounds_contains_every_sample_and_label() {
    let mut g = graph();
    for kind in [
        EdgeType::Bezier,
        EdgeType::SmoothStep,
        EdgeType::Step,
        EdgeType::Straight,
    ] {
        g.edges.get_mut(&1).expect("edge").edge_type = kind;
        let scene = Scene::new(g.clone()).expect("scene");
        let path = &scene.paths()[&1];
        assert!(path.points.iter().all(|p| path.bounds.contains(*p)));
        assert!(path.bounds.contains(path.label));
        assert!(path.distance(path.points[0]) < 0.001);
    }
}
#[test]
fn backwards_and_self_routes_have_finite_nonempty_geometry() {
    let mut g = graph();
    g.nodes.get_mut(&2).expect("node").pos = Point::new(-400., 0.);
    for target in [1, 2] {
        g.edges.get_mut(&1).expect("edge").target.node = target;
        let p = EdgePath::new(&g, &g.edges[&1], None, None).expect("route");
        assert!(p.bounds.valid());
        assert!(p.bounds.size.x > 0.);
    }
}
#[test]
fn hit_testing_uses_topmost_id_and_actual_stroke() {
    let mut g = graph();
    g.nodes.get_mut(&2).expect("node").pos = Point::default();
    let scene = Scene::new(g).expect("scene");
    let mut scratch = Vec::new();
    assert_eq!(scene.hit_node(Point::new(10., 10.), &mut scratch), Some(2));
    assert_eq!(scene.hit_node(Point::new(-100., -100.), &mut scratch), None);
    let p = scene.paths()[&1].points[10];
    assert_eq!(scene.hit_edge(p, 2., &mut scratch), Some(1));
    assert_eq!(scene.hit_edge(p, -1., &mut scratch), None);
}
#[test]
fn layered_layout_is_deterministic_cyclic_and_disconnected_safe() {
    for mut g in [Graph::default(), graph()] {
        if !g.nodes.is_empty() {
            g.edges.insert(2, Edge::new(2, 2, 1));
        }
        let l = LayeredLayout::default();
        let a = l.positions(&g);
        assert_eq!(a, l.positions(&g));
        assert_eq!(a.len(), g.nodes.len());
        assert!(a.values().all(|p| p.finite()));
    }
    let g = graph();
    let p = LayeredLayout::default().positions(&g);
    assert!(p[&2].x > p[&1].x);
}
#[test]
fn additive_selection_preserves_others() {
    let mut e = Editor::new(graph()).expect("editor");
    e.select(&BTreeSet::from([1]), false);
    e.select(&BTreeSet::from([2]), true);
    assert!(e.is_selected(1) && e.is_selected(2));
}
#[test]
fn overflowing_aggregate_bounds_are_rejected() {
    let nodes = [
        Node::new(1, "", Point::new(-3e38, 0.)),
        Node::new(2, "", Point::new(3e38, 0.)),
    ];
    assert_eq!(
        Graph::from_parts(nodes, []),
        Err(FlowError::InvalidGeometry)
    );
}
#[test]
fn shrinking_history_limit_bounds_redo_as_well_as_undo() {
    let mut e = Editor::new(graph()).expect("editor");
    for id in 4..7 {
        e.add_node(Node::new(id, "", Point::default()))
            .expect("add");
    }
    for _ in 0..3 {
        e.undo().expect("undo");
    }
    e.history_limit(1);
    assert!(e.redo().expect("redo"));
    assert!(!e.can_redo());
    assert_eq!(e.graph().nodes.len(), 4);
}
