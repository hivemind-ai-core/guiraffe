//! The per-application window manager: a registry of open windows keyed by
//! [`WindowKey`], singleton reuse, close guards, a typed cross-window event bus,
//! layout persistence and the quit policy.
//!
//! Reentrancy rule: the manager global is never borrowed while user callbacks run.
//! Callbacks are snapshotted (`Rc`) first, then invoked, so a callback may freely
//! open/close windows or publish events.

use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use gpui_kit::{
    AnyWindowHandle, App, Bounds, Global, Render, SharedString, Subscription, Task, Window,
    WindowBounds, WindowHandle, WindowId, WindowOptions, point, px, size,
};

use crate::{
    layout_store::{Layout, LayoutStore, LoadOutcome, SavedWindow},
    placement::{self, Display, PlacementRules, Rect},
};

/// Identity of a logical window. Windows opened with the same key while one is
/// alive reuse it (unless opened with [`OpenMode::New`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowKey {
    pub kind: SharedString,
    pub instance: Option<SharedString>,
}

impl WindowKey {
    pub fn kind(kind: impl Into<SharedString>) -> Self {
        Self {
            kind: kind.into(),
            instance: None,
        }
    }
    pub fn instance(kind: impl Into<SharedString>, instance: impl Into<SharedString>) -> Self {
        Self {
            kind: kind.into(),
            instance: Some(instance.into()),
        }
    }
    /// The persistence key: `kind` or `kind/instance`.
    pub fn storage_key(&self) -> String {
        match &self.instance {
            Some(i) => format!("{}/{}", self.kind, i),
            None => self.kind.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenMode {
    /// Focus the existing window with this key if alive, else open one.
    #[default]
    Reuse,
    /// Always open another window (cascaded) even if the key is open.
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuitPolicy {
    /// Quit the application when the last window closes (typical Linux/Windows).
    #[default]
    WhenLastWindowCloses,
    /// Keep running (macOS style, tray apps).
    Never,
}

#[derive(Debug, Clone, Default)]
pub struct WindowConfig {
    /// Where to persist window layout. `None` disables persistence.
    pub layout_path: Option<PathBuf>,
    pub quit_policy: QuitPolicy,
    pub rules: PlacementRules,
    /// Delay between the last bounds change and the layout write.
    pub save_debounce: Option<Duration>,
}

/// Returned from a close guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDecision {
    Allow,
    Veto,
}

type Guard = Rc<dyn Fn(&mut Window, &mut App) -> CloseDecision>;
type Handler = Rc<dyn Fn(&dyn Any, &mut Window, &mut App)>;

struct Entry {
    handle: AnyWindowHandle,
    key: WindowKey,
    /// Whether this entry owns the persisted layout slot for its key.
    persist: bool,
    guards: Vec<(u64, Guard)>,
    _bounds: Option<Subscription>,
}

struct BusSub {
    id: u64,
    window: WindowId,
    handler: Handler,
}

/// A handle to a bus subscription or close guard; pass to [`WindowManager::remove`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

pub struct WindowManager {
    config: WindowConfig,
    store: Option<LayoutStore>,
    layout: Layout,
    /// Set when the layout file was unreadable and moved aside at startup.
    pub recovered_layout: Option<(PathBuf, String)>,
    entries: HashMap<WindowId, Entry>,
    /// Open order, for deterministic iteration.
    order: Vec<WindowId>,
    bus: HashMap<TypeId, Vec<BusSub>>,
    /// Unmanaged helper windows (notification surfaces) that never block the quit policy.
    auxiliary: HashSet<WindowId>,
    next_id: u64,
    save_task: Option<Task<()>>,
    last_save_error: Option<String>,
    _subs: Vec<Subscription>,
}

impl Global for WindowManager {}

impl WindowManager {
    /// Install the manager. Calling again replaces the config but keeps open windows.
    pub fn init(config: WindowConfig, cx: &mut App) {
        let store = config.layout_path.clone().map(LayoutStore::new);
        let (layout, recovered) = match store.as_ref().map(LayoutStore::load) {
            Some(Ok(LoadOutcome::Loaded(l))) => (l, None),
            Some(Ok(LoadOutcome::Recovered { set_aside, reason })) => {
                (Layout::default(), Some((set_aside, reason)))
            }
            Some(Err(e)) => (Layout::default(), Some((PathBuf::new(), e.to_string()))),
            Some(Ok(LoadOutcome::Missing)) | None => (Layout::default(), None),
        };
        let closed = cx.on_window_closed(|cx, id| Self::handle_closed(id, cx));
        let quit = cx.on_app_quit(|cx| {
            Self::flush(cx);
            async {}
        });
        // Re-init keeps open windows and subscriptions registered.
        let (entries, order, bus, auxiliary, next_id) = if cx.has_global::<Self>() {
            let old = cx.remove_global::<Self>();
            (old.entries, old.order, old.bus, old.auxiliary, old.next_id)
        } else {
            Default::default()
        };
        cx.set_global(Self {
            config,
            store,
            layout,
            recovered_layout: recovered,
            entries,
            order,
            bus,
            auxiliary,
            next_id,
            save_task: None,
            last_save_error: None,
            _subs: vec![closed, quit],
        });
    }

    fn ensure(cx: &mut App) {
        if !cx.has_global::<Self>() {
            Self::init(WindowConfig::default(), cx);
        }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Open (or reuse) a window for `key`. Unless the caller set `window_bounds`,
    /// the last saved bounds for `key` are restored onto the current displays.
    pub fn open<V: Render + 'static>(
        key: WindowKey,
        mode: OpenMode,
        mut options: WindowOptions,
        build: impl FnOnce(&mut Window, &mut App) -> gpui_kit::Entity<V>,
        cx: &mut App,
    ) -> anyhow::Result<WindowHandle<V>> {
        Self::ensure(cx);
        if mode == OpenMode::Reuse
            && let Some(existing) = Self::find(&key, cx)
        {
            if let Some(typed) = existing.downcast::<V>() {
                existing.update(cx, |_, window, _| window.activate_window())?;
                return Ok(typed);
            }
            anyhow::bail!(
                "window {:?} is open with a different root view type",
                key.storage_key()
            );
        }
        let persist = Self::find(&key, cx).is_none();
        let siblings = cx
            .global::<Self>()
            .entries
            .values()
            .filter(|e| e.key == key)
            .count();
        if options.window_bounds.is_none() {
            options.window_bounds = Some(Self::restore_bounds(&key, siblings, cx));
        }
        let handle = cx.open_window(options, build)?;
        let any: AnyWindowHandle = handle.into();
        let wid = any.window_id();
        let bounds_sub = handle
            .update(cx, |_, window, cx| {
                let k = key.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    Self::should_close(wid, window, cx)
                });
                // Always observe: a later window can inherit the persistence slot.
                cx.observe_window_bounds(window, move |_, window, cx| {
                    let owns = cx
                        .try_global::<Self>()
                        .and_then(|m| m.entries.get(&wid))
                        .is_some_and(|e| e.persist);
                    if owns {
                        Self::record_bounds(&k, window, cx);
                    }
                })
            })
            .ok();
        let m = cx.global_mut::<Self>();
        m.entries.insert(
            wid,
            Entry {
                handle: any,
                key,
                persist,
                guards: Vec::new(),
                _bounds: bounds_sub,
            },
        );
        m.order.push(wid);
        Ok(handle)
    }

    fn restore_bounds(key: &WindowKey, siblings: usize, cx: &App) -> WindowBounds {
        let m = cx.global::<Self>();
        let displays: Vec<Display> = cx
            .displays()
            .iter()
            .map(|d| {
                let b = d.bounds();
                Display {
                    uuid: d.uuid().ok().map(|u| u.to_string()),
                    bounds: Rect::new(
                        b.origin.x.into(),
                        b.origin.y.into(),
                        b.size.width.into(),
                        b.size.height.into(),
                    ),
                }
            })
            .collect();
        let saved = m.layout.windows.get(&key.storage_key());
        let mut rect = placement::place(
            saved.map(|s| (s.bounds, s.display.as_deref())),
            &displays,
            &m.config.rules,
        );
        if siblings > 0 {
            let area = displays
                .iter()
                .max_by(|a, b| {
                    a.bounds
                        .overlap_area(&rect)
                        .total_cmp(&b.bounds.overlap_area(&rect))
                })
                .map(|d| d.bounds);
            rect = placement::cascade(rect, siblings, area.as_ref());
        }
        let b = Bounds::new(point(px(rect.x), px(rect.y)), size(px(rect.w), px(rect.h)));
        match saved {
            Some(s) if s.fullscreen && siblings == 0 => WindowBounds::Fullscreen(b),
            Some(s) if s.maximized && siblings == 0 => WindowBounds::Maximized(b),
            _ => WindowBounds::Windowed(b),
        }
    }

    fn record_bounds(key: &WindowKey, window: &mut Window, cx: &mut App) {
        let (b, maximized, fullscreen) = match window.window_bounds() {
            WindowBounds::Windowed(b) => (b, false, false),
            WindowBounds::Maximized(b) => (b, true, false),
            WindowBounds::Fullscreen(b) => (b, false, true),
        };
        let display = window
            .display(cx)
            .and_then(|d| d.uuid().ok())
            .map(|u| u.to_string());
        let saved = SavedWindow {
            bounds: Rect::new(
                b.origin.x.into(),
                b.origin.y.into(),
                b.size.width.into(),
                b.size.height.into(),
            ),
            maximized,
            fullscreen,
            display,
        };
        let Some(m) = cx.try_global::<Self>() else {
            return;
        };
        if m.layout.windows.get(&key.storage_key()) == Some(&saved) {
            return;
        }
        let delay = m.config.save_debounce.unwrap_or(Duration::from_millis(500));
        let m = cx.global_mut::<Self>();
        m.layout.windows.insert(key.storage_key(), saved);
        if m.store.is_some() {
            let timer = cx.background_executor().timer(delay);
            let task = cx.spawn(async move |cx| {
                timer.await;
                cx.update(Self::flush);
            });
            cx.global_mut::<Self>().save_task = Some(task);
        }
    }

    /// Write the layout now (also done automatically, debounced, and on quit).
    pub fn flush(cx: &mut App) {
        let Some(m) = cx.try_global::<Self>() else {
            return;
        };
        let Some(store) = m.store.clone() else { return };
        // Capture the live bounds of every open persisted window first.
        let live: Vec<(WindowKey, AnyWindowHandle)> = m
            .entries
            .values()
            .filter(|e| e.persist)
            .map(|e| (e.key.clone(), e.handle))
            .collect();
        for (key, h) in live {
            let _ = h.update(cx, |_, window, cx| Self::record_bounds(&key, window, cx));
        }
        let m = cx.global_mut::<Self>();
        m.save_task = None;
        m.last_save_error = store.save(&m.layout).err().map(|e| e.to_string());
    }

    pub fn last_save_error(cx: &App) -> Option<String> {
        cx.try_global::<Self>()
            .and_then(|m| m.last_save_error.clone())
    }

    fn should_close(wid: WindowId, window: &mut Window, cx: &mut App) -> bool {
        let guards: Vec<Guard> = cx
            .try_global::<Self>()
            .and_then(|m| m.entries.get(&wid))
            .map(|e| e.guards.iter().map(|(_, g)| g.clone()).collect())
            .unwrap_or_default();
        // Every guard runs (so each can prompt/save); any veto keeps the window.
        let mut allow = true;
        for g in guards {
            allow &= g(window, cx) == CloseDecision::Allow;
        }
        if allow {
            let key = cx
                .try_global::<Self>()
                .and_then(|m| m.entries.get(&wid))
                .filter(|e| e.persist)
                .map(|e| e.key.clone());
            if let Some(key) = key {
                Self::record_bounds(&key, window, cx);
                Self::flush(cx);
            }
        }
        allow
    }

    fn handle_closed(wid: WindowId, cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let m = cx.global_mut::<Self>();
        if let Some(entry) = m.entries.remove(&wid) {
            // Hand the persistence slot to another live window of the same key.
            let heir = m
                .order
                .iter()
                .copied()
                .find(|id| m.entries.get(id).is_some_and(|e| e.key == entry.key));
            if entry.persist
                && let Some(next) = heir.and_then(|id| m.entries.get_mut(&id))
            {
                next.persist = true;
            }
        }
        m.order.retain(|id| *id != wid);
        for subs in m.bus.values_mut() {
            subs.retain(|s| s.window != wid);
        }
        m.bus.retain(|_, v| !v.is_empty());
        m.auxiliary.remove(&wid);
        let policy = m.config.quit_policy;
        let managed_left = !m.entries.is_empty();
        let auxiliary = m.auxiliary.clone();
        let open: Vec<WindowId> = cx.windows().iter().map(|w| w.window_id()).collect();
        if should_quit(policy, managed_left, &open, wid, &auxiliary) {
            cx.quit();
        }
    }

    /// Mark an unmanaged helper window (a notification surface, a tray popup) so it never keeps
    /// the app alive under [`QuitPolicy::WhenLastWindowCloses`].
    /// Without an installed manager there is no quit policy to protect, so this is a no-op.
    pub fn mark_auxiliary(handle: AnyWindowHandle, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().auxiliary.insert(handle.window_id());
        }
    }

    /// Change the quit policy at runtime (e.g. when a tray icon appears or goes away).
    pub fn set_quit_policy(policy: QuitPolicy, cx: &mut App) {
        Self::ensure(cx);
        cx.global_mut::<Self>().config.quit_policy = policy;
    }

    pub fn quit_policy(cx: &App) -> QuitPolicy {
        cx.try_global::<Self>()
            .map(|m| m.config.quit_policy)
            .unwrap_or_default()
    }

    /// Register a guard consulted when the user asks to close `window`.
    pub fn add_close_guard(
        window: &Window,
        cx: &mut App,
        guard: impl Fn(&mut Window, &mut App) -> CloseDecision + 'static,
    ) -> Option<ListenerId> {
        Self::ensure(cx);
        let wid = window.window_handle().window_id();
        let m = cx.global_mut::<Self>();
        let id = m.id();
        m.entries.get_mut(&wid)?.guards.push((id, Rc::new(guard)));
        Some(ListenerId(id))
    }

    /// Subscribe `window` to events of type `E`. Removed automatically when the window closes.
    pub fn subscribe<E: 'static>(
        window: &Window,
        cx: &mut App,
        handler: impl Fn(&E, &mut Window, &mut App) + 'static,
    ) -> ListenerId {
        Self::ensure(cx);
        let wid = window.window_handle().window_id();
        let m = cx.global_mut::<Self>();
        let id = m.id();
        let handler: Handler = Rc::new(move |any, window, cx| {
            if let Some(e) = any.downcast_ref::<E>() {
                handler(e, window, cx);
            }
        });
        m.bus.entry(TypeId::of::<E>()).or_default().push(BusSub {
            id,
            window: wid,
            handler,
        });
        ListenerId(id)
    }

    /// Remove a bus subscription or close guard. Returns whether it existed.
    pub fn remove(id: ListenerId, cx: &mut App) -> bool {
        if !cx.has_global::<Self>() {
            return false;
        }
        let m = cx.global_mut::<Self>();
        let mut found = false;
        for subs in m.bus.values_mut() {
            let n = subs.len();
            subs.retain(|s| s.id != id.0);
            found |= subs.len() != n;
        }
        for e in m.entries.values_mut() {
            let n = e.guards.len();
            e.guards.retain(|(g, _)| *g != id.0);
            found |= e.guards.len() != n;
        }
        found
    }

    /// Deliver `event` to every subscribed window (in subscription order), returning
    /// how many handlers ran. Windows that closed meanwhile are skipped.
    pub fn publish<E: 'static>(event: E, cx: &mut App) -> usize {
        let Some(m) = cx.try_global::<Self>() else {
            return 0;
        };
        let targets: Vec<(AnyWindowHandle, Handler)> = m
            .bus
            .get(&TypeId::of::<E>())
            .into_iter()
            .flatten()
            .filter_map(|s| {
                m.entries
                    .get(&s.window)
                    .map(|e| (e.handle, s.handler.clone()))
            })
            .collect();
        let event = Rc::new(event);
        let mut delivered = 0;
        for (h, handler) in targets {
            let ev = event.clone();
            let hd = handler.clone();
            if h.update(cx, move |_, window, cx| hd(&*ev, window, cx))
                .is_ok()
            {
                delivered += 1;
            } else if Self::is_managed(h, cx) {
                // The window is alive but busy (we are inside its own update, e.g. a
                // handler publishing reentrantly). Deliver once it is free.
                let ev = event.clone();
                cx.defer(move |cx| {
                    let _ = h.update(cx, move |_, window, cx| handler(&*ev, window, cx));
                });
                delivered += 1;
            }
        }
        delivered
    }

    fn is_managed(h: AnyWindowHandle, cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|m| m.entries.contains_key(&h.window_id()))
    }

    pub fn find(key: &WindowKey, cx: &App) -> Option<AnyWindowHandle> {
        let m = cx.try_global::<Self>()?;
        m.order
            .iter()
            .filter_map(|id| m.entries.get(id))
            .find(|e| &e.key == key)
            .map(|e| e.handle)
    }

    /// All managed windows in open order.
    pub fn windows(cx: &App) -> Vec<(WindowKey, AnyWindowHandle)> {
        let Some(m) = cx.try_global::<Self>() else {
            return Vec::new();
        };
        m.order
            .iter()
            .filter_map(|id| m.entries.get(id))
            .map(|e| (e.key.clone(), e.handle))
            .collect()
    }

    pub fn saved(key: &WindowKey, cx: &App) -> Option<SavedWindow> {
        cx.try_global::<Self>()?
            .layout
            .windows
            .get(&key.storage_key())
            .cloned()
    }

    /// Ask a window to close, honoring guards. Returns false if vetoed or not found.
    pub fn request_close(handle: AnyWindowHandle, cx: &mut App) -> bool {
        let wid = handle.window_id();
        handle
            .update(cx, |_, window, cx| {
                let ok = Self::should_close(wid, window, cx);
                if ok {
                    window.remove_window();
                }
                ok
            })
            .unwrap_or(false)
    }
}

/// The quit decision when `closing` closes: quit when the policy asks for it, no managed window
/// is left, and every other open window is auxiliary (notification surfaces never keep the app
/// alive).
pub(crate) fn should_quit(
    policy: QuitPolicy,
    managed_left: bool,
    open: &[WindowId],
    closing: WindowId,
    auxiliary: &HashSet<WindowId>,
) -> bool {
    policy == QuitPolicy::WhenLastWindowCloses
        && !managed_left
        && open.iter().all(|w| *w == closing || auxiliary.contains(w))
}
