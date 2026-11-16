//! Scroll policy separate from GPUI's variable-height list implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stickiness { following: bool, unread: usize }
impl Default for Stickiness {fn default()->Self {Self {following: true, unread: 0}}}
impl Stickiness {
    pub fn following(self)->bool {self.following}
    pub fn unread(self)->usize {self.unread}
    /// Called only for an actual user scroll. Content growth must not detach the tail.
    pub fn user_scroll(&mut self, at_tail: bool) {self.following=at_tail; if at_tail {self.unread=0;}}
    /// Token updates belong to the same message and must not inflate the unread count.
    pub fn appended_message(&mut self) {if !self.following {self.unread=self.unread.saturating_add(1);}}
    pub fn jump(&mut self) {self.following=true;self.unread=0;}
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn follows_until_user_leaves() {let mut s=Stickiness::default();s.appended_message();assert!(s.following());assert_eq!(s.unread(),0);s.user_scroll(false);s.appended_message();s.appended_message();assert!(!s.following());assert_eq!(s.unread(),2);s.user_scroll(true);assert_eq!(s,Stickiness::default());}
    #[test] fn jump_resets_unread() {let mut s=Stickiness::default();s.user_scroll(false);s.appended_message();s.jump();assert_eq!(s,Stickiness::default());}
    #[test] fn detached_stream_does_not_reattach() {let mut s=Stickiness::default();s.user_scroll(false);for _ in 0..1000 {assert!(!s.following());}assert_eq!(s.unread(),0);}
}
