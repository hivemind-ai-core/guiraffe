//! Menu section labels (issue 149): the default shaded, monospace, centred band in both
//! themes and a scoped override through the style context. Keyboard navigation is in
//! `menu_keyboard.rs`: Kit resolves its monospace font once per process, and a headless
//! test app (no installed fonts) must not be the one that resolves it here.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    component::menu::PopupMenu, component::menu::PopupMenuItem, div, px, rgb, size,
};
use guiraffe_core::{
    GuiraffeTheme, MenuSectionAlign, MenuSectionId, MenuSectionStyle, RegisterMenuSection, Scheme,
    StylePatch, apply, provide,
};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

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
    scope: Option<MenuSectionId>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let body = div().w(px(220.)).child(self.menu.clone());
        let body = match self.scope {
            Some(id) => provide(StylePatch::new().menu_section(id), body).into_any_element(),
            None => body.into_any_element(),
        };
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(12.))
            .child(body)
    }
}

fn shoot(name: &str, scheme: Scheme, scope: Option<MenuSectionId>) {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    let mut s = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .with_setup(guiraffe_components::init);
    let img = s
        .render(size(px(260.), px(200.)), move |window, cx| {
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
            let menu = menu(window, cx, Log::default());
            cx.new(|_| Host { menu, scope })
        })
        .unwrap_or_else(|e| panic!("{e:#}"));
    assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
}

#[test]
fn sections_light_and_dark() {
    shoot("menu-sections-light", Scheme::Light, None);
    shoot("menu-sections-dark", Scheme::Dark, None);
}

#[test]
fn a_scoped_section_style_overrides_the_theme() {
    let id = MenuSectionStyle::default()
        .background(rgb(0x2b4c7e))
        .color(rgb(0xffffff))
        .align(MenuSectionAlign::Start)
        .padding_y(px(4.))
        .register();
    shoot("menu-sections-scoped", Scheme::Light, Some(id));
}
