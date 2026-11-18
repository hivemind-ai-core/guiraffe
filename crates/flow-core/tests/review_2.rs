use guiraffe_flow_core::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
fn graph() -> Graph {
    Graph::from_parts(
        (0..256).map(|id| Node::new(id, "", Point::new(id as f32 * 200., 0.))),
        [],
    )
    .expect("graph")
}
#[test]
fn review2_4_missing_parent_layout_is_rejected() {
    let mut g = graph();
    g.nodes.get_mut(&1).expect("node").parent = Some(999);
    assert!(LayeredLayout::default().positions(&g).is_empty());
}
#[test]
fn review2_4_parent_cycle_layout_is_rejected() {
    // The former implementation loops: run this only after the missing-parent
    // regression has established the missing validation boundary.
    let mut g = graph();
    g.nodes.get_mut(&1).expect("node").parent = Some(2);
    g.nodes.get_mut(&2).expect("node").parent = Some(1);
    assert!(LayeredLayout::default().positions(&g).is_empty());
}
#[test]
fn review2_8_selection_only_edit_has_no_history_or_scene_change() {
    let mut e = Editor::new(graph()).expect("editor");
    let scene = e.scene_shared();
    assert!(
        !e.edit(|g| {
            g.nodes.get_mut(&1).expect("node").selected = true;
            Ok(())
        })
        .expect("edit")
    );
    assert!(!e.can_undo());
    assert!(Arc::ptr_eq(&scene, &e.scene_shared()));
}
#[test]
fn review2_6_movement_rebuild_limits_query_degradation() {
    let mut index =
        SpatialIndex::new((0..2048).map(|id| (id, Rect::new(id as f32 * 10., 0., 5., 5.))));
    // Interleave both halves spatially: the old grouping makes each branch span
    // most of the graph despite a small final point query.
    for round in 0..4 {
        let changes = (0..2048)
            .map(|id| {
                (
                    id,
                    Some(Rect::new(
                        ((id * 997 + round) % 2048) as f32 * 10.,
                        0.,
                        5.,
                        5.,
                    )),
                )
            })
            .collect();
        index.update(&changes);
    }
    let mut ids = Vec::new();
    let visits = index.query(Rect::new(10000., 0., 1., 1.), &mut ids);
    assert!(visits < 100, "degraded BVH visited {visits} branches");
    assert_eq!(ids.len(), 1);
}
#[test]
fn review2_layout_root_groups_do_not_overlap() {
    let mut g = Graph::from_parts(
        [
            Node::new(1, "", Point::default()),
            Node::new(2, "", Point::default()),
            Node::new(3, "", Point::new(-100., -100.)),
        ],
        [],
    )
    .expect("graph");
    g.nodes.get_mut(&3).expect("child").parent = Some(1);
    g.nodes.get_mut(&2).expect("pinned").draggable = false;
    let mut e = Editor::new(g).expect("editor");
    e.arrange(&LayeredLayout::default()).expect("arrange");
    let a = e.graph().nodes[&1].rect().union(e.graph().nodes[&3].rect());
    assert!(!a.intersects(e.graph().nodes[&2].rect()));
}
#[test]
fn review2_9_selection_stays_outside_serialized_document() {
    let mut e = Editor::new(graph()).expect("editor");
    e.select(&BTreeSet::from([1]), false);
    let decoded: Graph =
        serde_json::from_str(&serde_json::to_string(e.graph()).expect("serialize"))
            .expect("decode");
    assert!(
        Editor::new(decoded)
            .expect("editor")
            .selected_nodes()
            .is_empty()
    );
}
#[test]
fn review2_10_rejected_arbitrary_edit_preserves_history_and_routes() {
    let mut e = Editor::new(graph()).expect("editor");
    let before = e.scene_shared();
    assert!(
        e.edit(|g| {
            g.nodes.get_mut(&1).expect("node").parent = Some(999);
            Ok(())
        })
        .is_err()
    );
    assert!(Arc::ptr_eq(&before, &e.scene_shared()));
    assert!(!e.can_undo());
    assert!(
        e.move_nodes(&BTreeMap::from([(1, Point::new(300., 0.))]), None)
            .expect("move")
    );
}

proptest::proptest! {
    #[test]
    fn editor_operation_sequences_match_reference_history(
        operations in proptest::collection::vec((0_u8..4, 0_u64..16, -40_i16..40), 0..80),
    ) {
        let mut editor = Editor::new(Graph::from_parts([], []).expect("empty graph")).expect("editor");
        let mut positions = BTreeMap::new();
        let mut undo = Vec::new();
        let mut redo = Vec::new();
        for (op, id, x) in operations {
            match op {
                0 | 1 => {
                    let before = positions.clone();
                    let point = Point::new(f32::from(x), 0.);
                    if op == 0 {
                        positions.insert(id, point);
                    } else {
                        positions.remove(&id);
                    }
                    let edited = editor.edit(|graph| {
                        if op == 0 {
                            graph.nodes.insert(id, Node::new(id, "", point));
                        } else {
                            graph.nodes.remove(&id);
                        }
                        Ok(())
                    }).expect("edit");
                    proptest::prop_assert_eq!(edited, before != positions);
                    if edited {
                        undo.push(before);
                        redo.clear();
                    }
                }
                2 => {
                    let expected = undo.pop();
                    proptest::prop_assert_eq!(editor.undo().expect("undo"), expected.is_some());
                    if let Some(previous) = expected {
                        redo.push(std::mem::replace(&mut positions, previous));
                    }
                }
                _ => {
                    let expected = redo.pop();
                    proptest::prop_assert_eq!(editor.redo().expect("redo"), expected.is_some());
                    if let Some(next) = expected {
                        undo.push(std::mem::replace(&mut positions, next));
                    }
                }
            }
            let actual: BTreeMap<_, _> = editor.graph().nodes.values().map(|n| (n.id, n.pos)).collect();
            proptest::prop_assert_eq!(&actual, &positions);
        }
    }

    #[test]
    fn spatial_updates_match_linear_scan(
        changes in proptest::collection::vec((0_u64..32, proptest::option::of(-50_i16..50), -50_i16..50), 0..100),
    ) {
        let mut index = SpatialIndex::new([]);
        let mut reference = BTreeMap::new();
        let mut actual = Vec::new();
        for (id, x, y) in changes {
            let area = Rect::new(-10., f32::from(y), 20., 15.);
            let rect = x.map(|x| Rect::new(f32::from(x), f32::from(y), 5., 5.));
            index.update(&BTreeMap::from([(id, rect)]));
            if let Some(rect) = rect {
                reference.insert(id, rect);
            } else {
                reference.remove(&id);
            }
            actual.clear();
            index.query(area, &mut actual);
            actual.sort_unstable();
            let expected: Vec<_> = reference.iter()
                .filter_map(|(id, bounds)| bounds.intersects(area).then_some(*id))
                .collect();
            proptest::prop_assert_eq!(&actual, &expected);
        }
    }
}

#[test]
fn coarse_node_regions_keep_coverage_with_fewer_primitives() {
    let scene = Scene::new(graph()).expect("scene");
    let area = scene.bounds().expect("bounds");
    let mut regions = Vec::new();
    scene.visit_node_regions(area, 3_000., |region| regions.push(region));
    assert!(regions.len() < 32);
    for node in scene.graph().nodes.values() {
        assert!(
            regions
                .iter()
                .any(|region| region.contains(node.rect().center()))
        );
    }
}

#[test]
fn default_node_payload_fits_document_storage_budget() {
    let node = Node::new(1, "", Point::new(0., 0.));
    let bytes = size_of::<Node>()
        + node.node_type.capacity()
        + node.handles.capacity() * size_of::<Handle>()
        + node
            .handles
            .iter()
            .map(|handle| handle.id.capacity())
            .sum::<usize>();
    assert!(bytes <= 256, "default node payload uses {bytes} bytes");
}
