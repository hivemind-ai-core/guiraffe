//! Spotlight data: actions and their shortcut chips. Plain data, no window needed.
use gpui_kit::SharedString;
use gpui_kit::assets::IconName;

/// A keyboard shortcut shown as keycap chips on the right of a result row.
///
/// Parsed from the labels the commands registry produces (`Keybinding::label`):
/// `"Ctrl+Shift+P"` (one chip per key), `"⌘⇧P"` (one chip per macOS modifier symbol)
/// and chords separated by spaces (`"Ctrl+K Ctrl+S"`).
///
/// ```
/// use guiraffe_overlays::spotlight::Shortcut;
/// let s = Shortcut::parse("Ctrl+K Ctrl+S");
/// assert_eq!(s.chords().len(), 2);
/// assert_eq!(s.chords()[0], ["Ctrl", "K"]);
/// assert_eq!(Shortcut::parse("⌘⇧P").chords()[0], ["⌘", "⇧", "P"]);
/// assert_eq!(Shortcut::parse("Ctrl++").chords()[0], ["Ctrl", "+"]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shortcut {
    chords: Vec<Vec<SharedString>>,
}

const MAC_MODIFIERS: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];

impl Shortcut {
    pub fn parse(label: &str) -> Self {
        let chords = label
            .split_whitespace()
            .map(|chord| {
                let mut keys: Vec<SharedString> = Vec::new();
                if chord.chars().count() > 1 && chord.contains('+') {
                    // "Ctrl++" names the plus key: a trailing empty part is "+".
                    let mut rest = chord;
                    while let Some(at) = rest.find('+') {
                        if at == 0 {
                            keys.push("+".into());
                            rest = &rest[1..];
                            continue;
                        }
                        keys.push(SharedString::from(rest[..at].to_owned()));
                        rest = &rest[at + 1..];
                    }
                    if !rest.is_empty() {
                        keys.push(SharedString::from(rest.to_owned()));
                    }
                } else {
                    let mut rest = chord;
                    while let Some(first) = rest.chars().next()
                        && MAC_MODIFIERS.contains(&first)
                        && rest.chars().count() > 1
                    {
                        keys.push(SharedString::from(first.to_string()));
                        rest = &rest[first.len_utf8()..];
                    }
                    keys.push(SharedString::from(rest.to_owned()));
                }
                keys
            })
            .filter(|keys| !keys.is_empty())
            .collect();
        Self { chords }
    }
    /// Chords in order; each chord is its keys in order.
    pub fn chords(&self) -> &[Vec<SharedString>] {
        &self.chords
    }
    pub fn is_empty(&self) -> bool {
        self.chords.is_empty()
    }
    /// The accessible text, e.g. `"Ctrl Shift P"`.
    pub fn spoken(&self) -> String {
        self.chords
            .iter()
            .map(|keys| {
                keys.iter()
                    .map(|k| k.as_ref())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// One entry of the command bar (Mantine's `SpotlightActionData`).
///
/// ```
/// use guiraffe_overlays::spotlight::SpotlightAction;
/// use gpui_kit::assets::IconName;
/// let action = SpotlightAction::new("git.commit", "Commit staged changes")
///     .description("Record the staged changes in the repository")
///     .group("Git")
///     .keywords("vcs save")
///     .icon(IconName::GitCommitHorizontal)
///     .shortcut("Ctrl+Enter");
/// assert_eq!(action.group.as_deref(), Some("Git"));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SpotlightAction {
    pub id: SharedString,
    pub title: SharedString,
    pub description: Option<SharedString>,
    /// Section the action is listed under; ungrouped actions come first, without a header.
    pub group: Option<SharedString>,
    /// Extra search terms (matched with a lower weight than the title).
    pub keywords: SharedString,
    pub icon: Option<IconName>,
    pub shortcut: Option<Shortcut>,
    /// Listed (dimmed, exposed as disabled) but skipped by navigation and never activated.
    pub disabled: bool,
}

impl SpotlightAction {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            group: None,
            keywords: SharedString::default(),
            icon: None,
            shortcut: None,
            disabled: false,
        }
    }
    pub fn description(mut self, value: impl Into<SharedString>) -> Self {
        self.description = Some(value.into());
        self
    }
    pub fn group(mut self, value: impl Into<SharedString>) -> Self {
        self.group = Some(value.into());
        self
    }
    pub fn keywords(mut self, value: impl Into<SharedString>) -> Self {
        self.keywords = value.into();
        self
    }
    pub fn icon(mut self, value: IconName) -> Self {
        self.icon = Some(value);
        self
    }
    /// A shortcut label, parsed by [`Shortcut::parse`]. An empty label clears it.
    pub fn shortcut(mut self, label: &str) -> Self {
        let parsed = Shortcut::parse(label);
        self.shortcut = (!parsed.is_empty()).then_some(parsed);
        self
    }
    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }
}

impl From<crate::search::Command> for SpotlightAction {
    fn from(command: crate::search::Command) -> Self {
        Self {
            keywords: command.keywords.into(),
            disabled: command.disabled,
            ..Self::new(command.id, command.label)
        }
    }
}

/// User-facing strings, so applications can localize the command bar.
#[derive(Debug, Clone, PartialEq)]
pub struct SpotlightLabels {
    pub placeholder: SharedString,
    pub pinned: SharedString,
    pub recent: SharedString,
    pub empty_title: SharedString,
    pub empty_description: SharedString,
    pub no_results_title: SharedString,
    pub no_results_description: SharedString,
    pub loading: SharedString,
    /// Accessible name of the dialog and listbox.
    pub name: SharedString,
}

impl Default for SpotlightLabels {
    fn default() -> Self {
        Self {
            placeholder: "Search commands…".into(),
            pinned: "Pinned".into(),
            recent: "Recent".into(),
            empty_title: "No commands yet".into(),
            empty_description: "Commands appear here once the app registers them.".into(),
            no_results_title: "No results".into(),
            no_results_description: "Try fewer letters or a different word.".into(),
            loading: "Loading commands…".into(),
            name: "Command palette".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_labels_from_both_platform_conventions() {
        assert_eq!(
            Shortcut::parse("Ctrl+Shift+P").chords(),
            [vec!["Ctrl", "Shift", "P"]
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>()]
        );
        assert_eq!(Shortcut::parse("⌘⇧P").chords()[0], ["⌘", "⇧", "P"]);
        assert_eq!(Shortcut::parse("⌘K ⌘S").chords().len(), 2);
        assert_eq!(Shortcut::parse("+").chords()[0], ["+"]);
        assert_eq!(Shortcut::parse("Ctrl+-").chords()[0], ["Ctrl", "-"]);
        assert_eq!(Shortcut::parse("⇧").chords()[0], ["⇧"]);
        assert!(Shortcut::parse("  ").is_empty());
        assert_eq!(Shortcut::parse("Ctrl+K Ctrl+S").spoken(), "Ctrl K, Ctrl S");
        assert_eq!(SpotlightAction::new("a", "A").shortcut("").shortcut, None);
    }

    #[test]
    fn legacy_commands_convert() {
        let mut command = crate::search::Command::new("open", "Open");
        command.keywords = "file".into();
        command.disabled = true;
        let action = SpotlightAction::from(command);
        assert_eq!(action.id.as_ref(), "open");
        assert_eq!(action.keywords.as_ref(), "file");
        assert!(action.disabled);
    }
}
