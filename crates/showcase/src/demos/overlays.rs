//! guiraffe-overlays: window-local toasts, confirmations, command palette, floating
//! tooltips, loading overlays and affixed elements.

use std::{cell::Cell, rc::Rc};

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyView, App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    canvas, component::button::Button, div, px,
};
use guiraffe_components::{Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, Variant};
use guiraffe_overlays::{
    Affix, FloatingTooltip, LoadingOverlay, OverlayHost, Spotlight,
    position::{FloatingOptions, Rect, Side},
    search::Command,
    toast::{Toast, ToastId, ToastPosition},
};

use super::{column, example};
use crate::catalog::Entry;

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "overlays",
            name: "Toasts & confirm",
            summary: "OverlayHost owns a window's layers: queued toasts and stacked, awaitable confirmations.",
            covers: &["OverlayHost"],
            demo: host,
        },
        Entry {
            krate: "overlays",
            name: "Spotlight",
            summary: "Command palette with fuzzy search; arrows move, Enter runs. Shown inline here and as a modal layer.",
            covers: &["Spotlight"],
            demo: spotlight,
        },
        Entry {
            krate: "overlays",
            name: "FloatingTooltip",
            summary: "Controlled tooltip positioned by the measured floating solver: flips and shifts to stay on screen.",
            covers: &["FloatingTooltip"],
            demo: floating,
        },
        Entry {
            krate: "overlays",
            name: "LoadingOverlay",
            summary: "Covers a relative container, blocks pointer and keyboard input while visible.",
            covers: &["LoadingOverlay"],
            demo: loading,
        },
        Entry {
            krate: "overlays",
            name: "Affix",
            summary: "Pins children to a corner or edge of the containing block.",
            covers: &["Affix"],
            demo: affix,
        },
    ]
}

// ------------------------------------------------------------------ host

struct HostDemo {
    toasts: u64,
    last: SharedString,
}

fn host(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| HostDemo {
        toasts: 0,
        last: "No confirmation yet".into(),
    })
    .into()
}

impl Render for HostDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toast = Button::new("toast")
            .label("Save draft")
            .on_click(cx.listener(|this, _, window, cx| {
                this.toasts += 1;
                let toast = Toast::new(
                    ToastId(this.toasts),
                    format!("Draft #{} saved", this.toasts),
                );
                let _ = OverlayHost::notify(window, cx, toast);
            }));
        let confirm = Button::new("confirm")
            .label("Delete project…")
            .on_click(cx.listener(|_, _, window, cx| {
                let Ok((_, result)) = OverlayHost::confirm(
                    window,
                    cx,
                    "Delete “aurora-web”?",
                    "All deployments and logs will be removed. This cannot be undone.",
                ) else {
                    return;
                };
                cx.spawn_in(window, async move |this, cx| {
                    let ok = result.await;
                    this.update(cx, |this, cx| {
                        this.last = if ok {
                            "Confirmed: project deleted"
                        } else {
                            "Cancelled: project kept"
                        }
                        .into();
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }));
        column([
            example(
                "Toasts stack in the window corner",
                div().flex().gap(px(Size::Sm.spacing())).child(toast),
            ),
            example(
                "Confirmation resolves a future",
                div()
                    .flex()
                    .items_center()
                    .gap(px(Size::Md.spacing()))
                    .child(confirm)
                    .child(Text::new(self.last.clone()).size(Size::Sm).dimmed(true)),
            ),
        ])
    }
}

// ------------------------------------------------------------------ spotlight

fn commands() -> Vec<Command> {
    [
        ("new-file", "New file", "create add"),
        ("open-recent", "Open recent project", "history"),
        ("toggle-theme", "Toggle dark mode", "scheme light"),
        ("git-commit", "Git: Commit staged changes", "vcs"),
        ("git-push", "Git: Push", "vcs remote"),
        ("format", "Format document", "prettier rustfmt"),
        ("settings", "Open settings", "preferences"),
    ]
    .into_iter()
    .map(|(id, label, kw)| Command {
        keywords: kw.into(),
        ..Command::new(id, label)
    })
    .collect()
}

struct SpotlightDemo {
    inline: Entity<Spotlight>,
    modal: Entity<Spotlight>,
}

fn spotlight(window: &mut Window, cx: &mut App) -> AnyView {
    let inline = cx.new(|cx| Spotlight::new(commands(), window, cx, |_, _, _| {}));
    let modal = cx.new(|cx| {
        Spotlight::new(commands(), window, cx, |id, window, cx| {
            let _ =
                OverlayHost::notify(window, cx, Toast::new(ToastId(20_000), format!("Ran {id}")));
        })
    });
    cx.new(|_| SpotlightDemo { inline, modal }).into()
}

impl Render for SpotlightDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let modal = self.modal.clone();
        column([
            example(
                "Open as a modal layer",
                Button::new("open-spotlight")
                    .label("Open command palette")
                    .on_click(move |_, window, cx| {
                        let _ = Spotlight::open(&modal, window, cx);
                    }),
            ),
            example(
                "Inline preview",
                div().w(px(420.)).child(self.inline.clone()),
            ),
        ])
    }
}

// ------------------------------------------------------------------ floating tooltip

/// Anchors are window coordinates, so each trigger records its painted bounds and the
/// tooltip follows on the next frame.
struct FloatingDemo {
    anchors: [Rc<Cell<Option<Bounds<Pixels>>>>; 3],
    hovered: [bool; 3],
}

fn floating(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| FloatingDemo {
        anchors: Default::default(),
        hovered: [false; 3],
    })
    .into()
}

fn rect(b: Bounds<Pixels>) -> Rect {
    Rect {
        x: b.origin.x.into(),
        y: b.origin.y.into(),
        width: b.size.width.into(),
        height: b.size.height.into(),
    }
}

impl Render for FloatingDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let specs = [
            (Side::Top, "Top", "Deploys the current branch"),
            (Side::Right, "Right", "⌘K opens the palette"),
            (Side::Bottom, "Bottom", "Last synced 2 minutes ago"),
        ];
        let mut row = div().flex().gap(px(180.)).py(px(64.)).px(px(120.));
        let mut tips = Vec::new();
        for (i, ((side, label, tip), cell)) in specs.into_iter().zip(&self.anchors).enumerate() {
            let record = cell.clone();
            row = row.child(
                div()
                    .id(("tooltip-trigger", i))
                    .test_support()
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if this.hovered[i] != *hovered {
                            this.hovered[i] = *hovered;
                            cx.notify();
                        }
                    }))
                    .relative()
                    .px(px(Size::Md.spacing()))
                    .py(px(Size::Xs.spacing()))
                    .rounded(px(Size::Sm.radius()))
                    .border_1()
                    .border_color(s.border.to_hsla())
                    .bg(s.surface.to_hsla())
                    .text_color(s.text.to_hsla())
                    .text_size(px(Size::Sm.font_size()))
                    .child(label)
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                if record.get() != Some(bounds) {
                                    record.set(Some(bounds));
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            );
            if self.hovered[i]
                && let Some(b) = cell.get()
            {
                tips.push(
                    FloatingTooltip::new(("showcase-tip", i), tip, rect(b)).options(
                        FloatingOptions {
                            side,
                            gap: Size::Sm.radius(),
                            ..Default::default()
                        },
                    ),
                );
            }
        }
        column([example(
            "Controlled tooltips on three sides",
            div().relative().child(row).children(tips),
        )])
    }
}

// ------------------------------------------------------------------ loading overlay

struct LoadingDemo {
    loading: bool,
}

fn loading(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| LoadingDemo { loading: true }).into()
}

impl Render for LoadingDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card = div()
            .relative()
            .w(px(360.))
            .flex()
            .flex_col()
            .gap(px(Size::Xs.spacing()))
            .p(px(Size::Md.spacing()))
            .child(Title::new("Monthly report").order(4))
            .child(Text::new("Revenue $48,210 · up 12% on October").size(Size::Sm))
            .child(
                Text::new("Churn 1.8% · 312 new accounts")
                    .size(Size::Sm)
                    .dimmed(true),
            )
            .child(
                LoadingOverlay::new()
                    .visible(self.loading)
                    .color(ColorName::Blue),
            );
        column([example(
            "Refreshing a card",
            div()
                .flex()
                .flex_col()
                .gap(px(Size::Md.spacing()))
                .child(card)
                .child(
                    Button::new("toggle-loading")
                        .label(if self.loading {
                            "Finish loading"
                        } else {
                            "Reload"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.loading = !this.loading;
                            cx.notify();
                        })),
                ),
        )])
    }
}

// ------------------------------------------------------------------ affix

fn affix(_: &mut Window, cx: &mut App) -> AnyView {
    crate::catalog::stateless(
        |_, cx| {
            let s = GuiraffeTheme::global(cx).surfaces();
            let pin = |pos, label: &'static str, color| {
                Affix::new()
                    .position(pos)
                    .offset(Size::Sm)
                    .child(chip(label, color, cx))
            };
            column([example(
                "Pinned to corners and edges of a relative container",
                div()
                    .relative()
                    .h(px(220.))
                    .rounded(px(Size::Sm.radius()))
                    .bg(s.body.to_hsla())
                    .border_1()
                    .border_color(s.border.to_hsla())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Text::new("Scrollable content").dimmed(true))
                    .child(pin(ToastPosition::TopLeft, "Top left", ColorName::Blue))
                    .child(pin(
                        ToastPosition::TopCenter,
                        "Top center",
                        ColorName::Violet,
                    ))
                    .child(pin(ToastPosition::TopRight, "Top right", ColorName::Teal))
                    .child(pin(
                        ToastPosition::BottomLeft,
                        "Bottom left",
                        ColorName::Orange,
                    ))
                    .child(pin(
                        ToastPosition::BottomCenter,
                        "Back to top ↑",
                        ColorName::Gray,
                    ))
                    .child(pin(
                        ToastPosition::BottomRight,
                        "Bottom right",
                        ColorName::Pink,
                    )),
            )])
        },
        cx,
    )
}

fn chip(label: &'static str, color: ColorName, cx: &App) -> gpui_kit::Div {
    let v = GuiraffeTheme::global(cx).variant(Some(color), Variant::Filled);
    div()
        .px(px(Size::Sm.spacing()))
        .py(px(2.))
        .rounded(px(Size::Xl.radius()))
        .bg(v.background.to_hsla())
        .text_color(v.foreground.to_hsla())
        .text_size(px(Size::Xs.font_size()))
        .child(label)
}

#[cfg(test)]
mod tooltip_tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{TestAppContext, point};

    #[gpui_kit::test]
    fn showcase_tooltips_follow_hover_and_disappear_on_leave(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (view, vcx) = cx.add_window_view(|_, _| FloatingDemo {
            anchors: Default::default(),
            hovered: [false; 3],
        });
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.update(|window, _| assert!(window.try_find(("showcase-tip", 0usize)).is_none()));
        vcx.update(|window, cx| window.hover(("tooltip-trigger", 0usize), cx));
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("showcase-tip", 0usize)).is_some());
        });
        vcx.simulate_mouse_move(point(px(5.), px(5.)), None, Default::default());
        vcx.run_until_parked();
        assert!(!view.read_with(vcx, |view, _| view.hovered[0]));
        vcx.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find(("showcase-tip", 0usize)).is_none());
        });
    }
}
