//! [`KeyedList`]: an ordered collection addressed by stable item keys.

use std::{
    fmt,
    hash::Hash,
    sync::atomic::{AtomicU64, Ordering},
};

static STAMPS: AtomicU64 = AtomicU64::new(1);

fn stamp() -> u64 {
    STAMPS.fetch_add(1, Ordering::Relaxed)
}

/// An ordered list of items, each with a stable key. Persistent: cloning is `O(1)`, and an
/// edit copies only the touched path when a snapshot (undo, a view) shares the list.
///
/// Item lenses ([`ListLens::by_key`](crate::ListLens::by_key)) address an item by its key, so
/// an insert, remove or move never retargets a binding to another item. Lookups by key are
/// `O(1)` expected; inserts, removes and moves are `O(log n)` plus an `O(n)` search for the
/// moved key's position.
///
/// ```
/// use guiraffe_state::KeyedList;
/// let mut clips: KeyedList<u32, &str> = [(1, "intro"), (2, "verse")].into_iter().collect();
/// clips.insert(1, 3, "bridge");
/// assert_eq!(clips.keys().copied().collect::<Vec<_>>(), [1, 3, 2]);
/// clips.move_to(&2, 0);
/// assert_eq!(clips.get(&2), Some(&"verse"));
/// assert_eq!(clips.key_at(0), Some(&2));
/// ```
pub struct KeyedList<K: Clone + Hash + Eq, T: Clone> {
    order: imbl::Vector<K>,
    items: imbl::HashMap<K, T>,
    // Changes whenever the order changes; equal stamps imply equal order.
    stamp: u64,
}

impl<K: Clone + Hash + Eq, T: Clone> Clone for KeyedList<K, T> {
    fn clone(&self) -> Self {
        Self {
            order: self.order.clone(),
            items: self.items.clone(),
            stamp: self.stamp,
        }
    }
}

impl<K: Clone + Hash + Eq, T: Clone> Default for KeyedList<K, T> {
    fn default() -> Self {
        Self {
            order: imbl::Vector::new(),
            items: imbl::HashMap::new(),
            stamp: stamp(),
        }
    }
}

impl<K: Clone + Hash + Eq, T: Clone + PartialEq> PartialEq for KeyedList<K, T> {
    fn eq(&self, other: &Self) -> bool {
        (self.stamp == other.stamp || self.order.ptr_eq(&other.order) || self.order == other.order)
            && (self.items.ptr_eq(&other.items) || self.items == other.items)
    }
}

impl<K: Clone + Hash + Eq + fmt::Debug, T: Clone + fmt::Debug> fmt::Debug for KeyedList<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Clone + Hash + Eq, T: Clone> FromIterator<(K, T)> for KeyedList<K, T> {
    fn from_iter<I: IntoIterator<Item = (K, T)>>(iter: I) -> Self {
        let mut list = Self::default();
        for (key, item) in iter {
            list.push(key, item);
        }
        list
    }
}

impl<K: Clone + Hash + Eq, T: Clone> KeyedList<K, T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn get(&self, key: &K) -> Option<&T> {
        self.items.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut T> {
        self.items.get_mut(key)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.items.contains_key(key)
    }

    /// The key at a position.
    pub fn key_at(&self, index: usize) -> Option<&K> {
        self.order.get(index)
    }

    /// The position of a key (`O(n)`).
    pub fn index_of(&self, key: &K) -> Option<usize> {
        self.order.index_of(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.order.iter()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &T)> {
        self.order
            .iter()
            .filter_map(|key| self.items.get(key).map(|item| (key, item)))
    }

    pub(crate) fn order(&self) -> &imbl::Vector<K> {
        &self.order
    }

    /// Changes whenever the order changes: inserts, removes and moves. Equal stamps mean an
    /// equal order, so list views compare stamps instead of keys.
    pub fn order_stamp(&self) -> u64 {
        self.stamp
    }

    /// Append an item, replacing an existing item with the same key in place.
    pub fn push(&mut self, key: K, item: T) {
        let len = self.order.len();
        self.insert(len, key, item);
    }

    /// Insert at `index` (clamped). An existing item with the same key is replaced in place
    /// and keeps its position.
    pub fn insert(&mut self, index: usize, key: K, item: T) {
        if self.items.insert(key.clone(), item).is_none() {
            self.order.insert(index.min(self.order.len()), key);
            self.stamp = stamp();
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<T> {
        let item = self.items.remove(key)?;
        if let Some(index) = self.order.index_of(key) {
            self.order.remove(index);
        }
        self.stamp = stamp();
        Some(item)
    }

    /// Move an item to `index` (clamped). Returns whether the key exists.
    pub fn move_to(&mut self, key: &K, index: usize) -> bool {
        let Some(from) = self.order.index_of(key) else {
            return false;
        };
        let key = self.order.remove(from);
        self.order.insert(index.min(self.order.len()), key);
        self.stamp = stamp();
        true
    }
}
