//! A per-window router built on guiraffe-state: navigation is a set of events on a
//! [`Store`], so devtools/logging interceptors and change-only selections work for routes.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::{App, Context};
use guiraffe_state::{Event, Fx, Selected, Store};

use crate::{history::History, route::Route};

#[derive(Debug, Clone, PartialEq)]
pub struct RouterState<R> {
    pub history: History<R>,
}

#[derive(Debug)]
struct Navigate<R>(R);
#[derive(Debug)]
struct Replace<R>(R);
#[derive(Debug)]
struct Back;
#[derive(Debug)]
struct Forward;

impl<R: Route> Event<RouterState<R>> for Navigate<R> {
    fn handle(self, fx: &mut Fx<'_, RouterState<R>>) {
        fx.update(|s| s.history.push(self.0));
    }
    fn undoable(&self) -> bool {
        false
    }
}

impl<R: Route> Event<RouterState<R>> for Replace<R> {
    fn handle(self, fx: &mut Fx<'_, RouterState<R>>) {
        fx.update(|s| s.history.replace(self.0));
    }
    fn undoable(&self) -> bool {
        false
    }
}

impl<R: Route> Event<RouterState<R>> for Back {
    fn handle(self, fx: &mut Fx<'_, RouterState<R>>) {
        fx.update(|s| s.history.back());
    }
    fn undoable(&self) -> bool {
        false
    }
}

impl<R: Route> Event<RouterState<R>> for Forward {
    fn handle(self, fx: &mut Fx<'_, RouterState<R>>) {
        fx.update(|s| s.history.forward());
    }
    fn undoable(&self) -> bool {
        false
    }
}

/// Outcome of a navigation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Navigated,
    /// Already there (no history entry added).
    Same,
    /// A guard refused (e.g. unsaved changes).
    Blocked,
    /// Back/forward with nowhere to go, or an unparsable path.
    Unavailable,
}

/// Decides whether leaving `from` for `to` may proceed.
pub type Guard<R> = Rc<dyn Fn(&R, &R, &mut App) -> bool>;

/// Handle to a registered guard; pass to [`Router::remove_guard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardId(u64);

type Guards<R> = Rc<RefCell<Vec<(u64, Guard<R>)>>>;

enum Action {
    Navigate,
    Replace,
    Back,
    Forward,
}

pub struct Router<R: Route> {
    store: Store<RouterState<R>>,
    guards: Guards<R>,
    next: Rc<RefCell<u64>>,
}

impl<R: Route> Clone for Router<R> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            guards: self.guards.clone(),
            next: self.next.clone(),
        }
    }
}

impl<R: Route> Router<R> {
    pub const DEFAULT_HISTORY: usize = 64;

    pub fn new(initial: R, cx: &mut App) -> Self {
        Self::with_history(History::new(initial, Self::DEFAULT_HISTORY), cx)
    }

    /// Restore a persisted history (e.g. from the window layout).
    pub fn with_history(history: History<R>, cx: &mut App) -> Self {
        Self {
            store: Store::new(RouterState { history }, cx),
            guards: Rc::default(),
            next: Rc::default(),
        }
    }

    /// The underlying store (add interceptors for logging/devtools, or select from it).
    pub fn store(&self) -> &Store<RouterState<R>> {
        &self.store
    }

    pub fn current(&self, cx: &App) -> R {
        self.store.state(cx).history.current().clone()
    }

    pub fn history(&self, cx: &App) -> History<R> {
        self.store.state(cx).history.clone()
    }

    /// Select from the current route; the view re-renders only when the selection changes
    /// (e.g. `|r| matches!(r, AppRoute::Settings { .. })` for a nav highlight).
    pub fn select<V: 'static, T: Clone + PartialEq + 'static>(
        &self,
        cx: &mut Context<V>,
        f: impl Fn(&R) -> T + 'static,
    ) -> Selected<T> {
        self.store
            .select(cx, move |s: &RouterState<R>| f(s.history.current()))
    }

    pub fn add_guard(&self, guard: impl Fn(&R, &R, &mut App) -> bool + 'static) -> GuardId {
        let mut next = self.next.borrow_mut();
        *next += 1;
        self.guards.borrow_mut().push((*next, Rc::new(guard)));
        GuardId(*next)
    }

    pub fn remove_guard(&self, id: GuardId) -> bool {
        let mut g = self.guards.borrow_mut();
        let n = g.len();
        g.retain(|(i, _)| *i != id.0);
        g.len() != n
    }

    fn allowed(&self, from: &R, to: &R, cx: &mut App) -> bool {
        let guards: Vec<Guard<R>> = self
            .guards
            .borrow()
            .iter()
            .map(|(_, g)| g.clone())
            .collect();
        guards.iter().all(|g| g(from, to, cx))
    }

    fn go(&self, target: Option<R>, action: Action, cx: &mut App) -> Nav {
        let Some(target) = target else {
            return Nav::Unavailable;
        };
        let from = self.current(cx);
        if from == target {
            return Nav::Same;
        }
        if !self.allowed(&from, &target, cx) {
            return Nav::Blocked;
        }
        match action {
            Action::Navigate => self.store.dispatch(Navigate(target), cx),
            Action::Replace => self.store.dispatch(Replace(target), cx),
            Action::Back => self.store.dispatch(Back, cx),
            Action::Forward => self.store.dispatch(Forward, cx),
        }
        self.store.flush(cx);
        Nav::Navigated
    }

    pub fn navigate(&self, route: R, cx: &mut App) -> Nav {
        self.go(Some(route), Action::Navigate, cx)
    }

    /// Navigate to a path such as `/projects/7?tab=files` (deep links).
    pub fn navigate_path(&self, path: &str, cx: &mut App) -> Nav {
        self.go(R::parse(path), Action::Navigate, cx)
    }

    pub fn replace(&self, route: R, cx: &mut App) -> Nav {
        self.go(Some(route), Action::Replace, cx)
    }

    pub fn back(&self, cx: &mut App) -> Nav {
        let target = self.store.state(cx).history.peek(-1).cloned();
        self.go(target, Action::Back, cx)
    }

    pub fn forward(&self, cx: &mut App) -> Nav {
        let target = self.store.state(cx).history.peek(1).cloned();
        self.go(target, Action::Forward, cx)
    }
}
