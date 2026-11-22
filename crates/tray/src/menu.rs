//! Tray menus, built from the command registry. Pure: no GPUI, no platform.
use guiraffe_commands::{CommandId, ContextStack, Registry};
use serde::{Deserialize, Serialize};

/// Identity of a clickable tray menu item, reported back in
/// [`TrayEvent::MenuItem`](crate::TrayEvent::MenuItem).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrayItemId {
    /// A registry command; routed to its handler by [`Tray::route_commands`](crate::Tray::route_commands).
    Command(CommandId),
    /// An app-defined item.
    Custom(String),
}

/// A resolved menu item, ready for a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayMenuItem {
    Item {
        id: TrayItemId,
        label: String,
        enabled: bool,
        /// `Some` renders a check mark item.
        checked: Option<bool>,
    },
    Separator,
    Submenu {
        label: String,
        items: Vec<TrayMenuItem>,
    },
}

impl TrayMenuItem {
    /// An app-defined item.
    pub fn custom(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Item {
            id: TrayItemId::Custom(id.into()),
            label: label.into(),
            enabled: true,
            checked: None,
        }
    }
}

/// What to put in the menu. Commands resolve against the registry when the menu is built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuEntry {
    /// One command: its title, enabled state and `when` visibility come from the registry.
    Command(CommandId),
    /// Every command of a category that is visible in the contexts, in registry order
    /// (the registry sorts by category, then title).
    Category(String),
    /// A check-mark command (e.g. "Do not disturb"); the state is the app's.
    Toggle {
        command: CommandId,
        checked: bool,
    },
    /// An app-defined item that isn't a command.
    Custom {
        id: String,
        label: String,
        enabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    /// A command entry from a command id string; invalid ids are skipped with a debug assert.
    pub fn command(id: &str) -> Option<Self> {
        let parsed = CommandId::parse(id).ok();
        debug_assert!(parsed.is_some(), "invalid command id {id:?}");
        parsed.map(Self::Command)
    }
}

/// Resolve `entries` against `registry` in `contexts`:
/// - unknown commands and commands hidden by their `when` clause are omitted;
/// - disabled commands stay, disabled (menus show what exists, conventions §12);
/// - empty submenus are dropped, and separators never lead, trail or repeat.
pub fn build_menu<R>(
    entries: &[MenuEntry],
    registry: &Registry<R>,
    contexts: &ContextStack,
) -> Vec<TrayMenuItem> {
    let mut out = Vec::new();
    for entry in entries {
        match entry {
            MenuEntry::Command(id) => {
                if let Some(item) = command_item(id, None, registry, contexts) {
                    out.push(item);
                }
            }
            MenuEntry::Toggle { command, checked } => {
                if let Some(item) = command_item(command, Some(*checked), registry, contexts) {
                    out.push(item);
                }
            }
            MenuEntry::Category(category) => {
                out.extend(
                    registry
                        .visible(contexts)
                        .filter(|c| &c.category == category)
                        .map(|c| TrayMenuItem::Item {
                            id: TrayItemId::Command(c.id.clone()),
                            label: c.title.clone(),
                            enabled: c.enabled,
                            checked: None,
                        }),
                );
            }
            MenuEntry::Custom { id, label, enabled } => out.push(TrayMenuItem::Item {
                id: TrayItemId::Custom(id.clone()),
                label: label.clone(),
                enabled: *enabled,
                checked: None,
            }),
            MenuEntry::Separator => out.push(TrayMenuItem::Separator),
            MenuEntry::Submenu { label, entries } => {
                let items = build_menu(entries, registry, contexts);
                if !items.is_empty() {
                    out.push(TrayMenuItem::Submenu {
                        label: label.clone(),
                        items,
                    });
                }
            }
        }
    }
    tidy_separators(out)
}

/// Menu of every visible command, one submenu per category (uncategorized ones at the top).
pub fn menu_by_category<R>(registry: &Registry<R>, contexts: &ContextStack) -> Vec<MenuEntry> {
    let mut categories: Vec<String> = Vec::new();
    let mut top = Vec::new();
    for c in registry.visible(contexts) {
        if c.category.is_empty() {
            top.push(MenuEntry::Command(c.id.clone()));
        } else if !categories.contains(&c.category) {
            categories.push(c.category.clone());
        }
    }
    let mut entries = top;
    if !entries.is_empty() && !categories.is_empty() {
        entries.push(MenuEntry::Separator);
    }
    entries.extend(categories.into_iter().map(|category| MenuEntry::Submenu {
        label: category.clone(),
        entries: vec![MenuEntry::Category(category)],
    }));
    entries
}

fn command_item<R>(
    id: &CommandId,
    checked: Option<bool>,
    registry: &Registry<R>,
    contexts: &ContextStack,
) -> Option<TrayMenuItem> {
    let command = registry.lookup(id)?;
    command.when.matches(contexts).then(|| TrayMenuItem::Item {
        id: TrayItemId::Command(command.id.clone()),
        label: command.title.clone(),
        enabled: command.enabled,
        checked,
    })
}

fn tidy_separators(items: Vec<TrayMenuItem>) -> Vec<TrayMenuItem> {
    let mut out: Vec<TrayMenuItem> = Vec::with_capacity(items.len());
    for item in items {
        let separator = matches!(item, TrayMenuItem::Separator);
        if separator
            && out
                .last()
                .is_none_or(|l| matches!(l, TrayMenuItem::Separator))
        {
            continue;
        }
        out.push(item);
    }
    if matches!(out.last(), Some(TrayMenuItem::Separator)) {
        out.pop();
    }
    out
}

/// Find an item by id (depth-first), e.g. to update a toggle.
pub fn find_item<'a>(items: &'a [TrayMenuItem], id: &TrayItemId) -> Option<&'a TrayMenuItem> {
    items.iter().find_map(|item| match item {
        TrayMenuItem::Item { id: item_id, .. } if item_id == id => Some(item),
        TrayMenuItem::Submenu { items, .. } => find_item(items, id),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_commands::{Command, When, WhenContext};

    fn id(s: &str) -> CommandId {
        CommandId::parse(s).expect("valid id")
    }

    fn registry() -> Registry<()> {
        let mut r = Registry::default();
        for (cid, title, category, enabled, when) in [
            ("app.show", "Show window", "", true, None),
            ("app.quit", "Quit", "", true, None),
            ("sync.now", "Sync now", "Sync", true, None),
            ("sync.pause", "Pause sync", "Sync", false, None),
            ("editor.save", "Save", "Editor", true, Some("Editor")),
        ] {
            let mut c = Command::new(id(cid), title, ())
                .category(category)
                .enabled(enabled);
            if let Some(ctx) = when {
                c = c.when(When::parse(ctx).expect("when"));
            }
            r.register(c).expect("register");
        }
        r
    }

    fn labels(items: &[TrayMenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                TrayMenuItem::Item {
                    label,
                    enabled,
                    checked,
                    ..
                } => format!(
                    "{label}{}{}",
                    if *enabled { "" } else { " (disabled)" },
                    match checked {
                        Some(true) => " [x]",
                        Some(false) => " [ ]",
                        None => "",
                    }
                ),
                TrayMenuItem::Separator => "---".into(),
                TrayMenuItem::Submenu { label, items } => {
                    format!("{label} > {}", labels(items).join(", "))
                }
            })
            .collect()
    }

    #[test]
    fn commands_resolve_titles_enabled_and_visibility() {
        let r = registry();
        let entries = [
            MenuEntry::Command(id("app.show")),
            MenuEntry::Command(id("editor.save")),
            MenuEntry::Command(id("does.not-exist")),
            MenuEntry::Separator,
            MenuEntry::Category("Sync".into()),
            MenuEntry::Separator,
            MenuEntry::Toggle {
                command: id("app.quit"),
                checked: true,
            },
        ];
        let menu = build_menu(&entries, &r, &ContextStack::default());
        assert_eq!(
            labels(&menu),
            [
                "Show window",
                "---",
                "Pause sync (disabled)",
                "Sync now",
                "---",
                "Quit [x]"
            ]
        );
        let mut editor = ContextStack::default();
        editor.push(WhenContext::new("Editor").expect("ctx"));
        let menu = build_menu(&entries, &r, &editor);
        assert_eq!(
            labels(&menu)[1],
            "Save",
            "when-clause visibility follows contexts"
        );
        assert_eq!(
            menu.first(),
            Some(&TrayMenuItem::Item {
                id: TrayItemId::Command(id("app.show")),
                label: "Show window".into(),
                enabled: true,
                checked: None
            })
        );
    }

    #[test]
    fn separators_and_empty_submenus_are_tidied() {
        let r = registry();
        let entries = [
            MenuEntry::Separator,
            MenuEntry::Submenu {
                label: "Nothing".into(),
                entries: vec![MenuEntry::Command(id("missing.one"))],
            },
            MenuEntry::Separator,
            MenuEntry::Separator,
            MenuEntry::Custom {
                id: "about".into(),
                label: "About".into(),
                enabled: true,
            },
            MenuEntry::Separator,
        ];
        let menu = build_menu(&entries, &r, &ContextStack::default());
        assert_eq!(labels(&menu), ["About"]);
        assert!(build_menu(&[MenuEntry::Separator], &r, &ContextStack::default()).is_empty());
    }

    #[test]
    fn category_menu_groups_into_submenus() {
        let r = registry();
        let ctx = ContextStack::default();
        let menu = build_menu(&menu_by_category(&r, &ctx), &r, &ctx);
        assert_eq!(
            labels(&menu),
            [
                "Quit",
                "Show window",
                "---",
                "Sync > Pause sync (disabled), Sync now"
            ]
        );
        assert!(find_item(&menu, &TrayItemId::Command(id("sync.now"))).is_some());
        assert!(find_item(&menu, &TrayItemId::Command(id("editor.save"))).is_none());
    }
}
