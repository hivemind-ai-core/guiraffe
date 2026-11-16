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
        None,
        "typed search params must parse"
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
    if cfg!(feature = "gpui-fast") {
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
    let id = router.add_guard(move |from, _to, _| !(matches!(from, AppRoute::Home) && d.get()));
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
