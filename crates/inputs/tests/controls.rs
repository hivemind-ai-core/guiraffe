//! spec/00 §12 / §12.2 interaction tests: ChipGroup, TagsInput remove, RangeSlider, Dropzone.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, Styled, TestAppContext, VisualTestContext, Window,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_core::focus_scope;
use guiraffe_inputs::*;
use guiraffe_test::layout::{assert_same_bounds, capture};

struct Host {
    input: Entity<InputState>,
    chips: Entity<ChipGroup>,
    range: Entity<RangeSlider>,
    drop: Entity<Dropzone>,
    tags: Entity<TagsInput>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().w(px(200.)).child(Input::new(&self.input)))
            .child(focus_scope(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .debug_selector(|| "chips".into())
                            .child(self.chips.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "range".into())
                            .child(self.range.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(|| "drop".into())
                            .child(self.drop.clone()),
                    ),
            ))
            .child(
                div()
                    .debug_selector(|| "tags".into())
                    .child(self.tags.clone()),
            )
    }
}

const SELS: [&str; 4] = ["chips", "range", "drop", "tags"];

fn mount(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext, FocusHandle) {
    cx.update(guiraffe_inputs::init);
    let (host, vcx) = cx.add_window_view(|window, cx| {
        let tags = TagsInput::new(TagRules::default(), "Add tag", window, cx);
        tags.update(cx, |t, cx| {
            t.add("rust", cx);
        });
        Host {
            input: cx.new(|cx| InputState::new(window, cx)),
            chips: ChipGroup::new(["Rust", "Zig", "Go"], true, cx),
            range: RangeSlider::new(Range2::new(0.0, 100.0, 1.0, 20.0, 80.0), cx),
            drop: Dropzone::new(vec!["image/*"], "Drop images", cx),
            tags,
        }
    });
    vcx.run_until_parked();
    let input = vcx.update(|window, cx| {
        let h = host.read(cx).input.focus_handle(cx);
        window.focus(&h, cx);
        h
    });
    vcx.run_until_parked();
    (host, vcx, input)
}

fn keyboard_focus_next(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.focus_next(cx);
        // Any keystroke marks the modality as keyboard, as a real Tab would.
        window.press("f13", cx);
    });
}

#[gpui_kit::test]
fn pointer_never_takes_focus_and_layout_is_stable(cx: &mut TestAppContext) {
    let (host, vcx, input) = mount(cx);
    let base = capture(vcx, &SELS);
    vcx.update(|window, cx| {
        window.click(("chip", 1usize), cx);
        window.click("range", cx);
        window.click("dropzone", cx);
        assert!(input.is_focused(window), "a pointer press stole focus");
    });
    let chips = host.read_with(vcx, |h, _| h.chips.clone());
    assert_eq!(chips.read_with(vcx, |c, _| c.selected()), vec![1]);
    assert_same_bounds(vcx, &SELS, &base, "chips, range and dropzone are clicked");
    vcx.update(|window, cx| window.hover(("chip", 0usize), cx));
    assert_same_bounds(vcx, &SELS, &base, "a chip is hovered");
    keyboard_focus_next(vcx);
    assert!(vcx.debug_bounds("focus-ring").is_some());
    assert_same_bounds(vcx, &SELS, &base, "a chip has keyboard focus");
}

#[gpui_kit::test]
fn keyboard_inside_scope_toggles_chips_moves_range_and_browses(cx: &mut TestAppContext) {
    let (host, vcx, _) = mount(cx);
    let (chips, range, drop) = host.read_with(vcx, |h, _| {
        (h.chips.clone(), h.range.clone(), h.drop.clone())
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&drop, move |_, e: &DropEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach()
    });
    keyboard_focus_next(vcx);
    vcx.update(|window, cx| window.press("space", cx));
    assert_eq!(chips.read_with(vcx, |c, _| c.selected()), vec![0]);
    // chips 1, 2, then the range slider.
    for _ in 0..3 {
        keyboard_focus_next(vcx);
    }
    assert!(vcx.update(|w, cx| range.read(cx).focus_handle(cx).is_focused(w)));
    vcx.simulate_keystrokes("right shift-left home");
    assert_eq!(range.read_with(vcx, |r, _| r.values()), (0.0, 79.0));
    vcx.simulate_keystrokes("end");
    assert_eq!(range.read_with(vcx, |r, _| r.values()), (0.0, 100.0));
    keyboard_focus_next(vcx);
    vcx.update(|window, cx| window.press("enter", cx));
    assert_eq!(*events.borrow(), vec![DropEvent::BrowseRequested]);
}

#[gpui_kit::test]
fn disabled_suppresses_everything(cx: &mut TestAppContext) {
    let (host, vcx, _) = mount(cx);
    let (chips, range, drop) = host.read_with(vcx, |h, _| {
        (h.chips.clone(), h.range.clone(), h.drop.clone())
    });
    vcx.update(|_, cx| {
        chips.update(cx, |c, cx| c.set_disabled(true, cx));
        range.update(cx, |c, cx| c.set_disabled(true, cx));
        drop.update(cx, |c, cx| c.set_disabled(true, cx));
    });
    vcx.update(|window, cx| {
        window.click(("chip", 0usize), cx);
        window.click("range", cx);
        assert_eq!(window.find(("chip", 0usize)).disabled(), Some(true));
        assert_eq!(window.find("range").disabled(), Some(true));
        assert_eq!(window.find("dropzone").disabled(), Some(true));
        let h = range.read(cx).focus_handle(cx);
        window.focus(&h, cx);
    });
    vcx.simulate_keystrokes("right");
    assert!(chips.read_with(vcx, |c, _| c.selected()).is_empty());
    assert_eq!(range.read_with(vcx, |r, _| r.values()), (20.0, 80.0));
    vcx.update(|_, cx| drop.update(cx, |d, cx| d.receive(&["a.png".into()], cx)));
}

#[gpui_kit::test]
fn accessibility_roles_and_values(cx: &mut TestAppContext) {
    let (host, vcx, input) = mount(cx);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        let rust = window.find_by_label("Rust");
        assert_eq!(rust.role(), Some(Role::CheckBox));
        assert_eq!(rust.checked(), Some(false));
        let range = window.find("range");
        assert_eq!(range.role(), Some(Role::Slider));
        assert_eq!(range.label(), Some("Range"));
        assert_eq!(
            window.find_by_label("Drop images").role(),
            Some(Role::Button)
        );
        let remove = window.find_by_label("Remove rust");
        assert_eq!(remove.role(), Some(Role::Button));
        window.click(remove_id(), cx);
        assert!(input.is_focused(window), "tag remove stole focus");
    });
    let tags = host.read_with(vcx, |h, _| h.tags.clone());
    assert!(tags.read_with(vcx, |t, _| t.tags().is_empty()));
}

fn remove_id() -> gpui_kit::ElementId {
    (gpui_kit::ElementId::from(("tag", 0usize)), "remove").into()
}
