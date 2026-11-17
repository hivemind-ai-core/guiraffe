//! Fallible key bindings.
//!
//! GPUI's `KeyBinding::new` panics on an invalid keystroke string and does not understand the
//! cross-platform `secondary-` modifier (⌘ on macOS, Ctrl elsewhere). [`binding`] and
//! [`bind_keys`] parse with the host OS and report bad specs instead of crashing the app.

use gpui_kit::{Action, App, KeyBinding};

/// A key binding spec that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidBinding {
    pub keys: String,
    pub context: Option<String>,
    pub reason: String,
}

impl std::fmt::Display for InvalidBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid key binding {:?}", self.keys)?;
        if let Some(c) = &self.context {
            write!(f, " (context {c:?})")?;
        }
        write!(f, ": {}", self.reason)
    }
}

impl std::error::Error for InvalidBinding {}

/// Parse one binding for the host OS (`secondary-` resolves to ⌘ or Ctrl).
pub fn binding<A: Action>(
    keys: &str,
    action: A,
    context: Option<&str>,
    cx: &App,
) -> Result<KeyBinding, InvalidBinding> {
    let bad = |reason: String| InvalidBinding {
        keys: keys.to_owned(),
        context: context.map(str::to_owned),
        reason,
    };
    if keys.trim().is_empty() {
        return Err(bad("empty keystroke".into()));
    }
    let predicate = match context {
        Some(c) => Some(std::rc::Rc::new(
            gpui_kit::KeyBindingContextPredicate::parse(c).map_err(|e| bad(e.to_string()))?,
        )),
        None => None,
    };
    KeyBinding::load(keys, Box::new(action), predicate, false, None, cx)
        .map_err(|e| bad(e.to_string()))
}

/// Register bindings, skipping (and returning) any that fail to parse, so one typo can't take
/// the app down. Callers decide whether to log, surface or assert on the errors.
pub fn bind_keys(
    bindings: Vec<Result<KeyBinding, InvalidBinding>>,
    cx: &mut App,
) -> Vec<InvalidBinding> {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for b in bindings {
        match b {
            Ok(b) => ok.push(b),
            Err(e) => errors.push(e),
        }
    }
    cx.bind_keys(ok);
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    gpui_kit::actions!(keys_test, [Probe]);

    #[test]
    fn invalid_specs_are_errors_not_panics_and_secondary_resolves() {
        let cx = gpui_kit::TestAppContext::single();
        cx.update(|cx| {
            assert!(
                binding("secondary-k", Probe, None, cx).is_ok(),
                "secondary- resolves per OS"
            );
            assert!(binding("ctrl-shift-p", Probe, Some("Editor"), cx).is_ok());
            assert!(
                binding("foo-k", Probe, None, cx).is_err(),
                "unknown modifier is reported"
            );
            assert!(
                binding("  ", Probe, None, cx).is_err(),
                "empty spec would silently bind nothing"
            );
            let bad_ctx = binding("ctrl-k", Probe, Some("Editor &&"), cx);
            assert!(bad_ctx.is_err(), "malformed context predicate is reported");
            let errors = bind_keys(
                vec![
                    binding("ctrl-k", Probe, None, cx),
                    binding("foo-k", Probe, None, cx),
                ],
                cx,
            );
            assert_eq!(
                errors.len(),
                1,
                "the good binding still registers; the bad one is returned"
            );
            assert!(errors[0].to_string().contains("foo-k"));
        });
    }
}
