use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div, px,
};
use guiraffe_components::{SegmentedControl, SegmentedEvent};
use guiraffe_test::render::RenderCounter;

struct Host {
    control: Entity<SegmentedControl>,
    renders: RenderCounter,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        self.renders.hit();
        div().w(px(400.)).child(self.control.clone())
    }
}

#[gpui_kit::test]
fn segmented_control_selects_and_emits_once(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let renders = RenderCounter::new();
    let r2 = renders.clone();
    let mut control = None;
    let (_host, vcx) = cx.add_window_view(|_, cx| {
        let c = SegmentedControl::new(["React", "Vue", "Svelte"], cx);
        control = Some(c.clone());
        Host {
            control: c,
            renders: r2,
        }
    });
    let control = control.unwrap_or_else(|| unreachable!());
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    let _sub = vcx.update(|_, cx| {
        cx.subscribe(&control, move |_, e: &SegmentedEvent, _| {
            ev.borrow_mut().push(e.clone())
        })
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(2, cx)));
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(2, cx))); // no-op
    vcx.update(|_, cx| control.update(cx, |c, cx| c.select(99, cx))); // clamped to 2: no-op
    vcx.run_until_parked();
    assert_eq!(*events.borrow(), vec![SegmentedEvent::Changed(2)]);
    assert_eq!(vcx.update(|_, cx| control.read(cx).selected()), 2);
}
