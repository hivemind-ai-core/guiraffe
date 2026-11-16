//! The gallery shell: header with theme toggle, router-driven sidebar, page area.
//!
//! Each page is its own entity, created once and kept, so page state survives navigation and
//! a page re-renders independently of the shell (conventions §6.1).

use std::collections::HashMap;

use gpui_kit::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, Window, div, prelude::*,
    px,
};
use guiraffe_components::{ActionIcon, Text, Title};
use guiraffe_core::{GuiraffeTheme, Scheme, Size, Variant, set_scheme};
use guiraffe_router::Router;
use guiraffe_state::Selected;

use crate::{Page, pages};

pub struct Gallery {
    router: Router<Page>,
    current: Selected<Page>,
    pages: HashMap<Page, AnyView>,
}

impl Gallery {
    pub fn new(initial: Page, cx: &mut Context<Self>) -> Self {
        let router = Router::new(initial, cx);
        let current = router.select(cx, |p| *p);
        Self {
            router,
            current,
            pages: HashMap::new(),
        }
    }

    pub fn router(&self) -> &Router<Page> {
        &self.router
    }

    fn page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) -> AnyView {
        self.pages
            .entry(page)
            .or_insert_with(|| pages::build(page, window, cx))
            .clone()
    }
}

fn sidebar_item(page: Page, active: bool, router: Router<Page>, cx: &App) -> impl IntoElement {
    let t = GuiraffeTheme::global(cx);
    let s = t.surfaces();
    let v = t.variant(None, Variant::Light);
    div()
        .id(page.title())
        .flex()
        .items_center()
        .gap(px(Size::Xs.spacing()))
        .px(px(Size::Sm.spacing()))
        .h(px(Size::Sm.control_height()))
        .rounded(px(t.default_radius.radius()))
        .cursor_pointer()
        .text_size(px(Size::Sm.font_size()))
        .text_color(if active {
            v.foreground.to_hsla()
        } else {
            s.text.to_hsla()
        })
        .when(active, |el| el.bg(v.background.to_hsla()))
        .when(!active, |el| el.hover(|st| st.bg(s.hover.to_hsla())))
        .child(page.icon())
        .child(page.title())
        .on_click(move |_, _, cx| {
            router.navigate(page, cx);
        })
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = *self.current.get(cx);
        let content = self.page(current, window, cx);
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let dark = t.scheme == Scheme::Dark;
        let router = self.router.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .child(
                div()
                    .h(px(52.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(Size::Md.spacing()))
                    .border_b_1()
                    .border_color(s.border.to_hsla())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(Title::new("GUIraffe 🦒").order(4))
                            .child(Text::new("gallery").dimmed(true).size(Size::Sm)),
                    )
                    .child(
                        ActionIcon::new(
                            "theme",
                            if dark {
                                gpui_kit::assets::IconName::Sun
                            } else {
                                gpui_kit::assets::IconName::Moon
                            },
                        )
                        .variant(Variant::Default)
                        .on_click(move |_, _, cx| {
                            set_scheme(if dark { Scheme::Light } else { Scheme::Dark }, cx)
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(208.))
                            .flex_none()
                            .p(px(Size::Xs.spacing()))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .border_r_1()
                            .border_color(s.border.to_hsla())
                            .bg(s.surface.to_hsla())
                            .children(
                                Page::ALL
                                    .iter()
                                    .map(|p| sidebar_item(*p, *p == current, router.clone(), cx)),
                            ),
                    )
                    .child(
                        div()
                            .id("page")
                            .min_h_0()
                            .when(current == Page::Chat, |el| el.h_full().overflow_hidden())
                            .flex_1()
                            .min_w_0()
                            .when(current != Page::Chat, |el| el.overflow_y_scroll())
                            .child(content),
                    ),
            )
    }
}

/// Open a gallery window through the window manager (with overlays) on `page`.
pub fn open(page: Page, cx: &mut App) -> anyhow::Result<()> {
    use guiraffe_windows::{OpenMode, WindowKey, WindowManager};
    let n = WindowManager::windows(cx).len();
    WindowManager::open(
        WindowKey::instance("gallery", n.to_string()),
        OpenMode::New,
        gpui_kit::WindowOptions {
            titlebar: Some(gpui_kit::TitlebarOptions {
                title: Some("GUIraffe gallery".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| Gallery::new(page, cx));
            guiraffe_overlays::root(view, window, cx)
        },
        cx,
    )?;
    Ok(())
}
