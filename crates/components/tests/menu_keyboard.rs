//! Keyboard navigation in a menu with section labels skips the labels (issue 149).
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    Context, Entity, Focusable, IntoElement, ParentElement, Render, TestAppContext, Window,
    component::menu::PopupMenu, component::menu::PopupMenuItem, div, px,
};

type Log = Rc<RefCell<Vec<&'static str>>>;

fn menu(window: &mut Window, cx: &mut gpui_kit::App, log: Log) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, move |menu, _, _| {
        let item = |label: &'static str| {
            let log = log.clone();
            PopupMenuItem::new(label).on_click(move |_, _, _| log.borrow_mut().push(label))
        };
        menu.min_w(px(200.))
            .label("Edit")
            .item(item("Cut"))
            .item(item("Copy"))
            .separator()
            .label("View")
            .item(item("Zoom in"))
    })
}

struct Host {
    menu: Entity<PopupMenu>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.menu.clone())
    }
}

#[gpui_kit::test]
fn keyboard_navigation_skips_section_labels(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let log = Log::default();
    let l = log.clone();
    let mut built = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let menu = menu(window, cx, l);
        built = Some(menu.clone());
        Host { menu }
    });
    let menu = built.unwrap_or_else(|| panic!("menu"));
    vcx.update(|window, cx| menu.focus_handle(cx).focus(window, cx));
    vcx.run_until_parked();
    // Down lands on Cut (past "Edit"), then Copy, then Zoom in (past the separator and "View").
    vcx.simulate_keystrokes("down down down enter");
    assert_eq!(*log.borrow(), vec!["Zoom in"]);
}
