//! Command parameters and drafts. Pure: no GPUI types.
//!
//! A command declares required, string-valued parameters. A [`Draft`] collects them from the
//! context (a valid selection), a prompt or the caller, and never runs until complete and
//! revalidated against the current context and choice catalogues.
use crate::{Command, CommandId, ContextStack};
use std::collections::{BTreeMap, BTreeSet};

/// One selectable value: a stable id and the label people see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}
impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamKind {
    /// Non-blank text of at most `max_chars` characters.
    Text { max_chars: usize },
    /// One of a fixed list.
    Choice(Vec<Choice>),
    /// One of a host-provided catalogue, looked up by key in [`Choices`] each time.
    Dynamic(String),
}

/// A required argument of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub id: String,
    pub label: String,
    pub kind: ParamKind,
    /// Context key whose value fills this parameter when it is currently valid.
    /// An invalid or missing value prompts; the first choice is never picked silently.
    pub from_context: Option<String>,
}
impl Param {
    pub fn text(id: impl Into<String>, label: impl Into<String>, max_chars: usize) -> Self {
        Self::new(id, label, ParamKind::Text { max_chars })
    }
    pub fn choice(id: impl Into<String>, label: impl Into<String>, choices: Vec<Choice>) -> Self {
        Self::new(id, label, ParamKind::Choice(choices))
    }
    pub fn dynamic(
        id: impl Into<String>,
        label: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::new(id, label, ParamKind::Dynamic(source.into()))
    }
    fn new(id: impl Into<String>, label: impl Into<String>, kind: ParamKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            from_context: None,
        }
    }
    /// Fill from this context key when its value is valid.
    pub fn from_context(mut self, key: impl Into<String>) -> Self {
        self.from_context = Some(key.into());
        self
    }
    pub fn choices<'a>(&'a self, catalogues: &'a Choices) -> &'a [Choice] {
        match &self.kind {
            ParamKind::Choice(choices) => choices,
            ParamKind::Dynamic(key) => catalogues.get(key),
            ParamKind::Text { .. } => &[],
        }
    }
    pub fn accepts(&self, value: &str, catalogues: &Choices) -> bool {
        match &self.kind {
            ParamKind::Text { max_chars } => {
                !value.trim().is_empty() && value.chars().count() <= *max_chars
            }
            _ => self.choices(catalogues).iter().any(|c| c.value == value),
        }
    }
}

/// Host-provided dynamic choice catalogues, keyed by [`ParamKind::Dynamic`] source.
/// Ids within a catalogue must be unique; a duplicate keeps the first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choices(BTreeMap<String, Vec<Choice>>);
impl Choices {
    pub fn set(&mut self, key: impl Into<String>, mut choices: Vec<Choice>) {
        let mut seen = BTreeSet::new();
        choices.retain(|c| seen.insert(c.value.clone()));
        self.0.insert(key.into(), choices);
    }
    pub fn get(&self, key: &str) -> &[Choice] {
        self.0.get(key).map(Vec::as_slice).unwrap_or_default()
    }
}

/// Schema checks run when a command is registered.
pub(crate) fn validate_schema(params: &[Param]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for param in params {
        if param.id.trim().is_empty() || !ids.insert(param.id.as_str()) {
            return Err(format!("empty or duplicate parameter id {:?}", param.id));
        }
        if param.label.trim().is_empty() {
            return Err(format!("parameter {} has no label", param.id));
        }
        match &param.kind {
            ParamKind::Text { max_chars: 0 } => {
                return Err(format!(
                    "text parameter {} needs a positive limit",
                    param.id
                ));
            }
            ParamKind::Choice(choices) => {
                let mut values = BTreeSet::new();
                if choices.is_empty()
                    || choices
                        .iter()
                        .any(|c| c.value.is_empty() || !values.insert(c.value.as_str()))
                {
                    return Err(format!(
                        "parameter {} has empty or duplicate choices",
                        param.id
                    ));
                }
            }
            ParamKind::Dynamic(key) if key.trim().is_empty() => {
                return Err(format!("parameter {} has no choice source", param.id));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Arguments being collected for one command. Values are checked as they are set, and the
/// whole draft is revalidated by [`validate`] before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    command: CommandId,
    args: BTreeMap<String, String>,
}
impl Draft {
    /// Start a draft, filling parameters whose context value is currently valid.
    pub fn prepare<Run>(
        command: &Command<Run>,
        contexts: &ContextStack,
        choices: &Choices,
    ) -> Self {
        let mut args = BTreeMap::new();
        for param in &command.params {
            if let Some(value) = param
                .from_context
                .as_deref()
                .and_then(|key| contexts.value(key))
                .filter(|value| param.accepts(value, choices))
            {
                args.insert(param.id.clone(), value.to_owned());
            }
        }
        Self {
            command: command.id.clone(),
            args,
        }
    }
    pub fn command(&self) -> &CommandId {
        &self.command
    }
    pub fn args(&self) -> &BTreeMap<String, String> {
        &self.args
    }
    /// The first parameter still missing, in declaration order.
    pub fn next<'a, Run>(&self, command: &'a Command<Run>) -> Option<&'a Param> {
        command
            .params
            .iter()
            .find(|param| !self.args.contains_key(&param.id))
    }
    /// Set one value; an unknown parameter or unacceptable value leaves the draft unchanged.
    pub fn set<Run>(
        &mut self,
        command: &Command<Run>,
        id: &str,
        value: &str,
        choices: &Choices,
    ) -> Result<(), String> {
        let param = command
            .params
            .iter()
            .find(|param| param.id == id)
            .ok_or_else(|| format!("unknown parameter {id}"))?;
        if !param.accepts(value, choices) {
            return Err(format!("invalid or unavailable value for {}", param.label));
        }
        self.args.insert(id.to_owned(), value.to_owned());
        Ok(())
    }
    /// Drop values that are no longer valid, so a retained prompt asks again.
    pub fn refresh<Run>(&mut self, command: &Command<Run>, choices: &Choices) {
        self.args.retain(|id, value| {
            command
                .params
                .iter()
                .find(|param| &param.id == id)
                .is_some_and(|param| param.accepts(value, choices))
        });
    }
}

/// Every parameter present, valid and nothing extra. Run again at consumption time.
pub fn validate<Run>(
    command: &Command<Run>,
    args: &BTreeMap<String, String>,
    choices: &Choices,
) -> Result<(), String> {
    if args.len() != command.params.len() {
        return Err("missing or extra arguments".into());
    }
    for param in &command.params {
        if !args
            .get(&param.id)
            .is_some_and(|value| param.accepts(value, choices))
        {
            return Err(format!("invalid or unavailable value for {}", param.label));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    fn archive() -> Command {
        Command::new(
            CommandId::parse("records.archive").expect("id"),
            "Archive",
            (),
        )
        .param(Param::dynamic("record", "Record", "records").from_context("selected"))
    }
    fn message() -> Command {
        Command::new(CommandId::parse("message").expect("id"), "Message", ())
            .param(Param::text("text", "Text", 8))
    }
    fn catalogue() -> Choices {
        let mut choices = Choices::default();
        choices.set(
            "records",
            vec![Choice::new("r1", "First"), Choice::new("r2", "Second")],
        );
        choices
    }
    fn selected(value: &str) -> ContextStack {
        let mut stack = ContextStack::default();
        stack.push(Context::parse(&format!("Records selected={value}")).expect("frame"));
        stack
    }

    #[test]
    fn selected_target_is_used_only_when_currently_valid() {
        let command = archive();
        let draft = Draft::prepare(&command, &selected("r1"), &catalogue());
        assert_eq!(draft.args()["record"], "r1");
        assert!(draft.next(&command).is_none());
        let draft = Draft::prepare(&command, &selected("deleted"), &catalogue());
        assert_eq!(draft.next(&command).map(|p| p.id.as_str()), Some("record"));
    }

    #[test]
    fn parameters_are_validated_when_set_and_before_running() {
        let command = archive();
        let mut choices = catalogue();
        let mut draft = Draft::prepare(&command, &ContextStack::default(), &choices);
        assert!(draft.set(&command, "record", "unknown", &choices).is_err());
        draft
            .set(&command, "record", "r1", &choices)
            .expect("valid");
        assert!(validate(&command, draft.args(), &choices).is_ok());
        choices.set("records", vec![Choice::new("r2", "Second")]);
        assert!(validate(&command, draft.args(), &choices).is_err());
        draft.refresh(&command, &choices);
        assert!(draft.next(&command).is_some());
    }

    #[test]
    fn unicode_text_limits_and_unknown_parameters_are_checked() {
        let command = message();
        let choices = Choices::default();
        let mut draft = Draft::prepare(&command, &ContextStack::default(), &choices);
        assert!(draft.set(&command, "text", "   ", &choices).is_err());
        assert!(draft.set(&command, "text", "123456789", &choices).is_err());
        assert!(draft.set(&command, "other", "ok", &choices).is_err());
        draft
            .set(&command, "text", "日本語✓", &choices)
            .expect("fits");
        assert!(validate(&command, draft.args(), &choices).is_ok());
        assert!(validate(&command, &BTreeMap::new(), &choices).is_err());
    }

    #[test]
    fn invalid_schemas_are_rejected_at_registration() {
        let id = || CommandId::parse("a").expect("id");
        for param in [
            Param::text("t", "T", 0),
            Param::choice("c", "C", vec![]),
            Param::choice("c", "C", vec![Choice::new("x", "X"), Choice::new("x", "Y")]),
            Param::dynamic("d", "D", " "),
            Param::text("", "T", 3),
        ] {
            assert!(Command::new(id(), "A", ()).param(param).validate().is_err());
        }
        let twice = Command::new(id(), "A", ())
            .param(Param::text("t", "T", 3))
            .param(Param::text("t", "U", 3));
        assert!(twice.validate().is_err());
    }
}
