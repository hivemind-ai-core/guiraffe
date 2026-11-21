//! DAW / animation track editing: clips on a track never overlap; moves, trims and splits
//! keep that invariant and respect snapping.

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub id: u32,
    pub start: f64,
    pub len: f64,
    /// Offset into the source media (trimming the start advances it).
    pub source_offset: f64,
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.len
    }
}

/// A track: clips sorted by start, non-overlapping.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    clips: Vec<Clip>,
    next_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

/// Minimum clip length (seconds) for trims and splits.
pub const MIN_LEN: f64 = 1e-3;

impl Track {
    pub(crate) fn from_validated_clips(clips: Vec<Clip>) -> Self {
        let next_id = clips.iter().map(|clip| clip.id).max().unwrap_or_default();
        Self { clips, next_id }
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    fn find(&self, id: u32) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// Free interval around position `i` (between neighbours), excluding the clip itself.
    fn room(&self, i: usize) -> (f64, f64) {
        let lo = if i == 0 {
            f64::NEG_INFINITY
        } else {
            self.clips[i - 1].end()
        };
        let hi = self
            .clips
            .get(i + 1)
            .map(|c| c.start)
            .unwrap_or(f64::INFINITY);
        (lo, hi)
    }

    /// Insert a clip if it fits without overlapping; returns its id.
    pub fn insert(&mut self, start: f64, len: f64) -> Option<u32> {
        let end = start + len;
        if !(start.is_finite() && end.is_finite()) || len < MIN_LEN || end <= start {
            return None;
        }
        let pos = self.clips.partition_point(|c| c.start < start);
        let lo = if pos == 0 {
            f64::NEG_INFINITY
        } else {
            self.clips[pos - 1].end()
        };
        let hi = self
            .clips
            .get(pos)
            .map(|c| c.start)
            .unwrap_or(f64::INFINITY);
        if start < lo - 1e-12 || end > hi + 1e-12 {
            return None;
        }
        self.next_id = self.next_id.checked_add(1)?;
        self.clips.insert(
            pos,
            Clip {
                id: self.next_id,
                start,
                len,
                source_offset: 0.0,
            },
        );
        Some(self.next_id)
    }

    /// Move clip `id` to `start`, clamped against its neighbours (it cannot jump over them).
    pub fn move_clip(&mut self, id: u32, start: f64) -> bool {
        let Some(i) = self.find(id) else { return false };
        if !start.is_finite() {
            return false;
        }
        let (lo, hi) = self.room(i);
        let len = self.clips[i].len;
        let s = start.max(lo).min(hi - len);
        let changed = s != self.clips[i].start;
        self.clips[i].start = s;
        changed
    }

    /// Trim an edge to time `t`, clamped to neighbours and a minimum length. Trimming the
    /// start also advances the source offset so the content stays in place.
    pub fn trim(&mut self, id: u32, edge: Edge, t: f64) -> bool {
        let Some(i) = self.find(id) else { return false };
        if !t.is_finite() {
            return false;
        }
        let (lo, hi) = self.room(i);
        let c = &mut self.clips[i];
        let before = (c.start, c.len, c.source_offset);
        match edge {
            Edge::Start => {
                let end = c.end();
                let s = t
                    .max(lo)
                    .max(end - c.len - c.source_offset)
                    .min(end - MIN_LEN);
                c.source_offset += s - c.start;
                c.len = end - s;
                c.start = s;
            }
            Edge::End => {
                let e = t.min(hi).max(c.start + MIN_LEN);
                c.len = e - c.start;
            }
        }
        (c.start, c.len, c.source_offset) != before
    }

    /// Split clip `id` at `t`; returns the new right-hand clip id.
    pub fn split(&mut self, id: u32, t: f64) -> Option<u32> {
        let i = self.find(id)?;
        let c = self.clips[i].clone();
        if !(t > c.start + MIN_LEN && t < c.end() - MIN_LEN) {
            return None;
        }
        self.next_id = self.next_id.checked_add(1)?;
        self.clips[i].len = t - c.start;
        let right = Clip {
            id: self.next_id,
            start: t,
            len: c.end() - t,
            source_offset: c.source_offset + (t - c.start),
        };
        self.clips.insert(i + 1, right);
        Some(self.next_id)
    }

    /// Invariant check (used by tests and debug assertions).
    pub fn is_valid(&self) -> bool {
        self.clips
            .windows(2)
            .all(|w| w[0].end() <= w[1].start + 1e-9)
            && self.clips.iter().all(|c| c.len >= MIN_LEN - 1e-12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest::proptest! {
        #[test]
        fn insertion_matches_linear_overlap_check(
            inserts in proptest::collection::vec((-100_i16..100, 1_i16..25), 0..100),
        ) {
            let mut track = Track::default();
            let mut reference = Vec::<(i16, i16)>::new();
            for (start, len) in inserts {
                let end = start + len;
                let fits = reference.iter().all(|&(a, b)| end <= a || start >= b);
                let actual = track.insert(f64::from(start), f64::from(len));
                proptest::prop_assert_eq!(actual.is_some(), fits);
                if fits {
                    reference.push((start, end));
                    reference.sort_unstable();
                }
                let spans: Vec<_> = track.clips().iter().map(|c| (c.start as i16, c.end() as i16)).collect();
                proptest::prop_assert_eq!(&spans, &reference);
            }
        }
    }

    #[test]
    fn insert_rejects_overlaps() {
        let mut t = Track::default();
        let a = t.insert(0.0, 2.0);
        assert!(a.is_some());
        assert!(t.insert(1.0, 2.0).is_none(), "overlaps a");
        assert!(t.insert(2.0, 1.0).is_some(), "touching is fine");
        assert!(t.insert(5.0, 0.0).is_none() && t.insert(f64::NAN, 1.0).is_none());
        assert!(t.is_valid());
    }

    #[test]
    fn overflowing_clip_end_is_rejected_without_mutation() {
        let mut track = Track::default();
        assert_eq!(track.insert(f64::MAX, f64::MAX), None);
        assert!(track.clips().is_empty());
        assert_eq!(track.insert(0., 1.), Some(1));
    }

    #[test]
    fn moves_clamp_against_neighbours() {
        let mut t = Track::default();
        let a = t.insert(0.0, 2.0).unwrap_or(0);
        let b = t.insert(5.0, 2.0).unwrap_or(0);
        assert!(t.move_clip(b, 1.0));
        assert_eq!(t.clips()[1].start, 2.0, "stops at a's end");
        assert!(!t.move_clip(a, 10.0), "boxed in by b: no change");
        assert_eq!(t.clips()[0].start, 0.0, "cannot jump over b");
        assert!(t.is_valid());
    }

    #[test]
    fn trims_respect_neighbours_min_length_and_source() {
        let mut t = Track::default();
        let a = t.insert(0.0, 2.0).unwrap_or(0);
        let b = t.insert(4.0, 4.0).unwrap_or(0);
        assert!(t.trim(b, Edge::Start, 5.0));
        let c = &t.clips()[1];
        assert_eq!(
            (c.start, c.len, c.source_offset),
            (5.0, 3.0, 1.0),
            "content stays in place"
        );
        assert!(t.trim(b, Edge::Start, 0.0));
        assert_eq!(
            t.clips()[1].start,
            4.0,
            "cannot extend before the source start"
        );
        assert!(t.trim(a, Edge::End, 100.0));
        assert_eq!(t.clips()[0].end(), 4.0, "end trim stops at the next clip");
        t.trim(a, Edge::End, -50.0);
        assert!((t.clips()[0].len - MIN_LEN).abs() < 1e-9);
        assert!(t.is_valid());
    }

    #[test]
    fn split_preserves_content_mapping() {
        let mut t = Track::default();
        let a = t.insert(1.0, 4.0).unwrap_or(0);
        let r = t.split(a, 3.0);
        assert!(r.is_some());
        assert_eq!(t.clips().len(), 2);
        assert_eq!(
            (
                t.clips()[1].start,
                t.clips()[1].len,
                t.clips()[1].source_offset
            ),
            (3.0, 2.0, 2.0)
        );
        assert!(t.split(a, 1.0).is_none(), "at the edge: no split");
        assert!(t.is_valid());
    }
}
