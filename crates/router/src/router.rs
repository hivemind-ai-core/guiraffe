//! A per-window router built on guiraffe-state: navigation is a set of events on a
//! [`Store`], so devtools/logging interceptors and change-only selections work for routes.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

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
/// Absolute, verified move (back/forward): only applies if `index` still holds `expect`.
#[derive(Debug)]
struct GoTo<R> {
    index: usize,
    expect: R,
}

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

impl<R: Route> Event<RouterState<R>> for GoTo<R> {
    fn handle(self, fx: &mut Fx<'_, RouterState<R>>) {
        fx.update(|s| s.history.go_to(self.index, &self.expect));
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
    /// A guard refused, or navigation was requested from inside a guard.
    Blocked,
    /// Back/forward with nowhere to go, or an unparsable path.
    Unavailable,
}

/// What a guard decides about leaving `from` for `to`.
#[derive(Debug, Clone, PartialEq)]
pub enum GuardOutcome<R> {
    Allow,
    /// Stay. To ask first ("unsaved changes?"), block, open a dialog, and on "Discard"
    /// call [`Router::navigate_unguarded`] with the original target.
    Block,
    /// Go somewhere else instead (e.g. a login route). Redirects are guarded too.
    Redirect(R),
}

pub type Guard<R> = Rc<dyn Fn(&R, &R, &mut App) -> GuardOutcome<R>>;

/// Handle to a registered guard; pass to [`Router::remove_guard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardId(u64);

type Guards<R> = Rc<RefCell<Vec<(u64, Guard<R>)>>>;

enum Move<R> {
    Push(R),
    Replace(R),
    /// To history `index`, which should hold the route.
    To(usize, R),
}

impl<R: Clone> Move<R> {
    fn target(&self) -> &R {
        match self {
            Move::Push(r) | Move::Replace(r) | Move::To(_, r) => r,
        }
    }
}

pub struct Router<R: Route> {
    store: Store<RouterState<R>>,
    guards: Guards<R>,
    next: Rc<Cell<u64>>,
    in_guard: Rc<Cell<bool>>,
}

impl<R: Route> Clone for Router<R> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            guards: self.guards.clone(),
            next: self.next.clone(),
            in_guard: self.in_guard.clone(),
        }
    }
}

/// Redirect chains longer than this are treated as a loop and blocked.
const MAX_REDIRECTS: usize = 8;

impl<R: Route> Router<R> {
    pub const DEFAULT_HISTORY: usize = 64;

    pub fn new(initial: R, cx: &mut App) -> Self {
        Self::with_history(History::new(initial, Self::DEFAULT_HISTORY), cx)
    }

    /// Restore a persisted history (see [`History::from_parts`]).
    pub fn with_history(history: History<R>, cx: &mut App) -> Self {
        debug_assert!(R::check().is_empty(), "invalid routes: {:?}", R::check());
        Self {
            store: Store::new(RouterState { history }, cx),
            guards: Rc::default(),
            next: Rc::default(),
            in_guard: Rc::default(),
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

    pub fn add_guard(
        &self,
        guard: impl Fn(&R, &R, &mut App) -> GuardOutcome<R> + 'static,
    ) -> GuardId {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.guards.borrow_mut().push((id, Rc::new(guard)));
        GuardId(id)
    }

    pub fn remove_guard(&self, id: GuardId) -> bool {
        let mut g = self.guards.borrow_mut();
        let n = g.len();
        g.retain(|(i, _)| *i != id.0);
        g.len() != n
    }

    /// Run guards, following redirects. `None` = blocked.
    fn resolve(&self, from: &R, mv: Move<R>, cx: &mut App) -> Option<Move<R>> {
        let guards: Vec<Guard<R>> = self
            .guards
            .borrow()
            .iter()
            .map(|(_, g)| g.clone())
            .collect();
        self.in_guard.set(true);
        let mut mv = mv;
        let mut outcome = None;
        'chain: for _ in 0..=MAX_REDIRECTS {
            for g in &guards {
                match g(from, mv.target(), cx) {
                    GuardOutcome::Allow => {}
                    GuardOutcome::Block => {
                        outcome = Some(None);
                        break 'chain;
                    }
                    GuardOutcome::Redirect(to) => {
                        // A redirect is a fresh navigation, never a history jump.
                        mv = Move::Push(to);
                        continue 'chain;
                    }
                }
            }
            outcome = Some(Some(mv));
            break;
        }
        self.in_guard.set(false);
        // Redirect loop (chain exhausted) counts as blocked.
        outcome.flatten()
    }

    fn go(&self, mv: Option<Move<R>>, guarded: bool, cx: &mut App) -> Nav {
        if self.in_guard.get() {
            // Navigating from inside a guard would interleave with the outer request:
            // guards must return `GuardOutcome::Redirect` instead.
            return Nav::Blocked;
        }
        let Some(mv) = mv else {
            return Nav::Unavailable;
        };
        let from = self.current(cx);
        let same = matches!(mv, Move::Push(_) | Move::Replace(_)) && *mv.target() == from;
        if same {
            return Nav::Same;
        }
        let mv = if guarded {
            match self.resolve(&from, mv, cx) {
                Some(mv) => mv,
                None => return Nav::Blocked,
            }
        } else {
            mv
        };
        if matches!(mv, Move::Push(_)) && *mv.target() == from {
            return Nav::Same; // redirected back to where we are
        }
        debug_assert!(
            mv.target().round_trips(),
            "route {:?} does not survive to_path/parse; it can't be deep-linked or restored",
            mv.target()
        );
        match mv {
            Move::Push(r) => self.store.dispatch(Navigate(r), cx),
            Move::Replace(r) => self.store.dispatch(Replace(r), cx),
            Move::To(index, expect) => self.store.dispatch(GoTo { index, expect }, cx),
        }
        self.store.flush(cx);
        Nav::Navigated
    }

    pub fn navigate(&self, route: R, cx: &mut App) -> Nav {
        self.go(Some(Move::Push(route)), true, cx)
    }

    /// Navigate without consulting guards (after the user confirmed "Discard changes").
    pub fn navigate_unguarded(&self, route: R, cx: &mut App) -> Nav {
        self.go(Some(Move::Push(route)), false, cx)
    }

    /// Navigate to a path such as `/projects/7?tab=files` (deep links).
    pub fn navigate_path(&self, path: &str, cx: &mut App) -> Nav {
        self.go(R::parse(path).map(Move::Push), true, cx)
    }

    pub fn replace(&self, route: R, cx: &mut App) -> Nav {
        self.go(Some(Move::Replace(route)), true, cx)
    }

    fn step(&self, delta: isize, cx: &mut App) -> Nav {
        let h = self.history(cx);
        let target = h
            .index()
            .checked_add_signed(delta)
            .and_then(|i| h.entries().get(i).cloned().map(|r| Move::To(i, r)));
        self.go(target, true, cx)
    }

    pub fn back(&self, cx: &mut App) -> Nav {
        self.step(-1, cx)
    }

    pub fn forward(&self, cx: &mut App) -> Nav {
        self.step(1, cx)
    }
}
