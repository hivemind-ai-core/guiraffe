use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    WindowHandle, WindowOptions, div, px, size,
};

use crate::*;

struct Root;
impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
struct Other;
impl Render for Other {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn open(key: &WindowKey, mode: OpenMode, cx: &mut TestAppContext) -> WindowHandle<Root> {
    let key = key.clone();
    cx.update(|cx| {
        WindowManager::open(
            key,
            mode,
            WindowOptions::default(),
            |_, cx| cx.new(|_| Root),
            cx,
        )
    })
    .expect("open")
}

fn init(cx: &mut TestAppContext, config: WindowConfig) {
    cx.update(|cx| {
        WindowManager::init(
            WindowConfig {
                quit_policy: QuitPolicy::Never,
                ..config
            },
            cx,
        )
    });
}

#[gpui_kit::test]
fn reuse_returns_same_window_new_opens_another(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let k = WindowKey::kind("main");
    let a = open(&k, OpenMode::Reuse, cx);
    let b = open(&k, OpenMode::Reuse, cx);
    assert_eq!(a, b);
    let c = open(&k, OpenMode::New, cx);
    assert_ne!(a, c);
    let other = open(&WindowKey::instance("doc", "1"), OpenMode::Reuse, cx);
    assert_ne!(other, a);
    let ws = cx.update(|cx| WindowManager::windows(cx));
    assert_eq!(ws.len(), 3);
    assert_eq!(ws[0].1, a.into());
}

#[gpui_kit::test]
fn new_windows_of_same_key_are_cascaded(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let k = WindowKey::kind("main");
    let a = open(&k, OpenMode::Reuse, cx);
    let b = open(&k, OpenMode::New, cx);
    let ba = a
        .update(cx, |_, w, _| w.window_bounds().get_bounds())
        .expect("a");
    let bb = b
        .update(cx, |_, w, _| w.window_bounds().get_bounds())
        .expect("b");
    assert_ne!(ba.origin, bb.origin);
}

#[gpui_kit::test]
fn reuse_with_wrong_view_type_errors(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let k = WindowKey::kind("main");
    open(&k, OpenMode::Reuse, cx);
    let r = cx.update(|cx| {
        WindowManager::open(
            k,
            OpenMode::Reuse,
            WindowOptions::default(),
            |_, cx| cx.new(|_| Other),
            cx,
        )
    });
    assert!(r.is_err());
}

#[gpui_kit::test]
fn close_guard_vetoes_then_allows(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let k = WindowKey::kind("editor");
    let h = open(&k, OpenMode::Reuse, cx);
    let dirty = Rc::new(RefCell::new(true));
    let asked = Rc::new(RefCell::new(0));
    let (d, a) = (dirty.clone(), asked.clone());
    h.update(cx, |_, window, cx| {
        WindowManager::add_close_guard(window, cx, move |_, _| {
            *a.borrow_mut() += 1;
            if *d.borrow() {
                CloseDecision::Veto
            } else {
                CloseDecision::Allow
            }
        })
    })
    .expect("guard");
    let mut vcx = VisualTestContext::from_window(h.into(), cx);
    assert!(!vcx.simulate_close());
    assert_eq!(*asked.borrow(), 1);
    assert!(cx.update(|cx| WindowManager::find(&k, cx)).is_some());
    *dirty.borrow_mut() = false;
    assert!(cx.update(|cx| WindowManager::request_close(h.into(), cx)));
    cx.run_until_parked();
    assert!(cx.update(|cx| WindowManager::find(&k, cx)).is_none());
}

#[gpui_kit::test]
fn every_guard_runs_and_any_veto_wins(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let h = open(&WindowKey::kind("w"), OpenMode::Reuse, cx);
    let ran = Rc::new(RefCell::new(Vec::new()));
    let (r1, r2) = (ran.clone(), ran.clone());
    let id = h
        .update(cx, |_, window, cx| {
            let id = WindowManager::add_close_guard(window, cx, move |_, _| {
                r1.borrow_mut().push(1);
                CloseDecision::Veto
            });
            WindowManager::add_close_guard(window, cx, move |_, _| {
                r2.borrow_mut().push(2);
                CloseDecision::Allow
            });
            id
        })
        .expect("guards")
        .expect("id");
    assert!(!cx.update(|cx| WindowManager::request_close(h.into(), cx)));
    assert_eq!(*ran.borrow(), vec![1, 2]);
    assert!(cx.update(|cx| WindowManager::remove(id, cx)));
    assert!(cx.update(|cx| WindowManager::request_close(h.into(), cx)));
}

#[derive(Clone)]
struct Ping(u32);

#[gpui_kit::test]
fn bus_delivers_to_live_windows_only(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let got = Rc::new(RefCell::new(Vec::new()));
    let mut hs = Vec::new();
    for i in 0..3 {
        let h = open(
            &WindowKey::instance("w", i.to_string()),
            OpenMode::Reuse,
            cx,
        );
        let g = got.clone();
        h.update(cx, |_, window, cx| {
            WindowManager::subscribe::<Ping>(window, cx, move |p, _, _| {
                g.borrow_mut().push((i, p.0))
            })
        })
        .expect("sub");
        hs.push(h);
    }
    assert_eq!(cx.update(|cx| WindowManager::publish(Ping(7), cx)), 3);
    assert_eq!(*got.borrow(), vec![(0, 7), (1, 7), (2, 7)]);
    hs[1]
        .update(cx, |_, w, _| w.remove_window())
        .expect("close");
    cx.run_until_parked();
    got.borrow_mut().clear();
    assert_eq!(cx.update(|cx| WindowManager::publish(Ping(8), cx)), 2);
    assert_eq!(*got.borrow(), vec![(0, 8), (2, 8)]);
    assert_eq!(
        cx.update(|cx| WindowManager::publish(42u8, cx)),
        0,
        "no subscribers for type"
    );
}

#[gpui_kit::test]
fn reentrant_publish_reaches_the_busy_window(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let a = open(&WindowKey::kind("a"), OpenMode::Reuse, cx);
    let b = open(&WindowKey::kind("b"), OpenMode::Reuse, cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    a.update(cx, |_, window, cx| {
        WindowManager::subscribe::<Ping>(window, cx, move |p, _, cx| {
            s.borrow_mut().push(p.0);
            if p.0 == 1 {
                WindowManager::publish(Ping(2), cx);
            }
        })
    })
    .expect("sub");
    let s = seen.clone();
    b.update(cx, |_, window, cx| {
        WindowManager::subscribe::<Ping>(window, cx, move |p, _, _| s.borrow_mut().push(100 + p.0))
    })
    .expect("sub");
    cx.update(|cx| WindowManager::publish(Ping(1), cx));
    cx.run_until_parked();
    let mut got = seen.borrow().clone();
    got.sort_unstable();
    assert_eq!(
        got,
        vec![1, 2, 101, 102],
        "every window gets both events exactly once"
    );
}

#[gpui_kit::test]
fn handler_may_close_its_window_mid_dispatch(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    let a = open(&WindowKey::kind("a"), OpenMode::Reuse, cx);
    let b = open(&WindowKey::kind("b"), OpenMode::Reuse, cx);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    a.update(cx, |_, window, cx| {
        WindowManager::subscribe::<Ping>(window, cx, move |p, window, cx| {
            s.borrow_mut().push(p.0);
            WindowManager::publish(Ping(p.0 + 10), cx); // deferred to a, which is closing
            window.remove_window();
        })
    })
    .expect("sub");
    let s = seen.clone();
    b.update(cx, |_, window, cx| {
        WindowManager::subscribe::<Ping>(window, cx, move |p, _, _| s.borrow_mut().push(100 + p.0))
    })
    .expect("sub");
    cx.update(|cx| WindowManager::publish(Ping(1), cx));
    cx.run_until_parked();
    let mut got = seen.borrow().clone();
    got.sort_unstable();
    assert_eq!(
        got,
        vec![1, 101, 111],
        "closed window gets nothing further, no panic"
    );
    assert!(
        cx.update(|cx| WindowManager::find(&WindowKey::kind("a"), cx))
            .is_none()
    );
    assert_eq!(cx.update(|cx| WindowManager::publish(Ping(3), cx)), 1);
}

#[gpui_kit::test]
fn layout_persists_and_restores(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("layout.json");
    let cfg = || WindowConfig {
        layout_path: Some(path.clone()),
        ..Default::default()
    };
    init(cx, cfg());
    let k = WindowKey::kind("main");
    let h = open(&k, OpenMode::Reuse, cx);
    let vcx = VisualTestContext::from_window(h.into(), cx);
    vcx.simulate_resize(size(px(700.0), px(500.0)));
    cx.update(WindowManager::flush);
    assert!(path.exists());
    let saved = cx.update(|cx| WindowManager::saved(&k, cx)).expect("saved");
    assert_eq!((saved.bounds.w, saved.bounds.h), (700.0, 500.0));
    assert!(cx.update(|cx| WindowManager::request_close(h.into(), cx)));
    cx.run_until_parked();
    // Fresh manager reads the file and restores the size.
    init(cx, cfg());
    let h2 = open(&k, OpenMode::Reuse, cx);
    let b = h2
        .update(cx, |_, w, _| w.window_bounds().get_bounds())
        .expect("b");
    assert_eq!(
        (f32::from(b.size.width), f32::from(b.size.height)),
        (700.0, 500.0)
    );
}

#[gpui_kit::test]
fn debounced_save_happens_without_flush(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("layout.json");
    init(
        cx,
        WindowConfig {
            layout_path: Some(path.clone()),
            save_debounce: Some(std::time::Duration::from_millis(50)),
            ..Default::default()
        },
    );
    let h = open(&WindowKey::kind("main"), OpenMode::Reuse, cx);
    VisualTestContext::from_window(h.into(), cx).simulate_resize(size(px(640.0), px(480.0)));
    assert!(!path.exists());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    assert!(path.exists());
}

#[gpui_kit::test]
fn persistence_slot_passes_to_sibling(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tmp");
    init(
        cx,
        WindowConfig {
            layout_path: Some(dir.path().join("l.json")),
            ..Default::default()
        },
    );
    let k = WindowKey::kind("main");
    let a = open(&k, OpenMode::Reuse, cx);
    let b = open(&k, OpenMode::New, cx);
    assert!(cx.update(|cx| WindowManager::request_close(a.into(), cx)));
    cx.run_until_parked();
    VisualTestContext::from_window(b.into(), cx).simulate_resize(size(px(555.0), px(444.0)));
    cx.update(WindowManager::flush);
    let saved = cx.update(|cx| WindowManager::saved(&k, cx)).expect("saved");
    assert_eq!(saved.bounds.w, 555.0);
}

#[gpui_kit::test]
fn corrupt_layout_is_reported_not_fatal(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("layout.json");
    std::fs::write(&path, b"garbage").expect("write");
    init(
        cx,
        WindowConfig {
            layout_path: Some(path),
            ..Default::default()
        },
    );
    assert!(cx.update(|cx| cx.global::<WindowManager>().recovered_layout.is_some()));
    open(&WindowKey::kind("main"), OpenMode::Reuse, cx);
}

#[gpui_kit::test]
fn rapid_open_close_leaves_no_residue(cx: &mut TestAppContext) {
    init(cx, WindowConfig::default());
    for i in 0..50 {
        let h = open(
            &WindowKey::instance("t", (i % 3).to_string()),
            OpenMode::New,
            cx,
        );
        h.update(cx, |_, window, cx| {
            WindowManager::subscribe::<Ping>(window, cx, |_, _, _| {})
        })
        .expect("sub");
        h.update(cx, |_, w, _| w.remove_window()).expect("close");
    }
    cx.run_until_parked();
    assert!(cx.update(|cx| WindowManager::windows(cx)).is_empty());
    assert_eq!(cx.update(|cx| WindowManager::publish(Ping(0), cx)), 0);
}

#[gpui_kit::test]
fn works_without_explicit_init(cx: &mut TestAppContext) {
    let h = open(&WindowKey::kind("main"), OpenMode::Reuse, cx);
    assert_eq!(
        cx.update(|cx| WindowManager::find(&WindowKey::kind("main"), cx)),
        Some(h.into())
    );
}
