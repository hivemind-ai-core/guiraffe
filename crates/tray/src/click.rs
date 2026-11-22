//! Single/double click classification from raw activations. Pure: time comes in as `now`.
use std::time::Duration;

/// What an activation means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickKind {
    Single,
    /// The second activation within the double-click interval. The first was already reported
    /// as [`ClickKind::Single`] (no delay is added to single clicks).
    Double,
}

/// Turns activations into single and double clicks.
///
/// StatusNotifierItem and macOS only report activations, so double clicks are detected here;
/// a platform that reports native double clicks (Windows) feeds them through
/// [`ClickTracker::native_double`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickTracker {
    interval: Duration,
    last: Option<Duration>,
}

impl ClickTracker {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }
    pub fn interval(&self) -> Duration {
        self.interval
    }
    /// Classify an activation at `now`.
    pub fn activate(&mut self, now: Duration) -> ClickKind {
        match self.last {
            Some(last) if now.saturating_sub(last) <= self.interval && now >= last => {
                // A third quick click starts a new pair.
                self.last = None;
                ClickKind::Double
            }
            _ => {
                self.last = Some(now);
                ClickKind::Single
            }
        }
    }
    /// A platform-reported double click; resets pairing.
    pub fn native_double(&mut self) -> ClickKind {
        self.last = None;
        ClickKind::Double
    }
}

impl Default for ClickTracker {
    fn default() -> Self {
        Self::new(Duration::from_millis(400))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn two_quick_activations_are_a_double_click() {
        let mut t = ClickTracker::new(ms(400));
        assert_eq!(t.activate(ms(1000)), ClickKind::Single);
        assert_eq!(t.activate(ms(1300)), ClickKind::Double);
        assert_eq!(
            t.activate(ms(1350)),
            ClickKind::Single,
            "third click starts over"
        );
        assert_eq!(t.activate(ms(1700)), ClickKind::Double);
    }

    #[test]
    fn slow_activations_stay_single() {
        let mut t = ClickTracker::new(ms(400));
        assert_eq!(t.activate(ms(0)), ClickKind::Single);
        assert_eq!(t.activate(ms(401)), ClickKind::Single);
        assert_eq!(t.activate(ms(900)), ClickKind::Single);
    }

    #[test]
    fn clock_going_backwards_is_not_a_double_click() {
        let mut t = ClickTracker::new(ms(400));
        t.activate(ms(1000));
        assert_eq!(t.activate(ms(900)), ClickKind::Single);
    }

    #[test]
    fn native_double_resets_pairing() {
        let mut t = ClickTracker::default();
        t.activate(ms(0));
        assert_eq!(t.native_double(), ClickKind::Double);
        assert_eq!(t.activate(ms(10)), ClickKind::Single);
    }
}
