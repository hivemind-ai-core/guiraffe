use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};
use guiraffe_inputs::*;

struct Host {
    tags: Entity<TagsInput>,
    range: Entity<RangeSlider>,
    chips: Entity<ChipGroup>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(self.tags.clone())
            .child(
                div()
                    .debug_selector(|| "range".into())
                    .child(self.range.clone()),
            )
            .child(self.chips.clone())
    }
}

fn mount(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(guiraffe_inputs::init);
    cx.add_window_view(|window, cx| Host {
        tags: TagsInput::new(
            TagRules {
                max_tags: Some(3),
                ..Default::default()
            },
            "Add tag",
            window,
            cx,
        ),
        range: RangeSlider::new(Range2::new(0.0, 100.0, 1.0, 20.0, 80.0), cx),
        chips: ChipGroup::new(["Rust", "Zig", "Go"], false, cx),
    })
}

#[gpui_kit::test]
fn tags_from_typing_enter_backspace_and_rejections(cx: &mut TestAppContext) {
    let (host, vcx) = mount(cx);
    let tags = vcx.update(|_, cx| host.read(cx).tags.clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&tags, move |_, e: &TagsEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
        .detach()
    });
    let input = vcx.update(|_, cx| tags.read(cx).input_for_test());
    vcx.update(|window, cx| {
        let h = input.read(cx).focus_handle(cx);
        window.focus(&h, cx);
    });
    vcx.simulate_input("rust, gpui,");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| tags.read(cx).tags().to_vec()),
        vec!["rust", "gpui"],
        "split on commas while typing"
    );
    vcx.simulate_input("RUST");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, TagsEvent::Rejected(t, TagRejected::Duplicate) if t == "RUST"))
    );
    vcx.update(|window, cx| input.update(cx, |i, cx| i.set_value("", window, cx)));
    vcx.simulate_keystrokes("backspace");
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| tags.read(cx).tags().to_vec()),
        vec!["rust"],
        "backspace on empty removes the last tag"
    );
}

#[gpui_kit::test]
fn range_slider_keyboard_and_pointer(cx: &mut TestAppContext) {
    let (host, vcx) = mount(cx);
    let range = vcx.update(|_, cx| host.read(cx).range.clone());
    vcx.update(|window, cx| {
        let h = range.read(cx).focus_handle(cx);
        window.focus(&h, cx);
    });
    vcx.simulate_keystrokes("right right shift-left");
    assert_eq!(vcx.update(|_, cx| range.read(cx).values()), (22.0, 79.0));
    // Pointer: press near the right end grabs the high thumb.
    vcx.run_until_parked();
    let b = vcx
        .debug_bounds("range")
        .unwrap_or_else(|| panic!("range bounds"));
    let at = |f: f32| gpui_kit::point(b.origin.x + b.size.width * f, b.center().y);
    vcx.simulate_mouse_down(
        at(0.95),
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        at(0.95),
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    let (lo, hi) = vcx.update(|_, cx| range.read(cx).values());
    assert_eq!(lo, 22.0, "low thumb untouched");
    assert!(
        (hi - 95.0).abs() <= 2.0,
        "high thumb moved to the press: {hi}"
    );
}

#[gpui_kit::test]
fn chips_and_dropzone(cx: &mut TestAppContext) {
    let (host, vcx) = mount(cx);
    let chips = vcx.update(|_, cx| host.read(cx).chips.clone());
    vcx.update(|_, cx| {
        chips.update(cx, |c, cx| {
            c.toggle(1, cx);
            c.toggle(1, cx);
            c.toggle(9, cx);
        })
    });
    assert_eq!(vcx.update(|_, cx| chips.read(cx).selected()), vec![1]);
    let dz = vcx.update(|_, cx| Dropzone::new(vec!["image/*"], "Drop images", cx));
    let got = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    vcx.update(|_, cx| {
        cx.subscribe(&dz, move |_, e: &DropEvent, _| {
            g.borrow_mut().push(e.clone())
        })
        .detach()
    });
    vcx.update(|_, cx| {
        dz.update(cx, |d, cx| {
            d.receive(&["a.png".into(), "notes.md".into()], cx)
        })
    });
    assert_eq!(
        *got.borrow(),
        vec![
            DropEvent::Accepted(vec!["a.png".into()]),
            DropEvent::Rejected(vec!["notes.md".into()])
        ]
    );
}
