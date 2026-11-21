// Added for GUIraffe (see vendor/README.md).
//! State an embedder keeps outside the window for as long as an element lays
//! out, prepaints and paints its children (a style context stack, say), which
//! the views rendered inside those children read.
//!
//! A view built again on its own, without the views around it (see
//! [`crate::fast::splice`]), does not run inside those elements. The window
//! captures the state where the view was laid out, as it captures the text
//! style, puts it back while the view is built again there, and lays a
//! retained view out again from scratch when the state it was laid out in
//! changed.

use std::cell::Cell;

/// The embedder's side of state inherited outside the window. The state is a
/// single word, so capturing it never allocates; `0` stands for "nothing
/// inherited" and is what is captured while no hook is set.
pub trait InheritedState {
    /// The state in effect now.
    fn capture(&self) -> u64;
    /// Puts `state` in effect, over whatever is, until the matching
    /// [`InheritedState::leave`].
    fn enter(&self, state: u64);
    /// Ends the innermost [`InheritedState::enter`].
    fn leave(&self);
}

thread_local! {
    static HOOK: Cell<Option<&'static dyn InheritedState>> = const { Cell::new(None) };
}

/// Sets the hook for state inherited outside the window, on this thread (the
/// UI thread). A later call replaces it.
pub fn set_inherited_state(state: &'static dyn InheritedState) {
    HOOK.set(Some(state));
}

#[inline]
pub(crate) fn capture() -> u64 {
    HOOK.get().map_or(0, |hook| hook.capture())
}

/// Puts `state` in effect, returning the hook to [`leave`] with: the one in
/// effect now, even if another is set before then.
#[inline]
pub(crate) fn enter(state: u64) -> Option<&'static dyn InheritedState> {
    let hook = HOOK.get()?;
    hook.enter(state);
    Some(hook)
}

#[inline]
pub(crate) fn leave(entered: Option<&'static dyn InheritedState>) {
    if let Some(hook) = entered {
        hook.leave();
    }
}
