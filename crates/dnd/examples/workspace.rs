//! Run with `cargo run -p guiraffe-dnd --example workspace -- [ide]`.
use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window, WindowOptions,
    component::{
        Root,
        dock::{BasePanel, DockLayout},
    },
    div, px,
};
use guiraffe_dnd::{
    Card, SortableBoard,
    dock::{self, DockEvent, Panel, PanelEvent},
    sortable::SortEvent,
};
use guiraffe_layout::LayoutStore;

struct Workspace {
    content: AnyView,
    log: Vec<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    fn record(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.log.push(message.into());
        if self.log.len() > 4 {
            self.log.remove(0);
        }
        cx.notify();
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(div().flex_1().min_h_0().child(self.content.clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child("Event log")
                    .children(self.log.iter().cloned()),
            )
    }
}

struct IdePanel {
    name: &'static str,
    focus: FocusHandle,
}

impl EventEmitter<PanelEvent> for IdePanel {}

impl BasePanel for IdePanel {
    fn panel_name(&self) -> &'static str {
        self.name
    }
}

impl Panel for IdePanel {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(self.name.into())
    }
}

impl Focusable for IdePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for IdePanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let body = match self.name {
            "Files" => "src/\n  main.rs\n  lib.rs\nCargo.toml",
            "main.rs" => "fn main() {\n    println!(\"Hello, GUIraffe!\");\n}",
            "lib.rs" => "pub mod workspace;",
            _ => "$ cargo check\nFinished dev profile",
        };
        div().size_full().p_4().font_family("monospace").child(body)
    }
}

fn ide_panel(name: &'static str, cx: &mut App) -> Entity<IdePanel> {
    cx.new(|cx| IdePanel {
        name,
        focus: cx.focus_handle(),
    })
}

fn ide(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    const KEY: &str = "guiraffe.examples.ide";
    const VERSION: usize = 1;
    for name in ["Files", "main.rs", "lib.rs", "Terminal"] {
        dock::register_panel(cx, name, move |_, _, cx| {
            dock::panel_handle(ide_panel(name, cx))
        });
    }
    let (area, skin) = dock::new_dock("ide", VERSION, window, cx);
    skin.set_toggle_button_visible(true, cx);
    let saved = cx
        .try_global::<LayoutStore>()
        .and_then(|store| dock::load(store, KEY, VERSION).ok().flatten());
    let restored =
        saved.is_some_and(|saved| area.update(cx, |area, cx| area.load(saved, window, cx).is_ok()));
    if !restored {
        let tabs = |names: &[&'static str], cx: &mut App| {
            names.iter().fold(DockLayout::tabs(), |tabs, name| {
                tabs.panel_view(dock::panel_handle(ide_panel(name, cx)), cx)
            })
        };
        area.update(cx, |area, cx| {
            area.set_center(
                DockLayout::h_split()
                    .child(tabs(&["Files"], cx), Some(px(180.)))
                    .child(
                        DockLayout::v_split()
                            .child(tabs(&["main.rs", "lib.rs"], cx), None)
                            .child(tabs(&["Terminal"], cx), Some(px(150.))),
                        None,
                    ),
                window,
                cx,
            );
        });
    }
    let persistence = dock::persist_changes(&area, KEY.into(), |error, _| eprintln!("{error}"), cx);
    let events = cx.subscribe(&area, |this, _, event: &DockEvent, cx| {
        if matches!(event, DockEvent::LayoutChanged) {
            this.record("Dock layout changed", cx);
        }
    });
    Workspace {
        content: area.into(),
        log: vec![
            if restored {
                "Layout restored"
            } else {
                "Default layout loaded"
            }
            .into(),
        ],
        _subscriptions: vec![persistence, events],
    }
}

fn kanban(cx: &mut Context<Workspace>) -> Workspace {
    let board = cx.new(|cx| {
        SortableBoard::new(
            vec![
                (
                    "Backlog".into(),
                    vec![
                        Card {
                            id: 0,
                            label: "Review keyboard navigation".into(),
                        },
                        Card {
                            id: 1,
                            label: "Draft release notes".into(),
                        },
                    ],
                ),
                (
                    "In progress".into(),
                    vec![Card {
                        id: 2,
                        label: "Verify drag and drop".into(),
                    }],
                ),
                (
                    "Done".into(),
                    vec![Card {
                        id: 3,
                        label: "Create demo".into(),
                    }],
                ),
            ],
            cx,
        )
    });
    let events = cx.subscribe(&board, |this, _, event: &SortEvent, cx| {
        this.record(
            format!(
                "Moved {} card(s) from column {} to column {}",
                event.reorder.original_indices.len(),
                event.source + 1,
                event.destination + 1
            ),
            cx,
        );
    });
    Workspace {
        content: board.into(),
        log: vec!["Shift-click to select multiple cards".into()],
        _subscriptions: vec![events],
    }
}

fn main() {
    let use_ide = std::env::args().nth(1).as_deref() == Some("ide");
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            guiraffe_layout::init(cx);
            if let Err(error) = cx.open_window(WindowOptions::default(), move |window, cx| {
                let view = cx.new(|cx| if use_ide { ide(window, cx) } else { kanban(cx) });
                cx.new(|cx| Root::new(view, window, cx))
            }) {
                eprintln!("{error}");
                cx.quit();
            }
        });
}
