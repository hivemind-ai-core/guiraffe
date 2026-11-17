//! spec/00 §12.5 interaction tests for ActionIcon, close_button and Pill remove.
use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, Styled, TestAppContext, VisualTestContext, Window,
    assets::IconName,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_components::{ActionIcon, Pill, close_button};
use guiraffe_core::focus_scope;
use guiraffe_test::layout::{assert_same_bounds, capture};

struct Host {
    input: Entity<InputState>,
    hits: Rc<Cell<u32>>,
    disabled_hits: Rc<Cell<u32>>,
    removed: Rc<Cell<u32>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (h, d, r) = (
            self.hits.clone(),
            self.disabled_hits.clone(),
            self.removed.clone(),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().w(px(200.)).child(Input::new(&self.input)))
            .child(focus_scope(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        div().debug_selector(|| "fav-box".into()).child(
                            ActionIcon::new("fav", IconName::Heart)
                                .label("Favorite")
                                .on_click(move |_, _, _| h.set(h.get() + 1)),
                        ),
                    )
                    .child(
                        ActionIcon::new("off", IconName::Star)
                            .label("Starred")
                            .disabled(true)
                            .on_click(move |_, _, _| d.set(d.get() + 1)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "close-box".into())
                            .child(close_button("close")),
                    )
                    .child(Pill::new("pill", "Rust").on_remove(move |_, _, _| r.set(r.get() + 1))),
            ))
    }
}

fn setup(cx: &mut TestAppContext) -> (&mut VisualTestContext, Entity<Host>, FocusHandle) {
    cx.update(guiraffe_components::init);
    let (host, vcx) = cx.add_window_view(|window, cx| Host {
        input: cx.new(|cx| InputState::new(window, cx)),
        hits: Rc::default(),
        disabled_hits: Rc::default(),
        removed: Rc::default(),
    });
    vcx.run_until_parked();
    let handle = vcx.update(|window, cx| {
        let h = host.read(cx).input.focus_handle(cx);
        window.focus(&h, cx);
        h
    });
    vcx.run_until_parked();
    (vcx, host, handle)
}

#[gpui_kit::test]
fn click_fires_without_taking_focus(cx: &mut TestAppContext) {
    let (vcx, host, input) = setup(cx);
    vcx.update(|window, cx| {
        window.click("fav", cx);
        assert!(input.is_focused(window), "ActionIcon click stole focus");
    });
    assert_eq!(host.read_with(vcx, |h, _| h.hits.get()), 1);
}

#[gpui_kit::test]
fn disabled_suppresses_click_and_is_exposed(cx: &mut TestAppContext) {
    let (vcx, host, input) = setup(cx);
    vcx.update(|window, cx| {
        window.click("off", cx);
        assert!(input.is_focused(window));
        let off = window.find("off");
        assert_eq!(off.disabled(), Some(true));
        assert_eq!(window.find("fav").disabled(), None);
    });
    assert_eq!(host.read_with(vcx, |h, _| h.disabled_hits.get()), 0);
}

#[gpui_kit::test]
fn keyboard_activation_inside_focus_scope(cx: &mut TestAppContext) {
    let (vcx, host, input) = setup(cx);
    vcx.update(|window, cx| {
        window.focus_next(cx);
        window.render_frame(cx);
        assert!(!input.is_focused(window));
        assert_eq!(
            window.find("fav").focused(),
            Some(true),
            "Tab reaches ActionIcon"
        );
        window.press("enter", cx);
        window.press("space", cx);
        // The disabled icon is skipped: next stop is the close button.
        window.focus_next(cx);
        window.render_frame(cx);
        assert_ne!(window.find("off").focused(), Some(true));
        assert_eq!(window.find("close").focused(), Some(true));
    });
    assert_eq!(host.read_with(vcx, |h, _| h.hits.get()), 2);
}

#[gpui_kit::test]
fn roles_and_names(cx: &mut TestAppContext) {
    let (vcx, host, _) = setup(cx);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_by_label("Favorite").role(), Some(Role::Button));
        assert_eq!(window.find_by_label("Close").role(), Some(Role::Button));
        let remove = window.find_by_label("Remove Rust");
        assert_eq!(remove.role(), Some(Role::Button));
        window.click_at(
            (gpui_kit::ElementId::from("pill"), "remove"),
            gpui_kit::point(px(4.), px(4.)),
            cx,
        );
    });
    assert_eq!(host.read_with(vcx, |h, _| h.removed.get()), 1);
}

#[gpui_kit::test]
fn focus_hover_and_press_never_move_layout(cx: &mut TestAppContext) {
    let (vcx, _, _) = setup(cx);
    let sels = ["fav-box", "close-box"];
    let base = capture(vcx, &sels);
    vcx.update(|window, cx| window.hover("fav", cx));
    assert_same_bounds(vcx, &sels, &base, "hovered");
    vcx.update(|window, cx| {
        window.focus_next(cx);
        // Any keystroke marks the modality as keyboard (as a real Tab would).
        window.press("f13", cx);
    });
    assert!(
        vcx.debug_bounds("focus-ring").is_some(),
        "focus-visible ring drawn"
    );
    assert_same_bounds(vcx, &sels, &base, "focused via Tab");
    vcx.update(|window, cx| window.click("fav", cx));
    assert_same_bounds(vcx, &sels, &base, "clicked");
}

/// The Indicator reserves its badge overhang: the mark lies inside the wrapper, so an
/// `overflow_hidden` ancestor (Paper) never clips it.
#[gpui_kit::test]
fn indicator_mark_stays_inside_its_wrapper(cx: &mut TestAppContext) {
    use guiraffe_components::{Indicator, ThemeIcon};
    struct V;
    impl Render for V {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().p(px(20.)).child(
                div().debug_selector(|| "wrap".into()).flex().child(
                    Indicator::new(ThemeIcon::new(IconName::Mail))
                        .label("12")
                        .color(guiraffe_core::ColorName::Red),
                ),
            )
        }
    }
    cx.update(guiraffe_components::init);
    let (_, vcx) = cx.add_window_view(|_, _| V);
    vcx.run_until_parked();
    let wrap = vcx.debug_bounds("wrap").expect("wrapper");
    let child = vcx
        .debug_bounds("indicator-child")
        .expect("the child of the indicator");
    // The wrapper is larger than the child on the sides of the corner the
    // mark sits in, so the mark, which reaches over that corner, still lies
    // inside the wrapper.
    assert!(
        wrap.top() <= child.top() && wrap.right() >= child.right(),
        "badge overflows its wrapper: {wrap:?} {child:?}"
    );
}
