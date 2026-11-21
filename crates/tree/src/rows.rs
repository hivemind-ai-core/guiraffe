use std::{collections::HashMap, ops::Range};

use crate::{NodeId, Row};

const BLOCK_SIZE: usize = 512;
type Link = Option<usize>;

#[derive(Clone, Copy)]
struct Entry {
    id: NodeId,
    parent: NodeId,
    depth: usize,
    siblings: u64,
}

impl Entry {
    fn row(self) -> Row {
        Row {
            id: self.id,
            depth: self.depth,
            parent: (self.depth != 0).then_some(self.parent),
        }
    }
}

struct Block {
    rows: Vec<Entry>,
    left: Link,
    right: Link,
    parent: Link,
    len: usize,
    priority: u64,
}

const ID_PAGE_SIZE: usize = 256;
const ID_PAGE_MIN_DENSITY: usize = ID_PAGE_SIZE / 8;

#[derive(Default)]
struct IdIndex {
    pages: HashMap<NodeId, Box<[u32; ID_PAGE_SIZE]>>,
    sparse: HashMap<NodeId, usize>,
}

impl IdIndex {
    fn address(id: NodeId) -> (NodeId, usize) {
        (id / ID_PAGE_SIZE as u64, id as usize % ID_PAGE_SIZE)
    }

    fn get(&self, id: NodeId) -> Option<usize> {
        let (page, offset) = Self::address(id);
        self.pages
            .get(&page)
            .and_then(|slots| (slots[offset] as usize).checked_sub(1))
            .or_else(|| self.sparse.get(&id).copied())
    }

    fn insert(&mut self, id: NodeId, position: usize) {
        let (page, offset) = Self::address(id);
        if let Some(slots) = self.pages.get_mut(&page)
            && let Ok(slot) = u32::try_from(position)
            && let Some(encoded) = slot.checked_add(1)
        {
            if slots[offset] == 0 {
                self.sparse.remove(&id);
            }
            slots[offset] = encoded;
        } else {
            if let Some(slots) = self.pages.get_mut(&page) {
                slots[offset] = 0;
            }
            self.sparse.insert(id, position);
        }
    }

    fn remove(&mut self, id: NodeId) {
        let (page, offset) = Self::address(id);
        if let Some(slots) = self.pages.get_mut(&page)
            && slots[offset] != 0
        {
            slots[offset] = 0;
        } else {
            self.sparse.remove(&id);
        }
    }

    fn promote(&mut self, page: NodeId) {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.pages.entry(page) {
            let mut slots = Box::new([0; ID_PAGE_SIZE]);
            let start = page * ID_PAGE_SIZE as u64;
            for (offset, slot) in slots.iter_mut().enumerate() {
                let id = start + offset as u64;
                if let Some(&position) = self.sparse.get(&id)
                    && let Ok(position) = u32::try_from(position)
                    && let Some(encoded) = position.checked_add(1)
                {
                    *slot = encoded;
                    self.sparse.remove(&id);
                }
            }
            entry.insert(slots);
        }
    }

    fn promote_dense_pages(&mut self, entries: &[Entry]) {
        let mut pages = [0; BLOCK_SIZE];
        for (out, entry) in pages.iter_mut().zip(entries) {
            *out = Self::address(entry.id).0;
        }
        let pages = &mut pages[..entries.len()];
        pages.sort_unstable();
        for group in pages.chunk_by(|a, b| a == b) {
            if group.len() >= ID_PAGE_MIN_DENSITY {
                self.promote(group[0]);
            }
        }
    }
}

/// An implicit treap of row blocks. The ID index stores positions inside
/// stable arena slots. Rank comes from the subtree lengths on the parent path.
#[derive(Default)]
pub(crate) struct Rows {
    blocks: Vec<Block>,
    free: Vec<usize>,
    ids: IdIndex,
    wide_siblings: HashMap<NodeId, (usize, usize)>,
    root: Link,
    serial: u64,
    #[cfg(test)]
    pub rewritten: usize,
}

impl Rows {
    fn size(&self, link: Link) -> usize {
        link.map_or(0, |key| self.blocks[key].len)
    }

    pub fn len(&self) -> usize {
        self.size(self.root)
    }

    pub fn rank(&self, id: NodeId) -> Option<usize> {
        let position = self.ids.get(id)?;
        let (mut key, offset) = (position / BLOCK_SIZE, position % BLOCK_SIZE);
        let mut rank = offset + self.size(self.blocks[key].left);
        while let Some(parent) = self.blocks[key].parent {
            let block = &self.blocks[parent];
            if block.right == Some(key) {
                rank += self.size(block.left) + block.rows.len();
            }
            key = parent;
        }
        Some(rank)
    }

    pub fn get(&self, rank: usize) -> Option<Row> {
        self.entry(rank).copied().map(Entry::row)
    }

    pub fn row_and_siblings(&self, rank: usize) -> Option<(Row, Option<(usize, usize)>)> {
        let &entry = self.entry(rank)?;
        let siblings = if entry.siblings == 0 {
            self.wide_siblings.get(&entry.id).copied()
        } else {
            Some((
                (entry.siblings >> 32) as usize,
                entry.siblings as u32 as usize,
            ))
        };
        Some((entry.row(), siblings))
    }

    fn entry(&self, mut rank: usize) -> Option<&Entry> {
        let mut link = self.root;
        while let Some(key) = link {
            let block = &self.blocks[key];
            let left = self.size(block.left);
            if rank < left {
                link = block.left;
            } else if let Some(entry) = block.rows.get(rank - left) {
                return Some(entry);
            } else {
                rank -= left + block.rows.len();
                link = block.right;
            }
        }
        None
    }

    pub fn descendants_end(&self, start: usize, depth: usize) -> usize {
        fn count(rows: &Rows, link: Link, skip: usize, depth: usize) -> (usize, bool) {
            let Some(key) = link else { return (0, false) };
            let block = &rows.blocks[key];
            let left = rows.size(block.left);
            let end = left + block.rows.len();
            let mut total = 0;
            if skip < left {
                let (counted, stopped) = count(rows, block.left, skip, depth);
                if stopped {
                    return (counted, true);
                }
                total += counted;
            }
            if skip < end {
                let entries = &block.rows[skip.saturating_sub(left)..];
                let counted = entries
                    .iter()
                    .take_while(|entry| entry.depth > depth)
                    .count();
                total += counted;
                if counted != entries.len() {
                    return (total, true);
                }
            }
            let (counted, stopped) = count(rows, block.right, skip.saturating_sub(end), depth);
            (total + counted, stopped)
        }
        start + count(self, self.root, start, depth).0
    }

    pub fn iter(&self) -> impl Iterator<Item = Row> {
        let mut stack = Vec::new();
        let mut link = self.root;
        let blocks = std::iter::from_fn(move || {
            while let Some(key) = link {
                stack.push(key);
                link = self.blocks[key].left;
            }
            let block = &self.blocks[stack.pop()?];
            link = block.right;
            Some(&block.rows)
        });
        blocks.flatten().copied().map(Entry::row)
    }

    fn children(&mut self, key: usize, left: Link, right: Link) {
        let len = self.size(left) + self.blocks[key].rows.len() + self.size(right);
        let block = &mut self.blocks[key];
        block.left = left;
        block.right = right;
        block.len = len;
        for child in left.into_iter().chain(right) {
            self.blocks[child].parent = Some(key);
        }
    }

    fn merge(&mut self, left: Link, right: Link) -> Link {
        let (Some(a), Some(b)) = (left, right) else {
            return left.or(right);
        };
        if self.blocks[a].priority > self.blocks[b].priority {
            let tail = self.merge(self.blocks[a].right, right);
            self.children(a, self.blocks[a].left, tail);
            left
        } else {
            let head = self.merge(left, self.blocks[b].left);
            self.children(b, head, self.blocks[b].right);
            right
        }
    }

    fn index(&mut self, key: usize) {
        #[cfg(test)]
        {
            self.rewritten += self.blocks[key].rows.len();
        }
        for (offset, entry) in self.blocks[key].rows.iter().enumerate() {
            self.ids.insert(entry.id, key * BLOCK_SIZE + offset);
        }
    }

    fn allocate(&mut self, rows: Vec<Entry>, bulk: bool) -> usize {
        // SplitMix64 permutes the allocation serial to give deterministic,
        // well-distributed treap priorities independent of user IDs.
        self.serial = self.serial.wrapping_add(0x9e3779b97f4a7c15);
        let mut priority = self.serial;
        priority = (priority ^ (priority >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        priority = (priority ^ (priority >> 27)).wrapping_mul(0x94d049bb133111eb);
        priority ^= priority >> 31;
        if bulk {
            self.ids.promote_dense_pages(&rows);
        }
        let block = Block {
            len: rows.len(),
            rows,
            left: None,
            right: None,
            parent: None,
            priority,
        };
        let key = if let Some(key) = self.free.pop() {
            self.blocks[key] = block;
            key
        } else {
            let key = self.blocks.len();
            self.blocks.push(block);
            key
        };
        self.index(key);
        key
    }

    fn split(&mut self, root: Link, rank: usize) -> (Link, Link) {
        let Some(key) = root else { return (None, None) };
        let block = &self.blocks[key];
        let (left, right) = (block.left, block.right);
        let start = self.size(left);
        let end = start + block.rows.len();
        if rank < start {
            let (head, tail) = self.split(left, rank);
            // Splitting a block can introduce a new priority above an ancestor.
            self.children(key, None, right);
            (head, self.merge(tail, root))
        } else if rank > end {
            let (head, tail) = self.split(right, rank - end);
            self.children(key, left, None);
            (self.merge(root, head), tail)
        } else if rank == start {
            self.children(key, None, right);
            (left, root)
        } else if rank == end {
            self.children(key, left, None);
            (root, right)
        } else {
            let tail = self.blocks[key].rows.split_off(rank - start);
            self.children(key, left, None);
            let tail = self.allocate(tail, false);
            (root, self.merge(Some(tail), right))
        }
    }

    fn discard(&mut self, root: Link) {
        if let Some(key) = root {
            let block = &mut self.blocks[key];
            for entry in std::mem::take(&mut block.rows) {
                self.ids.remove(entry.id);
                if entry.siblings == 0 {
                    self.wide_siblings.remove(&entry.id);
                }
            }
            let (left, right) = (block.left, block.right);
            self.free.push(key);
            self.discard(left);
            self.discard(right);
        }
    }

    pub fn splice(
        &mut self,
        range: Range<usize>,
        fill: impl FnOnce(&mut dyn FnMut(Row, (usize, usize))),
    ) {
        #[cfg(test)]
        {
            self.rewritten = 0;
        }
        let (left, tail) = self.split(self.root, range.start);
        let (removed, right) = self.split(tail, range.len());
        self.discard(removed);

        let mut middle = None;
        let mut rows = Vec::new();
        fill(&mut |row, siblings| {
            let packed = if let (Ok(position), Ok(total)) =
                (u32::try_from(siblings.0), u32::try_from(siblings.1))
            {
                (u64::from(position) << 32) | u64::from(total)
            } else {
                self.wide_siblings.insert(row.id, siblings);
                0
            };
            let entry = Entry {
                id: row.id,
                parent: row.parent.unwrap_or_default(),
                depth: row.depth,
                siblings: packed,
            };
            if rows.len() == BLOCK_SIZE {
                let key = self.allocate(std::mem::take(&mut rows), true);
                middle = self.merge(middle, Some(key));
            }
            if rows.is_empty() {
                rows.reserve_exact(BLOCK_SIZE);
            }
            rows.push(entry);
        });
        if !rows.is_empty() {
            rows.shrink_to_fit();
            let key = self.allocate(rows, false);
            middle = self.merge(middle, Some(key));
        }
        let head = self.join(left, middle);
        self.root = self.join(head, right);
        if let Some(root) = self.root {
            self.blocks[root].parent = None;
        }
    }

    /// Coalesce the two boundary blocks when they fit, so repeated local
    /// edits cannot leave a row-sized block behind at every touched position.
    fn join(&mut self, left: Link, right: Link) -> Link {
        let (Some(mut last), Some(mut first)) = (left, right) else {
            return left.or(right);
        };
        while let Some(next) = self.blocks[last].right {
            last = next;
        }
        while let Some(next) = self.blocks[first].left {
            first = next;
        }
        let len = self.blocks[last].rows.len();
        if len + self.blocks[first].rows.len() > BLOCK_SIZE {
            return self.merge(left, right);
        }
        let (head, _) = self.split(left, self.size(left) - len);
        let (_, tail) = self.split(right, self.blocks[first].rows.len());
        let rows = std::mem::take(&mut self.blocks[first].rows);
        self.free.push(first);
        self.blocks[last].rows.extend(rows);
        self.children(last, None, None);
        self.index(last);
        let head = self.merge(head, Some(last));
        self.merge(head, tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_block_splices_preserve_ranks_positions_and_order() {
        let mut rows = Rows::default();
        let mut expected = Vec::new();
        let mut next_id = 0;
        for (range, count) in [
            (0..0, 4_000),
            (511..1_537, 1_200),
            (512..512, 206),
            (1_024..2_048, 0),
            (2..3_182, 0),
            (1..1, 2_000),
        ] {
            let new_ids = next_id..next_id + count;
            next_id += count;
            expected.splice(range.clone(), new_ids.clone());
            rows.splice(range, |push| {
                for id in new_ids {
                    push(
                        Row {
                            id,
                            depth: 1,
                            parent: Some(u64::MAX),
                        },
                        (2, 3),
                    );
                }
            });
            assert_eq!(rows.len(), expected.len());
            assert!(rows.iter().map(|row| row.id).eq(expected.iter().copied()));
            for (rank, &id) in expected.iter().enumerate() {
                assert_eq!(rows.rank(id), Some(rank));
                assert_eq!(rows.get(rank).and_then(|row| row.parent), Some(u64::MAX));
                assert_eq!(
                    rows.row_and_siblings(rank)
                        .and_then(|(_, siblings)| siblings),
                    Some((2, 3))
                );
            }
        }
    }
}
