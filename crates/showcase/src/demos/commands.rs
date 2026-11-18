//! guiraffe-commands: one command catalog behind keyboard shortcuts, an in-app menu and the
//! Spotlight palette, with context-specific precedence and a live keymap conflict report.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, component::button::Button, div,
};
use guiraffe_commands::{
    Command, CommandId, CommandScope, Commands, ContextStack, Keybinding, Menu, Payload, When,
    WhenContext,
};
use guiraffe_components::Text;
use guiraffe_core::Size;

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "commands",
        name: "Commands & keymap",
        summary: "One registry drives shortcuts, the menu and the command palette. Shortcuts show per OS (⌘ on macOS, Ctrl elsewhere); the most specific context wins, disabled commands fall through, and conflicts are reported live.",
        covers: &["Menu", "CommandScope"],
        demo: keymap,
    }]
}

/// (id, title, category, binding, when, enabled)
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    bool,
);
const CATALOG: &[Row] = &[
    ("file.save", "Save", "File", "secondary+s", None, true),
    ("file.open", "Open…", "File", "secondary+o", None, true),
    (
        "file.delete",
        "Delete file",
        "File",
        "secondary+shift+delete",
        None,
        false,
    ),
    (
        "editor.format",
        "Format document",
        "Editor",
        "alt+shift+f",
        Some("Editor"),
        true,
    ),
    (
        "view.settings",
        "Settings",
        "View",
        "secondary+k secondary+s",
        None,
        true,
    ),
];
/// Toggled in to show the conflict report: same key as Save, and a chord prefix.
const CONFLICTS: &[(&str, &str, &str, &str)] = &[
    ("editor.quick-save", "Quick save", "Editor", "ctrl+s"),
    ("view.keymap", "Keymap prefix", "View", "secondary+k"),
];

struct KeymapDemo {
    commands: Commands,
    focus: FocusHandle,
    log: Entity<EventLog>,
    conflicts: bool,
}

fn parse(id: &str, binding: &str) -> Option<(CommandId, Keybinding)> {
    Some((CommandId::parse(id).ok()?, Keybinding::parse(binding).ok()?))
}

fn logged(log: &Entity<EventLog>) -> impl Fn(&Payload, &mut Window, &mut App) + 'static {
    let log = log.clone();
    move |p: &Payload, _: &mut Window, cx: &mut App| {
        EventLog::log(&log, format!("ran {} via {:?}", p.id, p.source), cx);
    }
}

fn keymap(window: &mut Window, cx: &mut App) -> AnyView {
    let commands = Commands::new(cx);
    let log = EventLog::new(cx);
    for (id, title, category, binding, when, enabled) in CATALOG {
        let Some((id, binding)) = parse(id, binding) else {
            continue;
        };
        let mut command = Command::new(id, *title, logged(&log))
            .category(*category)
            .keybinding(binding)
            .enabled(*enabled);
        if let Some(w) = when.and_then(|w| When::parse(w).ok()) {
            command = command.when(w);
        }
        let _ = commands.register(command, cx);
    }
    // Host context: the demo's document is an editor, so menu and report include
    // Editor-scoped commands (shortcuts also see the focused element's real contexts).
    let mut host = ContextStack::default();
    if let Ok(editor) = WhenContext::new("Editor") {
        host.push(editor);
    }
    commands.set_contexts(host, cx);
    cx.new(|cx| {
        cx.observe(commands.entity(), |_, _, cx| cx.notify())
            .detach();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut demo = KeymapDemo {
            commands,
            focus,
            log,
            conflicts: false,
        };
        demo.toggle_conflicts(cx);
        demo
    })
    .into()
}

impl KeymapDemo {
    fn toggle_conflicts(&mut self, cx: &mut Context<Self>) {
        self.conflicts = !self.conflicts;
        for (id, title, category, binding) in CONFLICTS {
            let Some((id, binding)) = parse(id, binding) else {
                continue;
            };
            if self.conflicts {
                let command = Command::new(id, *title, logged(&self.log))
                    .category(*category)
                    .keybinding(binding)
                    .when(When::parse("Editor").unwrap_or_default());
                let _ = self.commands.register(command, cx);
            } else {
                self.commands.unregister(&id, cx);
            }
        }
        cx.notify();
    }

    fn report(&self, cx: &App) -> Vec<SharedString> {
        // Host contexts: reading the window's live key contexts is invalid during render.
        let contexts = self.commands.entity().read(cx).contexts().clone();
        let platform = self.commands.platform(cx);
        let report = self.commands.conflict_report(&contexts, cx);
        let mut lines: Vec<SharedString> = report
            .exact
            .iter()
            .map(|c| {
                let shadowed = c
                    .shadowed
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{}: {} wins, shadows {shadowed}",
                    c.binding.label(platform),
                    c.winner
                )
                .into()
            })
            .collect();
        lines.extend(report.prefixes.iter().map(|p| {
            SharedString::from(format!(
                "{} ({}) waits for {} ({})",
                p.prefix.label(platform),
                p.prefix_command,
                p.chord.label(platform),
                p.chord_command
            ))
        }));
        if lines.is_empty() {
            lines.push("No conflicts".into());
        }
        lines
    }
}

impl Render for KeymapDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.commands.clone();
        let report = self.report(cx);
        let label = if self.conflicts {
            "Remove conflicting bindings"
        } else {
            "Add conflicting bindings"
        };
        CommandScope::new(&self.commands).child(
            div()
                .key_context("Editor")
                .track_focus(&self.focus)
                .child(column([
                    example(
                        "Shortcuts work while this demo has focus; the palette uses the same registry",
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("open-palette").label("Open command palette").on_click(
                                move |_, window, cx| {
                                    let _ = palette.open_spotlight(window, cx);
                                },
                            ))
                            .child(
                                Button::new("toggle-conflicts")
                                    .label(label)
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_conflicts(cx))),
                            ),
                    ),
                    example(
                        "Menu: the registry grouped by category, with per-OS shortcut labels",
                        Menu::new("keymap-menu", &self.commands),
                    ),
                    example(
                        "Live conflict report (exact and chord-prefix)",
                        div().flex().flex_col().gap_1().children(
                            report
                                .into_iter()
                                .map(|line| Text::new(line).size(Size::Sm)),
                        ),
                    ),
                    example("Event log", self.log.clone()),
                ])),
        )
    }
}
