use std::{collections::HashSet, ops::Range, sync::OnceLock, time::Instant};

pub type NodeId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Children<'a> {
    Ready(&'a [NodeId]),
    Unloaded,
}

/// A snapshot of the hierarchy. IDs must be unique and the hierarchy acyclic.
pub trait TreeSource {
    fn roots(&self) -> &[NodeId];
    fn children(&self, node: NodeId) -> Children<'_>;
    fn label(&self, node: NodeId) -> &str;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: NodeId,
    pub depth: usize,
    pub parent: Option<NodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
}

use crate::rows::Rows;

#[derive(Default)]
pub struct TreeModel {
    rows: Rows,
    snapshot: OnceLock<Vec<Row>>,
    expanded: HashSet<NodeId>,
    pending: HashSet<NodeId>,
    selected: HashSet<NodeId>,
    cursor: Option<NodeId>,
    prefix: String,
    last_typed: Option<Instant>,
}

impl TreeModel {
    pub fn new(source: &dyn TreeSource) -> Self {
        let mut model = Self::default();
        model.refresh(source);
        model
    }

    /// Apply a new source snapshot, including results delivered by async loaders.
    pub fn refresh(&mut self, source: &dyn TreeSource) {
        self.rows = Rows::default();
        self.snapshot.take();
        self.pending
            .retain(|&id| source.children(id) == Children::Unloaded);
        self.rows.splice(0..0, |append| {
            walk_visible(source, &self.expanded, source.roots(), None, 0, append);
        });
        if self.cursor.is_some_and(|id| self.row_index(id).is_none()) {
            self.cursor = self.row(0).map(|row| row.id);
        }
    }

    /// Materialize a contiguous snapshot after the most recent structural edit.
    /// Use [Self::row] and [Self::row_count] to access a viewport without a copy.
    pub fn rows(&self) -> &[Row] {
        self.snapshot.get_or_init(|| self.rows.iter().collect())
    }

    pub fn row(&self, index: usize) -> Option<Row> {
        self.rows.get(index)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// One-based position and total count within the row's sibling set.
    pub fn sibling_position(&self, index: usize) -> Option<(usize, usize)> {
        self.row_and_siblings(index)?.1
    }

    pub(crate) fn row_and_siblings(&self, index: usize) -> Option<(Row, Option<(usize, usize)>)> {
        self.rows.row_and_siblings(index)
    }

    pub fn expanded(&self, id: NodeId) -> bool {
        self.expanded.contains(&id)
    }

    pub fn selected(&self, id: NodeId) -> bool {
        self.selected.contains(&id)
    }

    pub fn cursor(&self) -> Option<NodeId> {
        self.cursor
    }

    pub fn row_index(&self, id: NodeId) -> Option<usize> {
        self.rows.rank(id)
    }

    /// Toggle a branch in the current snapshot. Call [Self::refresh] when the
    /// source changes. Returns the ID to request once if the branch is unloaded.
    pub fn set_expanded(
        &mut self,
        id: NodeId,
        open: bool,
        source: &dyn TreeSource,
    ) -> Option<NodeId> {
        let changed = if open {
            self.expanded.insert(id)
        } else {
            self.expanded.remove(&id)
        };
        if changed
            && let Some(index) = self.row_index(id)
            && let Some(row) = self.row(index)
        {
            let depth = row.depth;
            let start = index + 1;
            if open {
                if let Children::Ready(children) = source.children(id)
                    && !children.is_empty()
                {
                    self.snapshot.take();
                    self.rows.splice(start..start, |append| {
                        walk_visible(
                            source,
                            &self.expanded,
                            children,
                            Some(id),
                            depth + 1,
                            append,
                        );
                    });
                }
            } else {
                let end = self.rows.descendants_end(start, depth);
                if end != start {
                    if let Some(cursor) = self.cursor.and_then(|id| self.row_index(id))
                        && (start..end).contains(&cursor)
                    {
                        self.cursor = Some(id);
                    }
                    self.snapshot.take();
                    self.rows.splice(start..end, |_| {});
                }
            }
        }
        if self
            .cursor
            .is_some_and(|cursor| self.row_index(cursor).is_none())
        {
            self.cursor = self.row(0).map(|row| row.id);
        }
        (open && source.children(id) == Children::Unloaded && self.pending.insert(id)).then_some(id)
    }

    pub fn retry_children(&mut self, id: NodeId) {
        self.pending.remove(&id);
    }

    pub fn select(&mut self, id: NodeId, extend: bool) {
        self.cursor = Some(id);
        if !extend {
            self.selected.clear();
        }
        if !self.selected.insert(id) && extend {
            self.selected.remove(&id);
        }
    }

    pub fn navigate(&mut self, direction: Navigation, source: &dyn TreeSource) -> Option<NodeId> {
        let current = self.cursor.and_then(|id| self.row_index(id));
        let Some(index) = current else {
            self.cursor = if direction == Navigation::End {
                self.row(self.row_count().saturating_sub(1))
            } else {
                self.row(0)
            }
            .map(|row| row.id);
            return None;
        };
        let row = self.row(index)?;
        let next = match direction {
            Navigation::Up => self.row(index.saturating_sub(1)).map(|row| row.id),
            Navigation::Down => self.row(index + 1).map(|row| row.id),
            Navigation::Home => self.row(0).map(|row| row.id),
            Navigation::End => self
                .row(self.row_count().saturating_sub(1))
                .map(|row| row.id),
            Navigation::Left if self.expanded(row.id) => {
                return self.set_expanded(row.id, false, source);
            }
            Navigation::Left => row.parent,
            Navigation::Right
                if !self.expanded(row.id) && source.children(row.id) != Children::Ready(&[]) =>
            {
                return self.set_expanded(row.id, true, source);
            }
            Navigation::Right => self
                .row(index + 1)
                .filter(|child| child.parent == Some(row.id))
                .map(|row| row.id),
        };
        self.cursor = next.or(self.cursor);
        None
    }

    pub fn type_ahead(&mut self, ch: char, now: Instant, source: &dyn TreeSource) {
        if self
            .last_typed
            .is_none_or(|then| now.saturating_duration_since(then).as_millis() > 700)
        {
            self.prefix.clear();
        }
        self.last_typed = Some(now);
        self.prefix.extend(ch.to_lowercase());
        let start = self
            .cursor
            .and_then(|id| self.row_index(id))
            .map_or(0, |index| {
                index + usize::from(self.prefix.chars().count() == 1)
            });
        if let Some(row) = self
            .rows
            .iter()
            .skip(start)
            .chain(self.rows.iter().take(start))
            .find(|row| {
                source
                    .label(row.id)
                    .to_lowercase()
                    .starts_with(&self.prefix)
            })
        {
            self.cursor = Some(row.id);
        }
    }

    pub fn window(&self, scroll: f64, height: f64, row_height: f64) -> Range<usize> {
        if !row_height.is_finite() || row_height <= 0. || height <= 0. {
            return 0..0;
        }
        let scroll = scroll.max(0.);
        let start = (scroll / row_height).floor() as usize;
        let end = ((scroll + height) / row_height).ceil() as usize;
        start.min(self.rows.len())..end.max(start).min(self.rows.len())
    }
}

fn walk_visible(
    source: &dyn TreeSource,
    expanded: &HashSet<NodeId>,
    roots: &[NodeId],
    parent: Option<NodeId>,
    depth: usize,
    mut visit: impl FnMut(Row, (usize, usize)),
) {
    let mut stack = vec![(roots.iter(), roots.len(), parent)];
    while let Some((siblings, total, parent)) = stack.last_mut() {
        let parent = *parent;
        let total = *total;
        let Some(&id) = siblings.next() else {
            stack.pop();
            continue;
        };
        let position = total - siblings.len();
        visit(
            Row {
                id,
                depth: depth + stack.len() - 1,
                parent,
            },
            (position, total),
        );
        if expanded.contains(&id)
            && let Children::Ready(children) = source.children(id)
            && !children.is_empty()
        {
            stack.push((children.iter(), children.len(), Some(id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Forest(Vec<NodeId>);

    impl TreeSource for Forest {
        fn roots(&self) -> &[NodeId] {
            &self.0
        }

        fn children(&self, id: NodeId) -> Children<'_> {
            Children::Ready(if id == 0 { &[u64::MAX] } else { &[] })
        }

        fn label(&self, _: NodeId) -> &str {
            "row"
        }
    }

    #[test]
    fn local_expansion_rewrites_the_same_rows_at_each_scale() {
        let mut counts = Vec::new();
        for size in [10_000, 1_000_000] {
            let source = Forest((0..size).collect());
            let mut tree = TreeModel::new(&source);
            for _ in 0..8 {
                tree.set_expanded(0, true, &source);
                assert_eq!(tree.row_index(u64::MAX), Some(1));
                assert_eq!(tree.row_index(size - 1), Some(size as usize));
                let expanded = tree.rows.rewritten;

                tree.set_expanded(0, false, &source);
                assert_eq!(tree.row_index(u64::MAX), None);
                assert_eq!(tree.row_index(size - 1), Some(size as usize - 1));
                counts.push((expanded, tree.rows.rewritten));
            }
            assert_eq!(tree.rows().len(), size as usize);
            for (index, row) in tree.rows().iter().enumerate() {
                assert_eq!(tree.row(index), Some(*row));
                assert_eq!(tree.row_index(row.id), Some(index));
            }
        }
        assert!(counts.iter().all(|&count| count == counts[0]));
        assert!(counts[0].0 <= 1024 && counts[0].1 <= 1024);
    }
}
