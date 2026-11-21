//! guiraffe-overlays: window-local toasts, confirmations, command palette, floating
//! tooltips, loading overlays and affixed elements.

use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Window, canvas,
    component::button::Button, div, px,
};
use guiraffe_components::{Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, Variant};
use guiraffe_overlays::{
    Affix, Drawer, FloatingTooltip, LoadingOverlay, OverlayHost, Spotlight, SpotlightAction,
    SpotlightEvent, SpotlightState,
    position::{Align as FloatAlign, FloatingOptions, Rect, Side},
    stack::Policy,
    toast::{Toast, ToastId, ToastPosition},
};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{Applied, Knob, KnobValues, Preview, SIZE_NAMES, playground_with};

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
            summary: "Keyboard-first command bar: grouped results with icons, descriptions and shortcut chips; pinned and recent items; fuzzy search. ↑↓ move, Tab jumps between groups, Enter runs, Esc closes.",
            covers: &["Spotlight", "SpotlightState"],
            demo: spotlight,
        },
        Entry {
            krate: "overlays",
            name: "Spotlight search",
            summary: "The command bar while searching: fuzzy matches highlighted in each title, groups ordered by their best match, virtualised so 10k commands stay instant.",
            covers: &["Spotlight"],
            demo: spotlight_search,
        },
        Entry {
            krate: "overlays",
            name: "Spotlight states",
            summary: "No results keeps the query visible and suggests a next step; an empty catalog explains itself instead of showing a blank list.",
            covers: &["SpotlightState"],
            demo: spotlight_states,
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

/// (id, title, description, group, icon, shortcut, keywords)
type ActionRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
    IconName,
    &'static str,
    &'static str,
);

/// A small app's command set: groups, icons, descriptions and shortcut chips.
fn spotlight_actions(groups: bool, shortcuts: bool) -> Vec<SpotlightAction> {
    let rows: [ActionRow; 10] = [
        (
            "file.new",
            "New file",
            None,
            "File",
            IconName::FilePlus,
            "Ctrl+N",
            "create add",
        ),
        (
            "file.open",
            "Open recent project",
            Some("~/work/giraffe-garden"),
            "File",
            IconName::FolderOpen,
            "Ctrl+R",
            "history",
        ),
        (
            "file.save",
            "Save all",
            None,
            "File",
            IconName::Save,
            "Ctrl+Alt+S",
            "write",
        ),
        (
            "git.commit",
            "Git: Commit staged changes",
            Some("3 files staged on main"),
            "Git",
            IconName::GitCommitHorizontal,
            "Ctrl+Enter",
            "vcs",
        ),
        (
            "git.push",
            "Git: Push",
            Some("Upload commits to origin"),
            "Git",
            IconName::Upload,
            "",
            "vcs remote",
        ),
        (
            "git.branch",
            "Git: Create branch…",
            None,
            "Git",
            IconName::GitBranch,
            "",
            "vcs checkout",
        ),
        (
            "view.theme",
            "Toggle dark mode",
            None,
            "View",
            IconName::SunMoon,
            "Ctrl+K Ctrl+T",
            "scheme light",
        ),
        (
            "view.sidebar",
            "Toggle sidebar",
            None,
            "View",
            IconName::PanelLeft,
            "Ctrl+B",
            "panel",
        ),
        (
            "view.terminal",
            "Show terminal",
            Some("Disabled in this demo"),
            "View",
            IconName::SquareTerminal,
            "Ctrl+`",
            "shell console",
        ),
        (
            "app.settings",
            "Open settings",
            None,
            "Preferences",
            IconName::Settings,
            "Ctrl+,",
            "preferences config",
        ),
    ];
    rows.into_iter()
        .map(
            |(id, title, description, group, icon, shortcut, keywords)| {
                let mut action = SpotlightAction::new(id, title)
                    .icon(icon)
                    .keywords(keywords)
                    .disabled(id == "view.terminal");
                if let Some(d) = description {
                    action = action.description(d);
                }
                if groups {
                    action = action.group(group);
                }
                if shortcuts {
                    action = action.shortcut(shortcut);
                }
                action
            },
        )
        .collect()
}

/// The panel as it appears over a window: the theme scrim behind, anchored near the top.
fn spotlight_stage(panel: impl IntoElement, cx: &App) -> AnyElement {
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    div()
        .flex()
        .justify_center()
        .py(px(Size::Xl.spacing()))
        .px(px(Size::Md.spacing()))
        .rounded(px(Size::Sm.radius()))
        .bg(theme.scrim().over(surfaces.body).to_hsla())
        .child(panel)
        .into_any_element()
}

const SPOTLIGHT_STATES: &[&str] = &["browse", "search", "no results", "empty", "loading"];

struct SpotlightDemo {
    inline: std::cell::RefCell<Option<Entity<SpotlightState>>>,
    applied: Applied,
    log: Entity<EventLog>,
}

fn spotlight(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live Spotlight (type, ↑↓, Tab, Enter)",
        &[
            Knob::toggle("Groups").default(1),
            Knob::toggle("Recents & pinned").default(1),
            Knob::toggle("Shortcuts").default(1),
            Knob::new("State", SPOTLIGHT_STATES).default(0),
        ],
        |_, cx| SpotlightDemo {
            inline: Default::default(),
            applied: Applied::default(),
            log: EventLog::new(cx),
        },
        spotlight_preview,
        window,
        cx,
    )
}

fn build_spotlight(
    v: &KnobValues,
    log: &Entity<EventLog>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SpotlightState> {
    let (groups, recents, shortcuts) = (v.on(0), v.on(1), v.on(2));
    let state = v.index(3);
    let actions = if state == 3 {
        Vec::new()
    } else {
        spotlight_actions(groups, shortcuts)
    };
    let spotlight = cx.new(|cx| {
        let mut s = SpotlightState::new(window, cx)
            .actions(actions)
            .max_list_height(px(Size::Lg.control_height() * 5.))
            .loading(state == 4)
            .on_activate(move |id, window, cx| {
                let _ = OverlayHost::notify(
                    window,
                    cx,
                    Toast::new(ToastId(20_000), format!("Ran {id}")),
                );
            });
        if recents {
            s = s
                .pinned(["git.commit"])
                .recents(["view.theme", "file.open"]);
        }
        match state {
            1 => s.set_query("gc", window, cx),
            2 => s.set_query("xylophone", window, cx),
            4 => s.set_actions(Vec::new(), cx),
            _ => {}
        }
        s
    });
    EventLog::watch::<SpotlightEvent, _>(log, &spotlight, cx);
    spotlight
}

fn spotlight_preview(
    s: &SpotlightDemo,
    v: &KnobValues,
    window: &mut Window,
    cx: &mut Context<Preview<SpotlightDemo>>,
) -> AnyElement {
    if s.applied.changed(v) || s.inline.borrow().is_none() {
        let built = build_spotlight(v, &s.log, window, cx);
        *s.inline.borrow_mut() = Some(built);
    }
    let Some(inline) = s.inline.borrow().clone() else {
        return div().into_any_element();
    };
    let values = v.clone();
    let log = s.log.clone();
    column([
        div()
            .flex()
            .items_center()
            .gap(px(Size::Sm.spacing()))
            .child(
                Button::new("open-spotlight")
                    .label("Open as overlay")
                    .on_click(move |_, window, cx| {
                        let palette = build_spotlight(&values, &log, window, cx);
                        let _ = SpotlightState::open(&palette, window, cx);
                    }),
            )
            .child(
                Text::new("Escape or a click outside closes it and returns focus here.")
                    .size(Size::Sm)
                    .dimmed(true),
            )
            .into_any_element(),
        spotlight_stage(Spotlight::new(&inline), cx),
        div().h(px(120.)).child(s.log.clone()).into_any_element(),
    ])
}

/// A fixed open state with highlighted fuzzy matches, for the visual review.
struct SpotlightSearchDemo {
    state: Entity<SpotlightState>,
}

fn spotlight_search(window: &mut Window, cx: &mut App) -> AnyView {
    let state = cx.new(|cx| {
        let mut s = SpotlightState::new(window, cx)
            .actions(spotlight_actions(true, true))
            .max_list_height(px(Size::Lg.control_height() * 7.));
        s.set_query("tog", window, cx);
        s
    });
    cx.new(|_| SpotlightSearchDemo { state }).into()
}

struct SpotlightStatesDemo {
    no_results: Entity<SpotlightState>,
    empty: Entity<SpotlightState>,
}

fn spotlight_states(window: &mut Window, cx: &mut App) -> AnyView {
    let width = px(Size::Xs.container_width() * 0.65);
    let no_results = cx.new(|cx| {
        let mut s = SpotlightState::new(window, cx)
            .actions(spotlight_actions(true, true))
            .width(width);
        s.set_query("xylophone", window, cx);
        s
    });
    let empty = cx.new(|cx| SpotlightState::new(window, cx).width(width));
    cx.new(|_| SpotlightStatesDemo { no_results, empty }).into()
}

impl Render for SpotlightStatesDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap(px(Size::Md.spacing()))
            .child(div().flex_1().min_w_0().child(example(
                "No results",
                spotlight_stage(Spotlight::new(&self.no_results), cx),
            )))
            .child(div().flex_1().min_w_0().child(example(
                "Nothing registered yet",
                spotlight_stage(Spotlight::new(&self.empty), cx),
            )))
    }
}

impl Render for SpotlightSearchDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        column([example(
            "Query “tog”: matched letters are highlighted, groups ranked by their best match",
            spotlight_stage(Spotlight::new(&self.state), cx),
        )])
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
    playground_with(
        "Live FloatingTooltip (hover a trigger)",
        &[
            Knob::new("Align", &["start", "center", "end"]).default(1),
            Knob::new("Gap", &["0", "4", "12"]).default(1),
            Knob::toggle("Flip").default(1),
            Knob::toggle("Shift").default(1),
            Knob::toggle("Always open").default(1),
        ],
        |_, cx| (cx.new(|_| FloatingDemo::new()), Applied::default()),
        |(demo, applied), v, _, cx| {
            if applied.changed(v) {
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
            }
            demo.clone().into_any_element()
        },
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
    playground_with(
        "Live LoadingOverlay",
        &[Knob::size(), Knob::color(), Knob::variant()],
        |_, cx| {
            (
                cx.new(|_| LoadingDemo {
                    loading: true,
                    size: Size::Md,
                    color: ColorName::Blue,
                    variant: Variant::Filled,
                }),
                Applied::default(),
            )
        },
        |(demo, applied), v, _, cx| {
            if applied.changed(v) {
                let (size, color, variant) = (
                    v.size(0),
                    v.color(1).unwrap_or(ColorName::Blue),
                    v.variant(2),
                );
                demo.update(cx, |d, cx| {
                    (d.size, d.color, d.variant) = (size, color, variant);
                    cx.notify();
                });
            }
            demo.clone().into_any_element()
        },
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
