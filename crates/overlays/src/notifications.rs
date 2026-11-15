//! Toast service. Reusing an ID updates its message, position and lifetime in place.
use crate::{
    OverlayError, OverlayHost,
    toast::{Toast, ToastId},
};
use gpui_kit::{App, Window};
pub fn show(window: &mut Window, cx: &mut App, toast: Toast) -> Result<(), OverlayError> {
    OverlayHost::notify(window, cx, toast)
}
pub fn update(window: &mut Window, cx: &mut App, toast: Toast) -> Result<(), OverlayError> {
    show(window, cx, toast)
}
pub fn hide(window: &mut Window, cx: &mut App, id: ToastId) -> Result<(), OverlayError> {
    OverlayHost::dismiss_toast(window, cx, id)
}
pub fn clean(window: &mut Window, cx: &mut App) -> Result<(), OverlayError> {
    OverlayHost::clear_toasts(window, cx)
}
pub fn set_limit(window: &mut Window, cx: &mut App, limit: usize) -> Result<(), OverlayError> {
    OverlayHost::toast_limit(window, cx, limit)
}
