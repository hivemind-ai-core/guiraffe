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
