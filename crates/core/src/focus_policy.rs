//! Focus policy: an app default for how controls take keyboard focus, with per-component
//! overrides (API conventions §12.3).
//!
//! Three independent settings:
//! - **pointer**: whether a pointer press on the control takes keyboard focus,
//! - **tab**: whether the control is a Tab stop,
//! - **indicator**: when the focus indicator (ring, keyboard cursor) shows.
//!
//! Each is resolved component override → app default ([`set_app_focus_policy`]) → the
//! component's built-in behaviour. The library default leaves everything at
//! [`FocusOpt::Inherit`], so every control keeps its current behaviour until a host sets a
//! policy. Keyboard-first components (text fields, the command ribbon, keyboard-navigable
//! views) do not consult the app pointer setting: they always take focus on press.

use gpui_kit::{App, Global};

/// One focus setting of a component or the app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FocusOpt {
    /// Use the next level: the app default, then the component's built-in behaviour.
    #[default]
    Inherit,
    On,
    Off,
}

impl FocusOpt {
    /// This setting, or `fallback` when inherited.
    pub fn or(self, fallback: FocusOpt) -> FocusOpt {
        match self {
            FocusOpt::Inherit => fallback,
            set => set,
        }
    }
    /// Resolve against a built-in boolean.
    pub fn resolve(self, builtin: bool) -> bool {
        match self {
            FocusOpt::Inherit => builtin,
            FocusOpt::On => true,
            FocusOpt::Off => false,
        }
    }
}

impl From<bool> for FocusOpt {
    fn from(v: bool) -> Self {
        if v { FocusOpt::On } else { FocusOpt::Off }
    }
}

/// When a focus indicator shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FocusIndicator {
    #[default]
    Inherit,
    /// Whenever the control is focused.
    Focused,
    /// Only when focus came from the keyboard or assistive technology (focus-visible).
    Keyboard,
    /// Never (the host draws its own).
    Never,
}

impl FocusIndicator {
    pub fn or(self, fallback: FocusIndicator) -> FocusIndicator {
        match self {
            FocusIndicator::Inherit => fallback,
            set => set,
        }
    }
    /// Whether the indicator shows now. `Inherit` behaves as `Focused`.
    pub fn visible(self, focused: bool, keyboard: bool) -> bool {
        focused
            && match self {
                FocusIndicator::Inherit | FocusIndicator::Focused => true,
                FocusIndicator::Keyboard => keyboard,
                FocusIndicator::Never => false,
            }
    }
}

/// A component's or the app's focus policy. `Default` inherits everything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FocusPolicy {
    pub pointer: FocusOpt,
    pub tab: FocusOpt,
    pub indicator: FocusIndicator,
}

impl FocusPolicy {
    /// Inherit everything (the library default).
    pub const INHERIT: FocusPolicy = FocusPolicy {
        pointer: FocusOpt::Inherit,
        tab: FocusOpt::Inherit,
        indicator: FocusIndicator::Inherit,
    };
    /// Mouse-first: a press does not take focus, the control stays a Tab stop, and the
    /// indicator shows only for keyboard focus.
    pub const MOUSE_FIRST: FocusPolicy = FocusPolicy {
        pointer: FocusOpt::Off,
        tab: FocusOpt::On,
        indicator: FocusIndicator::Keyboard,
    };
    /// Pointer only: never focused by a press, not a Tab stop.
    pub const POINTER_ONLY: FocusPolicy = FocusPolicy {
        pointer: FocusOpt::Off,
        tab: FocusOpt::Off,
        indicator: FocusIndicator::Keyboard,
    };
    /// Keyboard-first: a press focuses, a Tab stop, the indicator shows whenever focused.
    pub const KEYBOARD_FIRST: FocusPolicy = FocusPolicy {
        pointer: FocusOpt::On,
        tab: FocusOpt::On,
        indicator: FocusIndicator::Focused,
    };

    pub fn pointer(mut self, v: impl Into<FocusOpt>) -> Self {
        self.pointer = v.into();
        self
    }
    pub fn tab(mut self, v: impl Into<FocusOpt>) -> Self {
        self.tab = v.into();
        self
    }
    pub fn indicator(mut self, v: FocusIndicator) -> Self {
        self.indicator = v;
        self
    }

    /// This policy over `app`, then the component's `builtin` behaviour.
    pub fn resolve(self, app: FocusPolicy, builtin: ResolvedFocus) -> ResolvedFocus {
        ResolvedFocus {
            pointer: self.pointer.or(app.pointer).resolve(builtin.pointer),
            tab: self.tab.or(app.tab).resolve(builtin.tab),
            indicator: match self.indicator.or(app.indicator) {
                FocusIndicator::Inherit => builtin.indicator,
                set => set,
            },
        }
    }

    /// [`FocusPolicy::resolve`] against the app default in `cx`.
    pub fn resolve_in(self, builtin: ResolvedFocus, cx: &App) -> ResolvedFocus {
        self.resolve(app_focus_policy(cx), builtin)
    }
}

/// A fully resolved policy: what the component does now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedFocus {
    pub pointer: bool,
    pub tab: bool,
    /// Never `Inherit` once resolved from a non-inheriting built-in.
    pub indicator: FocusIndicator,
}

impl ResolvedFocus {
    /// Whether the indicator shows: focused, and (for `Keyboard`) focus came from the keyboard.
    pub fn indicator_visible(&self, focused: bool, keyboard: bool) -> bool {
        self.indicator.visible(focused, keyboard)
    }
}

struct AppFocusPolicy(FocusPolicy);
impl Global for AppFocusPolicy {}

/// Set the app's default focus policy. Components resolve their own override over it.
/// Needs no window; controls pick it up on their next render.
pub fn set_app_focus_policy(policy: FocusPolicy, cx: &mut App) {
    cx.set_global(AppFocusPolicy(policy));
    cx.refresh_windows();
}

/// The app's default focus policy ([`FocusPolicy::INHERIT`] unless set).
pub fn app_focus_policy(cx: &App) -> FocusPolicy {
    cx.try_global::<AppFocusPolicy>()
        .map_or(FocusPolicy::INHERIT, |p| p.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: ResolvedFocus = ResolvedFocus {
        pointer: true,
        tab: false,
        indicator: FocusIndicator::Focused,
    };

    #[test]
    fn the_default_keeps_built_in_behaviour() {
        assert_eq!(
            FocusPolicy::INHERIT.resolve(FocusPolicy::INHERIT, GRID),
            GRID
        );
        assert_eq!(FocusPolicy::default(), FocusPolicy::INHERIT);
    }

    #[test]
    fn component_overrides_beat_the_app_default() {
        let app = FocusPolicy::MOUSE_FIRST;
        let r = FocusPolicy::INHERIT.resolve(app, GRID);
        assert_eq!(
            (r.pointer, r.tab, r.indicator),
            (false, true, FocusIndicator::Keyboard)
        );
        let r = FocusPolicy::INHERIT.pointer(true).resolve(app, GRID);
        assert!(r.pointer, "the component opts in");
        assert!(r.tab, "the rest still inherits from the app");
        let r = FocusPolicy::POINTER_ONLY.resolve(FocusPolicy::KEYBOARD_FIRST, GRID);
        assert_eq!((r.pointer, r.tab), (false, false), "the component opts out");
    }

    #[test]
    fn settings_resolve_independently() {
        let app = FocusPolicy::INHERIT.tab(true);
        let r = FocusPolicy::INHERIT
            .indicator(FocusIndicator::Never)
            .resolve(app, GRID);
        assert_eq!(
            (r.pointer, r.tab, r.indicator),
            (true, true, FocusIndicator::Never)
        );
    }

    #[test]
    fn indicators() {
        use FocusIndicator::*;
        for (i, focused, keyboard, shows) in [
            (Focused, true, false, true),
            (Focused, false, true, false),
            (Keyboard, true, false, false),
            (Keyboard, true, true, true),
            (Never, true, true, false),
            (Inherit, true, false, true),
        ] {
            assert_eq!(
                i.visible(focused, keyboard),
                shows,
                "{i:?} {focused} {keyboard}"
            );
        }
    }
}
