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

/// Read field `name`: from the path captures if the pattern binds it, else from the query.
/// A missing query value yields `T::default()`; an unparsable value fails the match.
pub fn field<T: FromStr + Default>(
    name: &'static str,
    caps: &Captures,
    loc: &Location,
) -> Option<T> {
    match caps.get(name).or_else(|| loc.query_value(name)) {
        Some(raw) => raw.parse().ok(),
        None => Some(T::default()),
    }
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
