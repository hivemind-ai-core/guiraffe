//! spec/00 §12.5 interaction tests for ActionIcon, close_button and Pill remove.
use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render, Role,
    Styled, TestAppContext, VisualTestContext, Window,
    assets::IconName,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_components::{ActionIcon, Pill, close_button};
use guiraffe_core::focus_scope;

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
                        ActionIcon::new("fav", IconName::Heart)
                            .label("Favorite")
                            .on_click(move |_, _, _| h.set(h.get() + 1)),
                    )
                    .child(
                        ActionIcon::new("off", IconName::Star)
                            .label("Starred")
                            .disabled(true)
                            .on_click(move |_, _, _| d.set(d.get() + 1)),
                    )
                    .child(close_button("close"))
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
