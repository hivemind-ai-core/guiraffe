use guiraffe_flow_core::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
fn graph() -> Graph {
    let mut child = Node::new(1, "child", Point::new(37., 41.));
    child.parent = Some(9);
    let mut parent = Node::new(9, "parent", Point::new(13., 17.));
    parent.size = Point::new(300., 200.);
    Graph::from_parts(
        [
            child,
            parent,
            Node::new(2, "other", Point::new(600., 0.)),
            Node::new(3, "far", Point::new(1000., 0.)),
        ],
        [Edge::new(1, 1, 2), Edge::new(2, 2, 3)],
    )
    .expect("graph")
}
#[test]
fn review_2_parent_drag_preserves_descendant_offset() {
    let mut e = Editor::new(graph()).expect("editor");
    e.move_nodes(&BTreeMap::from([(9, Point::new(33., 37.))]), None)
        .expect("move");
    assert_eq!(e.graph().nodes[&1].pos, Point::new(57., 61.));
}
#[test]
fn review_3_child_with_lower_id_is_above_parent() {
    let e = Editor::new(graph()).expect("editor");
    assert_eq!(
        e.scene().hit_node(Point::new(50., 50.), &mut Vec::new()),
        Some(1)
    );
}
#[test]
fn review_4_undo_does_not_restore_selection() {
    let mut e = Editor::new(graph()).expect("editor");
    e.select(&BTreeSet::from([1]), false);
    e.move_nodes(&BTreeMap::from([(2, Point::new(650., 0.))]), None)
        .expect("move");
    e.select(&BTreeSet::from([2]), false);
    e.undo().expect("undo");
    assert!(!e.is_selected(1));
    assert!(e.is_selected(2));
}
#[test]
fn review_6_arrange_moves_pinned_nodes_and_preserves_subtrees() {
    let mut g = graph();
    g.nodes.get_mut(&2).expect("node").draggable = false;
    let mut e = Editor::new(g).expect("editor");
    let offset = e.graph().nodes[&1].pos - e.graph().nodes[&9].pos;
    e.arrange(&LayeredLayout::default()).expect("arrange");
    assert_ne!(e.graph().nodes[&2].pos, Point::new(600., 0.));
    assert_eq!(e.graph().nodes[&1].pos - e.graph().nodes[&9].pos, offset);
}
#[test]
fn review_9_selection_keeps_shared_scene_and_routes() {
    let mut e = Editor::new(graph()).expect("editor");
    let scene = e.scene_shared();
    e.select(&BTreeSet::new(), false);
    assert!(
        Arc::ptr_eq(&scene, &e.scene_shared()),
        "empty selection cloned scene"
    );
    e.select(&BTreeSet::from([1]), false);
    assert!(
        Arc::ptr_eq(&scene, &e.scene_shared()),
        "selection cloned scene"
    );
}
#[test]
fn review_11_unaffected_route_storage_survives_move() {
    let mut e = Editor::new(graph()).expect("editor");
    let scene = e.scene_shared();
    e.move_nodes(&BTreeMap::from([(1, Point::new(50., 60.))]), None)
        .expect("move");
    assert_eq!(
        scene.paths()[&2].points.as_ptr(),
        e.scene().paths()[&2].points.as_ptr()
    );
}
#[test]
fn review_15_edge_ids_not_reused_after_delete_undo_or_serde() {
    let mut e = Editor::new(graph()).expect("editor");
    e.connect_endpoints(Endpoint::new(9, "out"), Endpoint::new(3, "in"), None)
        .expect("connect");
    let first = *e.graph().edges.keys().next_back().expect("id");
    e.undo().expect("undo");
    let serialized = serde_json::to_string(e.graph()).expect("serialize");
    let mut e =
        Editor::new(serde_json::from_str(&serialized).expect("deserialize")).expect("editor");
    e.connect_endpoints(Endpoint::new(9, "out"), Endpoint::new(3, "in"), None)
        .expect("connect");
    assert!(*e.graph().edges.keys().next_back().expect("id") > first);
}
#[test]
fn review_16_delete_undo_restores_routes_and_index_and_empty_delete_is_free() {
    let mut e = Editor::new(graph()).expect("editor");
    assert!(!e.delete_selected().expect("empty delete"));
    assert!(!e.can_undo());
    e.select(&BTreeSet::from([9]), false);
    e.delete_selected().expect("delete");
    e.undo().expect("undo");
    let fresh = Scene::new(e.graph().clone()).expect("fresh");
    assert_eq!(e.scene().paths(), fresh.paths());
    for area in [
        Rect::new(0., 0., 2000., 1000.),
        Rect::new(40., 40., 10., 10.),
    ] {
        let (mut a, mut b) = (Visible::default(), Visible::default());
        e.scene().visible(area, &mut a);
        fresh.visible(area, &mut b);
        assert_eq!(a.nodes, b.nodes);
        assert_eq!(a.edges, b.edges);
    }
}

#[test]
fn review_11_incremental_index_matches_fresh_after_mixed_edits_and_history() {
    let nodes = (0..100).map(|id| {
        Node::new(
            id,
            "",
            Point::new((id % 10) as f32 * 220., (id / 10) as f32 * 120.),
        )
    });
    let edges = (0..99).map(|id| Edge::new(id, id, id + 1));
    let mut e = Editor::new(Graph::from_parts(nodes, edges).expect("graph")).expect("editor");
    for step in 0..40 {
        let id = step % 20;
        e.move_nodes(
            &BTreeMap::from([(id, Point::new(step as f32 * 37., step as f32 * 13.))]),
            None,
        )
        .expect("move");
        if step % 5 == 0 {
            e.select(&BTreeSet::from([id]), false);
            e.delete_selected().expect("delete");
            e.undo().expect("undo");
            e.redo().expect("redo");
            e.undo().expect("restore");
        }
        let fresh = Scene::new(e.graph().clone()).expect("fresh");
        assert_eq!(e.scene().paths(), fresh.paths());
        assert_eq!(e.scene().bounds(), fresh.bounds());
        for x in 0..10 {
            let area = Rect::new(x as f32 * 190., step as f32 * 10., 300., 400.);
            let (mut a, mut b) = (Visible::default(), Visible::default());
            e.scene().visible(area, &mut a);
            fresh.visible(area, &mut b);
            assert_eq!(a.nodes, b.nodes);
            assert_eq!(a.edges, b.edges);
        }
    }
}
#[test]
fn review_11_spatial_insert_remove_refit_preserves_old_snapshot() {
    let mut index =
        SpatialIndex::new((0..80).map(|id| (id, Rect::new(id as f32 * 30., 0., 20., 20.))));
    let original = index.clone();
    for id in 80..200 {
        index.update(&BTreeMap::from([(
            id,
            Some(Rect::new((id % 20) as f32 * 30., id as f32, 20., 20.)),
        )]));
    }
    for id in 0..30 {
        index.update(&BTreeMap::from([(id, None)]));
    }
    for id in 30..80 {
        index.update(&BTreeMap::from([(
            id,
            Some(Rect::new(-100., id as f32, 20., 20.)),
        )]));
    }
    let mut ids = Vec::new();
    index.query(Rect::new(-200., -100., 10000., 10000.), &mut ids);
    assert_eq!(ids, (30..200).collect::<Vec<_>>());
    original.query(Rect::new(-200., -100., 10000., 10000.), &mut ids);
    assert_eq!(ids, (0..80).collect::<Vec<_>>());
    index.query(Rect::new(-100., 30., 20., 50.), &mut ids);
    assert_eq!(ids, (30..80).collect::<Vec<_>>());
}
