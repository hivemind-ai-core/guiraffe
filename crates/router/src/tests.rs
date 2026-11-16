use std::{cell::Cell, rc::Rc};

use gpui_kit::{Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};
use guiraffe_state::Selected;

use crate::*;

routes! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum AppRoute {
        Home = "/",
        NewProject = "/projects/new",
        Project { id: u64, tab: String } = "/projects/:id",
        File { id: u64, path: String } = "/projects/:id/files/*path",
        Search { q: String, page: u32 } = "/search",
    }
}

#[test]
fn parse_and_format_round_trip() {
    let cases = [
        ("/", AppRoute::Home),
        ("/projects/new", AppRoute::NewProject),
        (
            "/projects/42",
            AppRoute::Project {
                id: 42,
                tab: String::new(),
            },
        ),
        (
            "/projects/42?tab=files",
            AppRoute::Project {
                id: 42,
                tab: "files".into(),
            },
        ),
        (
            "/projects/1/files/src/main.rs",
            AppRoute::File {
                id: 1,
                path: "src/main.rs".into(),
            },
        ),
        (
            "/search?q=hello%20world&page=3",
            AppRoute::Search {
                q: "hello world".into(),
                page: 3,
            },
        ),
    ];
    for (path, route) in cases {
        assert_eq!(AppRoute::parse(path).as_ref(), Some(&route), "{path}");
        assert_eq!(
            AppRoute::parse(&route.to_path()).as_ref(),
            Some(&route),
            "round trip {route:?}"
        );
    }
    assert_eq!(
        AppRoute::Search {
            q: String::new(),
            page: 0
        }
        .to_path(),
        "/search",
        "defaults omitted"
    );
}

#[test]
fn order_matters_and_invalid_input_does_not_match() {
    assert_eq!(
        AppRoute::parse("/projects/new"),
        Some(AppRoute::NewProject),
        "specific before :id"
    );
    assert_eq!(
        AppRoute::parse("/projects/notanumber"),
        None,
        "typed params must parse"
    );
    assert_eq!(
        AppRoute::parse("/search?page=x"),
        Some(AppRoute::Search {
            q: String::new(),
            page: 0
        }),
        "bad search param falls back to default"
    );
    assert_eq!(AppRoute::parse("/nope"), None);
    assert_eq!(AppRoute::parse("/projects/1/extra"), None);
    assert_eq!(AppRoute::parse("/projects/%zz"), None);
}

#[test]
fn awkward_values_round_trip() {
    for q in ["a&b=c", "100%", "?/#", "ünï 🦒", "+plus+"] {
        let r = AppRoute::Search {
            q: q.into(),
            page: 1,
        };
        assert_eq!(AppRoute::parse(&r.to_path()), Some(r.clone()), "{q}");
    }
    for p in ["a/b/c", "spaces in/name.txt", "x"] {
        let r = AppRoute::File {
            id: 9,
            path: p.into(),
        };
        assert_eq!(AppRoute::parse(&r.to_path()), Some(r.clone()), "{p}");
    }
}

#[test]
fn declared_routes_are_valid_and_round_trip_checks_work() {
    assert!(AppRoute::check().is_empty(), "{:?}", AppRoute::check());
    assert!(
        AppRoute::Project {
            id: 1,
            tab: "x".into()
        }
        .round_trips()
    );
    routes! {
        #[derive(Debug, Clone, PartialEq)]
        enum Bad {
            Orphan = "/u/:id",
            Named { name: String } = "/n/:name",
        }
    }
    assert_eq!(Bad::check().len(), 1, "param without a field is reported");
    assert!(
        !Bad::Named {
            name: String::new()
        }
        .round_trips(),
        "empty path param can't be deep-linked"
    );
    assert_eq!(Bad::parse("/n/"), None);
}

#[test]
fn lenient_search_params_and_literal_plus_in_paths() {
    assert_eq!(
        AppRoute::parse("/search?q=x&page=garbage"),
        Some(AppRoute::Search {
            q: "x".into(),
            page: 0
        })
    );
    assert_eq!(
        AppRoute::parse("/projects/1/files/c++/a+b"),
        Some(AppRoute::File {
            id: 1,
            path: "c++/a+b".into()
        })
    );
    assert_eq!(
        AppRoute::parse("/search?q=a+b"),
        Some(AppRoute::Search {
            q: "a b".into(),
            page: 0
        }),
        "+ is a space in queries"
    );
}

#[test]
fn patterns_are_listed_and_params_extracted() {
    assert_eq!(AppRoute::patterns().len(), 5);
    assert_eq!(
        matcher::params("/projects/:id/files/*path"),
        vec!["id", "path"]
    );
}

struct Nav {
    is_project: Selected<bool>,
    renders: Rc<Cell<usize>>,
}

impl Render for Nav {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().child(if *self.is_project.get(cx) {
            "project"
        } else {
            "other"
        })
    }
}

#[gpui_kit::test]
fn navigation_history_and_selection(cx: &mut TestAppContext) {
    let router = cx.update(|cx| Router::new(AppRoute::Home, cx));
    let r = router.clone();
    let renders = Rc::new(Cell::new(0));
    let rr = renders.clone();
    let (view, vcx) = cx.add_window_view(move |_, cx| Nav {
        is_project: r.select(cx, |r| matches!(r, AppRoute::Project { .. })),
        renders: rr,
    });
    vcx.run_until_parked();
    let res = vcx.update(|_, cx| {
        router.navigate(
            AppRoute::Project {
                id: 1,
                tab: String::new(),
            },
            cx,
        )
    });
    assert_eq!(res, crate::Nav::Navigated);
    vcx.run_until_parked();
    assert!(vcx.update(|_, cx| *view.read(cx).is_project.get(cx)));
    let notified = renders.get();
    // Changing only a search param keeps the selection (still a project): no re-render on gpui-fast.
    vcx.update(|_, cx| {
        router.replace(
            AppRoute::Project {
                id: 1,
                tab: "files".into(),
            },
            cx,
        )
    });
    vcx.run_until_parked();
    if gpui_kit::GPUI_FAST {
        assert_eq!(renders.get(), notified);
    }
    assert_eq!(
        vcx.update(|_, cx| router.navigate(
            AppRoute::Project {
                id: 1,
                tab: "files".into()
            },
            cx
        )),
        crate::Nav::Same
    );
    assert_eq!(vcx.update(|_, cx| router.back(cx)), crate::Nav::Navigated);
    assert_eq!(vcx.update(|_, cx| router.current(cx)), AppRoute::Home);
    assert_eq!(vcx.update(|_, cx| router.back(cx)), crate::Nav::Unavailable);
    assert_eq!(
        vcx.update(|_, cx| router.forward(cx)),
        crate::Nav::Navigated
    );
    assert_eq!(
        vcx.update(|_, cx| router.navigate_path("/search?q=x", cx)),
        crate::Nav::Navigated
    );
    assert_eq!(
        vcx.update(|_, cx| router.current(cx)),
        AppRoute::Search {
            q: "x".into(),
            page: 0
        }
    );
    assert_eq!(
        vcx.update(|_, cx| router.navigate_path("/bogus", cx)),
        crate::Nav::Unavailable
    );
}

#[gpui_kit::test]
fn guards_block_and_can_be_removed(cx: &mut TestAppContext) {
    let router = cx.update(|cx| Router::new(AppRoute::Home, cx));
    let dirty = Rc::new(Cell::new(true));
    let d = dirty.clone();
    let id = router.add_guard(move |from, _to, _| {
        if matches!(from, AppRoute::Home) && d.get() {
            GuardOutcome::Block
        } else {
            GuardOutcome::Allow
        }
    });
    assert_eq!(
        cx.update(|cx| router.navigate(AppRoute::NewProject, cx)),
        crate::Nav::Blocked
    );
    assert_eq!(cx.update(|cx| router.current(cx)), AppRoute::Home);
    dirty.set(false);
    assert_eq!(
        cx.update(|cx| router.navigate(AppRoute::NewProject, cx)),
        crate::Nav::Navigated
    );
    assert!(router.remove_guard(id));
    assert!(!router.remove_guard(id));
}

#[gpui_kit::test]
fn history_restores_and_store_logs_navigation(cx: &mut TestAppContext) {
    let mut h = History::new(AppRoute::Home, 8);
    h.push(AppRoute::NewProject);
    let router = cx.update(|cx| Router::with_history(h.clone(), cx));
    assert_eq!(cx.update(|cx| router.history(cx)), h);
    assert_eq!(cx.update(|cx| router.back(cx)), crate::Nav::Navigated);
    assert_eq!(
        cx.update(|cx| router.store().stats(cx).events_processed),
        1,
        "navigation flows through the event store"
    );
}

#[gpui_kit::test]
fn redirect_guard_and_reentrancy(cx: &mut TestAppContext) {
    let router = cx.update(|cx| Router::new(AppRoute::Home, cx));
    // Redirect anything under Search to Home-with-NewProject (e.g. "log in first").
    router.add_guard(|_, to, _| match to {
        AppRoute::Search { .. } => GuardOutcome::Redirect(AppRoute::NewProject),
        _ => GuardOutcome::Allow,
    });
    assert_eq!(
        cx.update(|cx| router.navigate(
            AppRoute::Search {
                q: "x".into(),
                page: 0
            },
            cx
        )),
        crate::Nav::Navigated
    );
    assert_eq!(cx.update(|cx| router.current(cx)), AppRoute::NewProject);
    assert_eq!(
        cx.update(|cx| router.history(cx)).entries().len(),
        2,
        "one entry: the redirect, not both"
    );
    // A guard that tries to navigate itself is refused (it must Redirect instead).
    let r2 = router.clone();
    let inner = Rc::new(Cell::new(None));
    let i2 = inner.clone();
    router.add_guard(move |_, to, cx| {
        if matches!(to, AppRoute::Project { .. }) {
            i2.set(Some(r2.navigate(AppRoute::Home, cx)));
        }
        GuardOutcome::Allow
    });
    assert_eq!(
        cx.update(|cx| router.navigate(
            AppRoute::Project {
                id: 2,
                tab: String::new()
            },
            cx
        )),
        crate::Nav::Navigated
    );
    assert_eq!(inner.get(), Some(crate::Nav::Blocked));
    assert_eq!(
        cx.update(|cx| router.current(cx)),
        AppRoute::Project {
            id: 2,
            tab: String::new()
        },
        "history not corrupted"
    );
}

#[gpui_kit::test]
fn redirect_loops_are_blocked_and_unguarded_escape_hatch_works(cx: &mut TestAppContext) {
    let router = cx.update(|cx| Router::new(AppRoute::Home, cx));
    router.add_guard(|_, to, _| match to {
        AppRoute::NewProject => GuardOutcome::Redirect(AppRoute::Search {
            q: String::new(),
            page: 1,
        }),
        AppRoute::Search { .. } => GuardOutcome::Redirect(AppRoute::NewProject),
        _ => GuardOutcome::Allow,
    });
    assert_eq!(
        cx.update(|cx| router.navigate(AppRoute::NewProject, cx)),
        crate::Nav::Blocked
    );
    assert_eq!(
        cx.update(|cx| router.navigate_unguarded(AppRoute::NewProject, cx)),
        crate::Nav::Navigated
    );
    assert_eq!(cx.update(|cx| router.current(cx)), AppRoute::NewProject);
}

#[gpui_kit::test]
fn back_forward_work_with_duplicate_neighbours(cx: &mut TestAppContext) {
    let router = cx.update(|cx| Router::new(AppRoute::Home, cx));
    cx.update(|cx| {
        router.navigate(AppRoute::NewProject, cx);
        router.navigate(
            AppRoute::Search {
                q: "a".into(),
                page: 0,
            },
            cx,
        );
        router.back(cx);
        router.replace(
            AppRoute::Search {
                q: "a".into(),
                page: 0,
            },
            cx,
        );
    });
    // History is now [Home, Search(a), Search(a)] at index 1.
    assert_eq!(
        cx.update(|cx| router.forward(cx)),
        crate::Nav::Navigated,
        "not stuck on equal neighbour"
    );
    assert_eq!(cx.update(|cx| router.history(cx)).index(), 2);
}
