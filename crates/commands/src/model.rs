//! Handler-generic registry. This module contains no GPUI types or imports.
use crate::{
    Error,
    context::{ContextStack, When},
    keybinding::{Keybinding, Platform},
    params::Param,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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
    /// Shown by the ribbon and menus when the command is disabled.
    pub disabled_reason: Option<String>,
    /// Required arguments, filled from context, a prompt or the caller, in order.
    pub params: Vec<Param>,
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
            disabled_reason: None,
            params: Vec::new(),
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
    /// Why the command is disabled, shown instead of running it.
    pub fn disabled_reason(mut self, value: impl Into<String>) -> Self {
        self.disabled_reason = Some(value.into());
        self
    }
    /// Append a required argument; see [`Param`].
    pub fn param(mut self, value: Param) -> Self {
        self.params.push(value);
        self
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.title.trim().is_empty() {
            return Err(Error::EmptyTitle);
        }
        crate::params::validate_schema(&self.params)
            .map_err(|reason| Error::InvalidParameter(format!("{}: {reason}", self.id)))
    }
    /// The reason this command cannot run in `contexts`, or `None` when it can.
    pub fn unavailable_reason(&self, contexts: &ContextStack) -> Option<&str> {
        if !self.enabled {
            Some(self.disabled_reason.as_deref().unwrap_or("Disabled"))
        } else if !self.when.matches(contexts) {
            Some("Not available here")
        } else {
            None
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
            disabled_reason: self.disabled_reason,
            params: self.params,
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
    platform: Platform,
    /// Platform-resolved canonical binding -> commands bound to it, in catalog order.
    /// Maintained on every mutation so a keystroke never scans the catalog.
    index: HashMap<String, Vec<CommandId>>,
}
impl<Run> Default for Registry<Run> {
    fn default() -> Self {
        Self::with_platform(Platform::current())
    }
}
impl<Run> Registry<Run> {
    /// A registry that resolves `secondary` bindings for `platform`.
    pub fn with_platform(platform: Platform) -> Self {
        Self {
            commands: BTreeMap::new(),
            order: BTreeSet::new(),
            platform,
            index: HashMap::new(),
        }
    }
    pub fn platform(&self) -> Platform {
        self.platform
    }
    fn index_insert(&mut self, command: &Command<Run>) {
        let Some(binding) = &command.keybinding else {
            return;
        };
        let key = order(command);
        let commands = &self.commands;
        let ids = self
            .index
            .entry(binding.resolve(self.platform).as_str().to_owned())
            .or_default();
        // Keep each bucket in catalog order so ties resolve without sorting per keystroke.
        let at = ids
            .iter()
            .position(|id| commands.get(id).is_some_and(|other| order(other) > key))
            .unwrap_or(ids.len());
        ids.insert(at, command.id.clone());
    }
    fn index_remove(&mut self, command: &Command<Run>) {
        let Some(binding) = &command.keybinding else {
            return;
        };
        let key = binding.resolve(self.platform);
        if let Some(ids) = self.index.get_mut(key.as_str()) {
            ids.retain(|id| id != &command.id);
            if ids.is_empty() {
                self.index.remove(key.as_str());
            }
        }
    }
    pub fn register(&mut self, command: Command<Run>) -> Result<(), Error> {
        command.validate()?;
        if self.commands.contains_key(&command.id) {
            return Err(Error::Duplicate(command.id.to_string()));
        }
        self.order.insert(order(&command));
        self.index_insert(&command);
        self.commands.insert(command.id.clone(), command);
        Ok(())
    }
    pub fn unregister(&mut self, id: &CommandId) -> Option<Command<Run>> {
        let command = self.commands.remove(id)?;
        self.order.remove(&order(&command));
        self.index_remove(&command);
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
        let Some(command) = self.commands.remove(id) else {
            return Err(Error::NotFound(id.to_string()));
        };
        if command.keybinding == binding {
            self.commands.insert(id.clone(), command);
            return Ok(false);
        }
        self.index_remove(&command);
        let command = Command {
            keybinding: binding,
            ..command
        };
        self.index_insert(&command);
        self.commands.insert(id.clone(), command);
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
        let id = command.id.clone();
        self.unregister(&id);
        self.register(command)?;
        Ok(true)
    }
    /// Every command bound to `binding` (resolved for this registry's platform), in
    /// catalog order, enabled or not. An index lookup, not a catalog scan.
    pub fn bound_to(&self, binding: &Keybinding) -> impl Iterator<Item = &Command<Run>> {
        let resolved = binding.resolve(self.platform);
        self.index
            .get(resolved.as_str())
            .into_iter()
            .flatten()
            .filter_map(|id| self.commands.get(id))
    }
    /// As [`Self::bound_to`], keyed by a platform-resolved canonical string (the form
    /// carried by native routes), so dispatch does not re-parse the keystroke.
    pub fn bound_to_native<'a>(
        &'a self,
        resolved: &str,
    ) -> impl Iterator<Item = &'a Command<Run>> + 'a {
        self.index
            .get(resolved)
            .into_iter()
            .flatten()
            .filter_map(|id| self.commands.get(id))
    }
    /// Matching commands for one shortcut in precedence order: deepest matching `when`
    /// context first (specificity), catalog order breaking ties. Disabled commands are
    /// skipped, so they never swallow a key that a shadowed command could handle.
    pub fn candidates_native<'a>(
        &'a self,
        resolved: &str,
        contexts: &ContextStack,
    ) -> Vec<&'a Command<Run>> {
        let mut ranked = self
            .bound_to_native(resolved)
            .filter(|command| command.enabled)
            .filter_map(|command| command.when.depth(contexts).map(|d| (d, command)))
            .collect::<Vec<_>>();
        // Stable sort keeps catalog order for equal depth.
        ranked.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
        ranked.into_iter().map(|(_, command)| command).collect()
    }
    /// Resolve one shortcut: the most specific enabled, active command.
    pub fn resolve_binding(
        &self,
        binding: &Keybinding,
        contexts: &ContextStack,
    ) -> Option<&Command<Run>> {
        let resolved = binding.resolve(self.platform);
        self.resolve_native(resolved.as_str(), contexts)
    }
    /// As [`Self::resolve_binding`] for a platform-resolved canonical string. Allocation
    /// free: one index lookup plus one predicate evaluation per bound command.
    pub fn resolve_native(&self, resolved: &str, contexts: &ContextStack) -> Option<&Command<Run>> {
        let mut best: Option<(usize, &Command<Run>)> = None;
        for command in self.bound_to_native(resolved) {
            if !command.enabled {
                continue;
            }
            let Some(depth) = command.when.depth(contexts) else {
                continue;
            };
            // Buckets are in catalog order: only a strictly deeper match replaces.
            if best.is_none_or(|(d, _)| depth > d) {
                best = Some((depth, command));
            }
        }
        best.map(|(_, command)| command)
    }
    /// The effective keymap for `contexts`: per platform-resolved binding the winner
    /// (specificity, then catalog order), exact conflicts with every shadowed ID, and
    /// chord-prefix conflicts where a shorter binding shadows (delays) a longer chord.
    pub fn bindings<'a>(&'a self, contexts: &'a ContextStack) -> Bindings<'a, Run> {
        let mut winners: BTreeMap<Keybinding, &Command<Run>> = BTreeMap::new();
        let mut conflicts = Vec::new();
        for key in self.index.keys() {
            let ranked = self.candidates_native(key, contexts);
            let Some((winner, shadowed)) = ranked.split_first() else {
                continue;
            };
            let Some(binding) = winner.keybinding.as_ref().map(|b| b.resolve(self.platform)) else {
                continue;
            };
            if !shadowed.is_empty() {
                conflicts.push(Conflict {
                    binding: binding.clone(),
                    winner: winner.id.clone(),
                    shadowed: shadowed.iter().map(|c| c.id.clone()).collect(),
                });
            }
            winners.insert(binding, *winner);
        }
        conflicts.sort_by(|a, b| a.binding.cmp(&b.binding));
        let mut prefixes = Vec::new();
        for (prefix, short) in &winners {
            for (chord, long) in &winners {
                if prefix != chord && prefix.is_prefix_of(chord) {
                    prefixes.push(PrefixConflict {
                        prefix: prefix.clone(),
                        prefix_command: short.id.clone(),
                        chord: chord.clone(),
                        chord_command: long.id.clone(),
                    });
                }
            }
        }
        Bindings {
            winners,
            conflicts,
            prefixes,
        }
    }
}
pub struct Bindings<'a, Run> {
    pub winners: BTreeMap<Keybinding, &'a Command<Run>>,
    pub conflicts: Vec<Conflict>,
    /// Active bindings that are a strict chord prefix of another active binding.
    pub prefixes: Vec<PrefixConflict>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub binding: Keybinding,
    pub winner: CommandId,
    pub shadowed: Vec<CommandId>,
}
/// `prefix` (e.g. `ctrl+k`) is the first chord of `chord` (e.g. `ctrl+k ctrl+s`): pressing
/// it waits for the next keystroke, so `prefix_command` runs only after a timeout or a
/// non-matching key instead of immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixConflict {
    pub prefix: Keybinding,
    pub prefix_command: CommandId,
    pub chord: Keybinding,
    pub chord_command: CommandId,
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
    fn bound(value: &str, binding: &str, when: Option<&str>) -> Command {
        let c = command(value, value, "").keybinding(Keybinding::parse(binding).expect("binding"));
        match when {
            Some(w) => c.when(When::parse(w).expect("when")),
            None => c,
        }
    }
    fn stack(frames: &[&str]) -> ContextStack {
        let mut s = ContextStack::default();
        for f in frames {
            s.push(crate::context::Context::new(f).expect("frame"));
        }
        s
    }
    #[test]
    fn deepest_matching_context_wins_over_catalog_order() {
        // "a-global" and "b-workspace" sort before "c-editor" in the catalog, but the
        // focused Editor frame is deeper, so its binding is the most specific.
        let mut r = Registry::with_platform(Platform::Other);
        r.register(bound("a-global", "ctrl+s", None)).expect("a");
        r.register(bound("b-workspace", "ctrl+s", Some("Workspace")))
            .expect("b");
        r.register(bound("c-editor", "ctrl+s", Some("Editor")))
            .expect("c");
        let s = stack(&["Workspace", "Editor"]);
        let key = Keybinding::parse("ctrl+s").expect("binding");
        assert_eq!(
            r.resolve_binding(&key, &s).map(|c| c.id.as_str()),
            Some("c-editor")
        );
        let bindings = r.bindings(&s);
        let conflict = bindings.conflicts.first().expect("conflict");
        assert_eq!(conflict.winner, id("c-editor"));
        assert_eq!(conflict.shadowed, vec![id("b-workspace"), id("a-global")]);
        // Outside the editor the workspace binding is the most specific.
        let s = stack(&["Workspace"]);
        assert_eq!(
            r.resolve_binding(&key, &s).map(|c| c.id.as_str()),
            Some("b-workspace")
        );
        // Equal depth: catalog order breaks the tie.
        let s = stack(&["Workspace Editor"]);
        assert_eq!(
            r.resolve_binding(&key, &s).map(|c| c.id.as_str()),
            Some("b-workspace")
        );
    }
    #[test]
    fn disabled_most_specific_command_falls_through() {
        let mut r = Registry::with_platform(Platform::Other);
        r.register(bound("a-global", "ctrl+s", None)).expect("a");
        r.register(bound("b-editor", "ctrl+s", Some("Editor")).enabled(false))
            .expect("b");
        let s = stack(&["Editor"]);
        let key = Keybinding::parse("ctrl+s").expect("binding");
        assert_eq!(
            r.resolve_binding(&key, &s).map(|c| c.id.as_str()),
            Some("a-global"),
            "a disabled winner must not swallow the key"
        );
        assert_eq!(
            r.candidates_native("ctrl+s", &s)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["a-global"]
        );
        r.set_enabled(&id("a-global"), false).expect("disable");
        assert!(r.resolve_binding(&key, &s).is_none());
    }
    #[test]
    fn secondary_resolves_per_platform_and_conflicts_with_the_physical_key() {
        for (platform, physical, other) in [
            (Platform::Mac, "cmd+s", "ctrl+s"),
            (Platform::Other, "ctrl+s", "cmd+s"),
        ] {
            let mut r = Registry::with_platform(platform);
            r.register(bound("a-neutral", "secondary+s", None))
                .expect("a");
            r.register(bound("b-physical", physical, None)).expect("b");
            r.register(bound("c-other", other, None)).expect("c");
            let s = ContextStack::default();
            let bindings = r.bindings(&s);
            assert_eq!(bindings.conflicts.len(), 1, "{platform:?}");
            assert_eq!(bindings.conflicts[0].binding.as_str(), physical);
            assert_eq!(bindings.conflicts[0].shadowed, vec![id("b-physical")]);
            let key = Keybinding::parse("mod+s").expect("alias");
            assert_eq!(
                r.resolve_binding(&key, &s).map(|c| c.id.as_str()),
                Some("a-neutral")
            );
        }
    }
    #[test]
    fn chord_prefixes_are_reported_for_active_bindings_only() {
        let mut r = Registry::with_platform(Platform::Other);
        r.register(bound("a-palette", "ctrl+k", None)).expect("a");
        r.register(bound("b-settings", "ctrl+k ctrl+s", None))
            .expect("b");
        r.register(bound("c-term", "ctrl+t", Some("Terminal")))
            .expect("c");
        r.register(bound("d-term-chord", "ctrl+t ctrl+n", None))
            .expect("d");
        let s = stack(&["Editor"]);
        let prefixes = r.bindings(&s).prefixes;
        assert_eq!(
            prefixes,
            vec![PrefixConflict {
                prefix: Keybinding::parse("ctrl+k").expect("k"),
                prefix_command: id("a-palette"),
                chord: Keybinding::parse("ctrl+k ctrl+s").expect("chord"),
                chord_command: id("b-settings"),
            }],
            "the Terminal-only prefix is inactive in the editor"
        );
        assert_eq!(r.bindings(&stack(&["Terminal"])).prefixes.len(), 2);
    }
    #[test]
    fn binding_index_follows_every_mutation() {
        let mut r = Registry::with_platform(Platform::Other);
        let s = ContextStack::default();
        let save = Keybinding::parse("ctrl+s").expect("s");
        let open = Keybinding::parse("ctrl+o").expect("o");
        r.register(bound("a", "ctrl+s", None)).expect("a");
        assert_eq!(r.bound_to(&save).count(), 1);
        r.set_keybinding(&id("a"), Some(open.clone()))
            .expect("rebind");
        assert_eq!(r.bound_to(&save).count(), 0, "old key no longer indexed");
        assert!(r.resolve_binding(&save, &s).is_none());
        assert_eq!(
            r.resolve_binding(&open, &s).map(|c| c.id.as_str()),
            Some("a")
        );
        let mut changed = r.lookup(&id("a")).cloned().expect("a");
        changed.keybinding = Some(save.clone());
        r.replace(changed).expect("replace");
        assert_eq!(r.bound_to(&open).count(), 0);
        assert_eq!(r.bound_to(&save).count(), 1);
        r.set_keybinding(&id("a"), None).expect("clear");
        assert_eq!(r.bound_to(&save).count(), 0);
        r.set_keybinding(&id("a"), Some(save.clone()))
            .expect("again");
        r.unregister(&id("a"));
        assert_eq!(r.bound_to(&save).count(), 0);
        assert!(r.index.is_empty(), "empty buckets are dropped");
    }
}
