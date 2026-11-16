//! Live order statistics for item event addresses. No GPUI state.
//! Stable handles survive reorder; structural edits update only treap paths.
use std::{
    cell::RefCell,
    ops::Range,
    rc::{Rc, Weak},
};
type Link = Option<Rc<RefCell<Node>>>;
struct Node {
    id: u64,
    priority: u64,
    len: usize,
    active: bool,
    left: Link,
    right: Link,
    parent: Weak<RefCell<Node>>,
}
#[derive(Clone)]
pub(crate) struct Position(Rc<RefCell<Node>>);
impl Position {
    fn new(id: u64) -> Self {
        let mut x = id.wrapping_add(0x9e3779b97f4a7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^= x >> 31;
        Self(Rc::new(RefCell::new(Node {
            id,
            priority: x,
            len: 1,
            active: false,
            left: None,
            right: None,
            parent: Weak::new(),
        })))
    }
    pub fn id(&self) -> u64 {
        self.0.borrow().id
    }
    pub fn index(&self) -> Option<usize> {
        if !self.0.borrow().active {
            return None;
        }
        let mut current = self.0.clone();
        let mut rank = len(&current.borrow().left);
        loop {
            let parent = current.borrow().parent.upgrade();
            let Some(parent) = parent else {
                return Some(rank);
            };
            if parent
                .borrow()
                .right
                .as_ref()
                .is_some_and(|right| Rc::ptr_eq(right, &current))
            {
                rank += 1 + len(&parent.borrow().left);
            }
            current = parent;
        }
    }
}
fn len(link: &Link) -> usize {
    link.as_ref().map(|node| node.borrow().len).unwrap_or(0)
}
fn repair(node: &Rc<RefCell<Node>>) {
    let mut n = node.borrow_mut();
    n.len = 1 + len(&n.left) + len(&n.right);
    for child in n.left.iter().chain(n.right.iter()) {
        child.borrow_mut().parent = Rc::downgrade(node);
    }
}
fn detach_root(link: &Link) {
    if let Some(node) = link {
        node.borrow_mut().parent = Weak::new();
    }
}
fn split(root: Link, count: usize) -> (Link, Link) {
    let Some(root) = root else {
        return (None, None);
    };
    let left_len = len(&root.borrow().left);
    if count <= left_len {
        let left = root.borrow_mut().left.take();
        let (a, b) = split(left, count);
        root.borrow_mut().left = b;
        repair(&root);
        let b = Some(root);
        detach_root(&a);
        detach_root(&b);
        (a, b)
    } else {
        let right = root.borrow_mut().right.take();
        let (a, b) = split(right, count - left_len - 1);
        root.borrow_mut().right = a;
        repair(&root);
        let a = Some(root);
        detach_root(&a);
        detach_root(&b);
        (a, b)
    }
}
fn merge(a: Link, b: Link) -> Link {
    let root = match (a, b) {
        (None, b) => b,
        (a, None) => a,
        (Some(a), Some(b)) => {
            let first = {
                let a = a.borrow();
                let b = b.borrow();
                (a.priority, a.id) < (b.priority, b.id)
            };
            if first {
                let right = a.borrow_mut().right.take();
                a.borrow_mut().right = merge(right, Some(b));
                repair(&a);
                Some(a)
            } else {
                let left = b.borrow_mut().left.take();
                b.borrow_mut().left = merge(Some(a), left);
                repair(&b);
                Some(b)
            }
        }
    };
    detach_root(&root);
    root
}
fn deactivate(root: Link) {
    if let Some(node) = root {
        let (left, right) = {
            let mut node = node.borrow_mut();
            node.active = false;
            node.parent = Weak::new();
            node.len = 1;
            (node.left.take(), node.right.take())
        };
        deactivate(left);
        deactivate(right);
    }
}
#[derive(Default)]
pub(crate) struct Positions {
    root: Link,
    next: u64,
}
impl Positions {
    pub fn create(&mut self) -> Position {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        Position::new(id)
    }
    pub fn splice(&mut self, range: Range<usize>, positions: &[Position]) {
        if range.start > range.end || range.end > len(&self.root) {
            return;
        }
        let (left, tail) = split(self.root.take(), range.start);
        let (removed, right) = split(tail, range.len());
        deactivate(removed);
        let mut middle = None;
        for position in positions {
            position.0.borrow_mut().active = true;
            middle = merge(middle, Some(position.0.clone()));
        }
        self.root = merge(merge(left, middle), right);
    }
}
impl Drop for Positions {
    fn drop(&mut self) {
        deactivate(self.root.take());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_ranks_survive_insertion_deletion_reorder_and_detach() {
        let mut tree = Positions::default();
        let rows: Vec<_> = (0..1000).map(|_| tree.create()).collect();
        tree.splice(0..0, &rows);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.index(), Some(i));
        }
        let inserted = tree.create();
        tree.splice(20..25, std::slice::from_ref(&inserted));
        assert_eq!(inserted.index(), Some(20));
        assert_eq!(rows.get(25).and_then(Position::index), Some(21));
        for row in rows.iter().skip(20).take(5) {
            assert_eq!(row.index(), None);
        }
        let retained = vec![rows[999].clone(), rows[0].clone(), inserted.clone()];
        tree.splice(0..996, &retained);
        for (i, row) in retained.iter().enumerate() {
            assert_eq!(row.index(), Some(i));
        }
        assert_eq!(rows[1].index(), None);
    }
    #[test]
    fn generated_edits_agree_with_a_vec_oracle() {
        let mut tree = Positions::default();
        let mut expected: Vec<Position> = Vec::new();
        let mut seed = 9u64;
        for _ in 0..1500 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let start = seed as usize % (expected.len() + 1);
            let end = (start + ((seed >> 20) as usize % 5)).min(expected.len());
            let count = (seed >> 30) as usize % 5;
            let inserted: Vec<_> = (0..count).map(|_| tree.create()).collect();
            let removed: Vec<_> = expected.splice(start..end, inserted.clone()).collect();
            tree.splice(start..end, &inserted);
            for row in removed {
                assert_eq!(row.index(), None);
            }
            for (i, row) in expected.iter().enumerate() {
                assert_eq!(row.index(), Some(i));
            }
        }
    }
    #[test]
    fn dropping_tree_detaches_external_position_handles() {
        let handle = {
            let mut positions = Positions::default();
            let handle = positions.create();
            positions.splice(0..0, std::slice::from_ref(&handle));
            handle
        };
        assert_eq!(handle.index(), None);
    }
}
