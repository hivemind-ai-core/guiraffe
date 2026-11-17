//! ScrollArea::track_scroll lets the app jump to a child (section navigation).
use gpui_kit::{
    Context, IntoElement, ParentElement, Render, ScrollHandle, Styled, TestAppContext, Window, div,
    px,
};
use guiraffe_layout::ScrollArea;

struct Long(ScrollHandle);

impl Render for Long {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(200.)).w(px(200.)).child(
            ScrollArea::new("long")
                .track_scroll(&self.0)
                .children((0..30).map(|i| div().h(px(40.)).child(format!("row {i}")))),
        )
    }
}

#[gpui_kit::test]
fn scroll_to_item_moves_the_area(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let h = handle.clone();
    let (_, cx) = cx.add_window_view(|_, _| Long(h));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(handle.offset().y, px(0.));
    handle.scroll_to_item(20);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        handle.offset().y < px(-600.),
        "row 20 (y=800) brought into a 200px view: {:?}",
        handle.offset()
    );
}
