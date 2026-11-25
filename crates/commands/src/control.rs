//! Commands as clients of the action bus (control-protocol design, section 6).
//!
//! A command can be "apply action X to control Y" with no closure ([`crate::Handler::apply`]),
//! and every registered control action is addressable as the command
//! `control.<path>.<action>` without registering anything. Spotlight lists them when it
//! opens; a shortcut binds one with [`crate::Commands::control_command`]. Nothing is
//! enumerated or allocated for controls until a palette opens or a binding is made.
use crate::{Command, CommandId, Commands, Error, Handler};
use gpui_kit::{App, SharedString, Window};
use guiraffe_core::{
    protocol::{Action, ActionBus, ActionError, ControlId, Source},
    widget_value::Value,
};

/// Command-id prefix for control actions.
pub const PREFIX: &str = "control.";

/// A declarative command target: the action to apply and the control path to apply it to.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlAction {
    pub path: SharedString,
    pub action: Action,
}

impl ControlAction {
    /// `control.<path>.<action>`, with a `Null` payload (the shared bindable actions).
    pub fn parse(id: &str) -> Option<Self> {
        let (path, name) = id.strip_prefix(PREFIX)?.rsplit_once('.')?;
        if path.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self {
            path: path.to_owned().into(),
            action: Action {
                name: name.to_owned().into(),
                payload: Value::Null,
            },
        })
    }
    /// The command id this target answers to.
    pub fn command_id(path: &str, action: &str) -> String {
        format!("{PREFIX}{path}.{action}")
    }
    /// Resolve the path in `window` and dispatch with `source`.
    pub fn dispatch(&self, source: Source, window: &mut Window, cx: &mut App) -> Result<(), Error> {
        let id = ControlId::lookup(&self.path, cx)
            .ok_or_else(|| rejected(&self.path, ActionError::UnknownTarget))?;
        ActionBus::dispatch(id, &self.action, source, window, cx)
            .map(drop)
            .map_err(|error| rejected(&self.path, error))
    }
}

fn rejected(path: &str, error: ActionError) -> Error {
    Error::Action(format!("{path}: {error:?}"))
}

/// "set_value" -> "Set value".
fn title(action: &str) -> String {
    let mut out = String::with_capacity(action.len());
    for (i, c) in action.chars().enumerate() {
        match (i, c) {
            (0, c) => out.extend(c.to_uppercase()),
            (_, '_') => out.push(' '),
            (_, c) => out.push(c),
        }
    }
    out
}

/// The control's display name: its snapshot label, else the last path segment.
fn label(id: ControlId, path: &str, window: &Window, cx: &App) -> String {
    ActionBus::query(id, window, cx)
        .ok()
        .and_then(|snapshot| snapshot.label)
        .map(|label| label.to_string())
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned())
}

/// Catalogue metadata for a control kind: the widget's summary, when the kind is a
/// catalogue widget id.
fn summary(kind: &str, cx: &App) -> Option<&'static str> {
    cx.try_global::<guiraffe_core::widget_catalog::WidgetRegistry>()?
        .registrations()
        .find(|registration| registration.spec.id == kind)
        .map(|registration| registration.spec.summary)
}

/// One Spotlight entry per bindable action of every control registered in `window`.
/// Called only when a palette opens.
pub(crate) fn spotlight_actions(
    window: &Window,
    cx: &App,
) -> Vec<guiraffe_overlays::SpotlightAction> {
    let mut out = Vec::new();
    for (id, kind, capabilities) in ActionBus::controls(window, cx) {
        let Some(path) = id.path(cx) else { continue };
        let name = label(id, path, window, cx);
        let keywords = match summary(kind, cx) {
            Some(summary) => format!("{kind} {path} {summary}"),
            None => format!("{kind} {path}"),
        };
        for action in capabilities.action_names() {
            out.push(
                guiraffe_overlays::SpotlightAction::new(
                    ControlAction::command_id(path, action),
                    format!("{name}: {}", title(action)),
                )
                .group("Controls")
                .keywords(keywords.clone()),
            );
        }
    }
    // Deterministic order: the registry is a hash map.
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

impl Commands {
    /// A command for one control action, titled from the control's label and the action,
    /// for binding a shortcut or a ribbon entry: `commands.register_handler(
    /// commands.control_command("settings/theme", "toggle", window, cx)?.keybinding(kb), cx)`.
    /// The control must be registered in `window` for the label; the command dispatches
    /// wherever it is invoked.
    pub fn control_command(
        &self,
        path: &str,
        action: &str,
        window: &Window,
        cx: &App,
    ) -> Result<Command<Handler>, Error> {
        let id = ControlId::lookup(path, cx)
            .ok_or_else(|| rejected(path, ActionError::UnknownTarget))?;
        let (kind, capabilities) = ActionBus::describe(id, window, cx)
            .ok_or_else(|| rejected(path, ActionError::UnknownTarget))?;
        if !capabilities.action_names().any(|name| name == action) {
            return Err(rejected(path, ActionError::Unsupported));
        }
        let command_id = CommandId::parse(&ControlAction::command_id(path, action))?;
        let handler = Handler::apply(
            path.to_owned(),
            Action {
                name: action.to_owned().into(),
                payload: Value::Null,
            },
        );
        Ok(Command::new(
            command_id,
            format!("{}: {}", label(id, path, window, cx), title(action)),
            handler,
        )
        .category("Controls")
        .keywords(format!("{kind} {path}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_control_command_ids() {
        let parsed = ControlAction::parse("control.settings/appearance/theme.set_value");
        assert_eq!(
            parsed
                .as_ref()
                .map(|c| (c.path.as_ref(), c.action.name.as_ref())),
            Some(("settings/appearance/theme", "set_value"))
        );
        assert!(ControlAction::parse("file.save").is_none());
        assert!(ControlAction::parse("control.toggle").is_none());
        assert!(CommandId::parse("control.settings/theme.toggle").is_ok());
        assert_eq!(title("scroll_to"), "Scroll to");
    }
}
