use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    time::Instant,
};

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

#[derive(Default)]
pub struct TreeModel {
    rows: Vec<Row>,
    sibling_positions: Vec<(usize, usize)>,
    indices: HashMap<NodeId, usize>,
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
        self.rows.clear();
        self.sibling_positions.clear();
        self.indices.clear();
        self.pending
            .retain(|&id| source.children(id) == Children::Unloaded);
        walk_visible(
            source,
            &self.expanded,
            source.roots(),
            None,
            0,
            |row, siblings| {
                self.indices.insert(row.id, self.rows.len());
                self.rows.push(row);
                self.sibling_positions.push(siblings);
            },
        );
        if self.cursor.is_some_and(|id| self.row_index(id).is_none()) {
            self.cursor = self.rows.first().map(|row| row.id);
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// One-based position and total count within the row's sibling set.
    pub fn sibling_position(&self, index: usize) -> Option<(usize, usize)> {
        self.sibling_positions.get(index).copied()
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
        self.indices.get(&id).copied()
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
        if changed && let Some(index) = self.row_index(id) {
            let depth = self.rows[index].depth;
            let start = index + 1;
            if open {
                if let Children::Ready(children) = source.children(id) {
                    let mut descendants = Vec::new();
                    let mut positions = Vec::new();
                    walk_visible(
                        source,
                        &self.expanded,
                        children,
                        Some(id),
                        depth + 1,
                        |row, siblings| {
                            descendants.push(row);
                            positions.push(siblings);
                        },
                    );
                    if !descendants.is_empty() {
                        self.rows.splice(start..start, descendants);
                        self.sibling_positions.splice(start..start, positions);
                        self.reindex(start);
                    }
                }
            } else {
                let end = start
                    + self.rows[start..]
                        .iter()
                        .take_while(|row| row.depth > depth)
                        .count();
                if end != start {
                    if let Some(cursor) = self.cursor.and_then(|id| self.row_index(id))
                        && (start..end).contains(&cursor)
                    {
                        self.cursor = Some(id);
                    }
                    self.sibling_positions.drain(start..end);
                    for row in self.rows.drain(start..end) {
                        self.indices.remove(&row.id);
                    }
                    self.reindex(start);
                }
            }
        }
        if self
            .cursor
            .is_some_and(|cursor| self.row_index(cursor).is_none())
        {
            self.cursor = self.rows.first().map(|row| row.id);
        }
        (open && source.children(id) == Children::Unloaded && self.pending.insert(id)).then_some(id)
    }

    fn reindex(&mut self, start: usize) {
        for (offset, row) in self.rows[start..].iter().enumerate() {
            self.indices.insert(row.id, start + offset);
        }
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
                self.rows.last()
            } else {
                self.rows.first()
            }
            .map(|row| row.id);
            return None;
        };
        let row = self.rows[index];
        let next = match direction {
            Navigation::Up => self.rows.get(index.saturating_sub(1)).map(|row| row.id),
            Navigation::Down => self.rows.get(index + 1).map(|row| row.id),
            Navigation::Home => self.rows.first().map(|row| row.id),
            Navigation::End => self.rows.last().map(|row| row.id),
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
                .rows
                .get(index + 1)
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
