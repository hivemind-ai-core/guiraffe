//! Showcase shell: header (title, search, theme toggle), catalog sidebar grouped by crate,
//! and the selected entry's live demo. Demo views are created once and cached, so demo
//! state survives navigation and a demo re-renders independently of the shell.
//!
//! Keyboard (API conventions §12): Tab / Shift+Tab cycle the search, the theme toggle, the catalog
//! list (↑/↓ move the selection) and the current page's controls (the page is a focus scope).
//! Pointer clicks on the list or page controls never take focus, and switching pages
//! releases any focus left inside the old page.

use std::collections::HashMap;

use gpui_kit::{
    AnyView, App, Context, Entity, FocusHandle, IntoElement, KeyBinding, MouseButton,
    ParentElement, Render, Role, SharedString, TestSupportExt, Window, actions,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::*,
    px,
};
use guiraffe_components::{ActionIcon, Text, Title};
use guiraffe_core::{
    ColorName, GuiraffeTheme, Scheme, Size, StateKind, Variant, focus_ring, focus_scope,
    release_focus_within, ring_visible, set_scheme,
};

use crate::catalog::{Entry, entries, matches};

actions!(guiraffe_showcase, [NavPrev, NavNext]);

const NAV_CONTEXT: &str = "GrShowcaseNav";

/// Key bindings of the catalog list (called from [`crate::init`]).
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", NavPrev, Some(NAV_CONTEXT)),
        KeyBinding::new("down", NavNext, Some(NAV_CONTEXT)),
    ]);
}

pub struct Showcase {
    entries: Vec<Entry>,
    selected: usize,
    static_header: bool,
    search: Entity<InputState>,
    query: SharedString,
    demos: HashMap<usize, AnyView>,
    /// The catalog list: one tab stop, ↑/↓ move the selection.
    nav_focus: FocusHandle,
    /// The page container: focus left inside a page is parked here on navigation.
    page_focus: FocusHandle,
}

impl Showcase {
    pub(crate) fn with_static_header(mut self) -> Self {
        self.static_header = true;
        self
    }

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
            static_header: false,
            search,
            query: SharedString::default(),
            demos: HashMap::new(),
            nav_focus: cx.focus_handle(),
            page_focus: cx.focus_handle(),
        }
    }

    /// The page container's focus handle (focus inside the page is released to it).
    pub fn page_focus(&self) -> &FocusHandle {
        &self.page_focus
    }

    /// The header search input.
    pub fn search(&self) -> &Entity<InputState> {
        &self.search
    }

    pub fn nav_focus(&self) -> &FocusHandle {
        &self.nav_focus
    }

    /// Switch pages, first releasing any focus held by a control on the old page.
    pub fn navigate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.entries.len() && index != self.selected {
            release_focus_within(&self.page_focus, window, cx);
            self.select(index, cx);
        }
    }

    fn nav_step(&mut self, dir: i32, window: &mut Window, cx: &mut Context<Self>) {
        let visible: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches(e, &self.query))
            .map(|(i, _)| i)
            .collect();
        let pos = visible.iter().position(|i| *i == self.selected);
        let next = match (pos, dir < 0) {
            (Some(p), true) => p.checked_sub(1),
            (Some(p), false) => Some(p + 1),
            (None, _) => Some(0),
        };
        if let Some(&i) = next.and_then(|p| visible.get(p)) {
            self.navigate(i, window, cx);
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
        let quiet = t.press_colors(Variant::Subtle, Some(ColorName::Gray));
        let ring = t.state(StateKind::FocusRing, Variant::Light, None).border;
        let nav_ring = ring_visible(&self.nav_focus, window);
        let radius = px(t.default_radius.radius());
        let dark = t.scheme == Scheme::Dark;
        let entry = self.entries.get(self.selected).copied();
        let total = self.entries.len();

        // Sidebar: entries matching the query, grouped under their crate.
        let mut nav = div()
            .id("catalog")
            .test_support()
            .role(Role::ListBox)
            .aria_label("Components")
            .flex()
            .flex_col()
            .gap(px(1.));
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
                        .child(SharedString::from(if e.krate == "kit-charts" {
                            "CHARTS".to_owned()
                        } else {
                            e.krate.to_uppercase()
                        })),
                );
            }
            let active = i == self.selected;
            let (hover, pressed) = (quiet.hover.background, quiet.pressed.background);
            nav = nav.child(
                div()
                    .id(("entry", i))
                    .test_support()
                    .role(Role::ListBoxOption)
                    .aria_label(e.name)
                    .aria_selected(active)
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
                    .when(!active, |el| {
                        el.hover(move |st| st.bg(hover.to_hsla()))
                            .active(move |st| st.bg(pressed.to_hsla()))
                    })
                    .child(e.name)
                    // A click navigates but never takes keyboard focus (§12.3).
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(cx.listener(move |this, _, window, cx| this.navigate(i, window, cx))),
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
                        Text::new(if self.static_header {
                            "showcase".to_owned()
                        } else {
                            format!("showcase · {total} entries")
                        })
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
                        .label("Toggle color scheme")
                        .focusable(true)
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
                    .child(focus_ring(
                        div().flex().flex_none().child(
                            div()
                                .id("nav")
                                .track_focus(&self.nav_focus.clone().tab_stop(true))
                                .key_context(NAV_CONTEXT)
                                .on_action(cx.listener(|this, _: &NavPrev, window, cx| {
                                    this.nav_step(-1, window, cx)
                                }))
                                .on_action(cx.listener(|this, _: &NavNext, window, cx| {
                                    this.nav_step(1, window, cx)
                                }))
                                .w(px(220.))
                                .flex_none()
                                .p(px(Size::Xs.spacing()))
                                .overflow_y_scroll()
                                .border_r_1()
                                .border_color(s.border.to_hsla())
                                .bg(s.surface.to_hsla())
                                .child(nav),
                        ),
                        // On a wrapper, outside the list's scroll clip.
                        nav_ring,
                        px(0.),
                        ring,
                    ))
                    .child(
                        div()
                            .id("demo")
                            .track_focus(&self.page_focus)
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
                                            "recipes" => "DataUi screen document".to_string(),
                                            "kit-charts" => {
                                                "gpui-kit · component::chart".to_string()
                                            }
                                            "charts" => "guiraffe-charts".to_string(),
                                            k => {
                                                format!("guiraffe-{} · {}", k, e.covers.join(", "))
                                            }
                                        })
                                        .size(Size::Xs)
                                        .dimmed(true),
                                    )
                            })
                            // The page is a focus scope: its controls join Tab order.
                            .children(demo.map(focus_scope)),
                    ),
            )
    }
}
