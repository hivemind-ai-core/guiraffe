//! Pure auto-dismiss timing, driven by an explicit clock so tests use a fake one.
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

/// A monotonic clock. [`SystemClock`] in apps, [`ManualClock`] in tests.
pub trait Clock {
    /// Time since an arbitrary fixed origin.
    fn now(&self) -> Duration;
}

/// The real monotonic clock.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// A clock that only moves when told to. Clones share the same time.
#[derive(Debug, Clone, Default)]
pub struct ManualClock(Rc<Cell<Duration>>);

impl ManualClock {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn advance(&self, by: Duration) {
        self.0.set(self.0.get().saturating_add(by));
    }
    pub fn set(&self, now: Duration) {
        self.0.set(now);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// Counts down a notification's lifetime. Paused time (hover) doesn't count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DismissTimer {
    /// `None`: sticky, never expires.
    duration: Option<Duration>,
    /// Time used before the current run.
    banked: Duration,
    /// When the current run started; `None` while paused.
    running_since: Option<Duration>,
}

impl DismissTimer {
    /// A running timer started at `now`.
    pub fn start(duration: Option<Duration>, now: Duration) -> Self {
        Self {
            duration,
            banked: Duration::ZERO,
            running_since: Some(now),
        }
    }
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }
    pub fn is_paused(&self) -> bool {
        self.running_since.is_none()
    }
    /// Time counted so far.
    pub fn elapsed(&self, now: Duration) -> Duration {
        self.banked
            + self
                .running_since
                .map_or(Duration::ZERO, |since| now.saturating_sub(since))
    }
    /// Time left, `None` when sticky.
    pub fn remaining(&self, now: Duration) -> Option<Duration> {
        self.duration.map(|d| d.saturating_sub(self.elapsed(now)))
    }
    pub fn is_expired(&self, now: Duration) -> bool {
        self.remaining(now).is_some_and(|r| r.is_zero())
    }
    /// Fraction of the lifetime left (1 → 0), `None` when sticky.
    pub fn fraction_left(&self, now: Duration) -> Option<f32> {
        let d = self.duration?;
        if d.is_zero() {
            return Some(0.0);
        }
        let left = self.remaining(now)?;
        Some((left.as_secs_f32() / d.as_secs_f32()).clamp(0.0, 1.0))
    }
    /// Stop counting. Returns false if already paused.
    pub fn pause(&mut self, now: Duration) -> bool {
        match self.running_since.take() {
            Some(since) => {
                self.banked += now.saturating_sub(since);
                true
            }
            None => false,
        }
    }
    /// Continue counting. Returns false if already running.
    pub fn resume(&mut self, now: Duration) -> bool {
        if self.running_since.is_some() {
            return false;
        }
        self.running_since = Some(now);
        true
    }
    /// Start over with a new lifetime (an update), keeping the paused state.
    pub fn restart(&mut self, duration: Option<Duration>, now: Duration) {
        let paused = self.is_paused();
        *self = Self::start(duration, now);
        if paused {
            self.running_since = None;
        }
    }
    /// When (on the clock) this timer expires if it keeps running; `None` if sticky or paused.
    pub fn deadline(&self, now: Duration) -> Option<Duration> {
        self.running_since?;
        self.remaining(now).map(|r| now + r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: u64) -> Duration {
        Duration::from_secs(v)
    }

    #[test]
    fn counts_down_and_expires() {
        let clock = ManualClock::new();
        let t = DismissTimer::start(Some(s(5)), clock.now());
        clock.advance(s(3));
        assert_eq!(t.remaining(clock.now()), Some(s(2)));
        assert_eq!(t.fraction_left(clock.now()), Some(0.4));
        assert!(!t.is_expired(clock.now()));
        clock.advance(s(2));
        assert!(t.is_expired(clock.now()));
        clock.advance(s(10));
        assert_eq!(t.remaining(clock.now()), Some(Duration::ZERO), "saturates");
    }

    #[test]
    fn hover_pauses_the_countdown() {
        let clock = ManualClock::new();
        let mut t = DismissTimer::start(Some(s(5)), clock.now());
        clock.advance(s(2));
        assert!(t.pause(clock.now()));
        assert!(!t.pause(clock.now()), "idempotent");
        clock.advance(s(60));
        assert_eq!(
            t.remaining(clock.now()),
            Some(s(3)),
            "paused time doesn't count"
        );
        assert_eq!(t.deadline(clock.now()), None);
        assert!(t.resume(clock.now()));
        assert!(!t.resume(clock.now()));
        assert_eq!(t.deadline(clock.now()), Some(clock.now() + s(3)));
        clock.advance(s(3));
        assert!(t.is_expired(clock.now()));
    }

    #[test]
    fn sticky_never_expires() {
        let clock = ManualClock::new();
        let t = DismissTimer::start(None, clock.now());
        clock.advance(s(1_000_000));
        assert!(!t.is_expired(clock.now()));
        assert_eq!(t.remaining(clock.now()), None);
        assert_eq!(t.fraction_left(clock.now()), None);
        assert_eq!(t.deadline(clock.now()), None);
    }

    #[test]
    fn restart_keeps_pause_state() {
        let clock = ManualClock::new();
        let mut t = DismissTimer::start(Some(s(5)), clock.now());
        clock.advance(s(4));
        t.pause(clock.now());
        t.restart(Some(s(10)), clock.now());
        assert!(t.is_paused());
        assert_eq!(t.remaining(clock.now()), Some(s(10)));
    }

    #[test]
    fn zero_duration_is_immediately_expired() {
        let t = DismissTimer::start(Some(Duration::ZERO), Duration::ZERO);
        assert!(t.is_expired(Duration::ZERO));
        assert_eq!(t.fraction_left(Duration::ZERO), Some(0.0));
    }
}
