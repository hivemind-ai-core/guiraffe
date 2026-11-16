//! Deterministic toast admission and timekeeping; queued toasts start their clock on promotion.
use std::{collections::VecDeque, time::Duration};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToastId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastPosition {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}
impl ToastPosition {
    pub const ALL: [Self; 6] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];
}
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: ToastId,
    pub message: String,
    pub position: ToastPosition,
    pub auto_close: Option<Duration>,
    pub elapsed: Duration,
    pub paused: bool,
}
impl Toast {
    pub fn new(id: ToastId, message: impl Into<String>) -> Self {
        Self {
            id,
            message: message.into(),
            position: Default::default(),
            auto_close: Some(Duration::from_secs(5)),
            elapsed: Duration::ZERO,
            paused: false,
        }
    }
    /// Remaining fraction, or `None` for persistent toasts.
    pub fn progress(&self) -> Option<f32> {
        self.auto_close.map(|duration| {
            if duration.is_zero() {
                0.
            } else {
                (1. - self.elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0., 1.)
            }
        })
    }
}
#[derive(Debug)]
pub struct ToastQueue {
    limit: usize,
    visible: Vec<Toast>,
    queued: VecDeque<Toast>,
}
impl Default for ToastQueue {
    fn default() -> Self {
        Self::new(5)
    }
}
impl ToastQueue {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            visible: Vec::new(),
            queued: VecDeque::new(),
        }
    }
    pub fn limit(&self) -> usize {
        self.limit
    }
    pub fn visible(&self) -> &[Toast] {
        &self.visible
    }
    pub fn queued(&self) -> &VecDeque<Toast> {
        &self.queued
    }
    /// Existing IDs update in place and reset their timer without changing admission order.
    pub fn push(&mut self, mut toast: Toast) {
        toast.elapsed = Duration::ZERO;
        toast.paused = false;
        if let Some(old) = self
            .visible
            .iter_mut()
            .chain(self.queued.iter_mut())
            .find(|t| t.id == toast.id)
        {
            toast.paused = old.paused;
            *old = toast;
        } else {
            self.queued.push_back(toast);
        }
        self.promote();
    }
    pub fn remove(&mut self, id: ToastId) {
        self.visible.retain(|t| t.id != id);
        self.queued.retain(|t| t.id != id);
        self.promote();
    }
    pub fn pause(&mut self, id: ToastId, paused: bool) {
        if let Some(t) = self.visible.iter_mut().find(|t| t.id == id) {
            t.paused = paused;
        }
    }
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        if self.visible.len() > limit {
            let overflow = self.visible.split_off(limit);
            for mut t in overflow.into_iter().rev() {
                t.elapsed = Duration::ZERO;
                t.paused = false;
                self.queued.push_front(t);
            }
        }
        self.promote();
    }
    pub fn clear(&mut self) {
        self.visible.clear();
        self.queued.clear();
    }
    /// Advance the currently visible set only. Promotion happens after this tick.
    pub fn advance(&mut self, delta: Duration) {
        for t in &mut self.visible {
            if !t.paused {
                t.elapsed = t.elapsed.saturating_add(delta);
            }
        }
        self.visible
            .retain(|t| t.auto_close.is_none_or(|d| t.elapsed < d));
        self.promote();
    }
    fn promote(&mut self) {
        while self.visible.len() < self.limit {
            let Some(mut t) = self.queued.pop_front() else {
                break;
            };
            t.elapsed = Duration::ZERO;
            t.paused = false;
            self.visible.push(t);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifo_and_fresh_clock() {
        let mut q = ToastQueue::new(1);
        q.push(Toast::new(ToastId(1), "a"));
        q.push(Toast::new(ToastId(2), "b"));
        q.advance(Duration::from_secs(10));
        assert_eq!(q.visible()[0].id, ToastId(2));
        assert_eq!(q.visible()[0].progress(), Some(1.));
    }
    #[test]
    fn hover_pause_resume_and_progress() {
        let mut q = ToastQueue::default();
        q.push(Toast::new(ToastId(1), "a"));
        q.advance(Duration::from_secs(1));
        assert_eq!(q.visible()[0].progress(), Some(0.8));
        q.pause(ToastId(1), true);
        q.advance(Duration::from_secs(100));
        assert_eq!(q.visible().len(), 1);
        q.pause(ToastId(1), false);
        q.advance(Duration::from_secs(4));
        assert!(q.visible().is_empty());
    }
    #[test]
    fn update_visible_and_queued() {
        let mut q = ToastQueue::new(1);
        for i in 1..=2 {
            q.push(Toast::new(ToastId(i), "old"));
        }
        q.advance(Duration::from_secs(1));
        q.push(Toast::new(ToastId(1), "new"));
        q.push(Toast::new(ToastId(2), "queued update"));
        assert_eq!(q.visible()[0].message, "new");
        assert_eq!(q.visible()[0].elapsed, Duration::ZERO);
        q.remove(ToastId(1));
        assert_eq!(q.visible()[0].message, "queued update");
    }
    #[test]
    fn persistent_zero_limit_resize_clear() {
        let mut q = ToastQueue::new(0);
        let mut t = Toast::new(ToastId(1), "a");
        t.auto_close = None;
        q.push(t);
        assert!(q.visible().is_empty());
        q.set_limit(1);
        q.advance(Duration::MAX);
        assert_eq!(q.visible()[0].progress(), None);
        q.set_limit(0);
        assert_eq!(q.queued().len(), 1);
        q.clear();
        assert!(q.queued().is_empty());
    }
    #[test]
    fn shrink_preserves_fifo() {
        let mut q = ToastQueue::new(3);
        for i in 0..4 {
            q.push(Toast::new(ToastId(i), "a"));
        }
        q.set_limit(1);
        assert_eq!(
            q.queued().iter().map(|t| t.id.0).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        q.remove(ToastId(2));
        q.remove(ToastId(0));
        assert_eq!(q.visible()[0].id, ToastId(1));
    }
}
#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn review_3_updating_visible_toast_preserves_hover_pause() {
        let mut queue = ToastQueue::new(1);
        queue.push(Toast::new(ToastId(1), "old"));
        queue.pause(ToastId(1), true);
        queue.push(Toast::new(ToastId(1), "new"));
        assert!(queue.visible()[0].paused);
        queue.advance(Duration::from_secs(20));
        assert_eq!(queue.visible().len(), 1);
    }
}
