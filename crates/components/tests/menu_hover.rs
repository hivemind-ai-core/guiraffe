//! Issue 156: a dropdown and a context menu with the item under the pointer highlighted, in
//! both themes. The highlight is the theme's menu highlight, which clears 3:1 against the menu.
use gpui_kit::{
    AppContext, Context, ElementId, Entity, IntoElement, ParentElement, Render, Styled, Window,
    component::menu::{PopupMenu, PopupMenuItem},
    div, px, size,
    test::TestWindowExt,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Host(Entity<PopupMenu>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(12.))
            .child(div().w(px(220.)).child(self.0.clone()))
    }
}

/// A dropdown: checkable options under a section label.
fn dropdown(window: &mut Window, cx: &mut gpui_kit::App) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, |menu, _, _| {
        menu.min_w(px(200.))
            .label("Sort by")
            .item(PopupMenuItem::new("Name").checked(true))
            .item(PopupMenuItem::new("Date modified"))
            .item(PopupMenuItem::new("Size"))
    })
}

/// A context menu: Copy, Select all, then surface extras.
fn context(window: &mut Window, cx: &mut gpui_kit::App) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, |menu, _, _| {
        menu.min_w(px(200.))
            .item(PopupMenuItem::new("Copy"))
            .item(PopupMenuItem::new("Select all"))
            .separator()
            .item(PopupMenuItem::new("Find references"))
    })
}

fn shoot(
    name: &str,
    scheme: Scheme,
    build: fn(&mut Window, &mut gpui_kit::App) -> Entity<PopupMenu>,
    hovered: u64,
) {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    let mut s = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .with_setup(guiraffe_components::init);
    let img = s
        .render_after(
            size(px(260.), px(180.)),
            move |window, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let menu = build(window, cx);
                cx.new(|_| Host(menu))
            },
            move |window, cx| window.hover(ElementId::Integer(hovered), cx),
        )
        .unwrap_or_else(|e| panic!("{e:#}"));
    assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
}

#[test]
fn hovered_items_in_both_themes() {
    for (scheme, theme) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
        // Dropdown items: 0 label, 1 Name, 2 Date modified, 3 Size.
        shoot(&format!("menu-hover-dropdown-{theme}"), scheme, dropdown, 2);
        // Context items: 0 Copy, 1 Select all, 2 separator, 3 Find references.
        shoot(&format!("menu-hover-context-{theme}"), scheme, context, 1);
    }
}
