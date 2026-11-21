//! Reusable, bounded scratch for recursive layout.
#[cfg(feature = "block_layout")]
use super::block::BlockItem;
use crate::{tree::OofCandidate, util::sys::Vec};

const MAX_BUFFERS: usize = 16;
const MAX_BUFFER_BYTES: usize = 16 * 1024;

/// Scratch retained by a layout tree between calls.
#[derive(Default)]
pub struct LayoutScratch {
    #[cfg(feature = "block_layout")]
    pub(crate) block_items: BufferPool<BlockItem>,
    pub(crate) oof_work: BufferPool<OofCandidate>,
    pub(crate) hoisted_ids: BufferPool<crate::tree::NodeId>,
}

impl core::fmt::Debug for LayoutScratch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LayoutScratch").finish_non_exhaustive()
    }
}

impl Clone for LayoutScratch {
    fn clone(&self) -> Self {
        Self::default()
    }
}

pub(crate) struct BufferPool<T>(Vec<Vec<T>>);

impl<T> Default for BufferPool<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> BufferPool<T> {
    // Taking ownership leaves the tree free for recursively computing children.
    pub(crate) fn take(&mut self) -> Vec<T> {
        self.0.pop().unwrap_or_default()
    }

    pub(crate) fn recycle(&mut self, mut buffer: Vec<T>) {
        if self.0.len() < MAX_BUFFERS
            && buffer.capacity() != 0
            && buffer.capacity() <= MAX_BUFFER_BYTES / core::mem::size_of::<T>().max(1)
        {
            buffer.clear();
            self.0.push(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BufferPool;

    #[test]
    fn reuses_disjoint_buffers_after_nested_layout() {
        let mut pool = BufferPool::default();
        let mut outer = pool.take();
        outer.extend(0..10);
        let mut inner = pool.take();
        inner.extend(10..20);
        let allocation = outer.as_ptr();
        pool.recycle(inner);
        pool.recycle(outer);

        let reused = pool.take();
        assert!(reused.is_empty());
        assert_eq!(reused.as_ptr(), allocation);
    }
}
