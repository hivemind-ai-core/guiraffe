//! Pure core of layout animation (spec/29): size tracks, FLIP offsets and keyed presence.
//!
//! Time and preferences are passed in, so everything here is deterministic and testable
//! without a window. The GPUI elements in [`crate::layout_elements`] drive these.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    time::Instant,
};

use gpui_kit::{Pixels, Point, point, px};

use crate::{
    animated::Animated,
    presence::{Presence, PresenceFrame},
    timing::{MotionPrefs, Timing, Transition},
};

/// Sizes closer than this are equal (sub-pixel layout noise never starts an animation).
pub const SIZE_EPSILON: f32 = 0.01;

/// One animated extent (a width or a height) that follows a measured target.
#[derive(Debug, Clone, PartialEq)]
pub struct SizeTrack {
    anim: Animated<f32>,
}

impl SizeTrack {
    /// A track at rest at `size`.
    pub fn new(size: f32, timing: Timing) -> Self {
        Self {
            anim: Animated::new(sanitize(size), timing),
        }
    }

    /// The size the track is heading to (the last measurement).
    pub fn target(&self) -> f32 {
        *self.anim.target()
    }

    pub fn set_timing(&mut self, timing: Timing) {
        self.anim.set_timing(timing);
    }

    /// Feed a measurement. Retargets from the current value (no jump, springs keep velocity)
    /// and returns true when the target changed.
    pub fn observe(&mut self, measured: f32, now: Instant, prefs: &MotionPrefs) -> bool {
        let measured = sanitize(measured);
        if (measured - self.target()).abs() <= SIZE_EPSILON {
            return false;
        }
        self.anim.set_at(measured, now, prefs)
    }

    /// The size to lay out at `now` (never negative, even when a spring overshoots past 0)
    /// and whether it is still moving.
    pub fn value_at(&self, now: Instant, prefs: &MotionPrefs) -> (f32, bool) {
        let (v, moving) = self.anim.value_at(now, prefs);
        (v.max(0.0), moving)
    }

    /// Jump to `size` without animating.
    pub fn jump(&mut self, size: f32) {
        self.anim.jump(sanitize(size));
    }
}

fn sanitize(v: f32) -> f32 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// FLIP state of one keyed item: where it was laid out last frame, and the paint offset that
/// keeps it visually continuous after a structural change.
#[derive(Debug, Clone, PartialEq)]
pub struct FlipTrack {
    origin: Point<Pixels>,
    offset: Animated<Point<Pixels>>,
    /// Frame stamp of the last frame the key was laid out, for pruning removed keys.
    pub(crate) seen: u64,
}

const ZERO: Point<Pixels> = Point {
    x: Pixels::ZERO,
    y: Pixels::ZERO,
};

impl FlipTrack {
    pub fn new(origin: Point<Pixels>, timing: Timing) -> Self {
        Self {
            origin,
            offset: Animated::new(ZERO, timing),
            seen: 0,
        }
    }

    /// The origin recorded for the last layout.
    pub fn origin(&self) -> Point<Pixels> {
        self.origin
    }

    pub fn set_timing(&mut self, timing: Timing) {
        self.offset.set_timing(timing);
    }

    /// The item was laid out at `origin` after a structural change: start (or continue) an
    /// offset animation from where it is *visually* now to zero. Returns whether it animates.
    pub fn moved(&mut self, origin: Point<Pixels>, now: Instant, prefs: &MotionPrefs) -> bool {
        let (current, _) = self.offset.value_at(now, prefs);
        let visual = self.origin + current;
        self.origin = origin;
        let start = visual - origin;
        if f32::from(start.x).abs() <= SIZE_EPSILON && f32::from(start.y).abs() <= SIZE_EPSILON {
            self.offset.jump(ZERO);
            return false;
        }
        self.offset.jump(start);
        self.offset.set_at(ZERO, now, prefs)
    }

    /// Record a non-structural layout move: the item follows layout directly (any in-flight
    /// offset keeps easing out on top).
    pub fn follow(&mut self, origin: Point<Pixels>) {
        self.origin = origin;
    }

    /// Paint offset at `now` and whether it is still moving.
    pub fn offset_at(&self, now: Instant, prefs: &MotionPrefs) -> (Point<Pixels>, bool) {
        self.offset.value_at(now, prefs)
    }
}

/// Order-sensitive hash of a key sequence (allocation-free).
pub fn keys_hash<'a, K: Hash + 'a>(keys: impl IntoIterator<Item = &'a K>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut n = 0usize;
    for k in keys {
        k.hash(&mut h);
        n += 1;
    }
    n.hash(&mut h);
    h.finish()
}

/// The FLIP bookkeeping of a whole list, keyed by `K` (the elements use `ElementId`).
#[derive(Debug, Clone)]
pub struct FlipList<K: Hash + Eq> {
    items: HashMap<K, FlipTrack>,
    keys_hash: Option<u64>,
    frame: u64,
    structural: bool,
    timing: Timing,
}

impl<K: Hash + Eq + Clone> FlipList<K> {
    pub fn new(timing: Timing) -> Self {
        Self {
            items: HashMap::new(),
            keys_hash: None,
            frame: 0,
            structural: false,
            timing,
        }
    }

    /// Start a frame with the hash of this frame's key sequence. Returns whether the
    /// sequence changed (a structural frame: items that moved will animate).
    pub fn begin(&mut self, keys_hash: u64, timing: Timing) -> bool {
        self.frame = self.frame.wrapping_add(1);
        self.structural = self.keys_hash.is_some_and(|h| h != keys_hash);
        self.keys_hash = Some(keys_hash);
        if self.timing != timing {
            self.timing = timing;
            for t in self.items.values_mut() {
                t.set_timing(timing);
            }
        }
        self.structural
    }

    /// Lay out one item at `origin` (relative to the list); returns its paint offset and
    /// whether it is still moving. Allocates only for keys never seen before.
    pub fn place(
        &mut self,
        key: &K,
        origin: Point<Pixels>,
        now: Instant,
        prefs: &MotionPrefs,
    ) -> (Point<Pixels>, bool) {
        let frame = self.frame;
        let track = match self.items.get_mut(key) {
            Some(track) => {
                if self.structural && track.origin != origin {
                    track.moved(origin, now, prefs);
                } else {
                    track.follow(origin);
                }
                track
            }
            None => self
                .items
                .entry(key.clone())
                .or_insert_with(|| FlipTrack::new(origin, self.timing)),
        };
        track.seen = frame;
        track.offset_at(now, prefs)
    }

    /// End a frame: on structural frames forget keys that were not laid out.
    pub fn end(&mut self) {
        if self.structural {
            let frame = self.frame;
            self.items.retain(|_, t| t.seen == frame);
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Enter/exit bookkeeping for a keyed list: removed keys stay mounted (at their old place)
/// while they animate out; new keys animate in. Owned by the view that renders the list.
#[derive(Debug, Clone)]
pub struct KeyedPresence<K> {
    entries: Vec<Entry<K>>,
    transition: Transition,
}

#[derive(Debug, Clone)]
struct Entry<K> {
    key: K,
    presence: Presence,
    visibility: f32,
}

impl<K: Clone + Eq + Hash> KeyedPresence<K> {
    /// Start with `keys` shown at rest (nothing animates in on first display).
    pub fn new(keys: impl IntoIterator<Item = K>, transition: Transition) -> Self {
        Self {
            entries: keys
                .into_iter()
                .map(|key| Entry {
                    key,
                    presence: Presence::new(true, transition.enter, transition.exit),
                    visibility: 1.0,
                })
                .collect(),
            transition,
        }
    }

    /// Adopt a new key list. Keys that left keep their slot (after the surviving key that
    /// preceded them) and exit; new keys enter; a key re-added while exiting reverses.
    /// Returns false (and does nothing) when `keys` is exactly the shown list at rest.
    pub fn sync(&mut self, keys: &[K], now: Instant, prefs: &MotionPrefs) -> bool {
        if self.entries.len() == keys.len()
            && self
                .entries
                .iter()
                .zip(keys)
                .all(|(e, k)| &e.key == k && e.presence.is_visible_target())
        {
            return false;
        }
        let wanted: HashMap<&K, ()> = keys.iter().map(|k| (k, ())).collect();
        let mut old: HashMap<K, Entry<K>> = HashMap::with_capacity(self.entries.len());
        // Leaving entries, grouped by the surviving key they follow (None: the list start).
        let mut after: HashMap<Option<K>, Vec<K>> = HashMap::new();
        let mut anchor: Option<K> = None;
        for entry in self.entries.drain(..) {
            if wanted.contains_key(&entry.key) {
                anchor = Some(entry.key.clone());
            } else {
                after
                    .entry(anchor.clone())
                    .or_default()
                    .push(entry.key.clone());
            }
            old.insert(entry.key.clone(), entry);
        }
        let mut next = Vec::with_capacity(keys.len() + old.len());
        let mut push_leaving =
            |anchor: Option<K>, next: &mut Vec<Entry<K>>, old: &mut HashMap<K, Entry<K>>| {
                for key in after.remove(&anchor).unwrap_or_default() {
                    if let Some(mut e) = old.remove(&key) {
                        e.presence.set_visible(false, now, prefs);
                        next.push(e);
                    }
                }
            };
        push_leaving(None, &mut next, &mut old);
        for key in keys {
            let entry = match old.remove(key) {
                Some(mut e) => {
                    e.presence.set_visible(true, now, prefs);
                    e
                }
                None => {
                    let mut presence =
                        Presence::new(false, self.transition.enter, self.transition.exit);
                    presence.set_visible(true, now, prefs);
                    Entry {
                        key: key.clone(),
                        presence,
                        visibility: 0.0,
                    }
                }
            };
            next.push(entry);
            push_leaving(Some(key.clone()), &mut next, &mut old);
        }
        self.entries = next;
        true
    }

    /// Sample every entry at `now`, dropping finished exits. Returns whether any entry is
    /// still animating (the caller needs another frame).
    pub fn frame(&mut self, now: Instant, prefs: &MotionPrefs) -> bool {
        let before = self.entries.len();
        let mut animating = false;
        self.entries
            .retain_mut(|e| match e.presence.frame(now, prefs) {
                PresenceFrame::Unmounted => false,
                PresenceFrame::Mounted {
                    visibility,
                    animating: a,
                } => {
                    e.visibility = visibility.clamp(0.0, 1.0);
                    animating |= a;
                    true
                }
            });
        let remaining = self.entries.len();
        if remaining < before && self.entries.capacity() > remaining.saturating_mul(4).max(64) {
            self.entries.shrink_to(remaining.saturating_mul(2).max(64));
        }
        animating
    }

    /// Mounted entries in display order with their visibility (1 = at rest).
    pub fn iter(&self) -> impl Iterator<Item = (&K, f32)> {
        self.entries.iter().map(|e| (&e.key, e.visibility))
    }

    /// Whether `key` is mounted and leaving.
    pub fn is_exiting(&self, key: &K) -> bool {
        self.entries
            .iter()
            .any(|e| &e.key == key && !e.presence.is_visible_target())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `point(px(x), px(y))`, for tests and callers.
pub fn offset(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{easing::Easing, spring::Spring, transition::TransitionKind};
    use std::time::Duration;

    fn lin(ms: u64) -> Timing {
        Timing::tween(Duration::from_millis(ms), Easing::Linear)
    }
    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn size_track_interpolates_old_to_new() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut s = SizeTrack::new(100.0, lin(100));
        assert_eq!(s.value_at(t0, &p), (100.0, false));
        assert!(s.observe(200.0, t0, &p));
        assert!(!s.observe(200.004, t0, &p), "sub-epsilon noise is ignored");
        assert_eq!(s.value_at(t0, &p), (100.0, true), "starts at the old size");
        assert_eq!(s.value_at(at(t0, 50), &p), (150.0, true));
        assert_eq!(s.value_at(at(t0, 100), &p), (200.0, false));
    }

    #[test]
    fn size_track_retargets_from_current_value() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut s = SizeTrack::new(0.0, lin(100));
        s.observe(100.0, t0, &p);
        let (mid, _) = s.value_at(at(t0, 40), &p);
        assert!((mid - 40.0).abs() < 1e-3);
        s.observe(0.0, at(t0, 40), &p);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(
            close(s.value_at(at(t0, 40), &p).0, mid),
            "no jump on retarget"
        );
        assert!(close(s.value_at(at(t0, 90), &p).0, mid / 2.0));
        assert_eq!(s.value_at(at(t0, 140), &p), (0.0, false));
    }

    #[test]
    fn size_track_spring_never_negative_and_bad_input_is_safe() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut s = SizeTrack::new(50.0, Timing::Spring(Spring::BOUNCY));
        s.observe(0.0, t0, &p);
        for i in 0..200 {
            assert!(s.value_at(at(t0, i * 5), &p).0 >= 0.0);
        }
        s.observe(f32::NAN, t0, &p);
        assert!(s.target().is_finite());
    }

    #[test]
    fn reduced_motion_is_instant() {
        let p = MotionPrefs {
            reduced: true,
            ..Default::default()
        };
        let t0 = Instant::now();
        let mut s = SizeTrack::new(10.0, lin(100));
        s.observe(90.0, t0, &p);
        assert_eq!(s.value_at(t0, &p), (90.0, false));
        let mut f = FlipTrack::new(offset(0.0, 0.0), lin(100));
        f.moved(offset(0.0, 40.0), t0, &p);
        assert_eq!(f.offset_at(t0, &p), (offset(0.0, 0.0), false));
        assert!(
            !p.layout_enabled(Some(true)),
            "reduced motion disables layout motion"
        );
        assert!(MotionPrefs::default().layout_enabled(Some(true)));
        assert!(
            !MotionPrefs::default().layout_enabled(None),
            "off by default"
        );
    }

    #[test]
    fn flip_starts_at_old_position_and_continues_on_interrupt() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut f = FlipTrack::new(offset(0.0, 0.0), lin(100));
        assert!(f.moved(offset(0.0, 100.0), t0, &p));
        assert_eq!(
            f.offset_at(t0, &p).0,
            offset(0.0, -100.0),
            "painted where it was"
        );
        assert_eq!(f.offset_at(at(t0, 50), &p).0, offset(0.0, -50.0));
        // Interrupted: moves again to 200 while visually at 50.
        f.moved(offset(0.0, 200.0), at(t0, 50), &p);
        assert_eq!(
            f.offset_at(at(t0, 50), &p).0,
            offset(0.0, -150.0),
            "no jump"
        );
        assert_eq!(f.offset_at(at(t0, 150), &p), (offset(0.0, 0.0), false));
        assert!(
            !f.moved(offset(0.0, 200.0), at(t0, 150), &p),
            "no displacement"
        );
    }

    #[test]
    fn flip_list_animates_only_on_structural_frames_and_prunes() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let mut l: FlipList<u32> = FlipList::new(lin(100));
        assert!(
            !l.begin(keys_hash(&[1, 2]), lin(100)),
            "first frame is never structural"
        );
        l.place(&1, offset(0.0, 0.0), t0, &p);
        l.place(&2, offset(0.0, 20.0), t0, &p);
        l.end();
        // Non-structural move (e.g. resize): follows layout, no animation.
        assert!(!l.begin(keys_hash(&[1, 2]), lin(100)));
        assert_eq!(
            l.place(&2, offset(0.0, 30.0), t0, &p),
            (offset(0.0, 0.0), false)
        );
        l.end();
        // Reorder: both animate from their old places.
        assert!(l.begin(keys_hash(&[2, 1]), lin(100)));
        assert_eq!(
            l.place(&2, offset(0.0, 0.0), t0, &p),
            (offset(0.0, 30.0), true)
        );
        assert_eq!(
            l.place(&1, offset(0.0, 30.0), t0, &p),
            (offset(0.0, -30.0), true)
        );
        l.end();
        // Remove key 1: pruned on the structural frame.
        assert!(l.begin(keys_hash(&[2]), lin(100)));
        l.place(&2, offset(0.0, 0.0), t0, &p);
        l.end();
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn keys_hash_is_order_sensitive() {
        assert_ne!(keys_hash(&[1, 2]), keys_hash(&[2, 1]));
        assert_ne!(keys_hash(&[1]), keys_hash(&[1, 1]));
        assert_eq!(keys_hash(&[3, 4]), keys_hash(&[3, 4]));
    }

    fn keys(k: &KeyedPresence<&'static str>) -> Vec<(&'static str, f32)> {
        k.iter().map(|(k, v)| (*k, v)).collect()
    }

    #[test]
    fn presence_churn_releases_settled_exit_capacity() {
        let prefs = MotionPrefs::default();
        let start = Instant::now();
        let transition = Transition::new(TransitionKind::Fade, lin(100));
        let mut presence = KeyedPresence::new(0..10_000, transition);
        assert!(presence.sync(&[0], start, &prefs));
        assert_eq!(presence.len(), 10_000);
        assert!(!presence.frame(at(start, 200), &prefs));
        assert_eq!(presence.len(), 1);
        assert!(presence.entries.capacity() <= 64);
        assert!(!presence.frame(at(start, 300), &prefs));
        assert!(presence.entries.capacity() <= 64);
    }

    #[test]
    fn keyed_presence_enter_exit_in_place_and_readd() {
        let p = MotionPrefs::default();
        let t0 = Instant::now();
        let tr = Transition::new(TransitionKind::Fade, lin(100));
        let mut k = KeyedPresence::new(["a", "b", "c"], tr);
        assert!(!k.frame(t0, &p), "initial keys are at rest");
        assert!(!k.sync(&["a", "b", "c"], t0, &p), "same keys: no change");
        // Remove b, add d at the front.
        assert!(k.sync(&["d", "a", "c"], t0, &p));
        assert!(k.frame(at(t0, 50), &p));
        assert_eq!(
            keys(&k),
            vec![("d", 0.5), ("a", 1.0), ("b", 0.5), ("c", 1.0)],
            "b exits after a, its old predecessor"
        );
        assert!(k.is_exiting(&"b"));
        // Re-add b mid-exit: reverses from 0.5.
        assert!(k.sync(&["d", "a", "b", "c"], at(t0, 50), &p));
        k.frame(at(t0, 50), &p);
        assert_eq!(keys(&k)[2], ("b", 0.5));
        assert!(!k.frame(at(t0, 200), &p));
        assert_eq!(keys(&k).len(), 4);
        // Remove the first key: exits at the start; then unmounts.
        k.sync(&["a", "b", "c"], at(t0, 200), &p);
        k.frame(at(t0, 200), &p);
        assert_eq!(keys(&k)[0].0, "d");
        assert!(!k.frame(at(t0, 400), &p));
        assert_eq!(keys(&k), vec![("a", 1.0), ("b", 1.0), ("c", 1.0)]);
        // Reorder only: still a change.
        assert!(k.sync(&["c", "b", "a"], at(t0, 400), &p));
    }

    #[test]
    fn keyed_presence_reduced_motion_is_instant() {
        let p = MotionPrefs {
            reduced: true,
            ..Default::default()
        };
        let t0 = Instant::now();
        let mut k = KeyedPresence::new([1, 2], Transition::new(TransitionKind::Fade, lin(100)));
        k.sync(&[2, 3], t0, &p);
        assert!(!k.frame(t0, &p));
        assert_eq!(
            k.iter().map(|(k, v)| (*k, v)).collect::<Vec<_>>(),
            vec![(2, 1.0), (3, 1.0)]
        );
    }
}
