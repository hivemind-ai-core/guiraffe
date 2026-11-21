//! Minimize-to-tray: closing a window hides it to the tray instead of quitting.
use std::rc::Rc;

use gpui_kit::{App, Window};
use guiraffe_windows::{CloseDecision, ListenerId, QuitPolicy, WindowManager};

use crate::{Tray, TrayStatus};

/// What hiding does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HideMode {
    /// Close the window; a tray click calls `reopen` to open it again (state lives in the app's
    /// entities). Works everywhere, including Wayland, where windows can't be hidden.
    #[default]
    Close,
    /// Minimize the window and keep it alive. Some compositors (e.g. niri, sway) have no
    /// minimized state, so the window stays visible there; prefer [`HideMode::Close`].
    Minimize,
}

/// Make the close button of `window` hide it to `tray`. A tray click (while hidden) calls
/// `reopen`, which should open the window again (e.g. `WindowManager::open` with the same key,
/// which restores its saved bounds).
///
/// While the tray isn't [`TrayStatus::Ready`] (no tray host), closing works normally so the
/// window can't get lost. The app keeps running with the window closed: the quit policy is set
/// to [`QuitPolicy::Never`], so give the tray menu a Quit item that calls `cx.quit()`.
///
/// Returns the close guard's id (remove it with [`WindowManager::remove`] to undo), or `None`
/// if the window isn't managed by [`WindowManager`].
pub fn minimize_to_tray(
    window: &Window,
    tray: &Tray,
    mode: HideMode,
    reopen: impl Fn(&mut App) + 'static,
    cx: &mut App,
) -> Option<ListenerId> {
    let reopen: Rc<dyn Fn(&mut App)> = Rc::new(reopen);
    let tray_for_guard = tray.clone();
    let guard = WindowManager::add_close_guard(window, cx, move |window, cx| {
        let tray = &tray_for_guard;
        if tray.status(cx) != TrayStatus::Ready {
            return CloseDecision::Allow;
        }
        hide(window, tray, mode, cx);
        CloseDecision::Veto
    })?;
    WindowManager::set_quit_policy(QuitPolicy::Never, cx);
    tray.entity().update(cx, |t, _| t.restore = Some(reopen));
    Some(guard)
}

/// Hide `window` to the tray now (e.g. from a "Minimize to tray" button).
pub fn hide(window: &mut Window, tray: &Tray, mode: HideMode, cx: &mut App) {
    tray.entity().update(cx, |t, _| t.hidden = true);
    match mode {
        HideMode::Minimize => window.minimize_window(),
        HideMode::Close => {
            // Persist the bounds first: the guard vetoed the managed close.
            WindowManager::flush(cx);
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }
}

/// Show the hidden window again (what a tray click does). Returns false if nothing was hidden.
pub fn restore(tray: &Tray, cx: &mut App) -> bool {
    let restore = tray.entity().update(cx, |t, _| {
        let hidden = std::mem::take(&mut t.hidden);
        hidden.then(|| t.restore.clone()).flatten()
    });
    match restore {
        Some(f) => {
            f(cx);
            true
        }
        None => false,
    }
}
