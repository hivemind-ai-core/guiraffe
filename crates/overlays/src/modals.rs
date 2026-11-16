//! Window-local modal service. Callbacks run through Controls' native focus scopes.
use crate::Modal as Dialog;
use crate::{
    ConfirmResult, ContextBuilder, OverlayError, OverlayHost,
    stack::{OverlayId, Policy},
};
use gpui_kit::{App, SharedString, Window};
pub fn open(
    window: &mut Window,
    cx: &mut App,
    policy: Policy,
    build: impl Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
) -> Result<OverlayId, OverlayError> {
    OverlayHost::open(window, cx, policy, build)
}
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
) -> Result<(OverlayId, ConfirmResult), OverlayError> {
    OverlayHost::confirm(window, cx, title, message)
}
pub fn close(window: &mut Window, cx: &mut App, id: OverlayId) -> Result<(), OverlayError> {
    OverlayHost::close(window, cx, id)
}
pub fn close_all(window: &mut Window, cx: &mut App) -> Result<(), OverlayError> {
    OverlayHost::close_all(window, cx)
}
pub fn register_context(
    window: &mut Window,
    cx: &mut App,
    name: impl Into<String>,
    build: ContextBuilder,
) -> Result<(), OverlayError> {
    OverlayHost::register_context(window, cx, name, build)
}
pub fn open_context(
    window: &mut Window,
    cx: &mut App,
    name: &str,
    policy: Policy,
) -> Result<OverlayId, OverlayError> {
    OverlayHost::open_context(window, cx, name, policy)
}
