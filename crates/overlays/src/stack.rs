//! Pure stack ordering and dismissal admission. The Controls bridge performs focus/scroll effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OverlayId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Modal,
    Drawer,
    Spotlight,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismiss {
    Escape,
    Outside,
    Programmatic,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub escape: bool,
    pub outside: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            escape: true,
            outside: true,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: OverlayId,
    pub kind: OverlayKind,
    pub policy: Policy,
}
#[derive(Debug, Default)]
pub struct OverlayStack {
    next: u64,
    entries: Vec<Entry>,
}
impl OverlayStack {
    pub fn push(&mut self, kind: OverlayKind, policy: Policy) -> OverlayId {
        self.next += 1;
        let id = OverlayId(self.next);
        self.entries.push(Entry { id, kind, policy });
        id
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn top(&self) -> Option<&Entry> {
        self.entries.last()
    }
    pub fn scroll_locked(&self) -> bool {
        !self.entries.is_empty()
    }
    /// Only the top overlay can consume dismissal. A rejecting top never dismisses the layer below.
    pub fn dismiss(&mut self, id: OverlayId, reason: Dismiss) -> bool {
        if !self.top().is_some_and(|e| {
            e.id == id
                && match reason {
                    Dismiss::Escape => e.policy.escape,
                    Dismiss::Outside => e.policy.outside,
                    Dismiss::Programmatic => true,
                }
        }) {
            return false;
        }
        self.entries.pop();
        true
    }
    pub fn z_index(&self, id: OverlayId) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn top_policy_blocks_background_and_scroll_unlocks_last() {
        let mut s = OverlayStack::default();
        let a = s.push(OverlayKind::Modal, Policy::default());
        let b = s.push(
            OverlayKind::Modal,
            Policy {
                escape: false,
                outside: false,
            },
        );
        assert_eq!(s.z_index(b), Some(1));
        assert!(!s.dismiss(a, Dismiss::Programmatic));
        assert!(!s.dismiss(b, Dismiss::Escape));
        assert!(!s.dismiss(b, Dismiss::Outside));
        assert!(s.dismiss(b, Dismiss::Programmatic));
        assert!(s.scroll_locked());
        assert!(s.dismiss(a, Dismiss::Escape));
        assert!(!s.scroll_locked());
    }
    #[test]
    fn ids_never_reused() {
        let mut s = OverlayStack::default();
        let a = s.push(OverlayKind::Drawer, Policy::default());
        s.dismiss(a, Dismiss::Outside);
        let b = s.push(OverlayKind::Spotlight, Policy::default());
        assert_ne!(a, b);
        assert_eq!(s.z_index(a), None);
    }
}
