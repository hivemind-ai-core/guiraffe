//! The shared selection context menu on MarkdownView.
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement as _,
    Render, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};
use guiraffe_markdown::{MarkdownState, MarkdownView};

struct Host(Entity<MarkdownState>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .child(MarkdownView::new("menu").state(&self.0))
    }
}

fn mount(cx: &mut TestAppContext) -> (Entity<MarkdownState>, &mut VisualTestContext) {
    cx.update(guiraffe_core::init);
    let mut state = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let s = cx.new(|cx| MarkdownState::new("Hello **world**.\n\nSecond paragraph.", cx));
        state = Some(s.clone());
        Host(s)
    });
    vcx.run_until_parked();
    (state.unwrap_or_else(|| panic!("state")), vcx)
}

fn right_click(vcx: &mut VisualTestContext) {
    let p = point(px(12.), px(10.));
    vcx.simulate_mouse_down(p, MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(p, MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
}

fn copy_enabled(state: &Entity<MarkdownState>, vcx: &mut VisualTestContext) -> Option<bool> {
    vcx.update(|_, cx| {
        state
            .read(cx)
            .context_menu()
            .entries()
            .into_iter()
            .find(|(l, _)| l.as_ref() == "Copy")
            .map(|(_, e)| e)
    })
}

#[gpui_kit::test]
fn right_click_menu_copies_selection_select_all_and_escape(cx: &mut TestAppContext) {
    let (state, vcx) = mount(cx);
    vcx.update(|_, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("x".into())));
    right_click(vcx);
    let menu = vcx.update(|_, cx| state.read(cx).context_menu().clone());
    assert!(menu.is_open());
    assert_eq!(menu.position(), Some(point(px(12.), px(10.))));
    assert_eq!(copy_enabled(&state, vcx), Some(false), "nothing selected");
    vcx.update(|window, cx| assert!(menu.activate("Select all", window, cx)));
    vcx.run_until_parked();
    assert!(!menu.is_open());
    let selected = vcx.update(|_, cx| state.read(cx).text().read(cx).selected_text());
    assert!(selected.contains("Hello") && selected.contains("Second paragraph"));
    right_click(vcx);
    assert_eq!(copy_enabled(&state, vcx), Some(true));
    vcx.update(|window, cx| assert!(menu.activate("Copy", window, cx)));
    let clip = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(clip, Some(selected));
    right_click(vcx);
    assert!(menu.is_open());
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(!menu.is_open());
}
