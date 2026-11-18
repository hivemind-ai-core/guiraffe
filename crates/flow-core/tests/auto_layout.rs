use guiraffe_flow_core::{
    AutoLayout, Edge, Editor, Graph, LayoutCancellation, LayoutConstraint, LayoutDirection,
    LayoutEngine, LayoutError, LayoutOptions, LayoutResult, Node, Point,
};

fn graph(n: u64, edges: &[(u64, u64)]) -> Graph {
    Graph::from_parts(
        (0..n).map(|id| {
            let mut node = Node::new(id, "", Point::default());
            node.size = Point::new(17. + (id % 7) as f32, 11. + (id % 5) as f32);
            node
        }),
        edges
            .iter()
            .enumerate()
            .map(|(id, &(a, b))| Edge::new(id as u64 + 1, a, b)),
    )
    .expect("valid fixture")
}

fn assert_spaced(graph: &Graph, result: &LayoutResult, gap: Point) {
    let placed: Vec<_> = graph
        .nodes
        .values()
        .map(|n| (result.positions[&n.id], n.size))
        .collect();
    for (i, &(a, size_a)) in placed.iter().enumerate() {
        for &(b, size_b) in &placed[..i] {
            assert!(
                a.x + size_a.x + gap.x <= b.x + 0.001
                    || b.x + size_b.x + gap.x <= a.x + 0.001
                    || a.y + size_a.y + gap.y <= b.y + 0.001
                    || b.y + size_b.y + gap.y <= a.y + 0.001,
                "{a:?} and {b:?} overlap"
            );
        }
    }
}

#[test]
fn generated_graphs_are_deterministic_spaced_and_orthogonal() {
    let cancel = LayoutCancellation::default();
    let mut seed: u64 = 81923;
    for n in 3..30 {
        let mut edges = Vec::new();
        for a in 0..n {
            for b in 0..n {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                if a != b && seed >> 60 == 0 {
                    edges.push((a, b));
                }
            }
        }
        let graph = graph(n, &edges);
        for engine in [
            AutoLayout::Layered,
            AutoLayout::Tree,
            AutoLayout::Radial,
            AutoLayout::Grid { columns: 5 },
            AutoLayout::Force {
                seed,
                iterations: 6,
            },
        ] {
            let options = LayoutOptions::default();
            let result = engine
                .layout(&graph, &options, &cancel)
                .expect("generated layout");
            assert_eq!(
                result,
                engine
                    .layout(&graph, &options, &cancel)
                    .expect("repeated layout")
            );
            assert_spaced(&graph, &result, options.spacing);
            for route in result.routes.values() {
                assert!(
                    route
                        .windows(2)
                        .all(|p| p[0].x == p[1].x || p[0].y == p[1].y)
                );
            }
        }
    }
}

#[test]
fn constraints_groups_and_incremental_freeze() {
    let mut graph = graph(5, &[(0, 2), (2, 3)]);
    graph.nodes.get_mut(&1).expect("child").parent = Some(0);
    graph.nodes.get_mut(&1).expect("child").pos = Point::new(2., 3.);
    graph.nodes.get_mut(&4).expect("unaffected").pos = Point::new(2010., 980.);
    let options = LayoutOptions {
        pinned: [1].into(),
        affected: Some([0, 2, 3].into()),
        constraints: vec![
            LayoutConstraint::LeftOf(1, 2),
            LayoutConstraint::SameRank(2, 3),
            LayoutConstraint::Above(2, 3),
        ],
        ..LayoutOptions::default()
    };
    let result = AutoLayout::Layered
        .layout(&graph, &options, &LayoutCancellation::default())
        .expect("feasible");
    for id in [0, 1, 4] {
        assert_eq!(result.positions[&id], graph.nodes[&id].pos);
    }
    let p = &result.positions;
    assert!(p[&2].x >= p[&1].x + graph.nodes[&1].size.x + options.spacing.x);
    assert_eq!(p[&2].x, p[&3].x);
    assert!(p[&3].y >= p[&2].y + graph.nodes[&2].size.y + options.spacing.y);
}

#[test]
fn contradictory_constraint_and_cancellation_fail() {
    let graph = graph(2, &[]);
    let cancel = LayoutCancellation::default();
    let options = LayoutOptions {
        constraints: vec![
            LayoutConstraint::LeftOf(0, 1),
            LayoutConstraint::LeftOf(1, 0),
        ],
        ..LayoutOptions::default()
    };
    assert_eq!(
        AutoLayout::Layered.layout(&graph, &options, &cancel),
        Err(LayoutError::Infeasible)
    );
    cancel.cancel();
    assert_eq!(
        AutoLayout::Layered.layout(&graph, &options, &cancel),
        Err(LayoutError::Cancelled)
    );
}

#[test]
fn crossed_matching_regression_in_every_direction() {
    let graph = graph(4, &[(0, 3), (1, 2)]);
    let cancel = LayoutCancellation::default();
    for direction in [
        LayoutDirection::LeftToRight,
        LayoutDirection::RightToLeft,
        LayoutDirection::TopToBottom,
        LayoutDirection::BottomToTop,
    ] {
        let horizontal = matches!(
            direction,
            LayoutDirection::LeftToRight | LayoutDirection::RightToLeft
        );
        let cross = |p: Point| if horizontal { p.y } else { p.x };
        let count = |result: &LayoutResult| {
            let p = &result.positions;
            usize::from((cross(p[&0]) < cross(p[&1])) != (cross(p[&3]) < cross(p[&2])))
        };
        let mut options = LayoutOptions {
            direction,
            crossing_weight: 0.,
            ..LayoutOptions::default()
        };
        let baseline = AutoLayout::Layered
            .layout(&graph, &options, &cancel)
            .expect("baseline");
        options.crossing_weight = 1.;
        let reduced = AutoLayout::Layered
            .layout(&graph, &options, &cancel)
            .expect("reduced crossings");
        assert_eq!(count(&baseline), 1);
        assert_eq!(count(&reduced), 0);
    }
}

#[test]
fn prepared_routes_match_scene_and_commit_once() {
    let graph = graph(3, &[(0, 1), (1, 2)]);
    let mut editor = Editor::new(graph.clone()).expect("editor");
    let layout = AutoLayout::Layered
        .layout(
            &graph,
            &LayoutOptions::default(),
            &LayoutCancellation::default(),
        )
        .expect("layout");
    let stale = editor.prepare_layout(&layout).expect("prepared");
    let fresh = editor.prepare_layout(&layout).expect("prepared");
    editor.select(&[1].into(), false);
    assert!(editor.install_prepared_layout(fresh));
    assert_eq!(editor.scene().paths()[&1].points, layout.routes[&1]);
    assert!(editor.selected_nodes().contains(&1));
    assert!(!editor.install_prepared_layout(stale));
    assert!(editor.undo().expect("undo"));
    assert_eq!(editor.graph().nodes, graph.nodes);
    assert_eq!(editor.graph().edges, graph.edges);
    assert!(!editor.can_undo());
}

#[test]
fn generated_feasible_relative_constraints_are_satisfied() {
    let cancel = LayoutCancellation::default();
    for n in 3..32 {
        let graph = graph(n, &[]);
        for direction in [LayoutDirection::LeftToRight, LayoutDirection::BottomToTop] {
            let horizontal = direction == LayoutDirection::LeftToRight;
            let mut options = LayoutOptions {
                direction,
                pinned: [0].into(),
                ..LayoutOptions::default()
            };
            for id in 1..n {
                options.constraints.push(if horizontal {
                    LayoutConstraint::LeftOf(id - 1, id)
                } else {
                    LayoutConstraint::Above(id - 1, id)
                });
                options.constraints.push(if horizontal {
                    LayoutConstraint::AlignY(0, id)
                } else {
                    LayoutConstraint::AlignX(0, id)
                });
            }
            let result = AutoLayout::Grid { columns: 5 }
                .layout(&graph, &options, &cancel)
                .expect("feasible chain");
            assert_eq!(result.positions[&0], Point::default());
            assert_spaced(&graph, &result, options.spacing);
            for id in 1..n {
                let a = result.positions[&(id - 1)];
                let b = result.positions[&id];
                let gap = graph.nodes[&(id - 1)].size + options.spacing;
                if horizontal {
                    assert!(b.x >= a.x + gap.x);
                    assert_eq!(b.y, a.y);
                } else {
                    assert!(b.y >= a.y + gap.y);
                    assert_eq!(b.x, a.x);
                }
            }
        }
    }
}
