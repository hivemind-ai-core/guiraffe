//! guiraffe-commands: the command ribbon. A keyboard-first command bar over the same registry
//! as shortcuts, menus and Spotlight; Ctrl+Space or the status-bar button opens it.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, component::button::Button, div, px,
};
use guiraffe_commands::{
    Choice, Command, CommandId, CommandScope, Commands, ContextStack, Keybinding, Param, Payload,
    When, WhenContext,
    ribbon::{
        CommandRibbon, RibbonButton, RibbonCategory, RibbonEntry, RibbonEvent, RibbonLayout,
        RibbonMenu, RibbonModel, RibbonState, ribbon_scope,
    },
};
use guiraffe_core::{GuiraffeTheme, Size};
use std::{collections::BTreeMap, rc::Rc};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "commands",
        name: "Command ribbon",
        summary: "A keyboard-first command bar above the status bar. Ctrl+Space opens it; type an entry's key to descend or run, Backspace goes up, Escape goes to the root and then closes. Categories follow the page and the selection; disabled commands explain why.",
        covers: &["CommandRibbon", "RibbonButton", "RibbonState"],
        demo: ribbon,
    }]
}

/// (id, title, shortcut, enabled)
const COMMANDS: &[(&str, &str, Option<&str>, bool)] = &[
    ("file.new", "New file", Some("secondary+n"), true),
    ("file.open", "Open…", Some("secondary+o"), true),
    ("file.save", "Save", Some("secondary+s"), true),
    ("file.revert", "Revert file", None, false),
    ("edit.undo", "Undo", Some("secondary+z"), true),
    ("edit.redo", "Redo", Some("secondary+shift+z"), true),
    ("edit.find", "Find", Some("secondary+f"), true),
    ("edit.replace", "Replace", Some("secondary+h"), true),
    ("view.wrap", "Soft wrap", None, true),
    ("view.lines", "Line numbers", None, true),
    ("view.minimap", "Minimap", None, true),
    ("selection.copy", "Copy", Some("secondary+c"), true),
    ("selection.upper", "Upper case", None, true),
    ("selection.lower", "Lower case", None, true),
];

fn id(value: &str) -> Option<CommandId> {
    CommandId::parse(value).ok()
}

fn entry(entry_id: &str, key: &str, command: &str, icon: Option<&str>) -> Option<RibbonEntry> {
    let entry = RibbonEntry::command(entry_id, key, id(command)?);
    Some(match icon {
        Some(icon) => entry.icon(format!("icons/{icon}.svg")),
        None => entry,
    })
}

fn category(title: &str, entries: impl IntoIterator<Item = Option<RibbonEntry>>) -> RibbonCategory {
    entries
        .into_iter()
        .flatten()
        .fold(RibbonCategory::new(title), RibbonCategory::entry)
}

fn layout() -> RibbonLayout {
    let when = |w: &str| When::parse(w).unwrap_or_default();
    RibbonLayout::new("root")
        .menu(
            RibbonMenu::new("root", "Commands")
                .category(category(
                    "Global",
                    [
                        Some(RibbonEntry::menu("file", "f", "file").icon("icons/file.svg")),
                        Some(RibbonEntry::menu("view", "v", "view").icon("icons/eye.svg")),
                        entry("find", "/", "edit.find", Some("search")),
                    ],
                ))
                .category(
                    category(
                        "Editor",
                        [
                            entry("undo", "u", "edit.undo", Some("undo")),
                            entry("redo", "r", "edit.redo", Some("redo")),
                            entry("replace", "h", "edit.replace", Some("replace")),
                            entry("go-to", "g", "editor.go-to", None),
                        ],
                    )
                    .visible(when("Editor")),
                )
                .category(
                    category(
                        "Selection",
                        [
                            entry("copy", "c", "selection.copy", Some("copy")),
                            Some(RibbonEntry::menu("case", "a", "case")),
                        ],
                    )
                    .visible(when("Selection")),
                ),
        )
        .menu(RibbonMenu::new("file", "File").category(category(
            "File",
            [
                entry("new", "n", "file.new", Some("plus")),
                entry("open", "o", "file.open", Some("folder-open")),
                entry("save", "s", "file.save", None),
                entry("revert", "r", "file.revert", Some("undo-2")),
            ],
        )))
        .menu(RibbonMenu::new("view", "View").category(category(
            "View",
            [
                entry("wrap", "w", "view.wrap", None),
                entry("lines", "l", "view.lines", None),
                entry("minimap", "m", "view.minimap", Some("map")),
            ],
        )))
        .menu(RibbonMenu::new("case", "Case").category(category(
            "Case",
            [
                entry("upper", "u", "selection.upper", None),
                entry("lower", "l", "selection.lower", None),
            ],
        )))
}

struct RibbonDemo {
    commands: Commands,
    ribbon: Entity<RibbonState>,
    focus: FocusHandle,
    log: Entity<EventLog>,
    selection: bool,
}

fn ribbon(window: &mut Window, cx: &mut App) -> AnyView {
    let commands = Commands::new(cx);
    let log = EventLog::new(cx);
    let logged = |log: &Entity<EventLog>| {
        let log = log.clone();
        move |p: &Payload, _: &mut Window, cx: &mut App| {
            let args: Vec<String> = p.args.iter().map(|(k, v)| format!("{k}={v}")).collect();
            EventLog::log(
                &log,
                format!("ran {} via {:?} {}", p.id, p.source, args.join(" ")),
                cx,
            );
        }
    };
    for (command, title, key, enabled) in COMMANDS {
        let Some(command_id) = id(command) else {
            continue;
        };
        let mut command = Command::new(command_id, *title, logged(&log))
            .enabled(*enabled)
            .disabled_reason("The file has no saved version");
        if let Some(key) = key.and_then(|k| Keybinding::parse(k).ok()) {
            command = command.keybinding(key);
        }
        let _ = commands.register(command, cx);
    }
    if let Some(go_to) = id("editor.go-to") {
        let command = Command::new(go_to, "Go to symbol…", logged(&log))
            .param(Param::dynamic("symbol", "Symbol", "symbols").from_context("symbol"));
        let _ = commands.register(command, cx);
    }
    let model = {
        let state = commands.entity().read(cx);
        RibbonModel::build(&layout(), state.catalog(), &BTreeMap::new())
    };
    let model = match model {
        Ok(model) => Rc::new(model),
        Err(error) => {
            EventLog::log(&log, format!("ribbon layout rejected: {error}"), cx);
            let empty = RibbonLayout::new("root").menu(RibbonMenu::new("root", "Commands"));
            let state = commands.entity().read(cx);
            match RibbonModel::build(&empty, state.catalog(), &BTreeMap::new()) {
                Ok(model) => Rc::new(model),
                Err(_) => return cx.new(|_| gpui_kit::Empty).into(),
            }
        }
    };
    cx.new(|cx| {
        let ribbon = cx.new(|cx| {
            let mut state = RibbonState::new(&commands, model, window, cx);
            state.set_choices(
                "symbols",
                vec![
                    Choice::new("main", "fn main"),
                    Choice::new("render", "fn render"),
                    Choice::new("layout", "fn layout"),
                ],
                cx,
            );
            state
        });
        let events = log.clone();
        cx.subscribe(&ribbon, move |_, _, event: &RibbonEvent, cx| {
            if let RibbonEvent::Refused(reason)
            | RibbonEvent::Ran {
                result: Err(reason),
                ..
            } = event
            {
                EventLog::log(&events, format!("refused: {reason}"), cx);
            }
        })
        .detach();
        cx.observe(&ribbon, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut demo = RibbonDemo {
            commands,
            ribbon,
            focus,
            log,
            selection: false,
        };
        demo.sync_contexts(cx);
        demo
    })
    .into()
}

impl RibbonDemo {
    /// Host contexts: the page is an editor; the selection adds its category.
    fn sync_contexts(&mut self, cx: &mut Context<Self>) {
        let mut contexts = ContextStack::default();
        let frame = if self.selection {
            "Editor Selection"
        } else {
            "Editor"
        };
        if let Ok(frame) = WhenContext::parse(frame) {
            contexts.push(frame);
        }
        self.commands.set_contexts(contexts, cx);
        cx.notify();
    }
}

impl Render for RibbonDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let label = if self.selection {
            "Clear the selection"
        } else {
            "Select a word"
        };
        let editor =
            div()
                .h(px(Size::Xl.control_height() * 4.))
                .p(px(Size::Sm.spacing()))
                .child(
                    div().flex().gap_2().child(
                        Button::new("toggle-selection")
                            .label(label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.selection = !this.selection;
                                this.sync_contexts(cx);
                            })),
                    ),
                );
        let app = ribbon_scope(&self.ribbon)
            .flex()
            .flex_col()
            .border_1()
            .border_color(surfaces.border.to_hsla())
            .child(
                CommandScope::new(&self.commands).child(
                    div()
                        .key_context("Editor")
                        .track_focus(&self.focus)
                        .child(editor),
                ),
            )
            .children(CommandRibbon::when_open(&self.ribbon, cx))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .border_t_1()
                    .border_color(surfaces.border.to_hsla())
                    .child(RibbonButton::new(&self.ribbon)),
            );
        column([
            example(
                "Ctrl+Space or the status-bar button: f s saves, v w toggles wrap, Backspace goes up, Escape to root and close",
                app,
            ),
            example("Event log", self.log.clone()),
        ])
    }
}
