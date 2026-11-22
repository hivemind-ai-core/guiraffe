//! Opt-in focus for non-text controls (API conventions §12.3), prototype.
//!
//! [`ScopedButton`] wraps a GPUI Controls `Button` and decides its tab stop at render time
//! from [`guiraffe_core::StyleContext`]: inside [`guiraffe_core::focus_scope`] it joins Tab
//! navigation; outside, it is never a tab stop. Kit's Button already suppresses focus on
//! mouse down, so a pointer click never steals focus from a text input either way.

use gpui_kit::{App, IntoElement, RenderOnce, Window, component::button::Button};

#[derive(IntoElement)]
pub struct ScopedButton {
    inner: Button,
    focusable: Option<bool>,
}

impl ScopedButton {
    pub fn new(inner: Button) -> Self {
        Self {
            inner,
            focusable: None,
        }
    }

    /// Opt this one control in (or out) regardless of the enclosing scope.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = Some(focusable);
        self
    }
}

impl RenderOnce for ScopedButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.inner.tab_stop(guiraffe_core::tab_stop(self.focusable))
    }
}
