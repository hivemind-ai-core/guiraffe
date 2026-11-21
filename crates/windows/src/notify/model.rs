//! Notification data and the pure stack model (admission, queueing, timers, leave state).
use std::{collections::VecDeque, time::Duration};

use serde::{Deserialize, Serialize};

use super::{
    stack::{CardMetrics, StackConfig, StackLayout, layout},
    timer::DismissTimer,
};

/// Identity of a window notification; reuse it to update in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WindowNotificationId(pub u64);

/// Semantic kind: picks the accent color and default icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum NotificationKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
    /// No accent, no default icon.
    Neutral,
}

/// The card's icon.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CardIcon {
    /// The kind's icon (none for [`NotificationKind::Neutral`]).
    #[default]
    Auto,
    None,
    /// An SVG from the app's asset source, e.g. `"icons/download.svg"`.
    Svg(String),
}

/// A button on a card. Pressing it emits its `key` and dismisses the card.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardAction {
    pub key: String,
    pub label: String,
    /// Rendered as the primary (filled) button.
    pub primary: bool,
}

/// The content of an in-app desktop notification card. Props are data (conventions §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowNotification {
    pub title: String,
    /// Clamped to two lines.
    pub body: String,
    pub kind: NotificationKind,
    pub icon: CardIcon,
    pub actions: Vec<CardAction>,
    /// Determinate progress 0..=1 (e.g. a download). `None` hides the bar.
    pub progress: Option<f32>,
    /// Auto-dismiss after this long; `None` is sticky. Paused while hovered.
    pub timeout: Option<Duration>,
    /// Clicking the card body emits `Clicked` and dismisses it.
    pub dismiss_on_click: bool,
    /// Show the close button.
    pub closable: bool,
}

impl Default for WindowNotification {
    fn default() -> Self {
        Self {
            title: String::new(),
            body: String::new(),
            kind: NotificationKind::Info,
            icon: CardIcon::Auto,
            actions: Vec::new(),
            progress: None,
            timeout: Some(Duration::from_secs(5)),
            dismiss_on_click: true,
            closable: true,
        }
    }
}

impl WindowNotification {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }
    pub fn kind(mut self, kind: NotificationKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn icon(mut self, icon: CardIcon) -> Self {
        self.icon = icon;
        self
    }
    /// Add a button; the first added with `primary` set is filled.
    pub fn action(mut self, key: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push(CardAction {
            key: key.into(),
            label: label.into(),
            primary: false,
        });
        self
    }
    pub fn primary_action(mut self, key: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push(CardAction {
            key: key.into(),
            label: label.into(),
            primary: true,
        });
        self
    }
    /// Progress 0..=1; values outside are clamped, NaN hides the bar.
    pub fn progress(mut self, value: f32) -> Self {
        debug_assert!(!value.is_nan(), "progress is NaN");
        self.progress = (!value.is_nan()).then(|| value.clamp(0.0, 1.0));
        self
    }
    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn sticky(self) -> Self {
        self.timeout(None)
    }
    pub fn dismiss_on_click(mut self, value: bool) -> Self {
        self.dismiss_on_click = value;
        self
    }
    pub fn closable(mut self, value: bool) -> Self {
        self.closable = value;
        self
    }
    /// Deterministic card height for stacking.
    pub fn height(&self, metrics: &CardMetrics) -> f32 {
        metrics.height(
            !self.body.is_empty(),
            !self.actions.is_empty(),
            self.progress.is_some(),
        )
    }
}

/// One card in the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: WindowNotificationId,
    pub content: WindowNotification,
    pub timer: DismissTimer,
    /// Exit animation running; the card keeps its slot until [`StackModel::finish_leave`].
    pub leaving: bool,
    /// Bumped on every update, so views can restart per-card animations.
    pub revision: u64,
}

/// What [`StackModel::push`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pushed {
    Shown,
    Queued,
    Updated,
}

/// Admission and timing for one corner's stack. Pure: time comes in as `now`.
#[derive(Debug, Clone, Default)]
pub struct StackModel {
    max_visible: usize,
    /// Newest first.
    visible: Vec<Card>,
    queued: VecDeque<Card>,
    paused: bool,
}

impl StackModel {
    pub fn new(max_visible: usize) -> Self {
        Self {
            max_visible: max_visible.max(1),
            ..Default::default()
        }
    }
    pub fn visible(&self) -> &[Card] {
        &self.visible
    }
    pub fn queued(&self) -> impl Iterator<Item = &Card> {
        self.queued.iter()
    }
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.queued.is_empty()
    }
    pub fn get(&self, id: WindowNotificationId) -> Option<&Card> {
        self.visible
            .iter()
            .chain(self.queued.iter())
            .find(|c| c.id == id)
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Show, queue, or update in place (same id: new content, timer restarted).
    pub fn push(
        &mut self,
        id: WindowNotificationId,
        content: WindowNotification,
        now: Duration,
    ) -> Pushed {
        if let Some(card) = self.visible.iter_mut().find(|c| c.id == id) {
            card.timer.restart(content.timeout, now);
            card.content = content;
            card.revision += 1;
            // An update revives a card that was on its way out.
            card.leaving = false;
            return Pushed::Updated;
        }
        if let Some(card) = self.queued.iter_mut().find(|c| c.id == id) {
            card.content = content;
            card.revision += 1;
            return Pushed::Updated;
        }
        let mut timer = DismissTimer::start(content.timeout, now);
        if self.paused {
            timer.pause(now);
        }
        let card = Card {
            id,
            content,
            timer,
            leaving: false,
            revision: 0,
        };
        if self.visible.len() < self.max_visible {
            self.visible.insert(0, card);
            Pushed::Shown
        } else {
            self.queued.push_back(card);
            Pushed::Queued
        }
    }

    /// Start removing `id`: visible cards begin leaving, queued ones are dropped. Returns whether
    /// it was present (and not already leaving).
    pub fn begin_leave(&mut self, id: WindowNotificationId) -> bool {
        if let Some(card) = self.visible.iter_mut().find(|c| c.id == id) {
            if card.leaving {
                return false;
            }
            card.leaving = true;
            return true;
        }
        let before = self.queued.len();
        self.queued.retain(|c| c.id != id);
        self.queued.len() != before
    }

    /// Drop a leaving card and promote queued ones (their timers start at `now`). Returns the
    /// promoted ids.
    pub fn finish_leave(
        &mut self,
        id: WindowNotificationId,
        now: Duration,
    ) -> Vec<WindowNotificationId> {
        self.visible.retain(|c| !(c.id == id && c.leaving));
        self.promote(now)
    }

    fn promote(&mut self, now: Duration) -> Vec<WindowNotificationId> {
        let mut promoted = Vec::new();
        while self.visible.len() < self.max_visible {
            let Some(mut card) = self.queued.pop_front() else {
                break;
            };
            card.timer = DismissTimer::start(card.content.timeout, now);
            if self.paused {
                card.timer.pause(now);
            }
            promoted.push(card.id);
            // Promoted cards join at the far end: they are older than the visible ones.
            self.visible.push(card);
        }
        promoted
    }

    /// Change how many cards show at once (promotes or keeps extra cards until they close).
    pub fn set_max_visible(&mut self, max: usize, now: Duration) -> Vec<WindowNotificationId> {
        self.max_visible = max.max(1);
        self.promote(now)
    }

    /// Pause or resume every visible timer (pointer over the stack).
    pub fn set_paused(&mut self, paused: bool, now: Duration) -> bool {
        if self.paused == paused {
            return false;
        }
        self.paused = paused;
        for card in &mut self.visible {
            if paused {
                card.timer.pause(now);
            } else {
                card.timer.resume(now);
            }
        }
        true
    }

    /// Visible, not-yet-leaving cards whose time is up.
    pub fn expired(&self, now: Duration) -> Vec<WindowNotificationId> {
        self.visible
            .iter()
            .filter(|c| !c.leaving && c.timer.is_expired(now))
            .map(|c| c.id)
            .collect()
    }

    /// The earliest clock time a visible card expires, to schedule one wake-up.
    pub fn next_deadline(&self, now: Duration) -> Option<Duration> {
        self.visible
            .iter()
            .filter(|c| !c.leaving)
            .filter_map(|c| c.timer.deadline(now))
            .min()
    }

    /// Geometry of the visible cards (leaving ones keep their slot).
    pub fn layout(&self, config: &StackConfig, metrics: &CardMetrics) -> StackLayout {
        let heights: Vec<f32> = self
            .visible
            .iter()
            .map(|c| c.content.height(metrics))
            .collect();
        layout(config, &heights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::timer::{Clock, ManualClock};

    fn id(n: u64) -> WindowNotificationId {
        WindowNotificationId(n)
    }
    fn note(t: &str, secs: u64) -> WindowNotification {
        WindowNotification::new(t).timeout(Some(Duration::from_secs(secs)))
    }

    #[test]
    fn newest_first_then_queue_and_promote() {
        let clock = ManualClock::new();
        let mut m = StackModel::new(2);
        assert_eq!(m.push(id(1), note("a", 5), clock.now()), Pushed::Shown);
        assert_eq!(m.push(id(2), note("b", 5), clock.now()), Pushed::Shown);
        assert_eq!(m.push(id(3), note("c", 5), clock.now()), Pushed::Queued);
        let order: Vec<u64> = m.visible().iter().map(|c| c.id.0).collect();
        assert_eq!(order, [2, 1]);
        clock.advance(Duration::from_secs(4));
        assert!(m.begin_leave(id(2)));
        assert_eq!(m.visible().len(), 2, "leaving card keeps its slot");
        assert_eq!(m.finish_leave(id(2), clock.now()), [id(3)]);
        let promoted = m.get(id(3)).map(|c| c.timer);
        assert_eq!(
            promoted.and_then(|t| t.remaining(clock.now())),
            Some(Duration::from_secs(5)),
            "queued card's clock starts on promotion"
        );
    }

    #[test]
    fn auto_dismiss_with_fake_clock_and_hover_pause() {
        let clock = ManualClock::new();
        let mut m = StackModel::new(3);
        m.push(id(1), note("a", 3), clock.now());
        m.push(id(2), note("b", 6), clock.now());
        m.push(
            id(3),
            WindowNotification::new("sticky").sticky(),
            clock.now(),
        );
        assert_eq!(m.next_deadline(clock.now()), Some(Duration::from_secs(3)));
        clock.advance(Duration::from_secs(2));
        assert!(m.set_paused(true, clock.now()));
        assert!(!m.set_paused(true, clock.now()));
        assert_eq!(
            m.next_deadline(clock.now()),
            None,
            "nothing expires while hovered"
        );
        clock.advance(Duration::from_secs(100));
        assert!(m.expired(clock.now()).is_empty());
        m.set_paused(false, clock.now());
        clock.advance(Duration::from_secs(1));
        assert_eq!(m.expired(clock.now()), [id(1)]);
        m.begin_leave(id(1));
        assert!(
            m.expired(clock.now()).is_empty(),
            "leaving cards don't expire twice"
        );
        clock.advance(Duration::from_secs(3));
        assert_eq!(m.expired(clock.now()), [id(2)]);
    }

    #[test]
    fn update_in_place_restarts_timer_and_revives() {
        let clock = ManualClock::new();
        let mut m = StackModel::new(3);
        m.push(id(1), note("a", 5), clock.now());
        clock.advance(Duration::from_secs(4));
        m.begin_leave(id(1));
        assert_eq!(
            m.push(id(1), note("a2", 5).progress(0.5), clock.now()),
            Pushed::Updated
        );
        let card = m.get(id(1)).cloned();
        assert_eq!(card.as_ref().map(|c| c.content.title.as_str()), Some("a2"));
        assert_eq!(card.as_ref().map(|c| c.leaving), Some(false));
        assert_eq!(card.as_ref().map(|c| c.revision), Some(1));
        assert_eq!(
            card.and_then(|c| c.timer.remaining(clock.now())),
            Some(Duration::from_secs(5))
        );
        assert_eq!(m.visible().len(), 1);
    }

    #[test]
    fn queued_cards_can_be_removed_and_updated() {
        let mut m = StackModel::new(1);
        m.push(id(1), note("a", 5), Duration::ZERO);
        m.push(id(2), note("b", 5), Duration::ZERO);
        assert_eq!(
            m.push(id(2), note("b2", 5), Duration::ZERO),
            Pushed::Updated
        );
        assert_eq!(
            m.queued().next().map(|c| c.content.title.as_str()),
            Some("b2")
        );
        assert!(m.begin_leave(id(2)));
        assert!(m.queued().next().is_none());
        assert!(!m.begin_leave(id(9)));
    }

    #[test]
    fn layout_uses_deterministic_heights() {
        let metrics = CardMetrics {
            base: 70.0,
            no_body: -18.0,
            actions: 34.0,
            progress: 12.0,
        };
        let mut m = StackModel::new(3);
        m.push(
            id(1),
            WindowNotification::new("t").body("b"),
            Duration::ZERO,
        );
        m.push(
            id(2),
            WindowNotification::new("t").action("x", "X").progress(0.2),
            Duration::ZERO,
        );
        let l = m.layout(&StackConfig::default(), &metrics);
        let hs: Vec<f32> = l.slots.iter().map(|r| r.h).collect();
        assert_eq!(hs, [70.0 - 18.0 + 34.0 + 12.0, 70.0]);
    }

    #[test]
    fn progress_is_clamped() {
        assert_eq!(
            WindowNotification::new("t").progress(2.0).progress,
            Some(1.0)
        );
        assert_eq!(
            WindowNotification::new("t").progress(-1.0).progress,
            Some(0.0)
        );
    }
}
