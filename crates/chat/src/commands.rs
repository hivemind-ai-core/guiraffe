//! UTF-8-safe completion trigger and replacement logic.
use std::ops::Range;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Slash,
    Mention,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger<'a> {
    pub kind: TriggerKind,
    pub range: Range<usize>,
    pub query: &'a str,
}
/// Cursor offsets are UTF-8 byte offsets. Invalid boundaries yield no completion.
pub fn trigger(text: &str, cursor: usize) -> Option<Trigger<'_>> {
    let before = text.get(..cursor)?;
    let start = before.rfind(char::is_whitespace).map_or(0, |i| {
        i + before[i..].chars().next().map_or(0, char::len_utf8)
    });
    let token = before.get(start..)?;
    let kind = match token.chars().next()? {
        '/' if before[..start].trim().is_empty() => TriggerKind::Slash,
        '@' => TriggerKind::Mention,
        _ => return None,
    };
    Some(Trigger {
        kind,
        range: start..cursor,
        query: &token[1..],
    })
}
pub fn complete(text: &str, trigger: &Trigger<'_>, value: &str) -> Option<String> {
    let prefix = text.get(..trigger.range.start)?;
    let suffix = text.get(trigger.range.end..)?;
    let marker = match trigger.kind {
        TriggerKind::Slash => "/",
        TriggerKind::Mention => "@",
    };
    Some(format!("{prefix}{marker}{value} {suffix}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slash_only_at_start() {
        assert_eq!(trigger(" /rev", 5).map(|t| t.query), Some("rev"));
        assert!(trigger("please /rev", 11).is_none());
        assert!(trigger("https://a", 9).is_none());
    }
    #[test]
    fn unicode_and_cursor() {
        let s = "好 @工程 tail";
        let cursor = "好 @工程".len();
        let t = trigger(s, cursor).expect("valid test input");
        assert_eq!(t.query, "工程");
        assert_eq!(complete(s, &t, "src"), Some("好 @src  tail".into()));
        assert!(trigger(s, 1).is_none());
        assert!(trigger(s, 999).is_none());
    }
    #[test]
    fn boundaries_and_empty_queries() {
        for s in ["/", "@", "\n/", "\t@", "　/"] {
            assert!(trigger(s, s.len()).is_some());
        }
        for s in ["", "x@y", "// x", "hello "] {
            assert!(trigger(s, s.len()).is_none());
        }
    }
    #[test]
    fn replacements_preserve_suffix() {
        let s = "/he suffix";
        let t = trigger(s, 3).expect("valid test input");
        assert_eq!(
            complete(s, &t, "help").expect("valid test input"),
            "/help  suffix"
        );
    }
}
