//! Incremental, cancellable search over every loaded node, including descendants of collapsed
//! branches. The scan walks the source depth-first in bounded steps; the caller drops it to
//! cancel. On completion it reveals matches by expanding their collapsed ancestors and returns
//! the visible ranks of the matches and of their ancestors (the matched paths).
use crate::{Children, NodeId, TreeModel, TreeSource};

/// Byte range of the first case-insensitive occurrence of `needle` (already lowercase) in
/// `label`. Compares each char's lowercase expansion in place, so it allocates nothing.
pub(crate) fn match_range(label: &str, needle: &str) -> Option<std::ops::Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    label.char_indices().find_map(|(start, _)| {
        let mut hay = label[start..].char_indices().flat_map(|(at, c)| {
            c.to_lowercase()
                .map(move |lower| (at + c.len_utf8(), lower))
        });
        let mut end = start;
        for wanted in needle.chars() {
            let (after, lower) = hay.next()?;
            if lower != wanted {
                return None;
            }
            end = start + after;
        }
        Some(start..end)
    })
}

/// One level of the depth-first walk: the branch being iterated (`None` for the roots), the
/// next child index, and whether this branch is already queued for reveal.
struct Frame {
    node: Option<NodeId>,
    next: usize,
    revealed: bool,
}

pub(crate) struct SearchScan {
    needle: String,
    /// Expand collapsed ancestors of matches. Off for rescans after the user changes
    /// expansion, so collapsing a branch during a search is not undone; the collapsed branch
    /// then stays listed as the path to its hidden matches.
    reveal: bool,
    stack: Vec<Frame>,
    hits: Vec<NodeId>,
    /// Collapsed ancestors of matches, with their depth so the deepest open first.
    collapsed: Vec<(usize, NodeId)>,
}

impl SearchScan {
    pub(crate) fn new(needle: String, reveal: bool) -> Self {
        Self {
            needle,
            reveal,
            stack: vec![Frame {
                node: None,
                next: 0,
                revealed: true,
            }],
            hits: Vec::new(),
            collapsed: Vec::new(),
        }
    }

    /// Visit at most `budget` nodes. Returns true once every loaded node has been visited.
    /// `include` filters hits (for example hidden advanced rows); it does not prune subtrees.
    pub(crate) fn step(
        &mut self,
        source: &dyn TreeSource,
        model: &TreeModel,
        budget: usize,
        include: impl Fn(NodeId) -> bool,
    ) -> bool {
        for _ in 0..budget {
            let Some(frame) = self.stack.last_mut() else {
                return true;
            };
            let siblings = match frame.node {
                None => source.roots(),
                Some(parent) => match source.children(parent) {
                    Children::Ready(children) => children,
                    Children::Unloaded => &[],
                },
            };
            let Some(&id) = siblings.get(frame.next) else {
                self.stack.pop();
                continue;
            };
            frame.next += 1;
            if include(id) && match_range(source.label(id), &self.needle).is_some() {
                self.hits.push(id);
                self.queue_ancestors(model);
            }
            if let Children::Ready(children) = source.children(id)
                && !children.is_empty()
            {
                self.stack.push(Frame {
                    node: Some(id),
                    next: 0,
                    revealed: false,
                });
            }
        }
        self.stack.is_empty()
    }

    /// Queue the collapsed ancestors on the current path. Frames are marked as they are
    /// queued, so each branch is examined once however many matches it contains.
    fn queue_ancestors(&mut self, model: &TreeModel) {
        for depth in (0..self.stack.len()).rev() {
            let frame = &mut self.stack[depth];
            if frame.revealed {
                break;
            }
            frame.revealed = true;
            if let Some(node) = frame.node
                && !model.expanded(node)
            {
                self.collapsed.push((depth, node));
            }
        }
    }

    /// Open the queued branches (when revealing) and return the sorted visible ranks of every
    /// match, every collapsed branch on a match path, and their ancestors. Branches open
    /// deepest first, so hidden ones only join the expanded set and the outermost collapsed
    /// branch splices its revealed subtree once.
    pub(crate) fn finish(mut self, source: &dyn TreeSource, model: &mut TreeModel) -> Vec<usize> {
        if self.reveal {
            self.collapsed
                .sort_by_key(|&(depth, _)| std::cmp::Reverse(depth));
            for &(_, node) in &self.collapsed {
                model.set_expanded(node, true, source);
            }
        }
        let mut ranks = Vec::with_capacity(self.hits.len());
        let paths = self.collapsed.into_iter().map(|(_, node)| node);
        for id in self.hits.into_iter().chain(paths) {
            let Some(rank) = model.row_index(id) else {
                continue;
            };
            ranks.push(rank);
            let mut parent = model.row(rank).and_then(|row| row.parent);
            while let Some(id) = parent {
                let Some(rank) = model.row_index(id) else {
                    break;
                };
                ranks.push(rank);
                parent = model.row(rank).and_then(|row| row.parent);
            }
        }
        ranks.sort_unstable();
        ranks.dedup();
        ranks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_range_is_case_insensitive_and_on_char_boundaries() {
        assert_eq!(match_range("Font Size", "size"), Some(5..9));
        assert_eq!(match_range("Opacity", "o"), Some(0..1));
        assert_eq!(match_range("Ünïcode ÄB", "äb"), Some(10..13));
        assert_eq!(match_range("Name", "names"), None);
        assert_eq!(match_range("Name", ""), None);
    }

    /// 0 ─┬─ 1 ── 2 "Needle"
    ///    └─ 3 "Other"
    /// 4 ── 5 (unloaded)
    /// 6 "needle root"
    struct Nested;
    impl TreeSource for Nested {
        fn roots(&self) -> &[NodeId] {
            &[0, 4, 6]
        }
        fn children(&self, id: NodeId) -> Children<'_> {
            match id {
                0 => Children::Ready(&[1, 3]),
                1 => Children::Ready(&[2]),
                4 => Children::Ready(&[5]),
                5 => Children::Unloaded,
                _ => Children::Ready(&[]),
            }
        }
        fn label(&self, id: NodeId) -> &str {
            [
                "Root",
                "Group",
                "Needle",
                "Other",
                "Lazy",
                "Pending",
                "needle root",
            ][id as usize]
        }
    }

    fn run(scan: &mut SearchScan, model: &TreeModel, budget: usize) -> usize {
        let mut steps = 1;
        while !scan.step(&Nested, model, budget, |_| true) {
            steps += 1;
        }
        steps
    }

    #[test]
    fn reveals_matches_inside_collapsed_branches_with_their_paths() {
        let mut model = TreeModel::new(&Nested);
        let mut scan = SearchScan::new("needle".into(), true);
        // One node per step: the walk is bounded and resumable.
        assert!(run(&mut scan, &model, 1) > 7);
        let ranks = scan.finish(&Nested, &mut model);
        assert!(model.expanded(0) && model.expanded(1) && !model.expanded(4));
        let ids: Vec<_> = ranks
            .iter()
            .filter_map(|&rank| model.row(rank).map(|row| row.id))
            .collect();
        assert_eq!(ids, [0, 1, 2, 6]);
    }

    #[test]
    fn rescans_keep_the_user_collapse() {
        let mut model = TreeModel::new(&Nested);
        let mut scan = SearchScan::new("needle".into(), false);
        run(&mut scan, &model, 512);
        let ranks = scan.finish(&Nested, &mut model);
        assert!(!model.expanded(0));
        assert_eq!(
            ranks
                .iter()
                .filter_map(|&r| model.row(r))
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            // The collapsed root stays listed as the path to its hidden match.
            [0, 6]
        );
    }

    #[test]
    fn excluded_rows_are_not_hits_but_their_children_are() {
        let mut model = TreeModel::new(&Nested);
        let mut scan = SearchScan::new("e".into(), true);
        while !scan.step(&Nested, &model, 3, |id| id != 2) {}
        let ranks = scan.finish(&Nested, &mut model);
        let ids: Vec<_> = ranks
            .iter()
            .filter_map(|&rank| model.row(rank).map(|row| row.id))
            .collect();
        // "Needle" is excluded but "Other", "Pending" and "needle root" match. "Pending" is
        // loaded (only its own children are not), so its collapsed parent opens.
        assert_eq!(ids, [0, 3, 4, 5, 6]);
    }
}
