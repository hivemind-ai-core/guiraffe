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
#[cfg(test)]
thread_local! {static EVALUATIONS:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};static NORMALIZATIONS:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
#[cfg(test)]
pub(crate) fn evaluations() -> usize {
    EVALUATIONS.get()
}
#[cfg(test)]
pub(crate) fn normalizations() -> usize {
    NORMALIZATIONS.get()
}
#[derive(Debug)]
struct PreparedText {
    chars: Vec<char>,
    bonuses: Vec<i64>,
}
impl PreparedText {
    fn new(text: &str) -> Self {
        #[cfg(test)]
        NORMALIZATIONS.set(NORMALIZATIONS.get() + 1);
        let mut chars = Vec::with_capacity(text.len());
        let mut bonuses = Vec::with_capacity(text.len());
        let mut previous = None;
        for character in text.chars() {
            let boundary = previous.is_none_or(|p: char| {
                !p.is_alphanumeric() || (p.is_lowercase() && character.is_uppercase())
            });
            for (offset, lower) in character.to_lowercase().enumerate() {
                chars.push(lower);
                bonuses.push(if boundary && offset == 0 { 15 } else { 0 });
            }
            previous = Some(character);
        }
        Self { chars, bonuses }
    }
    fn score(&self, query: &[char], work: &mut Workspace) -> Option<i64> {
        if query.is_empty() {
            return Some(0);
        }
        if query.len() > self.chars.len() {
            return None;
        }
        // Linear subsequence preflight avoids allocating/filling DP rows for rejected candidates.
        let mut remaining = self.chars.iter();
        for needle in query {
            remaining.find(|c| *c == needle)?;
        }
        const NONE: i64 = i64::MIN / 4;
        let n = self.chars.len();
        work.previous.resize(n, NONE);
        work.current.resize(n, NONE);
        work.previous.fill(NONE);
        for (j, character) in self.chars.iter().enumerate() {
            if *character == query[0] {
                work.previous[j] = 10 + self.bonuses[j] - j as i64;
            }
        }
        for needle in &query[1..] {
            work.current.fill(NONE);
            let mut prefix = NONE;
            for j in 0..n {
                if j >= 2 {
                    prefix = prefix.max(work.previous[j - 2] + (j - 2) as i64);
                }
                if self.chars[j] != *needle {
                    continue;
                }
                let adjacent = if j > 0 {
                    work.previous[j - 1] + 20
                } else {
                    NONE
                };
                let separated = prefix - j as i64 + 1;
                let best = adjacent.max(separated);
                if best > NONE / 2 {
                    work.current[j] = best + 10 + self.bonuses[j];
                }
            }
            std::mem::swap(&mut work.current, &mut work.previous);
        }
        work.previous
            .iter()
            .copied()
            .max()
            .filter(|score| *score > NONE / 2)
            .map(|score| score - n as i64 / 4)
    }
}
#[derive(Debug, Default)]
struct Workspace {
    previous: Vec<i64>,
    current: Vec<i64>,
}
#[derive(Debug)]
struct IndexedCommand {
    label: PreparedText,
    keywords: PreparedText,
    disabled: bool,
}
/// Precomputed Unicode-normalized command text and reusable DP buffers.
/// Repeating the same query/limit returns the cached result without searching again.
#[derive(Debug)]
pub struct SearchIndex {
    entries: Vec<IndexedCommand>,
    work: Workspace,
    scores: Vec<(i64, usize)>,
    query: Option<String>,
    limit: usize,
    result: Vec<usize>,
}
impl SearchIndex {
    pub fn new(commands: &[Command]) -> Self {
        Self {
            entries: commands
                .iter()
                .map(|c| IndexedCommand {
                    label: PreparedText::new(&c.label),
                    keywords: PreparedText::new(&c.keywords),
                    disabled: c.disabled,
                })
                .collect(),
            work: Workspace::default(),
            scores: Vec::with_capacity(commands.len()),
            query: None,
            limit: 0,
            result: Vec::new(),
        }
    }
    pub fn search(&mut self, query: &str, limit: usize) -> &[usize] {
        let query = query.trim();
        if self.query.as_deref() == Some(query) && self.limit == limit {
            return &self.result;
        }
        #[cfg(test)]
        EVALUATIONS.set(EVALUATIONS.get() + 1);
        self.query = Some(query.to_owned());
        self.limit = limit;
        let normalized: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
        self.scores.clear();
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.disabled {
                continue;
            }
            let score = entry
                .label
                .score(&normalized, &mut self.work)
                .into_iter()
                .chain(entry.keywords.score(&normalized, &mut self.work))
                .max();
            if let Some(score) = score {
                self.scores.push((score, index));
            }
        }
        self.scores
            .sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.result.clear();
        self.result
            .extend(self.scores.iter().take(limit).map(|(_, index)| *index));
        &self.result
    }
}
/// Optimal subsequence score, with word-boundary and adjacency bonuses (fzf-style DP).
pub fn fuzzy_score(query: &str, text: &str) -> Option<i64> {
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    PreparedText::new(text).score(&query, &mut Workspace::default())
}
pub fn search_indices(query: &str, commands: &[Command], limit: usize) -> Vec<usize> {
    SearchIndex::new(commands).search(query, limit).to_vec()
}
pub fn search<'a>(query: &str, commands: &'a [Command], limit: usize) -> Vec<&'a Command> {
    search_indices(query, commands, limit)
        .into_iter()
        .filter_map(|i| commands.get(i))
        .collect()
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
#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn review_9_best_alignment_can_skip_an_earlier_match() {
        let later = fuzzy_score("abc", "a----------abc").expect("subsequence");
        let forced = fuzzy_score("abc", "a----------a-b-c").expect("subsequence");
        assert!(
            later > forced,
            "the later consecutive match must win: {later} vs {forced}"
        );
        let score = fuzzy_score("abc", "a----------abc").expect("score");
        assert_eq!(
            score,
            25 - 11 + 30 + 30 - 14 / 4,
            "choose the optimal later contiguous path"
        );
    }
}
