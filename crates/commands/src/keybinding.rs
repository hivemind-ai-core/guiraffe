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
const MODIFIERS: [(&str, u8); 5] = [
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
    let function = value
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    let single = value.len() == 1 && value.chars().all(|c| c.is_ascii_graphic() && c != '+');
    (named || function || single).then(|| value.to_owned())
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
        Ok(Self {
            canonical: encode("+", false),
            native: encode("-", true),
            strokes,
        })
    }
}
impl Keybinding {
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
}
