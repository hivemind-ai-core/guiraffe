use guiraffe_tree::{Children, Navigation, NodeId, TreeModel, TreeSource};
use proptest::prelude::*;

struct Source {
    ready: bool,
    roots: Vec<NodeId>,
}

impl TreeSource for Source {
    fn roots(&self) -> &[NodeId] {
        &self.roots
    }
    fn children(&self, node: NodeId) -> Children<'_> {
        match node {
            0 if self.ready => Children::Ready(&[1000, 1001]),
            0 => Children::Unloaded,
            _ => Children::Ready(&[]),
        }
    }
    fn label(&self, node: NodeId) -> &str {
        if node == 1001 { "Gamma" } else { "Alpha" }
    }
}

#[test]
fn asynchronous_children_preserve_selection_and_collapse_cursor() {
    let mut source = Source {
        ready: false,
        roots: vec![0, 1],
    };
    let mut model = TreeModel::new(&source);
    assert_eq!(model.set_expanded(0, true, &source), Some(0));
    assert_eq!(model.set_expanded(0, true, &source), None);
    source.ready = true;
    model.refresh(&source);
    model.select(1, false);
    model.select(1000, true);
    assert!(model.selected(1) && model.selected(1000));
    model.navigate(Navigation::Down, &source);
    assert_eq!(model.cursor(), Some(1001));
    model.set_expanded(0, false, &source);
    assert_eq!(model.cursor(), Some(0));
    model.navigate(Navigation::Right, &source);
    model.type_ahead('G', std::time::Instant::now(), &source);
    assert_eq!(model.cursor(), Some(1001));
}

proptest! {
    #[test]
    fn fractional_viewports_match_intersecting_rows(count in 0u64..500, scroll in 0u32..40000, height in 1u32..4000) {
        let source = Source { ready: false, roots: (0..count).collect() };
        let model = TreeModel::new(&source);
        let scroll = f64::from(scroll) / 4.;
        let height = f64::from(height) / 4.;
        let expected = (0..count as usize)
            .filter(|index| (*index + 1) as f64 * 20. > scroll && (*index as f64) * 20. < scroll + height)
            .collect::<Vec<_>>();
        prop_assert_eq!(model.window(scroll, height, 20.).collect::<Vec<_>>(), expected);
    }
}
struct Forest {
    roots: Vec<NodeId>,
    children: Vec<Vec<NodeId>>,
}

impl TreeSource for Forest {
    fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    fn children(&self, id: NodeId) -> Children<'_> {
        Children::Ready(&self.children[id as usize])
    }

    fn label(&self, _: NodeId) -> &str {
        "Node"
    }
}

fn visible(
    forest: &Forest,
    siblings: &[NodeId],
    expanded: &[bool],
    parent: Option<NodeId>,
    depth: usize,
    out: &mut Vec<guiraffe_tree::Row>,
) {
    for &id in siblings {
        out.push(guiraffe_tree::Row { id, depth, parent });
        if expanded[id as usize] {
            visible(
                forest,
                &forest.children[id as usize],
                expanded,
                Some(id),
                depth + 1,
                out,
            );
        }
    }
}

proptest! {
    #[test]
    fn expansion_matches_recursive_preorder(
        ancestors in prop::collection::vec(any::<usize>(), 1..64),
        operations in prop::collection::vec((any::<usize>(), any::<bool>()), 1..200),
    ) {
        let count = ancestors.len();
        let mut forest = Forest { roots: Vec::new(), children: vec![Vec::new(); count] };
        for (id, seed) in ancestors.into_iter().enumerate() {
            let parent = seed % (id + 1);
            if parent == id {
                forest.roots.push(id as NodeId);
            } else {
                forest.children[parent].push(id as NodeId);
            }
        }
        let mut model = TreeModel::new(&forest);
        let mut expanded = vec![false; count];
        for (target, open) in operations {
            let target = target % count;
            model.set_expanded(target as NodeId, open, &forest);
            expanded[target] = open;
            let mut expected = Vec::new();
            visible(&forest, forest.roots(), &expanded, None, 0, &mut expected);
            prop_assert_eq!(model.rows(), expected.as_slice());
            for (index, row) in expected.iter().enumerate() {
                let siblings = row.parent.map_or(forest.roots(), |id| forest.children[id as usize].as_slice());
                let ordinal = siblings.iter().position(|id| *id == row.id).expect("member of its sibling set") + 1;
                prop_assert_eq!(model.sibling_position(index), Some((ordinal, siblings.len())));
            }
            for (id, &open) in expanded.iter().enumerate() {
                let id = id as NodeId;
                prop_assert_eq!(model.expanded(id), open);
                prop_assert_eq!(model.row_index(id), expected.iter().position(|row| row.id == id));
            }
        }
    }
}
