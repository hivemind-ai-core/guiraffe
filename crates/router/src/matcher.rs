//! Runtime support for the [`routes!`](crate::routes) macro. Patterns are `/literal/:param/*rest`.

use std::{fmt::Display, str::FromStr};

use crate::path::Location;

/// Captured parameters of a matched pattern.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Captures(Vec<(&'static str, String)>);

impl Captures {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Match `pattern` against decoded `segments`. `*name` (last segment only) captures the
/// rest of the path joined with `/` (possibly empty).
pub fn match_pattern(pattern: &'static str, segments: &[String]) -> Option<Captures> {
    let parts: Vec<&'static str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let mut caps = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if let Some(name) = part.strip_prefix('*') {
            caps.push((name, segments.get(i..).unwrap_or_default().join("/")));
            return Some(Captures(caps));
        }
        let seg = segments.get(i)?;
        match part.strip_prefix(':') {
            Some(name) => caps.push((name, seg.clone())),
            None if part == seg => {}
            None => return None,
        }
    }
    (segments.len() == parts.len()).then_some(Captures(caps))
}

/// Read field `name`. **Path params** are strict: an empty or unparsable value fails the
/// match. **Search params** are lenient (like a browser): missing or unparsable values fall
/// back to `T::default()`, so a bad `?tab=` never breaks a deep link. First value wins for
/// repeated keys.
pub fn field<T: FromStr + Default>(
    name: &'static str,
    caps: &Captures,
    loc: &Location,
) -> Option<T> {
    match caps.get(name) {
        Some("") => None,
        Some(raw) => raw.parse().ok(),
        None => Some(
            loc.query_value(name)
                .and_then(|v| v.parse().ok())
                .unwrap_or_default(),
        ),
    }
}

/// Check one variant's pattern against its field names: every `:param`/`*rest` must be a field,
/// names must be unique, and `*rest` must be last. Returns human-readable problems.
pub fn check(variant: &'static str, pattern: &'static str, fields: &[&'static str]) -> Vec<String> {
    let mut problems = Vec::new();
    if !pattern.starts_with('/') {
        problems.push(format!(
            "{variant}: pattern {pattern:?} must start with '/'"
        ));
    }
    let parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let mut seen = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let name = part.strip_prefix(':').or_else(|| part.strip_prefix('*'));
        if part.starts_with('*') && i + 1 != parts.len() {
            problems.push(format!(
                "{variant}: {part} must be the last segment of {pattern:?}"
            ));
        }
        if let Some(name) = name {
            if !fields.contains(&name) {
                problems.push(format!("{variant}: pattern {pattern:?} binds `{name}` but the variant has no such field"));
            }
            if seen.contains(&name) {
                problems.push(format!("{variant}: `{name}` appears twice in {pattern:?}"));
            }
            seen.push(name);
        }
    }
    problems
}

pub fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

pub fn display<T: Display>(v: &T) -> String {
    v.to_string()
}

/// Build a location from `pattern`, filling params from `values`. Values the pattern does not
/// bind become query parameters unless they equal their default (`defaults[i]`).
pub fn build(
    pattern: &'static str,
    values: Vec<(&'static str, String)>,
    defaults: &[bool],
) -> Location {
    let mut used = vec![false; values.len()];
    let mut segments = Vec::new();
    for part in pattern.split('/').filter(|s| !s.is_empty()) {
        let (name, rest) = match (part.strip_prefix(':'), part.strip_prefix('*')) {
            (Some(n), _) => (Some(n), false),
            (_, Some(n)) => (Some(n), true),
            _ => (None, false),
        };
        match name {
            Some(name) => {
                if let Some(i) = values.iter().position(|(k, _)| *k == name) {
                    used[i] = true;
                    if rest {
                        segments.extend(
                            values[i]
                                .1
                                .split('/')
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned),
                        );
                    } else {
                        segments.push(values[i].1.clone());
                    }
                }
            }
            None => segments.push(part.to_owned()),
        }
    }
    let query = values
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !used[*i] && !defaults.get(*i).copied().unwrap_or(false))
        .map(|(_, (k, v))| (k.to_owned(), v))
        .collect();
    Location { segments, query }
}

/// Names bound by a pattern (for validation in tests/debug).
pub fn params(pattern: &'static str) -> Vec<&'static str> {
    pattern
        .split('/')
        .filter_map(|p| p.strip_prefix(':').or_else(|| p.strip_prefix('*')))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reports_pattern_problems() {
        assert!(check("P", "/p/:id", &["id"]).is_empty());
        assert_eq!(check("U", "/u/:id", &[]).len(), 1, "param without field");
        assert_eq!(check("T", "/p/:Id", &["id"]).len(), 1, "typo");
        assert_eq!(check("D", "/a/:id/:id", &["id"]).len(), 1, "duplicate");
        assert_eq!(
            check("R", "/a/*rest/b", &["rest"]).len(),
            1,
            "rest not last"
        );
        assert_eq!(check("S", "nope", &[]).len(), 1, "leading slash");
    }
}
