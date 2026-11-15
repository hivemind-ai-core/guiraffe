//! Unicode-aware, deterministic subsequence matching. No UI dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub keywords: String,
    pub disabled: bool,
}
impl Command {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            keywords: String::new(),
            disabled: false,
        }
    }
}
/// Higher scores prefer boundaries and adjacent characters; matching is case insensitive.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    let query: Vec<char> = query.to_lowercase().chars().collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut index = 0;
    let mut score = 0;
    let mut previous = None;
    for needle in query {
        let found = (index..text.len()).find(|&i| text[i] == needle)?;
        score += 10;
        if found == 0 || !text[found - 1].is_alphanumeric() {
            score += 15;
        }
        if previous.is_some_and(|p| p + 1 == found) {
            score += 20;
        }
        score -= (found - index) as i64;
        previous = Some(found);
        index = found + 1;
    }
    Some(score - text.len() as i64 / 4)
}
pub fn search<'a>(query: &str, commands: &'a [Command], limit: usize) -> Vec<&'a Command> {
    let mut matches: Vec<_> = commands
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.disabled)
        .filter_map(|(i, c)| {
            let score = fuzzy_score(query.trim(), &c.label)
                .into_iter()
                .chain(fuzzy_score(query.trim(), &c.keywords))
                .max()?;
            Some((score, i, c))
        })
        .collect();
    matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    matches.into_iter().take(limit).map(|(_, _, c)| c).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_and_rejection() {
        assert!(fuzzy_score("éD", "Édit Document").is_some());
        assert_eq!(fuzzy_score("zz", "Open"), None);
        assert_eq!(fuzzy_score("", ""), Some(0));
        assert_eq!(fuzzy_score("aa", "a"), None);
    }
    #[test]
    fn contiguous_preferred() {
        assert!(
            fuzzy_score("open", "Open").expect("score")
                > fuzzy_score("open", "O p e n").expect("score")
        );
    }
    #[test]
    fn ranking_keywords_disabled_and_limit() {
        let mut commands = vec![
            Command::new("a", "Open"),
            Command::new("b", "Open"),
            Command::new("c", "Close"),
        ];
        commands[2].keywords = "open".into();
        commands[0].disabled = true;
        assert_eq!(
            search("open", &commands, 2)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "c"]
        );
        assert!(search("", &commands, 0).is_empty());
    }
}
