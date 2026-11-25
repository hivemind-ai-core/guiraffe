//! The ribbon's layout, its compiled form and chord navigation. Pure: no GPUI types.
//!
//! A [`RibbonLayout`] is a DAG of menus. Each menu has categories (columns shown side by side,
//! in registration order: Global, then the page's, then the selection's) and each category has
//! entries that open a submenu, run a registry command or apply a control action through the
//! action bus. [`RibbonModel::build`] validates the whole layout once; [`Navigator`] walks it.
use crate::{CommandId, ContextStack, Registry, When};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// What an entry does when activated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RibbonTarget {
    /// Run a registry command.
    Command(CommandId),
    /// Descend into another menu of the layout.
    Menu(String),
    /// Apply a payload-free shared action (see `guiraffe_core::protocol::names`) to the
    /// control registered at `control` on the window's action bus.
    Action {
        control: String,
        action: String,
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonEntry {
    /// Stable, layout-unique id; chord overrides refer to it.
    pub id: String,
    /// One printable, non-space character, unique within its menu.
    pub key: String,
    pub target: RibbonTarget,
    /// Navigation condition: a hidden entry cannot be reached.
    pub visible: When,
    /// Asset path of an icon (`icons/<name>.svg`); the slot is reserved either way.
    pub icon: Option<String>,
}
impl RibbonEntry {
    pub fn new(id: impl Into<String>, key: impl Into<String>, target: RibbonTarget) -> Self {
        Self {
            id: id.into(),
            key: key.into(),
            target,
            visible: When::always(),
            icon: None,
        }
    }
    pub fn command(id: impl Into<String>, key: impl Into<String>, command: CommandId) -> Self {
        Self::new(id, key, RibbonTarget::Command(command))
    }
    pub fn menu(id: impl Into<String>, key: impl Into<String>, menu: impl Into<String>) -> Self {
        Self::new(id, key, RibbonTarget::Menu(menu.into()))
    }
    pub fn action(
        id: impl Into<String>,
        key: impl Into<String>,
        label: impl Into<String>,
        control: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self::new(
            id,
            key,
            RibbonTarget::Action {
                control: control.into(),
                action: action.into(),
                label: label.into(),
            },
        )
    }
    pub fn visible(mut self, when: When) -> Self {
        self.visible = when;
        self
    }
    pub fn icon(mut self, path: impl Into<String>) -> Self {
        self.icon = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonCategory {
    pub title: String,
    pub visible: When,
    pub entries: Vec<RibbonEntry>,
}
impl RibbonCategory {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            visible: When::always(),
            entries: Vec::new(),
        }
    }
    pub fn visible(mut self, when: When) -> Self {
        self.visible = when;
        self
    }
    pub fn entry(mut self, entry: RibbonEntry) -> Self {
        self.entries.push(entry);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonMenu {
    pub id: String,
    pub label: String,
    pub categories: Vec<RibbonCategory>,
}
impl RibbonMenu {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            categories: Vec::new(),
        }
    }
    pub fn category(mut self, category: RibbonCategory) -> Self {
        self.categories.push(category);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonLayout {
    pub root: String,
    pub menus: Vec<RibbonMenu>,
}
impl RibbonLayout {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            menus: Vec::new(),
        }
    }
    pub fn menu(mut self, menu: RibbonMenu) -> Self {
        self.menus.push(menu);
        self
    }
}

/// Why a layout or override set was rejected. The previous model stays active.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RibbonError {
    #[error("empty or duplicate menu id {0:?}")]
    Menu(String),
    #[error("root menu {0:?} does not exist")]
    Root(String),
    #[error("empty or duplicate entry id {0:?}")]
    Entry(String),
    #[error("entry {entry} has an invalid chord key {key:?}: one printable, non-space character")]
    Key { entry: String, key: String },
    #[error("chord {key:?} is used twice in menu {menu}")]
    Collision { menu: String, key: String },
    #[error("entry {entry} refers to unknown {what} {target}")]
    Target {
        entry: String,
        what: &'static str,
        target: String,
    },
    #[error("menu cycle through {0}")]
    Cycle(String),
    #[error("override refers to unknown entry {0}")]
    Override(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Command(CommandId),
    Menu(usize),
    Action {
        control: String,
        action: String,
        label: String,
    },
}
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub id: String,
    pub key: String,
    pub target: Target,
    pub visible: When,
    pub icon: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct Category {
    pub title: String,
    pub visible: When,
    pub entries: std::ops::Range<usize>,
}
#[derive(Debug, Clone)]
pub(crate) struct Menu {
    pub label: String,
    pub categories: std::ops::Range<usize>,
}

/// A validated layout with index-based lookups; built once per layout or override change.
#[derive(Debug, Clone)]
pub struct RibbonModel {
    pub(crate) menus: Vec<Menu>,
    pub(crate) categories: Vec<Category>,
    pub(crate) entries: Vec<Entry>,
    entry_ids: HashMap<String, usize>,
    root: usize,
}

fn chord_key(input: &str) -> bool {
    let mut chars = input.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_whitespace() && !c.is_control())
}

impl RibbonModel {
    /// Validate ids, references (commands must be in `registry`), cycles, chord keys and chord
    /// collisions (unique within a menu across all its categories, hidden or not), and apply
    /// chord overrides (entry id to key). Any failure leaves nothing half-built.
    pub fn build<Run>(
        layout: &RibbonLayout,
        registry: &Registry<Run>,
        chords: &BTreeMap<String, String>,
    ) -> Result<Self, RibbonError> {
        let mut menu_index = HashMap::new();
        for (index, menu) in layout.menus.iter().enumerate() {
            if menu.id.trim().is_empty() || menu_index.insert(menu.id.as_str(), index).is_some() {
                return Err(RibbonError::Menu(menu.id.clone()));
            }
        }
        let root = *menu_index
            .get(layout.root.as_str())
            .ok_or_else(|| RibbonError::Root(layout.root.clone()))?;
        let mut menus = Vec::with_capacity(layout.menus.len());
        let mut categories = Vec::new();
        let mut entries: Vec<Entry> = Vec::new();
        let mut entry_ids = HashMap::new();
        for menu in &layout.menus {
            let first_category = categories.len();
            let mut keys = BTreeSet::new();
            for category in &menu.categories {
                let first_entry = entries.len();
                for entry in &category.entries {
                    if entry.id.trim().is_empty()
                        || entry_ids.insert(entry.id.clone(), entries.len()).is_some()
                    {
                        return Err(RibbonError::Entry(entry.id.clone()));
                    }
                    let key = chords.get(&entry.id).unwrap_or(&entry.key);
                    if !chord_key(key) {
                        return Err(RibbonError::Key {
                            entry: entry.id.clone(),
                            key: key.clone(),
                        });
                    }
                    // Reject ambiguous keys even when predicates look disjoint today.
                    if !keys.insert(key.clone()) {
                        return Err(RibbonError::Collision {
                            menu: menu.id.clone(),
                            key: key.clone(),
                        });
                    }
                    let target = match &entry.target {
                        RibbonTarget::Command(id) => {
                            if registry.lookup(id).is_none() {
                                return Err(RibbonError::Target {
                                    entry: entry.id.clone(),
                                    what: "command",
                                    target: id.to_string(),
                                });
                            }
                            Target::Command(id.clone())
                        }
                        RibbonTarget::Menu(id) => {
                            Target::Menu(*menu_index.get(id.as_str()).ok_or_else(|| {
                                RibbonError::Target {
                                    entry: entry.id.clone(),
                                    what: "menu",
                                    target: id.clone(),
                                }
                            })?)
                        }
                        RibbonTarget::Action {
                            control,
                            action,
                            label,
                        } => Target::Action {
                            control: control.clone(),
                            action: action.clone(),
                            label: label.clone(),
                        },
                    };
                    entries.push(Entry {
                        id: entry.id.clone(),
                        key: key.clone(),
                        target,
                        visible: entry.visible.clone(),
                        icon: entry.icon.clone(),
                    });
                }
                categories.push(Category {
                    title: category.title.clone(),
                    visible: category.visible.clone(),
                    entries: first_entry..entries.len(),
                });
            }
            menus.push(Menu {
                label: menu.label.clone(),
                categories: first_category..categories.len(),
            });
        }
        if let Some(id) = chords.keys().find(|id| !entry_ids.contains_key(*id)) {
            return Err(RibbonError::Override(id.clone()));
        }
        let model = Self {
            menus,
            categories,
            entries,
            entry_ids,
            root,
        };
        model.check_cycles(layout)?;
        Ok(model)
    }

    fn check_cycles(&self, layout: &RibbonLayout) -> Result<(), RibbonError> {
        // 0 unvisited, 1 on the stack, 2 done. Iterative so deep layouts cannot overflow.
        let mut state = vec![0u8; self.menus.len()];
        for start in 0..self.menus.len() {
            if state[start] != 0 {
                continue;
            }
            let mut stack = vec![(start, self.children(start))];
            state[start] = 1;
            while let Some((menu, children)) = stack.last_mut() {
                let menu = *menu;
                match children.next() {
                    Some(child) if state[child] == 1 => {
                        let id = layout.menus.get(child).map(|m| m.id.clone());
                        return Err(RibbonError::Cycle(id.unwrap_or_default()));
                    }
                    Some(child) if state[child] == 0 => {
                        state[child] = 1;
                        stack.push((child, self.children(child)));
                    }
                    Some(_) => {}
                    None => {
                        state[menu] = 2;
                        stack.pop();
                    }
                }
            }
        }
        Ok(())
    }

    fn children(&self, menu: usize) -> std::vec::IntoIter<usize> {
        self.menu_entries(menu)
            .filter_map(|(_, e)| match e.target {
                Target::Menu(child) => Some(child),
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn menu_entries(&self, menu: usize) -> impl Iterator<Item = (usize, &Entry)> {
        let categories = self.menus.get(menu).map(|m| m.categories.clone());
        categories.into_iter().flatten().flat_map(move |c| {
            let range = self.categories.get(c).map(|c| c.entries.clone());
            range
                .into_iter()
                .flatten()
                .filter_map(move |e| self.entries.get(e).map(|entry| (e, entry)))
        })
    }

    /// Entries reachable in `menu` under `contexts`, with category conditions applied.
    pub(crate) fn visible_entries<'a>(
        &'a self,
        menu: usize,
        contexts: &'a ContextStack,
    ) -> impl Iterator<Item = (usize, &'a Entry)> + 'a {
        let categories = self.menus.get(menu).map(|m| m.categories.clone());
        categories
            .into_iter()
            .flatten()
            .filter_map(move |c| self.categories.get(c))
            .filter(move |c| c.visible.matches(contexts))
            .flat_map(move |c| {
                c.entries
                    .clone()
                    .filter_map(move |e| self.entries.get(e).map(|entry| (e, entry)))
            })
            .filter(move |(_, e)| e.visible.matches(contexts))
    }

    pub fn root_label(&self) -> &str {
        self.menus.get(self.root).map_or("", |m| m.label.as_str())
    }
    /// The entry index for a stable entry id.
    pub fn entry_index(&self, id: &str) -> Option<usize> {
        self.entry_ids.get(id).copied()
    }
    pub fn entry_id(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|e| e.id.as_str())
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The categories and rows of the navigator's current menu. Built when the ribbon opens,
    /// navigates or its inputs change, never per frame.
    pub fn sections<Run>(
        &self,
        navigator: &Navigator,
        registry: &Registry<Run>,
        contexts: &ContextStack,
    ) -> Vec<Section> {
        let mut navigator = navigator.clone();
        navigator.reconcile(self, contexts);
        let menu = navigator.current(self);
        let Some(menu) = self.menus.get(menu) else {
            return Vec::new();
        };
        menu.categories
            .clone()
            .filter_map(|c| self.categories.get(c))
            .filter(|c| c.visible.matches(contexts))
            .filter_map(|category| {
                let items: Vec<Item> = category
                    .entries
                    .clone()
                    .filter_map(|e| self.entries.get(e).map(|entry| (e, entry)))
                    .filter(|(_, e)| e.visible.matches(contexts))
                    .map(|(index, entry)| {
                        let (label, submenu, disabled) = match &entry.target {
                            Target::Command(id) => match registry.lookup(id) {
                                Some(command) => (
                                    command.title.clone(),
                                    false,
                                    command.unavailable_reason(contexts).map(str::to_owned),
                                ),
                                None => (id.to_string(), false, Some("Unavailable".into())),
                            },
                            Target::Menu(child) => (
                                self.menus
                                    .get(*child)
                                    .map(|m| m.label.clone())
                                    .unwrap_or_default(),
                                true,
                                None,
                            ),
                            Target::Action { label, .. } => (label.clone(), false, None),
                        };
                        Item {
                            entry: index,
                            key: entry.key.clone(),
                            label,
                            icon: entry.icon.clone(),
                            submenu,
                            disabled_reason: disabled,
                            shortcut: match &entry.target {
                                Target::Command(id) => registry
                                    .lookup(id)
                                    .and_then(|c| c.keybinding.as_ref())
                                    .map(|b| b.label(registry.platform())),
                                _ => None,
                            },
                        }
                    })
                    .collect();
                (!items.is_empty()).then(|| Section {
                    title: category.title.clone(),
                    items,
                })
            })
            .collect()
    }
}

/// One category column group of the current menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}
/// One row: icon slot, chord key, label, optional shortcut and a submenu marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub entry: usize,
    pub key: String,
    pub label: String,
    pub icon: Option<String>,
    pub submenu: bool,
    pub disabled_reason: Option<String>,
    pub shortcut: Option<String>,
}

/// The result of activating an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Moved into a submenu; nothing ran.
    Descended,
    /// Run this command (after collecting its parameters).
    Command(CommandId),
    /// Apply `action` to the control at `control`.
    Action { control: String, action: String },
}

/// Why an activation did nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NavError {
    #[error("no entry for [{0}]")]
    NoEntry(String),
    #[error("entry is no longer reachable")]
    Unreachable,
    #[error("{0}")]
    Disabled(String),
}

/// The path into the menu DAG as entry indices, not menu ids, so the conditions of the
/// entries actually used stay attached even when several parents share a submenu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Navigator {
    path: Vec<usize>,
}
impl Navigator {
    pub fn depth(&self) -> usize {
        self.path.len()
    }
    pub fn root(&mut self) {
        self.path.clear();
    }
    pub fn back(&mut self) {
        self.path.pop();
    }
    /// The current menu's index. A broken path stops at its last valid menu.
    pub(crate) fn current(&self, model: &RibbonModel) -> usize {
        let mut menu = model.root;
        for entry in &self.path {
            match model.entries.get(*entry).map(|e| &e.target) {
                Some(Target::Menu(child)) => menu = *child,
                _ => break,
            }
        }
        menu
    }
    /// Labels from the root to the current menu.
    pub fn breadcrumbs<'a>(&self, model: &'a RibbonModel) -> Vec<&'a str> {
        let mut labels = vec![model.root_label()];
        for entry in &self.path {
            if let Some(Target::Menu(child)) = model.entries.get(*entry).map(|e| &e.target) {
                labels.push(model.menus.get(*child).map_or("", |m| m.label.as_str()));
            }
        }
        labels
    }
    /// Truncate the path at the first entry no longer visible under `contexts`.
    pub fn reconcile(&mut self, model: &RibbonModel, contexts: &ContextStack) {
        let mut menu = model.root;
        let mut valid = 0;
        for entry in &self.path {
            let reachable = model
                .visible_entries(menu, contexts)
                .any(|(index, _)| index == *entry);
            match model.entries.get(*entry).map(|e| &e.target) {
                Some(Target::Menu(child)) if reachable => {
                    menu = *child;
                    valid += 1;
                }
                _ => break,
            }
        }
        self.path.truncate(valid);
    }
    /// Activate an entry by index, rechecking that it is still reachable (a click on a stale
    /// row) and that a command is available. Descending only changes the path.
    pub fn activate<Run>(
        &mut self,
        entry: usize,
        model: &RibbonModel,
        registry: &Registry<Run>,
        contexts: &ContextStack,
    ) -> Result<Step, NavError> {
        self.reconcile(model, contexts);
        let menu = self.current(model);
        let (_, found) = model
            .visible_entries(menu, contexts)
            .find(|(index, _)| *index == entry)
            .ok_or(NavError::Unreachable)?;
        match &found.target {
            Target::Menu(_) => {
                self.path.push(entry);
                Ok(Step::Descended)
            }
            Target::Command(id) => {
                let command = registry.lookup(id).ok_or(NavError::Unreachable)?;
                if let Some(reason) = command.unavailable_reason(contexts) {
                    return Err(NavError::Disabled(reason.to_owned()));
                }
                Ok(Step::Command(id.clone()))
            }
            Target::Action {
                control, action, ..
            } => Ok(Step::Action {
                control: control.clone(),
                action: action.clone(),
            }),
        }
    }
    /// Activate the entry whose chord is `key` in the current menu.
    pub fn press<Run>(
        &mut self,
        key: &str,
        model: &RibbonModel,
        registry: &Registry<Run>,
        contexts: &ContextStack,
    ) -> Result<Step, NavError> {
        self.reconcile(model, contexts);
        let entry = model
            .visible_entries(self.current(model), contexts)
            .find(|(_, e)| e.key == key)
            .map(|(index, _)| index)
            .ok_or_else(|| NavError::NoEntry(key.to_owned()))?;
        self.activate(entry, model, registry, contexts)
    }
    /// Whether `key` would run something rather than descend (held keys never execute).
    pub fn would_execute(&self, key: &str, model: &RibbonModel, contexts: &ContextStack) -> bool {
        let mut nav = self.clone();
        nav.reconcile(model, contexts);
        model
            .visible_entries(nav.current(model), contexts)
            .find(|(_, e)| e.key == key)
            .is_some_and(|(_, e)| !matches!(e.target, Target::Menu(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Keybinding, context::Context};

    fn id(value: &str) -> CommandId {
        CommandId::parse(value).expect("id")
    }
    fn registry() -> Registry {
        let mut registry = Registry::default();
        registry
            .register(
                Command::new(id("records.archive"), "Archive record", ())
                    .keybinding(Keybinding::parse("ctrl+a").expect("key"))
                    .when(When::parse("writable").expect("when")),
            )
            .expect("archive");
        registry
            .register(Command::new(id("message"), "Show message", ()))
            .expect("message");
        registry
    }
    fn layout() -> RibbonLayout {
        RibbonLayout::new("root")
            .menu(
                RibbonMenu::new("root", "Root")
                    .category(RibbonCategory::new("Global").entry(RibbonEntry::command(
                        "message-entry",
                        "m",
                        id("message"),
                    )))
                    .category(
                        RibbonCategory::new("Records")
                            .visible(When::parse("records").expect("when"))
                            .entry(RibbonEntry::menu("records-entry", "r", "records")),
                    ),
            )
            .menu(RibbonMenu::new("records", "Records").category(
                RibbonCategory::new("Selection").entry(RibbonEntry::command(
                    "archive-entry",
                    "a",
                    id("records.archive"),
                )),
            ))
    }
    fn build(layout: &RibbonLayout) -> Result<RibbonModel, RibbonError> {
        RibbonModel::build(layout, &registry(), &BTreeMap::new())
    }
    fn contexts(flags: &[&str]) -> ContextStack {
        let mut stack = ContextStack::default();
        if !flags.is_empty() {
            stack.push(Context::parse(&flags.join(" ")).expect("frame"));
        }
        stack
    }

    #[test]
    fn inherited_visibility_prevents_stale_submenu_access() {
        let model = build(&layout()).expect("model");
        let registry = registry();
        let mut nav = Navigator::default();
        let none = contexts(&["writable"]);
        assert!(nav.press("r", &model, &registry, &none).is_err());
        let on_page = contexts(&["writable", "records"]);
        assert_eq!(
            nav.press("r", &model, &registry, &on_page),
            Ok(Step::Descended)
        );
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.breadcrumbs(&model), ["Root", "Records"]);
        // Leaving the page while the bar is in the submenu: a stale chord does nothing.
        assert!(nav.press("a", &model, &registry, &none).is_err());
        assert_eq!(nav.depth(), 0);
        assert_eq!(model.sections(&nav, &registry, &none).len(), 1);
    }

    #[test]
    fn categories_keep_registration_order_and_rows_carry_keys_labels_and_shortcuts() {
        let model = build(&layout()).expect("model");
        let registry = registry();
        let cx = contexts(&["records"]);
        let sections = model.sections(&Navigator::default(), &registry, &cx);
        let titles: Vec<_> = sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Global", "Records"]);
        assert!(sections[1].items[0].submenu);
        let mut nav = Navigator::default();
        nav.press("r", &model, &registry, &cx).expect("descend");
        let rows = model.sections(&nav, &registry, &cx);
        let row = &rows[0].items[0];
        assert_eq!(row.key, "a");
        assert_eq!(row.label, "Archive record");
        assert!(row.shortcut.is_some());
        assert_eq!(row.disabled_reason.as_deref(), Some("Not available here"));
    }

    #[test]
    fn disabled_leaf_is_refused_for_click_and_chord_alike() {
        let model = build(&layout()).expect("model");
        let registry = registry();
        let cx = contexts(&["records"]);
        let mut nav = Navigator::default();
        nav.press("r", &model, &registry, &cx).expect("descend");
        let archive = model.entry_index("archive-entry").expect("entry");
        assert!(matches!(
            nav.press("a", &model, &registry, &cx),
            Err(NavError::Disabled(_))
        ));
        assert!(matches!(
            nav.activate(archive, &model, &registry, &cx),
            Err(NavError::Disabled(_))
        ));
        let writable = contexts(&["records", "writable"]);
        assert_eq!(
            nav.activate(archive, &model, &registry, &writable),
            Ok(Step::Command(id("records.archive")))
        );
    }

    #[test]
    fn entry_predicates_are_enforced_for_mouse_and_keyboard() {
        let mut layout = layout();
        layout.menus[0].categories[1].entries[0].visible = When::parse("selection").expect("w");
        let model = build(&layout).expect("model");
        let registry = registry();
        let entry = model.entry_index("records-entry").expect("entry");
        let mut nav = Navigator::default();
        let cx = contexts(&["records"]);
        assert_eq!(
            nav.activate(entry, &model, &registry, &cx),
            Err(NavError::Unreachable)
        );
        let cx = contexts(&["records", "selection"]);
        assert_eq!(
            nav.activate(entry, &model, &registry, &cx),
            Ok(Step::Descended)
        );
    }

    #[test]
    fn chord_collisions_across_categories_are_rejected_even_if_hidden() {
        let mut overrides = BTreeMap::new();
        overrides.insert("records-entry".to_owned(), "m".to_owned());
        assert!(matches!(
            RibbonModel::build(&layout(), &registry(), &overrides),
            Err(RibbonError::Collision { .. })
        ));
        overrides.insert("records-entry".to_owned(), "+".to_owned());
        let model = RibbonModel::build(&layout(), &registry(), &overrides).expect("model");
        let mut nav = Navigator::default();
        let cx = contexts(&["records"]);
        assert_eq!(
            nav.press("+", &model, &registry(), &cx),
            Ok(Step::Descended)
        );
        // The same key may be reused in a different menu.
        let mut layout = layout();
        layout.menus[1].categories[0].entries[0].key = "m".into();
        assert!(build(&layout).is_ok());
    }

    #[test]
    fn unknown_ids_duplicates_missing_targets_bad_keys_and_cycles_are_rejected() {
        let mut v = layout();
        v.menus.push(v.menus[1].clone());
        assert!(matches!(build(&v), Err(RibbonError::Menu(_))));
        let mut v = layout();
        v.menus[0].categories[0].entries[0].target = RibbonTarget::Command(id("missing"));
        assert!(matches!(build(&v), Err(RibbonError::Target { .. })));
        let mut v = layout();
        v.menus[1].categories[0].entries[0].target = RibbonTarget::Menu("root".into());
        assert!(matches!(build(&v), Err(RibbonError::Cycle(_))));
        let mut v = layout();
        v.menus[1].categories[0].entries[0].id = "message-entry".into();
        assert!(matches!(build(&v), Err(RibbonError::Entry(_))));
        for key in ["", "ab", " ", "\n"] {
            let mut v = layout();
            v.menus[0].categories[0].entries[0].key = key.into();
            assert!(matches!(build(&v), Err(RibbonError::Key { .. })), "{key:?}");
        }
        let mut v = layout();
        v.root = "nope".into();
        assert!(matches!(build(&v), Err(RibbonError::Root(_))));
        let mut overrides = BTreeMap::new();
        overrides.insert("missing".to_owned(), "x".to_owned());
        assert!(matches!(
            RibbonModel::build(&layout(), &registry(), &overrides),
            Err(RibbonError::Override(_))
        ));
    }

    #[test]
    fn shared_submenu_preserves_the_predicates_of_the_entry_used() {
        let mut v = layout();
        let alternative = RibbonEntry::menu("alt-path", "s", "records")
            .visible(When::parse("alternate").expect("when"));
        v.menus[0].categories[0].entries.push(alternative);
        let model = build(&v).expect("model");
        let registry = registry();
        let mut nav = Navigator::default();
        nav.press("s", &model, &registry, &contexts(&["alternate"]))
            .expect("descend");
        assert_eq!(nav.depth(), 1);
        // The records category is now visible, but the alternate path that was used is not.
        nav.reconcile(&model, &contexts(&["records"]));
        assert_eq!(nav.depth(), 0);
    }

    #[test]
    fn backspace_and_escape_semantics_are_back_and_root() {
        let mut v = layout();
        v.menus[1].categories[0]
            .entries
            .push(RibbonEntry::menu("deeper", "d", "deep"));
        v.menus.push(RibbonMenu::new("deep", "Deep").category(
            RibbonCategory::new("Deep").entry(RibbonEntry::command("deep-msg", "x", id("message"))),
        ));
        let model = build(&v).expect("model");
        let registry = registry();
        let cx = contexts(&["records"]);
        let mut nav = Navigator::default();
        nav.press("r", &model, &registry, &cx).expect("r");
        nav.press("d", &model, &registry, &cx).expect("d");
        assert_eq!(nav.breadcrumbs(&model), ["Root", "Records", "Deep"]);
        assert!(nav.would_execute("x", &model, &cx));
        assert!(!Navigator::default().would_execute("r", &model, &cx));
        nav.back();
        assert_eq!(nav.depth(), 1);
        nav.root();
        assert_eq!(nav.depth(), 0);
        assert_eq!(
            nav.press("m", &model, &registry, &cx),
            Ok(Step::Command(id("message")))
        );
    }

    #[test]
    fn action_entries_target_controls_without_a_registry_command() {
        let mut v = layout();
        v.menus[0].categories[0].entries.push(RibbonEntry::action(
            "bold",
            "b",
            "Bold",
            "format/bold",
            "toggle",
        ));
        let model = build(&v).expect("model");
        assert_eq!(
            Navigator::default().press("b", &model, &registry(), &contexts(&[])),
            Ok(Step::Action {
                control: "format/bold".into(),
                action: "toggle".into()
            })
        );
    }

    #[test]
    fn a_large_vocabulary_compiles_and_projects_quickly() {
        // 2k commands across 200 categories (design §5): build once, project the root.
        let mut registry = Registry::default();
        let mut root = RibbonMenu::new("root", "Root");
        let keys: Vec<char> = ('a'..='z').chain('A'..='Z').chain('0'..='9').collect();
        let mut menus = Vec::new();
        for c in 0..200 {
            let menu_id = format!("m{c}");
            let mut category = RibbonCategory::new(format!("Category {c}"));
            for (e, key) in keys.iter().enumerate().take(10) {
                let command = id(&format!("c{c}.e{e}"));
                registry
                    .register(Command::new(
                        command.clone(),
                        format!("Command {c}.{e}"),
                        (),
                    ))
                    .expect("register");
                category = category.entry(RibbonEntry::command(
                    format!("e{c}-{e}"),
                    key.to_string(),
                    command,
                ));
            }
            menus.push(RibbonMenu::new(menu_id.clone(), format!("Menu {c}")).category(category));
            let key = keys[c % keys.len()].to_string();
            let group = c / keys.len();
            if root.categories.len() <= group {
                root = root.category(RibbonCategory::new(format!("Group {group}")));
            }
            if let Some(category) = root.categories.get_mut(group) {
                category
                    .entries
                    .push(RibbonEntry::menu(format!("open-{c}"), key, menu_id));
            }
        }
        // Keys repeat across root categories, so give each group its own menu instead.
        let mut layout = RibbonLayout::new("root");
        let mut top = RibbonMenu::new("root", "Root");
        for (group, category) in root.categories.into_iter().enumerate() {
            let id = format!("group{group}");
            top = top.category(RibbonCategory::new(format!("Group {group}")).entry(
                RibbonEntry::menu(format!("g{group}"), keys[group].to_string(), id.clone()),
            ));
            let mut sub = RibbonMenu::new(id, format!("Group {group}"));
            sub.categories.push(category);
            layout = layout.menu(sub);
        }
        layout = layout.menu(top);
        for menu in menus {
            layout = layout.menu(menu);
        }
        let started = std::time::Instant::now();
        let model = RibbonModel::build(&layout, &registry, &BTreeMap::new()).expect("model");
        let mut nav = Navigator::default();
        let cx = ContextStack::default();
        let mut worst = std::time::Duration::ZERO;
        for key in keys.iter().take(4) {
            let step = std::time::Instant::now();
            nav.root();
            nav.press(&key.to_string(), &model, &registry, &cx)
                .expect("group");
            nav.press("a", &model, &registry, &cx).expect("menu");
            let sections = model.sections(&nav, &registry, &cx);
            assert_eq!(sections[0].items.len(), 10);
            worst = worst.max(step.elapsed());
        }
        assert_eq!(model.len(), 2_000 + 200 + 4);
        // Generous for debug builds on a loaded machine; the bench holds the real budget.
        assert!(
            started.elapsed().as_millis() < 2_000,
            "{:?}",
            started.elapsed()
        );
        assert!(worst.as_millis() < 50, "{worst:?}");
    }
}
