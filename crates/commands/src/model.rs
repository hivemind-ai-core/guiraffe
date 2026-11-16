//! Handler-generic registry. This module contains no GPUI types or imports.
use crate::{
    Error,
    context::{ContextStack, When},
    keybinding::Keybinding,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct CommandId(String);
impl CommandId {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let mut segment_start = true;
        for c in value.chars() {
            if matches!(c, '.' | '-' | '_') {
                if segment_start {
                    return Err(Error::InvalidId(value.into()));
                }
                segment_start = true;
            } else if c.is_ascii_lowercase() || (!segment_start && c.is_ascii_digit()) {
                segment_start = false;
            } else {
                return Err(Error::InvalidId(value.into()));
            }
        }
        if segment_start || value.len() > 256 {
            return Err(Error::InvalidId(value.into()));
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl FromStr for CommandId {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Error> {
        Self::parse(value)
    }
}
impl TryFrom<String> for CommandId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::parse(&value)
    }
}
impl From<CommandId> for String {
    fn from(value: CommandId) -> Self {
        value.0
    }
}
impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command<Run = ()> {
    pub id: CommandId,
    pub title: String,
    pub category: String,
    pub keywords: String,
    pub keybinding: Option<Keybinding>,
    pub when: When,
    pub enabled: bool,
    pub run: Run,
}
impl<Run> Command<Run> {
    pub fn new(id: CommandId, title: impl Into<String>, run: Run) -> Self {
        Self {
            id,
            title: title.into(),
            category: String::new(),
            keywords: String::new(),
            keybinding: None,
            when: When::always(),
            enabled: true,
            run,
        }
    }
    pub fn category(mut self, value: impl Into<String>) -> Self {
        self.category = value.into();
        self
    }
    pub fn keywords(mut self, value: impl Into<String>) -> Self {
        self.keywords = value.into();
        self
    }
    pub fn keybinding(mut self, value: Keybinding) -> Self {
        self.keybinding = Some(value);
        self
    }
    pub fn when(mut self, value: When) -> Self {
        self.when = value;
        self
    }
    pub fn enabled(mut self, value: bool) -> Self {
        self.enabled = value;
        self
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.title.trim().is_empty() {
            Err(Error::EmptyTitle)
        } else {
            Ok(())
        }
    }
    pub fn map_run<Other>(self, f: impl FnOnce(Run) -> Other) -> Command<Other> {
        Command {
            id: self.id,
            title: self.title,
            category: self.category,
            keywords: self.keywords,
            keybinding: self.keybinding,
            when: self.when,
            enabled: self.enabled,
            run: f(self.run),
        }
    }
}
type OrderKey = (String, String, CommandId);
fn order<Run>(command: &Command<Run>) -> OrderKey {
    (
        command.category.to_lowercase(),
        command.title.to_lowercase(),
        command.id.clone(),
    )
}
#[derive(Debug, Clone)]
pub struct Registry<Run = ()> {
    commands: BTreeMap<CommandId, Command<Run>>,
    order: BTreeSet<OrderKey>,
}
impl<Run> Default for Registry<Run> {
    fn default() -> Self {
        Self {
            commands: BTreeMap::new(),
            order: BTreeSet::new(),
        }
    }
}
impl<Run> Registry<Run> {
    pub fn register(&mut self, command: Command<Run>) -> Result<(), Error> {
        command.validate()?;
        if self.commands.contains_key(&command.id) {
            return Err(Error::Duplicate(command.id.to_string()));
        }
        self.order.insert(order(&command));
        self.commands.insert(command.id.clone(), command);
        Ok(())
    }
    pub fn unregister(&mut self, id: &CommandId) -> Option<Command<Run>> {
        let command = self.commands.remove(id)?;
        self.order.remove(&order(&command));
        Some(command)
    }
    pub fn lookup(&self, id: &CommandId) -> Option<&Command<Run>> {
        self.commands.get(id)
    }
    pub fn len(&self) -> usize {
        self.commands.len()
    }
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
    pub fn ordered(&self) -> impl Iterator<Item = &Command<Run>> {
        self.order
            .iter()
            .filter_map(|(_, _, id)| self.commands.get(id))
    }
    /// Disabled entries remain visible to menus and keymap editors.
    pub fn visible<'a>(
        &'a self,
        contexts: &'a ContextStack,
    ) -> impl Iterator<Item = &'a Command<Run>> {
        self.ordered()
            .filter(|command| command.when.matches(contexts))
    }
    pub fn set_enabled(&mut self, id: &CommandId, enabled: bool) -> Result<bool, Error> {
        let command = self
            .commands
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        if command.enabled == enabled {
            return Ok(false);
        }
        command.enabled = enabled;
        Ok(true)
    }
    pub fn set_keybinding(
        &mut self,
        id: &CommandId,
        binding: Option<Keybinding>,
    ) -> Result<bool, Error> {
        let command = self
            .commands
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        if command.keybinding == binding {
            return Ok(false);
        }
        command.keybinding = binding;
        Ok(true)
    }
    pub fn replace(&mut self, command: Command<Run>) -> Result<bool, Error>
    where
        Run: PartialEq,
    {
        command.validate()?;
        let old = self
            .lookup(&command.id)
            .ok_or_else(|| Error::NotFound(command.id.to_string()))?;
        if old == &command {
            return Ok(false);
        }
        self.order.remove(&order(old));
        self.order.insert(order(&command));
        self.commands.insert(command.id.clone(), command);
        Ok(true)
    }
    /// Resolve one shortcut without allocating the complete keymap/conflict report.
    pub fn resolve_binding(
        &self,
        binding: &Keybinding,
        contexts: &ContextStack,
    ) -> Option<&Command<Run>> {
        self.ordered().find(|command| {
            command.enabled
                && command.keybinding.as_ref() == Some(binding)
                && command.when.matches(contexts)
        })
    }
    /// Resolve exact binding conflicts among enabled, reachable commands. The first
    /// catalog entry (category/title/id) wins, independently of registration order.
    pub fn bindings<'a>(&'a self, contexts: &'a ContextStack) -> Bindings<'a, Run> {
        let mut winners: BTreeMap<Keybinding, &Command<Run>> = BTreeMap::new();
        let mut conflicts: BTreeMap<Keybinding, Conflict> = BTreeMap::new();
        for command in self.visible(contexts).filter(|command| command.enabled) {
            let Some(binding) = &command.keybinding else {
                continue;
            };
            if let Some(winner) = winners.get(binding) {
                conflicts
                    .entry(binding.clone())
                    .or_insert_with(|| Conflict {
                        binding: binding.clone(),
                        winner: winner.id.clone(),
                        shadowed: Vec::new(),
                    })
                    .shadowed
                    .push(command.id.clone());
            } else {
                winners.insert(binding.clone(), command);
            }
        }
        Bindings {
            winners,
            conflicts: conflicts.into_values().collect(),
        }
    }
}
pub struct Bindings<'a, Run> {
    pub winners: BTreeMap<Keybinding, &'a Command<Run>>,
    pub conflicts: Vec<Conflict>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub binding: Keybinding,
    pub winner: CommandId,
    pub shadowed: Vec<CommandId>,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn id(value: &str) -> CommandId {
        CommandId::parse(value).expect("valid id")
    }
    fn command(value: &str, title: &str, category: &str) -> Command {
        Command::new(id(value), title, ()).category(category)
    }
    #[test]
    fn slugs_validate_every_segment() {
        for value in ["file.save", "editor-format", "app_open", "a", "a1.b2"] {
            assert!(CommandId::parse(value).is_ok());
        }
        for value in ["", "A", "a b", "1a", "a..b", "a-", ".a", "a.2b", "é", "a/b"] {
            assert!(CommandId::parse(value).is_err(), "{value}");
        }
    }
    #[test]
    fn ordering_duplicate_and_removal() {
        let mut r = Registry::default();
        for cmd in [
            command("z", "Zulu", "File"),
            command("b", "Beta", "Edit"),
            command("a", "Alpha", "File"),
        ] {
            r.register(cmd).expect("register");
        }
        assert_eq!(
            r.ordered().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["b", "a", "z"]
        );
        assert!(matches!(
            r.register(command("a", "Changed", "Other")),
            Err(Error::Duplicate(_))
        ));
        assert_eq!(r.len(), 3);
        assert_eq!(r.lookup(&id("a")).map(|c| c.title.as_str()), Some("Alpha"));
        assert!(r.unregister(&id("a")).is_some());
        assert!(r.unregister(&id("a")).is_none());
        assert_eq!(r.len(), 2);
    }
    #[test]
    fn binding_winner_is_independent_of_registration_order() {
        for reverse in [false, true] {
            let mut r = Registry::default();
            let mut commands = vec![
                command("b", "Beta", "File"),
                command("a", "Alpha", "File"),
                command("c", "Gamma", "File"),
            ];
            if reverse {
                commands.reverse();
            }
            for command in commands {
                r.register(command.keybinding(Keybinding::parse("ctrl+s").expect("binding")))
                    .expect("register");
            }
            let contexts = ContextStack::default();
            let bindings = r.bindings(&contexts);
            assert_eq!(
                bindings.winners.values().next().map(|c| c.id.as_str()),
                Some("a")
            );
            let conflict = bindings.conflicts.first().expect("conflict");
            assert_eq!(conflict.winner, id("a"));
            assert_eq!(conflict.shadowed, vec![id("b"), id("c")]);
        }
    }
    #[test]
    fn disabled_and_disjoint_contexts_do_not_conflict() {
        let mut r = Registry::default();
        for (value, when, enabled) in [
            ("a", "Editor", true),
            ("b", "Terminal", true),
            ("c", "Editor", false),
        ] {
            r.register(
                command(value, value, "")
                    .keybinding(Keybinding::parse("ctrl+s").expect("binding"))
                    .when(When::parse(when).expect("context"))
                    .enabled(enabled),
            )
            .expect("register");
        }
        let mut context = ContextStack::default();
        context.push(crate::context::Context::new("Editor").expect("frame"));
        assert_eq!(r.visible(&context).count(), 2);
        let bindings = r.bindings(&context);
        assert_eq!(bindings.winners.len(), 1);
        assert!(bindings.conflicts.is_empty());
    }
    #[test]
    fn mutations_are_change_checked_and_reindex() {
        let mut r = Registry::default();
        let command = command("a", "Zulu", "Z");
        r.register(command.clone()).expect("register");
        assert!(!r.replace(command.clone()).expect("noop"));
        assert!(!r.set_enabled(&id("a"), true).expect("noop"));
        assert!(r.set_enabled(&id("a"), false).expect("change"));
        assert!(
            r.set_keybinding(&id("a"), Some(Keybinding::parse("cmd+k").expect("binding")))
                .expect("change")
        );
        assert!(
            !r.set_keybinding(&id("a"), Some(Keybinding::parse("CMD+K").expect("binding")))
                .expect("noop")
        );
        assert!(r.replace(command.category("A")).expect("replace"));
        assert_eq!(r.ordered().next().map(|c| c.category.as_str()), Some("A"));
        assert!(matches!(
            r.set_enabled(&id("missing"), true),
            Err(Error::NotFound(_))
        ));
    }
    #[test]
    fn failed_registration_and_replacement_are_atomic() {
        let mut r = Registry::default();
        assert_eq!(
            r.register(command("empty", " ", "")),
            Err(Error::EmptyTitle)
        );
        assert!(r.is_empty());
        r.register(command("a", "Valid", "")).expect("register");
        assert_eq!(r.replace(command("a", "", "")), Err(Error::EmptyTitle));
        assert_eq!(r.lookup(&id("a")).map(|c| c.title.as_str()), Some("Valid"));
    }
}
