//! Setting the selection by id (#161): exact, a no-op when unchanged, outside undo.

use guiraffe_flow_core::{Edge, Editor, Graph, Node, Point};

fn editor() -> Editor {
    Editor::new(
        Graph::from_parts(
            [
                Node::new(1, "A", Point::new(0., 0.)),
                Node::new(2, "B", Point::new(300., 0.)),
                Node::new(3, "C", Point::new(600., 0.)),
            ],
            [Edge::new(1, 1, 2), Edge::new(2, 2, 3)],
        )
        .expect("graph"),
    )
    .expect("editor")
}

#[test]
fn sets_exactly_the_given_ids_and_ignores_unknown_ones() {
    let mut editor = editor();
    assert!(editor.set_selection([1, 3, 99], [2, 77]));
    assert_eq!(
        editor.selected_nodes().iter().copied().collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(
        editor.selected_edges().iter().copied().collect::<Vec<_>>(),
        [2]
    );
    assert!(editor.set_selection([2], []));
    assert_eq!(
        editor.selected_nodes().iter().copied().collect::<Vec<_>>(),
        [2]
    );
    assert!(editor.selected_edges().is_empty());
}

#[test]
fn the_current_selection_again_is_a_no_op() {
    let mut editor = editor();
    editor.set_selection([1], [1]);
    let (nodes, edges) = (
        editor.selected_nodes_shared(),
        editor.selected_edges_shared(),
    );
    assert!(!editor.set_selection([1, 99], [1]));
    assert!(std::sync::Arc::ptr_eq(
        &nodes,
        &editor.selected_nodes_shared()
    ));
    assert!(std::sync::Arc::ptr_eq(
        &edges,
        &editor.selected_edges_shared()
    ));
}

#[test]
fn selection_never_enters_undo_history() {
    let mut editor = editor();
    let depth = editor.undo_depth();
    editor.set_selection([1, 2], [1]);
    editor.set_selection([3], []);
    assert_eq!(editor.undo_depth(), depth);
    assert!(!editor.can_undo());
}
