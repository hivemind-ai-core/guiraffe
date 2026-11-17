//! Showcase shell: header (title, search, theme toggle), catalog sidebar grouped by crate,
//! and the selected entry's live demo. Demo views are created once and cached, so demo
//! state survives navigation and a demo re-renders independently of the shell.

use std::collections::HashMap;

use gpui_kit::{
    AnyView, App, Context, Entity, IntoElement, ParentElement, Render, SharedString, Window,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px,
};
use guiraffe_components::{ActionIcon, Text, Title};
use guiraffe_core::{GuiraffeTheme, Scheme, Size, Variant, set_scheme};

use crate::catalog::{Entry, entries, matches};

pub struct Showcase {
    entries: Vec<Entry>,
    selected: usize,
    search: Entity<InputState>,
    query: SharedString,
    demos: HashMap<usize, AnyView>,
}

impl Showcase {
    /// `start`: optional entry name (case-insensitive) to open first.
    pub fn new(start: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let entries = entries();
        let selected = start
            .and_then(|s| entries.iter().position(|e| e.name.eq_ignore_ascii_case(s)))
            .unwrap_or(0);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search components…"));
        cx.subscribe_in(&search, window, |this, input, ev, _, cx| {
            if let InputEvent::Change = ev {
                this.query = input.read(cx).value();
                cx.notify();
            }
        })
        .detach();
        Self {
            entries,
            selected,
            search,
            query: SharedString::default(),
            demos: HashMap::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.entries.len() && index != self.selected {
            self.selected = index;
            cx.notify();
        }
    }

    /// The (cached) demo view for entry `index`.
    pub fn demo(&mut self, index: usize, window: &mut Window, cx: &mut App) -> Option<AnyView> {
        let e = *self.entries.get(index)?;
        Some(
            self.demos
                .entry(index)
                .or_insert_with(|| (e.demo)(window, cx))
                .clone(),
        )
    }
}

impl Render for Showcase {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let demo = self.demo(self.selected, window, cx);
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let active_v = t.variant(None, Variant::Light);
        let radius = px(t.default_radius.radius());
        let dark = t.scheme == Scheme::Dark;
        let entry = self.entries.get(self.selected).copied();
        let total = self.entries.len();

        // Sidebar: entries matching the query, grouped under their crate.
        let mut nav = div().flex().flex_col().gap(px(1.));
        let mut last_crate = "";
        let mut shown = 0;
        for (i, e) in self.entries.iter().enumerate() {
            if !matches(e, &self.query) {
                continue;
            }
            shown += 1;
            if e.krate != last_crate {
                last_crate = e.krate;
                nav = nav.child(
                    div()
                        .pt(px(Size::Sm.spacing()))
                        .pb(px(2.))
                        .px(px(Size::Xs.spacing()))
                        .text_size(px(Size::Xs.font_size()))
                        .text_color(s.dimmed.to_hsla())
                        .child(SharedString::from(e.krate.to_uppercase())),
                );
            }
            let active = i == self.selected;
            nav = nav.child(
                div()
                    .id(("entry", i))
                    .px(px(Size::Sm.spacing()))
                    .h(px(Size::Xs.control_height()))
                    .flex()
                    .items_center()
                    .rounded(radius)
                    .cursor_pointer()
                    .text_size(px(Size::Sm.font_size()))
                    .when(active, |el| {
                        el.bg(active_v.background.to_hsla())
                            .text_color(active_v.foreground.to_hsla())
                    })
                    .when(!active, |el| el.hover(|st| st.bg(s.hover.to_hsla())))
                    .child(e.name)
                    .on_click(cx.listener(move |this, _, _, cx| this.select(i, cx))),
            );
        }
        if shown == 0 {
            nav = nav.child(
                div()
                    .p(px(Size::Sm.spacing()))
                    .child(Text::new("No matches").dimmed(true).size(Size::Sm)),
            );
        }

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
                    .gap(px(Size::Md.spacing()))
                    .px(px(Size::Md.spacing()))
                    .border_b_1()
                    .border_color(s.border.to_hsla())
                    .child(Title::new("GUIraffe 🦒").order(4))
                    .child(
                        Text::new(format!("showcase · {total} entries"))
                            .dimmed(true)
                            .size(Size::Sm),
                    )
                    .child(div().flex_1())
                    .child(div().w(px(260.)).child(Input::new(&self.search)))
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
                            .id("nav")
                            .w(px(220.))
                            .flex_none()
                            .p(px(Size::Xs.spacing()))
                            .overflow_y_scroll()
                            .border_r_1()
                            .border_color(s.border.to_hsla())
                            .bg(s.surface.to_hsla())
                            .child(nav),
                    )
                    .child(
                        div()
                            .id("demo")
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(Size::Xl.spacing()))
                            .flex()
                            .flex_col()
                            .gap(px(Size::Md.spacing()))
                            .when_some(entry, |el, e| {
                                el.child(Title::new(e.name).order(2))
                                    .child(Text::new(e.summary).dimmed(true))
                                    .child(
                                        Text::new(match e.krate {
                                            "kit" => "gpui-kit · GPUI Controls, themed by GUIraffe"
                                                .to_string(),
                                            "charts" => "gpui-kit · component::chart".to_string(),
                                            k => {
                                                format!("guiraffe-{} · {}", k, e.covers.join(", "))
                                            }
                                        })
                                        .size(Size::Xs)
                                        .dimmed(true),
                                    )
                            })
                            .children(demo),
                    ),
            )
    }
}
