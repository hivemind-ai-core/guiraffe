// Added for GUIraffe on top of gpui-fast.
//! Giving back what per-frame buffers grew to for a frame much larger than
//! those that follow.
//!
//! A frame's element-state map, its log of element-state accesses, its tab
//! stop history and the entity access log are cleared, not dropped, between
//! frames, so that a steady frame allocates nothing for them. They keep the
//! capacity of the largest frame drawn: a window that once drew 100,000
//! elements holds tens of megabytes for them while it draws a hundred.
//!
//! Each buffer is trimmed once its use has stayed under a quarter of its
//! capacity for [`LOW_FRAMES`] frames in a row, to twice the most it used in
//! those frames. A frame whose use comes back up before that resets the count,
//! so frames that alternate between large and small are never trimmed and
//! steady frames never allocate; only a lasting drop trims, once. Buffers under
//! [`MIN_BYTES`] are left alone.

/// How many frames in a row a buffer must use under a quarter of its
/// capacity before it is trimmed.
pub(crate) const LOW_FRAMES: u32 = 8;

/// Buffers holding less than this are never trimmed.
pub(crate) const MIN_BYTES: usize = 64 * 1024;

/// How long one buffer's use has stayed low, and the most it used meanwhile.
#[derive(Default)]
pub(crate) struct Trim {
    low_frames: u32,
    peak: usize,
}

impl Trim {
    /// Notes a frame that used `used` of a buffer's `capacity` items of
    /// `item_bytes` each, returning the capacity to trim it to, if it is due.
    pub(crate) fn note(
        &mut self,
        used: usize,
        capacity: usize,
        item_bytes: usize,
    ) -> Option<usize> {
        if capacity.saturating_mul(item_bytes) < MIN_BYTES || used.saturating_mul(4) >= capacity {
            *self = Trim::default();
            return None;
        }
        self.peak = self.peak.max(used);
        self.low_frames += 1;
        if self.low_frames < LOW_FRAMES {
            return None;
        }
        let target = self.peak.saturating_mul(2);
        *self = Trim::default();
        Some(target)
    }
}

/// The trims of a window's per-frame buffers.
#[derive(Default)]
pub(crate) struct FrameTrims {
    element_states: Trim,
    accessed_element_states: Trim,
    tab_history: Trim,
    access_log: Trim,
}

/// Trims the window's per-frame buffers that are due, once a frame drawn in
/// full has become the rendered frame and the next frame was cleared. The
/// rendered frame's use is what a frame needs; both frames' buffers, which
/// take turns being drawn into, are trimmed to it.
pub(crate) fn trim_frames(window: &mut crate::Window, cx: &mut crate::App) {
    use std::mem::size_of;
    let trims = &mut window.fast_frame_trims;
    let (rendered, next) = (&mut window.rendered_frame, &mut window.next_frame);

    let capacity = rendered
        .element_states
        .capacity()
        .max(next.element_states.capacity());
    let item = size_of::<(
        (crate::GlobalElementId, std::any::TypeId),
        crate::window::ElementStateBox,
    )>();
    if let Some(target) = trims
        .element_states
        .note(rendered.element_states.len(), capacity, item)
    {
        rendered.element_states.shrink_to(target);
        next.element_states.shrink_to(target);
    }

    let capacity = rendered
        .accessed_element_states
        .capacity()
        .max(next.accessed_element_states.capacity());
    let item = size_of::<(crate::GlobalElementId, std::any::TypeId)>();
    if let Some(target) =
        trims
            .accessed_element_states
            .note(rendered.accessed_element_states.len(), capacity, item)
    {
        rendered.accessed_element_states.shrink_to(target);
        next.accessed_element_states.shrink_to(target);
    }

    let history = |frame: &crate::window::Frame| frame.tab_stops.insertion_history.capacity();
    let capacity = history(rendered).max(history(next));
    let item = size_of::<crate::tab_stop::TabStopOperation>();
    if let Some(target) =
        trims
            .tab_history
            .note(rendered.tab_stops.insertion_history.len(), capacity, item)
    {
        rendered.tab_stops.insertion_history.shrink_to(target);
        next.tab_stops.insertion_history.shrink_to(target);
    }

    let log = &mut cx.entities.access_log;
    let (used, capacity) = log.take_frame_use();
    if let Some(target) = trims
        .access_log
        .note(used, capacity, size_of::<crate::EntityId>())
    {
        log.trim_access_log(target);
    }
}

#[cfg(test)]
mod tests {
    use super::{LOW_FRAMES, MIN_BYTES, Trim};

    #[test]
    fn only_a_lasting_drop_trims_once() {
        let capacity = MIN_BYTES;
        let mut trim = Trim::default();
        // Steady, or alternating with a large frame: never.
        for frame in 0..=100 {
            let used = if frame % 4 == 0 { capacity } else { 10 };
            assert_eq!(trim.note(used, capacity, 1), None);
        }
        for _ in 0..LOW_FRAMES - 1 {
            assert_eq!(trim.note(10, capacity, 1), None);
        }
        assert_eq!(trim.note(12, capacity, 1), Some(24));
        // Once trimmed, a buffer near its use is left alone.
        assert_eq!(trim.note(12, 24, 1), None);
    }
}
