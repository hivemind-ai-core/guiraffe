//! SegmentedControl, API conventions §12 / §12.2: no focus on click, opt-in keyboard, disabled,
//! AccessKit roles, and layout that never moves on focus / click / selection.
//! Regression: the showcase Grid knob ("4 / 6 / 12") grew by a 1px border on click/focus.
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, Styled, TestAppContext, VisualTestContext, Window,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_components::SegmentedControl;
use guiraffe_core::focus_scope;
use guiraffe_test::layout::{assert_same_bounds, capture};

struct Host {
    input: Entity<InputState>,
    seg: Entity<SegmentedControl>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
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
                        div()
                            .debug_selector(|| "seg".into())
                            .child(self.seg.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "right".into())
                            .w(px(20.))
                            .h(px(20.)),
                    ),
            ))
            .child(
                div()
                    .debug_selector(|| "below".into())
                    .w(px(20.))
                    .h(px(20.)),
            )
    }
}

const SELS: [&str; 3] = ["seg", "right", "below"];

fn setup(cx: &mut TestAppContext) -> (&mut VisualTestContext, Entity<Host>, FocusHandle) {
    cx.update(guiraffe_components::init);
    let (host, vcx) = cx.add_window_view(|window, cx| Host {
        input: cx.new(|cx| InputState::new(window, cx)),
        seg: SegmentedControl::new(["4", "6", "12"], cx),
    });
    vcx.run_until_parked();
    let input = vcx.update(|window, cx| {
        let h = host.read(cx).input.focus_handle(cx);
        window.focus(&h, cx);
        h
    });
    vcx.run_until_parked();
    (vcx, host, input)
}

fn seg(vcx: &mut VisualTestContext, host: &Entity<Host>) -> Entity<SegmentedControl> {
    host.read_with(vcx, |h, _| h.seg.clone())
}

#[gpui_kit::test]
fn click_selects_keeps_input_focus_and_layout(cx: &mut TestAppContext) {
    let (vcx, host, input) = setup(cx);
    let base = capture(vcx, &SELS);
    vcx.update(|window, cx| {
        window.click(("segment", 2usize), cx);
        assert!(input.is_focused(window), "segment click stole focus");
    });
    let s = seg(vcx, &host);
    assert_eq!(s.read_with(vcx, |s, _| s.selected()), 2);
    assert_same_bounds(vcx, &SELS, &base, "a segment is clicked and selected");
    vcx.update(|window, cx| window.hover(("segment", 1usize), cx));
    assert_same_bounds(vcx, &SELS, &base, "a segment is hovered");
}

#[gpui_kit::test]
fn tab_focus_shows_ring_without_moving_and_arrows_work(cx: &mut TestAppContext) {
    let (vcx, host, _) = setup(cx);
    let base = capture(vcx, &SELS);
    vcx.update(|window, cx| {
        window.focus_next(cx);
        // Any keystroke marks the modality as keyboard (as a real Tab would).
        window.press("f13", cx);
    });
    let s = seg(vcx, &host);
    assert!(vcx.update(|w, cx| s.read(cx).focus_handle(cx).is_focused(w)));
    assert!(
        vcx.debug_bounds("focus-ring").is_some(),
        "focus-visible ring drawn"
    );
    assert_same_bounds(vcx, &SELS, &base, "focused via Tab");
    vcx.simulate_keystrokes("right end");
    assert_eq!(s.read_with(vcx, |s, _| s.selected()), 2);
    vcx.simulate_keystrokes("home");
    assert_eq!(s.read_with(vcx, |s, _| s.selected()), 0);
    assert_same_bounds(vcx, &SELS, &base, "selection moved by keyboard");
}

#[gpui_kit::test]
fn disabled_suppresses_pointer_and_keys(cx: &mut TestAppContext) {
    let (vcx, host, _) = setup(cx);
    let s = seg(vcx, &host);
    vcx.update(|_, cx| s.update(cx, |s, cx| s.set_disabled(true, cx)));
    vcx.update(|window, cx| {
        window.click(("segment", 1usize), cx);
        assert_eq!(window.find("segmented").disabled(), Some(true));
        let h = s.read(cx).focus_handle(cx);
        window.focus(&h, cx);
    });
    vcx.simulate_keystrokes("right");
    assert_eq!(s.read_with(vcx, |s, _| s.selected()), 0);
}

#[gpui_kit::test]
fn radio_group_roles(cx: &mut TestAppContext) {
    let (vcx, _, _) = setup(cx);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("segmented").role(), Some(Role::RadioGroup));
        let six = window.find_by_label("6");
        assert_eq!(six.role(), Some(Role::RadioButton));
        assert_eq!(six.selected(), Some(false));
        assert_eq!(window.find_by_label("4").selected(), Some(true));
    });
}
