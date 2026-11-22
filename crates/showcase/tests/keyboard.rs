//! API conventions §12 in the showcase shell: Tab cycles search → theme → catalog → page controls
//! and Shift+Tab goes back; clicks never take focus or move layout (regression: the Grid
//! "Columns 4 / 6 / 12" knob grew a 1px border on click/focus); switching pages releases
//! focus left on the old page.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, Focusable, TestAppContext, WindowOptions,
    test::TestWindowExt,
};
use guiraffe_showcase::shell::Showcase;

fn open(cx: &mut TestAppContext, start: &str) -> (AnyWindowHandle, Entity<Showcase>) {
    let slot: Rc<RefCell<Option<Entity<Showcase>>>> = Rc::default();
    let s2 = slot.clone();
    let start = start.to_owned();
    let handle = cx.update(move |cx| {
        guiraffe_core::init(cx);
        guiraffe_showcase::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
            let view = cx.new(|cx| Showcase::new(Some(&start), window, cx));
            *s2.borrow_mut() = Some(view.clone());
            guiraffe_overlays::root(view, window, cx)
        })
        .expect("window")
    });
    cx.run_until_parked();
    let view = slot.borrow().clone().expect("showcase view");
    (handle.0, view)
}

fn focus_search(cx: &mut TestAppContext, h: AnyWindowHandle, view: &Entity<Showcase>) {
    let v = view.clone();
    cx.update_window(h, move |_, window, cx| {
        let f = v.read(cx).search().focus_handle(cx);
        window.focus(&f, cx);
        window.render_frame(cx);
    })
    .expect("window");
}

#[gpui_kit::test]
fn tab_leaves_the_search_and_shift_tab_returns(cx: &mut TestAppContext) {
    let (h, view) = open(cx, "Grid");
    focus_search(cx, h, &view);
    let v = view.clone();
    cx.update_window(h, move |_, window, cx| {
        let search = v.read(cx).search().focus_handle(cx);
        window.press("tab", cx);
        assert!(
            !search.is_focused(window),
            "Tab is trapped in the search input"
        );
        assert!(window.focused(cx).is_some());
        window.press("tab", cx);
        assert!(
            v.read(cx).nav_focus().is_focused(window),
            "second Tab reaches the catalog list"
        );
        window.press("tab", cx);
        assert!(
            v.read(cx).page_focus().contains_focused(window, cx),
            "third Tab reaches the page's controls (focus scope)"
        );
        window.press("shift-tab", cx);
        assert!(
            v.read(cx).nav_focus().is_focused(window),
            "Shift+Tab goes back"
        );
        window.press("shift-tab", cx);
        window.press("shift-tab", cx);
        assert!(search.is_focused(window), "Shift+Tab returns to the search");
    })
    .expect("window");
}

#[gpui_kit::test]
fn catalog_arrows_navigate(cx: &mut TestAppContext) {
    let (h, view) = open(cx, "Grid");
    let before = view.read_with(cx, |v, _| v.selected());
    let v = view.clone();
    cx.update_window(h, move |_, window, cx| {
        let f = v.read(cx).nav_focus().clone();
        window.focus(&f, cx);
        window.press("down", cx);
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |v, _| v.selected()), before + 1);
}

#[gpui_kit::test]
fn grid_knob_click_keeps_focus_and_layout(cx: &mut TestAppContext) {
    let (h, view) = open(cx, "Grid");
    focus_search(cx, h, &view);
    let v = view.clone();
    cx.update_window(h, move |_, window, cx| {
        let knobs = window.find_all("segmented");
        assert!(!knobs.is_empty(), "the Grid page has knobs");
        let catalog = window.find("catalog").bounds();
        let base: Vec<_> = knobs.iter().map(|k| k.bounds()).collect();
        let target = base[base.len() - 1];
        window.drag(target.center(), target.center(), cx);
        let search = v.read(cx).search().focus_handle(cx);
        assert!(
            search.is_focused(window),
            "knob click stole focus from the search"
        );
        let after: Vec<_> = window
            .find_all("segmented")
            .iter()
            .map(|k| k.bounds())
            .collect();
        assert_eq!(base, after, "a knob grew or moved on click");
        assert_eq!(window.find("catalog").bounds(), catalog, "siblings moved");
    })
    .expect("window");
}

#[gpui_kit::test]
fn navigating_releases_focus_left_on_the_old_page(cx: &mut TestAppContext) {
    let (h, view) = open(cx, "Grid");
    let v = view.clone();
    cx.update_window(h, move |_, window, cx| {
        let page = v.read(cx).page_focus().clone();
        // Keyboard-focus a control on the page.
        window.focus(&page, cx);
        window.focus_next(cx);
        window.render_frame(cx);
        assert!(page.contains_focused(window, cx) && !page.is_focused(window));
        window.click(("entry", 0usize), cx);
        assert!(
            page.is_focused(window),
            "focus parked on the page container"
        );
    })
    .expect("window");
    assert_eq!(view.read_with(cx, |v, _| v.selected()), 0);
}
