//! Portable, owned keybinding syntax independent of GPUI and the host OS.
use crate::Error;
use std::{fmt, str::FromStr};
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Keybinding {
    canonical: String,
    native: String,
    strokes: Vec<Stroke>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Stroke {
    modifiers: u8,
    key: String,
}
/// The platform-neutral primary modifier: Cmd on macOS, Ctrl elsewhere.
const SECONDARY: u8 = 32;
const CTRL: u8 = 1;
const CMD: u8 = 8;
const MODIFIERS: [(&str, u8); 6] = [
    ("secondary", SECONDARY),
    ("ctrl", 1),
    ("alt", 2),
    ("shift", 4),
    ("cmd", 8),
    ("fn", 16),
];
fn modifier(value: &str) -> Option<u8> {
    match value {
        "ctrl" | "control" => Some(1),
        "alt" | "option" => Some(2),
        "shift" => Some(4),
        "cmd" | "command" | "meta" | "super" => Some(8),
        "fn" => Some(16),
        "secondary" | "mod" | "primary" => Some(SECONDARY),
        _ => None,
    }
}
fn key(value: &str) -> Option<String> {
    let value = match value {
        "return" => "enter",
        "esc" => "escape",
        "arrowup" => "up",
        "arrowdown" => "down",
        "arrowleft" => "left",
        "arrowright" => "right",
        "-" => "minus",
        v => v,
    };
    let named = matches!(
        value,
        "enter"
            | "escape"
            | "space"
            | "tab"
            | "backspace"
            | "delete"
            | "insert"
            | "home"
            | "end"
            | "pageup"
            | "pagedown"
            | "up"
            | "down"
            | "left"
            | "right"
            | "plus"
            | "minus"
    );
    if let Some(number) = value
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| (1..=24).contains(n))
    {
        return Some(format!("f{number}"));
    }
    let single = value.len() == 1 && value.chars().all(|c| c.is_ascii_graphic() && c != '+');
    (named || single).then(|| value.to_owned())
}
impl FromStr for Keybinding {
    type Err = Error;
    fn from_str(source: &str) -> Result<Self, Error> {
        let text = source.trim().to_ascii_lowercase();
        if text.is_empty() || text.len() > 1024 {
            return Err(Error::InvalidBinding(source.into()));
        }
        let mut strokes = Vec::new();
        for chord in text.split(' ') {
            let mut parts = chord.split('+').collect::<Vec<_>>();
            let last = parts
                .pop()
                .ok_or_else(|| Error::InvalidBinding(source.into()))?;
            if modifier(last).is_some() {
                return Err(Error::InvalidBinding(source.into()));
            }
            let key = key(last).ok_or_else(|| Error::InvalidBinding(source.into()))?;
            let mut modifiers = 0;
            for part in parts {
                let bit = modifier(part).ok_or_else(|| Error::InvalidBinding(source.into()))?;
                if modifiers & bit != 0 {
                    return Err(Error::InvalidBinding(source.into()));
                }
                modifiers |= bit;
            }
            strokes.push(Stroke { modifiers, key });
        }
        Ok(Self::from_strokes(strokes))
    }
}
/// The keyboard convention a binding is resolved for. `secondary`/`mod` means Cmd on
/// [`Platform::Mac`] and Ctrl on [`Platform::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Mac,
    Other,
}
impl Platform {
    /// The convention of the compilation target.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}
fn title_key(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
impl Keybinding {
    fn from_strokes(strokes: Vec<Stroke>) -> Self {
        let encode = |separator: &str, native: bool| {
            strokes
                .iter()
                .map(|stroke| {
                    let mut parts = MODIFIERS
                        .iter()
                        .filter(|(_, bit)| stroke.modifiers & bit != 0)
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>();
                    let key = match (native, stroke.key.as_str()) {
                        (true, "minus") => "-",
                        (true, "plus") => "+",
                        (_, value) => value,
                    };
                    parts.push(key);
                    parts.join(separator)
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        Self {
            canonical: encode("+", false),
            native: encode("-", true),
            strokes,
        }
    }
    /// Whether any stroke uses the platform-neutral `secondary` modifier.
    pub fn is_platform_neutral(&self) -> bool {
        self.strokes.iter().any(|s| s.modifiers & SECONDARY != 0)
    }
    /// Replace `secondary` with Cmd (macOS) or Ctrl (elsewhere). Bindings that name
    /// the physical key are returned unchanged, so `secondary+s` and `ctrl+s` compare
    /// equal on Linux and Windows and conflict there.
    pub fn resolve(&self, platform: Platform) -> Self {
        if !self.is_platform_neutral() {
            return self.clone();
        }
        let physical = match platform {
            Platform::Mac => CMD,
            Platform::Other => CTRL,
        };
        Self::from_strokes(
            self.strokes
                .iter()
                .map(|s| Stroke {
                    modifiers: if s.modifiers & SECONDARY != 0 {
                        (s.modifiers & !SECONDARY) | physical
                    } else {
                        s.modifiers
                    },
                    key: s.key.clone(),
                })
                .collect(),
        )
    }
    /// Human-readable shortcut for menus and palettes: `⌘⇧P` on macOS, `Ctrl+Shift+P`
    /// elsewhere.
    pub fn label(&self, platform: Platform) -> String {
        let resolved = self.resolve(platform);
        resolved
            .strokes
            .iter()
            .map(|s| {
                let key = match s.key.as_str() {
                    "minus" => "-".to_owned(),
                    "plus" => "+".to_owned(),
                    k if k.chars().count() == 1 => k.to_ascii_uppercase(),
                    k => title_key(k),
                };
                let mut out = String::new();
                for (bit, mac, other) in [
                    (CTRL, "⌃", "Ctrl"),
                    (2, "⌥", "Alt"),
                    (4, "⇧", "Shift"),
                    (CMD, "⌘", "Super"),
                    (16, "fn", "Fn"),
                ] {
                    if s.modifiers & bit != 0 {
                        match platform {
                            Platform::Mac => out.push_str(mac),
                            Platform::Other => {
                                out.push_str(other);
                                out.push('+');
                            }
                        }
                    }
                }
                out.push_str(&key);
                out
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    pub fn parse(value: &str) -> Result<Self, Error> {
        value.parse()
    }
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
    /// GPUI uses hyphen-separated modifiers; this is an owned conversion, never a leaked static string.
    pub fn gpui_keystrokes(&self) -> &str {
        &self.native
    }
    pub fn chord_count(&self) -> usize {
        self.strokes.len()
    }
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        other.strokes.starts_with(&self.strokes)
    }
}
impl fmt::Display for Keybinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_forms_and_aliases() {
        for (source, canonical, native) in [
            ("CTRL+SHIFT+P", "ctrl+shift+p", "ctrl-shift-p"),
            ("cmd+k", "cmd+k", "cmd-k"),
            ("alt+enter", "alt+enter", "alt-enter"),
            ("ctrl+k ctrl+s", "ctrl+k ctrl+s", "ctrl-k ctrl-s"),
            ("Command+Option+Esc", "alt+cmd+escape", "alt-cmd-escape"),
            ("ctrl+minus", "ctrl+minus", "ctrl--"),
            ("ctrl+plus", "ctrl+plus", "ctrl-+"),
            ("CTRL+F01", "ctrl+f1", "ctrl-f1"),
        ] {
            let b = Keybinding::parse(source).expect("valid");
            assert_eq!(b.as_str(), canonical);
            assert_eq!(b.gpui_keystrokes(), native);
            assert_eq!(b, Keybinding::parse(canonical).expect("roundtrip"));
        }
    }
    #[test]
    fn every_modifier_permutation_is_canonical() {
        fn permute(prefix: Vec<&str>, remaining: Vec<&str>) {
            if remaining.is_empty() {
                let input = format!("{}+p", prefix.join("+"));
                assert_eq!(
                    Keybinding::parse(&input).expect("valid").as_str(),
                    "ctrl+alt+shift+cmd+fn+p"
                );
                return;
            }
            for i in 0..remaining.len() {
                let mut rest = remaining.clone();
                let next = rest.remove(i);
                let mut prefix = prefix.clone();
                prefix.push(next);
                permute(prefix, rest);
            }
        }
        permute(vec![], vec!["cmd", "shift", "alt", "ctrl", "fn"]);
    }
    #[test]
    fn invalid_input_never_becomes_a_binding() {
        for text in [
            "",
            " ",
            "+",
            "ctrl+",
            "ctrl",
            "ctrl+shift",
            "hyper+a",
            "ctrl+ctrl+p",
            "control+ctrl+p",
            "ctrl++p",
            "ctrl+k  ctrl+s",
            "ctrl+k\tctrl+s",
            "ctrl+unknown",
            "p+ctrl",
            "ctrl+f25",
            "ctrl+é",
        ] {
            assert!(Keybinding::parse(text).is_err(), "{text:?}");
        }
    }
    #[test]
    fn key_space_and_prefixes() {
        for c in b'a'..=b'z' {
            let text = format!("ctrl+{}", char::from(c));
            assert!(Keybinding::parse(&text).is_ok());
        }
        for n in 1..=24 {
            assert!(Keybinding::parse(&format!("f{n}")).is_ok());
        }
        let short = Keybinding::parse("ctrl+k").expect("valid");
        let long = Keybinding::parse("ctrl+k ctrl+s").expect("valid");
        assert!(short.is_prefix_of(&long));
        assert!(!long.is_prefix_of(&short));
        assert_eq!(long.chord_count(), 2);
    }
    #[test]
    fn secondary_modifier_is_platform_neutral() {
        for source in ["secondary+shift+p", "mod+shift+p", "Shift+Mod+P"] {
            let b = Keybinding::parse(source).expect("valid");
            assert_eq!(b.as_str(), "secondary+shift+p");
            assert_eq!(b.gpui_keystrokes(), "secondary-shift-p");
            assert!(b.is_platform_neutral());
            assert_eq!(b.resolve(Platform::Mac).as_str(), "shift+cmd+p");
            assert_eq!(b.resolve(Platform::Other).as_str(), "ctrl+shift+p");
            assert_eq!(b.label(Platform::Mac), "⇧⌘P");
            assert_eq!(b.label(Platform::Other), "Ctrl+Shift+P");
        }
        let chord = Keybinding::parse("mod+k mod+s").expect("chord");
        assert_eq!(chord.resolve(Platform::Mac).as_str(), "cmd+k cmd+s");
        assert_eq!(chord.label(Platform::Other), "Ctrl+K Ctrl+S");
        let physical = Keybinding::parse("ctrl+s").expect("physical");
        assert!(!physical.is_platform_neutral());
        assert_eq!(physical.resolve(Platform::Mac), physical);
        assert_eq!(physical.label(Platform::Mac), "⌃S");
        assert!(
            Keybinding::parse("mod+secondary+s").is_err(),
            "aliases collide"
        );
        assert_eq!(
            Keybinding::parse("alt+enter")
                .expect("enter")
                .label(Platform::Other),
            "Alt+Enter"
        );
    }
}
