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

// Keep the public row slice contiguous while indexing IDs relative to a
// block. A splice shifts block starts instead of rehashing every trailing row.
const ROW_BLOCK: usize = 512;

struct Block {
    start: usize,
    key: usize,
}

#[derive(Default)]
struct RowIndex {
    ids: HashMap<NodeId, usize>,
    blocks: Vec<Block>,
    starts: Vec<usize>,
    free: Vec<usize>,
}

impl RowIndex {
    fn rebuild(&mut self, rows: &[Row]) {
        self.ids.clear();
        self.blocks.clear();
        self.starts.clear();
        self.free.clear();
        self.ids.reserve(rows.len());
        self.replace_blocks(0..0, 0, rows);
    }

    fn get(&self, id: NodeId) -> Option<usize> {
        let &location = self.ids.get(&id)?;
        Some(self.starts[location / ROW_BLOCK] + location % ROW_BLOCK)
    }

    fn block_containing(&self, row: usize) -> usize {
        self.blocks.partition_point(|block| block.start <= row) - 1
    }

    fn replace_blocks(&mut self, range: Range<usize>, start: usize, rows: &[Row]) {
        self.free
            .extend(self.blocks[range.clone()].iter().map(|block| block.key));
        let count = rows.len().div_ceil(ROW_BLOCK);
        // Divide evenly so repeated edits never leave a trail of tiny blocks.
        let chunk = rows.len().div_ceil(count.max(1)).max(1);
        let replacement = rows.chunks(chunk).enumerate().map(|(index, rows)| {
            let start = start + index * chunk;
            let key = self.free.pop().unwrap_or_else(|| {
                self.starts.push(0);
                self.starts.len() - 1
            });
            self.starts[key] = start;
            for (offset, row) in rows.iter().enumerate() {
                self.ids.insert(row.id, key * ROW_BLOCK + offset);
            }
            Block { start, key }
        });
        self.blocks.splice(range, replacement);
    }

    fn splice(&mut self, before: Range<usize>, inserted: usize, rows: &[Row]) {
        // Edits always follow a visible parent, so the previous row exists.
        let first = self.block_containing(before.start - 1);
        let last = self.block_containing(before.end);
        let start = self.blocks[first].start;
        let old_len = rows.len() + before.len() - inserted;
        let old_end = self
            .blocks
            .get(last + 1)
            .map_or(old_len, |block| block.start);
        let end = old_end - before.len() + inserted;
        for block in &mut self.blocks[last + 1..] {
            block.start = block.start - before.len() + inserted;
            self.starts[block.key] = block.start;
        }
        self.replace_blocks(first..last + 1, start, &rows[start..end]);
    }
}

#[derive(Default)]
pub struct TreeModel {
    rows: Vec<Row>,
    sibling_positions: Vec<(usize, usize)>,
    indices: RowIndex,
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
        self.pending
            .retain(|&id| source.children(id) == Children::Unloaded);
        walk_visible(
            source,
            &self.expanded,
            source.roots(),
            None,
            0,
            |row, siblings| {
                self.rows.push(row);
                self.sibling_positions.push(siblings);
            },
        );
        self.indices.rebuild(&self.rows);
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
        self.indices.get(id)
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
                if let Children::Ready(children) = source.children(id)
                    && !children.is_empty()
                {
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
                    let inserted = descendants.len();
                    self.rows.splice(start..start, descendants);
                    self.sibling_positions.splice(start..start, positions);
                    self.indices.splice(start..start, inserted, &self.rows);
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
                        self.indices.ids.remove(&row.id);
                    }
                    self.indices.splice(start..end, 0, &self.rows);
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

#[cfg(test)]
mod index_tests {
    use super::*;

    #[test]
    fn local_splice_keeps_trailing_id_locations() {
        for count in [ROW_BLOCK * 4, 1_000_000] {
            let mut rows: Vec<_> = (0..count)
                .map(|index| Row {
                    id: index as u64,
                    depth: 0,
                    parent: None,
                })
                .collect();
            let mut index = RowIndex::default();
            index.rebuild(&rows);
            let tail = count as u64 - 1;
            let packed = index.ids[&tail];

            let child = Row {
                id: u64::MAX,
                depth: 1,
                parent: Some(0),
            };
            rows.insert(1, child);
            index.splice(1..1, 1, &rows);
            assert_eq!(index.get(child.id), Some(1));
            assert_eq!(index.get(tail), Some(count));
            assert_eq!(index.ids[&tail], packed);

            rows.remove(1);
            index.ids.remove(&child.id);
            index.splice(1..2, 0, &rows);
            assert_eq!(index.get(child.id), None);
            assert_eq!(index.ids[&tail], packed);
            for (position, row) in rows.iter().enumerate() {
                assert_eq!(index.get(row.id), Some(position));
            }
        }
    }
}
