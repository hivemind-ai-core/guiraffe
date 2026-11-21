use guiraffe_flow_core::{
    Edge, Editor, Graph, LayoutCancellation, Node, Point, Rect, Scene, Visible,
};

fn crossing_graph() -> Graph {
    Graph::from_parts(
        [
            Node::new(1, "", Point::new(0., 0.)),
            Node::new(2, "", Point::new(2_000., 0.)),
        ],
        [Edge::new(9, 1, 2)],
    )
    .expect("crossing graph")
}

#[test]
fn local_preview_publishes_crossing_edges_after_indexing() {
    let graph = crossing_graph();
    let expected = Scene::new(graph.clone()).expect("eager scene");
    let scene = Scene::new_deferred(graph).expect("deferred scene");
    assert!(scene.paths().is_empty());
    assert!(scene.index_pending());
    assert_eq!(scene.edge_route(9), expected.edge_route(9));

    let area = Rect::new(900., 0., 200., 100.);
    let mut visible = Visible::default();
    scene.visible(area, &mut visible);
    assert!(visible.edges.is_empty());

    let cancelled = LayoutCancellation::default();
    cancelled.cancel();
    assert!(scene.prepare_indexes(&cancelled).is_err());
    assert!(scene.index_pending());

    let active = LayoutCancellation::default();
    assert!(scene.prepare_indexes(&active).expect("edge index"));
    assert!(!scene.prepare_indexes(&active).expect("already indexed"));
    scene.visible(area, &mut visible);
    assert_eq!(visible.edges, [9]);
    let mut incident = Vec::new();
    scene.append_incident_edges([2], &mut incident);
    assert_eq!(incident, [9]);
    assert_eq!(
        scene.hit_edge(Point::new(950., 36.), 2., &mut Vec::new()),
        Some(9)
    );
    assert!(scene.paths().is_empty());
}

#[test]
fn endpoint_edit_replaces_index_and_keeps_the_previous_snapshot() {
    let mut editor = Editor::new_deferred(crossing_graph()).expect("editor");
    let previous = editor.scene_shared();
    previous
        .prepare_indexes(&LayoutCancellation::default())
        .expect("initial index");
    editor
        .edit(|graph| {
            graph.nodes.get_mut(&2).expect("endpoint").pos.y = 900.;
            Ok(())
        })
        .expect("move endpoint");
    let current = editor.scene();
    assert!(current.index_pending());
    assert!(!previous.index_pending());
    assert_ne!(current.edge_route(9), previous.edge_route(9));
    assert!(current.paths().is_empty());
}

#[test]
fn conservative_index_covers_route_samples_for_each_routing_style() {
    use guiraffe_flow_core::{EdgeType, Side};

    let sides = [Side::Top, Side::Right, Side::Bottom, Side::Left];
    for edge_type in [
        EdgeType::Bezier,
        EdgeType::Straight,
        EdgeType::Step,
        EdgeType::SmoothStep,
        EdgeType::OrthogonalHorizontal,
        EdgeType::OrthogonalVertical,
    ] {
        for from in sides {
            for to in sides {
                let mut source = Node::new(1, "", Point::new(450., -80.));
                let mut target = Node::new(2, "", Point::new(10., 180.));
                source.handles[1].side = from;
                target.handles[0].side = to;
                let edge = Edge {
                    edge_type,
                    ..Edge::new(1, 1, 2)
                };
                let graph = Graph::from_parts([source, target], [edge]).expect("valid handles");
                let scene = Scene::new_deferred(graph).expect("scene");
                scene
                    .prepare_indexes(&LayoutCancellation::default())
                    .expect("index");
                let route = scene.edge_route(1).expect("route");
                let mut visible = Visible::default();
                for point in &route.points {
                    scene.visible_candidates(Rect::new(point.x, point.y, 0., 0.), &mut visible);
                    assert_eq!(
                        visible.edges,
                        [1],
                        "{edge_type:?}, {from:?}, {to:?}, {point:?}"
                    );
                }
            }
        }
    }
}
