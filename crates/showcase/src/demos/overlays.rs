//! guiraffe-overlays: window-local toasts, confirmations, command palette, floating
//! tooltips, loading overlays and affixed elements.

use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Window, canvas,
    component::button::Button, div, px,
};
use guiraffe_components::{Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, Variant};
use guiraffe_overlays::{
    Affix, Drawer, FloatingTooltip, LoadingOverlay, OverlayHost, Spotlight,
    position::{Align as FloatAlign, FloatingOptions, Rect, Side},
    search::Command,
    stack::Policy,
    toast::{Toast, ToastId, ToastPosition},
};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{
    Knob, KnobValues, Preview, SIZE_NAMES, playground_with, playground_with_update,
};

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "overlays",
            name: "Toasts & confirm",
            summary: "OverlayHost owns a window's layers: positioned, timed toasts; modals, awaitable confirmations and drawers with close policies.",
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

struct HostState {
    toasts: Cell<u64>,
    log: Entity<EventLog>,
}

const TOAST_POSITIONS: [ToastPosition; 6] = [
    ToastPosition::TopLeft,
    ToastPosition::TopCenter,
    ToastPosition::TopRight,
    ToastPosition::BottomLeft,
    ToastPosition::BottomCenter,
    ToastPosition::BottomRight,
];
const SIDES: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

fn host(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live overlays",
        &[
            Knob::new("Toast at", &["↖", "↑", "↗", "↙", "↓", "↘"]).default(2),
            Knob::new("Auto-close", &["2s", "5s", "sticky"]).default(1),
            Knob::new("Max visible", &["1", "3", "5"]).default(1),
            Knob::toggle("Esc closes").default(1),
            Knob::toggle("Click outside closes").default(1),
            Knob::new("Drawer side", &["top", "right", "bottom", "left"]).default(1),
            Knob::new("Drawer size", SIZE_NAMES).default(2),
        ],
        |_, cx| HostState {
            toasts: Cell::new(0),
            log: EventLog::new(cx),
        },
        host_preview,
        window,
        cx,
    )
}

fn host_preview(
    s: &HostState,
    v: &KnobValues,
    _: &mut Window,
    cx: &mut Context<Preview<HostState>>,
) -> AnyElement {
    let position = v.pick(0, &TOAST_POSITIONS).unwrap_or_default();
    let auto_close = v
        .pick(
            1,
            &[
                Some(Duration::from_secs(2)),
                Some(Duration::from_secs(5)),
                None,
            ],
        )
        .flatten();
    let limit = v.pick(2, &[1usize, 3, 5]).unwrap_or(3);
    let policy = Policy {
        escape: v.on(3),
        outside: v.on(4),
    };
    let side = v.pick(5, &SIDES).unwrap_or(Side::Right);
    let size = v.size(6);

    let toast = Button::new("toast")
        .label("Save draft")
        .on_click(cx.listener(move |p, _, window, cx| {
            let n = p.state.toasts.get() + 1;
            p.state.toasts.set(n);
            let _ = OverlayHost::toast_limit(window, cx, limit);
            let mut toast = Toast::new(ToastId(n), format!("Draft #{n} saved"));
            toast.position = position;
            toast.auto_close = auto_close;
            let _ = OverlayHost::notify(window, cx, toast);
            EventLog::log(
                &p.state.log,
                format!("notify ToastId({n}) at {position:?}"),
                cx,
            );
        }));
    let clear = Button::new("clear-toasts")
        .label("Clear toasts")
        .on_click(cx.listener(|p, _, window, cx| {
            let _ = OverlayHost::clear_toasts(window, cx);
            EventLog::log(&p.state.log, "clear_toasts", cx);
        }));
    let confirm = Button::new("confirm")
        .label("Delete project…")
        .on_click(cx.listener(|_, _, window, cx| {
            let Ok((id, result)) = OverlayHost::confirm(
                window,
                cx,
                "Delete “aurora-web”?",
                "All deployments and logs will be removed. This cannot be undone.",
            ) else {
                return;
            };
            cx.spawn_in(window, async move |this, cx| {
                let ok = result.await;
                this.update(cx, |p, cx| {
                    EventLog::log(&p.state.log, format!("confirm {id:?} resolved: {ok}"), cx);
                })
                .ok();
            })
            .detach();
        }));
    let log = s.log.clone();
    let modal = Button::new("modal")
        .label("Invite teammates…")
        .on_click(move |_, window, cx| {
            let on_close = log.clone();
            let opened = OverlayHost::open(window, cx, policy, move |d, _, _| {
                let on_close = on_close.clone();
                d.title(Title::new("Invite teammates").order(4))
                    .child(
                        Text::new("They get editor access to every project in Acme.")
                            .size(Size::Sm)
                            .dimmed(true),
                    )
                    .on_close(move |_, _, cx| EventLog::log(&on_close, "modal on_close", cx))
            });
            if let Ok(id) = opened {
                EventLog::log(
                    &log,
                    format!(
                        "open {id:?} escape={} outside={}",
                        policy.escape, policy.outside
                    ),
                    cx,
                );
            }
        });
    let log = s.log.clone();
    let drawer = Button::new("drawer")
        .label("Open drawer")
        .on_click(move |_, window, cx| {
            let opened = Drawer::new()
                .position(side)
                .size(size)
                .close_on_escape(policy.escape)
                .close_on_click_outside(policy.outside)
                .open(window, cx, |sheet, _, _| {
                    sheet.title("Filters").child(
                        Text::new("Status, owner and date range filters live here.").size(Size::Sm),
                    )
                });
            let line = match opened {
                Ok(id) => format!("Drawer {id:?} on {side:?}, size {size:?}"),
                Err(e) => format!("Drawer refused: {e:?}"),
            };
            EventLog::log(&log, line, cx);
        });
    let row = |items: Vec<AnyElement>| {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(Size::Sm.spacing()))
            .children(items)
    };
    div()
        .flex()
        .gap(px(Size::Xl.spacing()))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(Size::Md.spacing()))
                .child(
                    Text::new("Toasts: position, auto-close and limit from the knobs")
                        .size(Size::Sm)
                        .dimmed(true),
                )
                .child(row(vec![
                    toast.into_any_element(),
                    clear.into_any_element(),
                ]))
                .child(
                    Text::new("Modal, confirmation and drawer use the close policy")
                        .size(Size::Sm)
                        .dimmed(true),
                )
                .child(row(vec![
                    modal.into_any_element(),
                    confirm.into_any_element(),
                    drawer.into_any_element(),
                ])),
        )
        .child(div().flex_1().child(s.log.clone()))
        .into_any_element()
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
    anchors: [Rc<Cell<Option<Bounds<Pixels>>>>; 4],
    hovered: [bool; 4],
    /// Knob-driven solver options; `side` is per trigger.
    options: FloatingOptions,
    always: bool,
}

impl FloatingDemo {
    fn new() -> Self {
        Self {
            anchors: Default::default(),
            hovered: [false; 4],
            options: FloatingOptions {
                gap: Size::Sm.radius(),
                ..Default::default()
            },
            always: false,
        }
    }
}

fn floating(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live FloatingTooltip (hover a trigger)",
        &[
            Knob::new("Align", &["start", "center", "end"]).default(1),
            Knob::new("Gap", &["0", "4", "12"]).default(1),
            Knob::toggle("Flip").default(1),
            Knob::toggle("Shift").default(1),
            Knob::toggle("Always open").default(1),
        ],
        |_, cx| cx.new(|_| FloatingDemo::new()),
        |demo, v, cx| {
            let align = v
                .pick(0, &[FloatAlign::Start, FloatAlign::Center, FloatAlign::End])
                .unwrap_or(FloatAlign::Center);
            let gap = v.pick(1, &[0., 4., 12.]).unwrap_or(4.);
            let (flip, shift, always) = (v.on(2), v.on(3), v.on(4));
            demo.update(cx, |d, cx| {
                d.options = FloatingOptions {
                    align,
                    gap,
                    flip,
                    shift,
                    ..d.options
                };
                d.always = always;
                cx.notify();
            });
        },
        |demo, _, _, _| demo.clone().into_any_element(),
        window,
        cx,
    )
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
            (Side::Left, "Left", "Opens on the left"),
        ];
        let mut triggers = Vec::new();
        let mut tips = Vec::new();
        for (i, ((side, label, tip), cell)) in specs.into_iter().zip(&self.anchors).enumerate() {
            let record = cell.clone();
            triggers.push(
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
            if (self.always || self.hovered[i])
                && let Some(b) = cell.get()
            {
                tips.push(
                    FloatingTooltip::new(("showcase-tip", i), tip, rect(b)).options(
                        FloatingOptions {
                            side,
                            ..self.options
                        },
                    ),
                );
            }
        }
        let mut t = triggers.into_iter().map(Some).collect::<Vec<_>>();
        let mut take = |i: usize| t.get_mut(i).and_then(Option::take);
        let (top, right, bottom, left) = (take(0), take(1), take(2), take(3));
        div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(64.))
            .py(px(56.))
            .child(div().flex().justify_around().children(top).children(bottom))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px(px(8.))
                    .children(right)
                    .children(left),
            )
            .children(tips)
    }
}

// ------------------------------------------------------------------ loading overlay

struct LoadingDemo {
    loading: bool,
    size: Size,
    color: ColorName,
    variant: Variant,
}

fn loading(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live LoadingOverlay",
        &[Knob::size(), Knob::color(), Knob::variant()],
        |_, cx| {
            cx.new(|_| LoadingDemo {
                loading: true,
                size: Size::Md,
                color: ColorName::Blue,
                variant: Variant::Filled,
            })
        },
        |demo, v, cx| {
            let (size, color, variant) = (
                v.size(0),
                v.color(1).unwrap_or(ColorName::Blue),
                v.variant(2),
            );
            demo.update(cx, |d, cx| {
                (d.size, d.color, d.variant) = (size, color, variant);
                cx.notify();
            });
        },
        |demo, _, _, _| demo.clone().into_any_element(),
        window,
        cx,
    )
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
                    .size(self.size)
                    .color(self.color)
                    .variant(self.variant),
            );
        column([example(
            "Refreshing a card (visible toggles with the button)",
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

fn affix(window: &mut Window, cx: &mut App) -> AnyView {
    crate::knobs::with_knobs(
        "Live Affix",
        &[
            Knob::new("Position", &["↖", "↑", "↗", "↙", "↓", "↘"]).default(5),
            Knob::new("Offset", SIZE_NAMES).default(2),
        ],
        |v, _, cx| {
            let s = GuiraffeTheme::global(cx).surfaces();
            let pos = v.pick(0, &TOAST_POSITIONS).unwrap_or_default();
            div()
                .relative()
                .h(px(120.))
                .rounded(px(Size::Sm.radius()))
                .bg(s.body.to_hsla())
                .border_1()
                .border_color(s.border.to_hsla())
                .child(Affix::new().position(pos).offset(v.size(1)).child(chip(
                    "Back to top ↑",
                    ColorName::Blue,
                    cx,
                )))
                .into_any_element()
        },
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
                    .h(px(180.))
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
        window,
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
        let (view, vcx) = cx.add_window_view(|_, _| FloatingDemo::new());
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
