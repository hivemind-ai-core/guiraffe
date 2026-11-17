//! Turning a knob updates and re-renders the preview entity.
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::{AnyElement, App, IntoElement, TestAppContext, Window, div};
use guiraffe_showcase::knobs::{Knob, KnobValues, playground_entity};

static RENDERS: AtomicUsize = AtomicUsize::new(0);

fn render(
    _: &(),
    _: &KnobValues,
    _: &mut Window,
    _: &mut gpui_kit::Context<guiraffe_showcase::knobs::Preview<()>>,
) -> AnyElement {
    RENDERS.fetch_add(1, Ordering::SeqCst);
    div().into_any_element()
}

#[gpui_kit::test]
fn knob_change_reaches_preview(cx: &mut TestAppContext) {
    cx.update(guiraffe_showcase::init);
    let (_root, vcx) = cx.add_window_view(|window, cx| {
        let (pg, preview) = playground_entity(
            "t",
            &[Knob::size(), Knob::toggle("On")],
            |_: &mut Window, _: &mut App| (),
            render,
            window,
            cx,
        );
        Root(pg, preview)
    });
    vcx.run_until_parked();
    let before = RENDERS.load(Ordering::SeqCst);
    assert!(before >= 1, "preview rendered");
    let (knob, preview) = vcx.update(|_, cx| {
        let root = _root.read(cx);
        (root.0.read(cx).knob(1).cloned(), root.1.clone())
    });
    let knob = knob.expect("knob 1 exists");
    vcx.update(|_, cx| knob.update(cx, |k, cx| k.select(1, cx)));
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(preview.read(cx).values().0, vec![2, 1]);
    });
    assert!(
        RENDERS.load(Ordering::SeqCst) > before,
        "preview re-rendered"
    );
}

struct Root(
    gpui_kit::Entity<guiraffe_showcase::knobs::Playground>,
    gpui_kit::Entity<guiraffe_showcase::knobs::Preview<()>>,
);

impl gpui_kit::Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}
