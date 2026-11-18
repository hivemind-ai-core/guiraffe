use std::sync::Arc;

/// Indexed, shared source for typed, borrowed rows. Keys should be unique within a layer.
pub trait ChartSource: Send + Sync + 'static {
    type Row;

    fn len(&self) -> usize;
    fn row(&self, index: usize) -> Option<&Self::Row>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn key(&self, index: usize) -> u64 {
        index as u64
    }
}

impl<T: Send + Sync + 'static> ChartSource for Vec<T> {
    type Row = T;

    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn row(&self, index: usize) -> Option<&T> {
        self.get(index)
    }
}

impl<T: Send + Sync + 'static> ChartSource for Arc<[T]> {
    type Row = T;

    fn len(&self) -> usize {
        self.as_ref().len()
    }

    fn row(&self, index: usize) -> Option<&T> {
        self.get(index)
    }
}

struct Chunk<T> {
    start: usize,
    rows: Arc<Vec<T>>,
}

/// An append-only source. Cloning a snapshot shares the chunk directory and every row.
pub struct Stream<T> {
    start: usize,
    end: usize,
    chunks: Arc<Vec<Chunk<T>>>,
}

impl<T> Clone for Chunk<T> {
    fn clone(&self) -> Self {
        Self {
            start: self.start,
            rows: self.rows.clone(),
        }
    }
}

impl<T> Clone for Stream<T> {
    fn clone(&self) -> Self {
        Self {
            start: self.start,
            end: self.end,
            chunks: self.chunks.clone(),
        }
    }
}

impl<T> Default for Stream<T> {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            chunks: Arc::default(),
        }
    }
}

impl<T> Stream<T> {
    /// Move one batch into a new chunk. Existing readers retain their original snapshot.
    pub fn append(&mut self, rows: Vec<T>) {
        if !rows.is_empty() {
            let start = self.end;
            self.end += rows.len();
            Arc::make_mut(&mut self.chunks).push(Chunk {
                start,
                rows: Arc::new(rows),
            });
        }
    }

    /// Keep a rolling window over the latest rows. Existing snapshots and
    /// retained datum keys are stable when old chunks leave the window.
    pub fn retain_last(&mut self, count: usize) {
        self.start = self.start.max(self.end.saturating_sub(count));
        let expired = self
            .chunks
            .partition_point(|chunk| chunk.start + chunk.rows.len() <= self.start);
        if expired != 0 {
            Arc::make_mut(&mut self.chunks).drain(..expired);
        }
    }
}

impl<T: Send + Sync + 'static> ChartSource for Stream<T> {
    type Row = T;

    fn len(&self) -> usize {
        self.end - self.start
    }

    fn key(&self, index: usize) -> u64 {
        self.start.saturating_add(index) as u64
    }

    fn row(&self, index: usize) -> Option<&T> {
        let index = self.start.checked_add(index)?;
        let chunk = self.chunks.partition_point(|chunk| chunk.start <= index);
        let chunk = self.chunks.get(chunk.checked_sub(1)?)?;
        chunk.rows.get(index - chunk.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn stream_matches_flat_rows_and_keeps_snapshots(
            batches in proptest::collection::vec(proptest::collection::vec(proptest::num::i32::ANY, 0..16), 0..32),
        ) {
            let mut stream = Stream::default();
            let mut reference = Vec::new();
            let mut snapshots = Vec::new();
            for batch in batches {
                snapshots.push((stream.clone(), reference.len()));
                reference.extend_from_slice(&batch);
                stream.append(batch);
            }
            snapshots.push((stream, reference.len()));
            for (snapshot, len) in snapshots {
                proptest::prop_assert_eq!(snapshot.len(), len);
                for (index, expected) in reference[..len].iter().enumerate() {
                    proptest::prop_assert_eq!(snapshot.row(index), Some(expected));
                }
                proptest::prop_assert_eq!(snapshot.row(len), None);
            }
        }
    }

    #[test]
    fn rolling_window_preserves_snapshot_and_datum_keys() {
        let mut stream = Stream::default();
        stream.append(vec![0, 1]);
        stream.append(vec![2, 3]);
        let snapshot = stream.clone();
        stream.retain_last(1);
        assert_eq!(stream.len(), 1);
        assert_eq!(stream.row(0), Some(&3));
        assert_eq!(stream.key(0), snapshot.key(3));
        assert_eq!(snapshot.row(0), Some(&0));
        stream.retain_last(0);
        assert_eq!(stream.row(0), None);
        stream.append(vec![4]);
        assert_eq!(stream.row(0), Some(&4));
        assert_eq!(stream.key(0), 4);
    }

    #[test]
    fn appends_preserve_snapshot_and_borrow_existing_rows() {
        let mut stream = Stream::default();
        stream.append(vec![String::from("first"), String::from("second")]);
        let snapshot = stream.clone();
        stream.append(vec![String::from("third")]);
        stream.append(vec![]);
        assert_eq!(stream.len(), 3);
        assert_eq!(snapshot.len(), 2);
        assert!(
            stream
                .row(0)
                .zip(snapshot.row(0))
                .is_some_and(|(a, b)| std::ptr::eq(a, b))
        );
        assert_eq!(stream.row(2).map(String::as_str), Some("third"));
        assert_eq!(snapshot.row(2), None);
    }
}
