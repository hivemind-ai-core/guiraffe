use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, TestAppContext,
    Window, div,
};
use std::{cell::Cell, rc::Rc};

struct Child(Rc<Cell<usize>>);

impl Render for Child {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.set(self.0.get() + 1);
        div().size_full()
    }
}

struct Host(Entity<Child>, Rc<Cell<usize>>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.1.set(self.1.get() + 1);
        div().size_full().child(
            self.0
                .clone()
                .cached(gpui_kit::StyleRefinement::default().size_full()),
        )
    }
}

struct Root(Entity<Host>);

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            self.0
                .clone()
                .cached(gpui_kit::StyleRefinement::default().size_full()),
        )
    }
}

#[gpui_kit::test]
fn notifying_only_a_cached_child(cx: &mut TestAppContext) {
    let child_renders = Rc::new(Cell::new(0));
    let host_renders = Rc::new(Cell::new(0));
    let child = cx.new(|_| Child(child_renders.clone()));
    let host = cx.new(|_| Host(child.clone(), host_renders.clone()));
    let (_, window) = cx.add_window_view(|_, _| Root(host));
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    window.run_until_parked();
    let before = (child_renders.get(), host_renders.get());
    assert!(before.0 > 0 && before.1 > 0);

    window.update(|_, cx| cx.notify(child.entity_id()));
    window.run_until_parked();
    assert!(child_renders.get() > before.0);
    if gpui_kit::GPUI_FAST {
        assert_eq!(host_renders.get(), before.1);
    } else {
        assert!(host_renders.get() > before.1);
    }
}
