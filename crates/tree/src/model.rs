use std::{collections::HashSet, ops::Range, time::Instant};

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
        self.pending
            .retain(|&id| source.children(id) == Children::Unloaded);
        let mut stack = vec![(source.roots().iter(), None)];
        while let Some((siblings, parent)) = stack.last_mut() {
            let parent = *parent;
            let Some(&id) = siblings.next() else {
                stack.pop();
                continue;
            };
            self.rows.push(Row {
                id,
                depth: stack.len() - 1,
                parent,
            });
            if self.expanded.contains(&id)
                && let Children::Ready(children) = source.children(id)
            {
                stack.push((children.iter(), Some(id)));
            }
        }
        if self.cursor.is_some_and(|id| self.row_index(id).is_none()) {
            self.cursor = self.rows.first().map(|row| row.id);
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
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
        self.rows.iter().position(|row| row.id == id)
    }

    /// Returns the ID to request once if an expanded branch is unloaded.
    pub fn set_expanded(
        &mut self,
        id: NodeId,
        open: bool,
        source: &dyn TreeSource,
    ) -> Option<NodeId> {
        if open {
            self.expanded.insert(id);
        } else {
            self.expanded.remove(&id);
            if let Some(cursor) = self.cursor
                && let (Some(ancestor), Some(active)) = (self.row_index(id), self.row_index(cursor))
                && ancestor < active
                && self.rows[ancestor + 1..=active]
                    .iter()
                    .all(|row| row.depth > self.rows[ancestor].depth)
            {
                self.cursor = Some(id);
            }
        }
        self.refresh(source);
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
