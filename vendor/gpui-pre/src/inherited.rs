// Added for GUIraffe (see vendor/README.md).
//! State an embedder keeps outside the window for as long as an element lays
//! out, prepaints and paints its children (a style context stack, say).
//!
//! The same interface as gpui-fast's, which builds a notified view again
//! without the views around it and has to carry this state to it. This
//! backend renders every view inside its ancestors, so the hook is never
//! called.

/// The embedder's side of state inherited outside the window. The state is a
/// single word; `0` stands for "nothing inherited".
pub trait InheritedState {
    /// The state in effect now.
    fn capture(&self) -> u64;
    /// Puts `state` in effect, over whatever is, until the matching
    /// [`InheritedState::leave`].
    fn enter(&self, state: u64);
    /// Ends the innermost [`InheritedState::enter`].
    fn leave(&self);
}

/// Sets the hook for state inherited outside the window. Upstream renders
/// every view with its ancestors, so nothing needs carrying and the hook is
/// not kept.
pub fn set_inherited_state(_state: &'static dyn InheritedState) {}
