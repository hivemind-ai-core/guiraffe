//! Layout-stability assertions (API conventions §12.2): focus, hover and selected states must not
//! change a control's bounds or its neighbours'.
//!
//! Wrap each control and its siblings in a `debug_selector` (a wrapper sizes to its child,
//! so a resized control or a moved neighbour both show up), capture once, change state,
//! then compare.

use gpui_kit::{Bounds, Pixels, VisualTestContext};

/// Laid-out bounds of each debug selector in the last frame.
pub fn capture(vcx: &mut VisualTestContext, selectors: &[&'static str]) -> Vec<Bounds<Pixels>> {
    vcx.run_until_parked();
    selectors
        .iter()
        .map(|s| {
            vcx.debug_bounds(s)
                .unwrap_or_else(|| panic!("selector {s:?} was not laid out"))
        })
        .collect()
}

/// Assert every selector has exactly the bounds captured in `base` (`state` names the change).
pub fn assert_same_bounds(
    vcx: &mut VisualTestContext,
    selectors: &[&'static str],
    base: &[Bounds<Pixels>],
    state: &str,
) {
    let now = capture(vcx, selectors);
    for ((sel, a), b) in selectors.iter().zip(base).zip(&now) {
        assert_eq!(a, b, "{sel:?} moved or resized when {state}");
    }
}
