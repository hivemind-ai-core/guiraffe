//! App shell: router sidebar, header with the theme toggle, Ctrl/Cmd+K command palette.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, KeyBinding, ParentElement, Render,
    Styled, Window, actions, div, prelude::*, px,
};
use guiraffe_components::{ActionIcon, Text, Title};
use guiraffe_core::{GuiraffeTheme, Scheme, Size, Variant, set_scheme};
use guiraffe_overlays::{Spotlight, search::Command};
use guiraffe_router::{Route, Router, routes};

use crate::pages::{ChatPage, Dashboard, DocsPage, FormPage, GridPage};

routes! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Page {
        Dashboard = "/",
        Grid = "/grid",
        Docs = "/docs",
        Chat = "/chat",
        Form = "/form",
    }
}

pub const PAGES: [(Page, &str); 5] = [
    (Page::Dashboard, "Dashboard"),
    (Page::Grid, "Data grid"),
    (Page::Docs, "Document"),
    (Page::Chat, "Chat"),
    (Page::Form, "Form"),
];

actions!(compare, [OpenPalette]);

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("secondary-k", OpenPalette, None)]);
}

pub fn toggle_theme(cx: &mut App) {
    let dark = GuiraffeTheme::global(cx).scheme == Scheme::Dark;
    set_scheme(if dark { Scheme::Light } else { Scheme::Dark }, cx);
}

pub struct Shell {
    pub router: Router<Page>,
    page: guiraffe_state::Selected<Page>,
    pub dashboard: Entity<Dashboard>,
    pub grid: Option<Entity<GridPage>>,
    pub docs: Option<Entity<DocsPage>>,
    pub chat: Option<Entity<ChatPage>>,
    pub form: Option<Entity<FormPage>>,
    palette: Entity<Spotlight>,
}

fn commands() -> Vec<Command> {
    let mut c: Vec<Command> = PAGES
        .iter()
        .map(|(p, label)| Command::new(p.to_path(), format!("Go to {label}")))
        .collect();
    c.push(Command {
        keywords: "dark light scheme".into(),
        ..Command::new("theme", "Toggle theme")
    });
    c
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let router = Router::new(Page::Dashboard, cx);
        let page = router.select(cx, |p| *p);
        let r = router.clone();
        let palette = cx.new(|cx| {
            Spotlight::new(commands(), window, cx, move |id, _, cx| {
                if id == "theme" {
                    toggle_theme(cx);
                } else {
                    r.navigate_path(id, cx);
                }
            })
        });
        Self {
            router,
            page,
            dashboard: cx.new(Dashboard::new),
            grid: None,
            docs: None,
            chat: None,
            form: None,
            palette,
        }
    }

    pub fn current(&self, cx: &App) -> Page {
        *self.page.get(cx)
    }

    /// The current page's view, created on first visit.
    fn page_view(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) -> AnyView {
        fn get<T: Render>(
            slot: &mut Option<Entity<T>>,
            make: impl FnOnce(&mut Window, &mut Context<T>) -> T,
            window: &mut Window,
            cx: &mut App,
        ) -> AnyView {
            slot.get_or_insert_with(|| cx.new(|cx| make(window, cx)))
                .clone()
                .into()
        }
        match page {
            Page::Dashboard => self.dashboard.clone().into(),
            Page::Grid => get(&mut self.grid, GridPage::new, window, cx),
            Page::Docs => get(&mut self.docs, |_, cx| DocsPage::new(cx), window, cx),
            Page::Chat => get(&mut self.chat, ChatPage::new, window, cx),
            Page::Form => get(&mut self.form, FormPage::new, window, cx),
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.current(cx);
        let on_dash = page == Page::Dashboard;
        if self.dashboard.read(cx).active != on_dash {
            self.dashboard.update(cx, |d, _| d.active = on_dash);
        }
        let view = self.page_view(page, window, cx);
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let active = t.variant(None, Variant::Light);
        let dark = t.scheme == Scheme::Dark;
        let title = PAGES
            .iter()
            .find(|(p, _)| *p == page)
            .map_or("", |(_, l)| *l);

        let nav = PAGES.iter().map(|(p, label)| {
            let (p, router) = (*p, self.router.clone());
            div()
                .id(*label)
                .px(px(12.))
                .h(px(34.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(Size::Sm.font_size()))
                .when(p == page, |el| {
                    el.bg(active.background.to_hsla())
                        .text_color(active.foreground.to_hsla())
                })
                .when(p != page, |el| el.hover(|st| st.bg(s.hover.to_hsla())))
                .child(*label)
                .on_click(move |_, _, cx| {
                    router.navigate(p, cx);
                })
        });
        let palette = self.palette.clone();

        div()
            .id("shell")
            .key_context("CompareShell")
            .on_action(move |_: &OpenPalette, window, cx| {
                let _ = Spotlight::open(&palette, window, cx);
            })
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
                    .gap(px(16.))
                    .px(px(16.))
                    .border_b_1()
                    .border_color(s.border.to_hsla())
                    .child(Title::new("Compare: GUIraffe").order(4))
                    .child(Text::new(title).dimmed(true).size(Size::Sm))
                    .child(div().flex_1())
                    .child(Text::new("Ctrl+K: commands").dimmed(true).size(Size::Xs))
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
                        .label("Toggle color scheme")
                        .on_click(|_, _, cx| toggle_theme(cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(200.))
                            .flex_none()
                            .p(px(8.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .border_r_1()
                            .border_color(s.border.to_hsla())
                            .bg(s.surface.to_hsla())
                            .children(nav),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .p(px(20.))
                            .flex()
                            .flex_col()
                            .child(view),
                    ),
            )
    }
}
