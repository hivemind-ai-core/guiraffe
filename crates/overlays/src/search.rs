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
pub(crate) struct PreparedText {
    chars: Vec<char>,
    bonuses: Vec<i64>,
}
impl PreparedText {
    pub(crate) fn new(text: &str) -> Self {
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
    pub(crate) fn score(&self, query: &[char], work: &mut Workspace) -> Option<i64> {
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
pub(crate) struct Workspace {
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
/// Lowercase `query` into `out` (reusing its allocation), the same normalization the
/// prepared candidates use.
pub(crate) fn normalize_into(query: &str, out: &mut Vec<char>) {
    out.clear();
    out.extend(query.chars().flat_map(char::to_lowercase));
}

/// The byte ranges of `text` that the optimal [`fuzzy_score`] alignment of `query` matched,
/// with adjacent characters merged into one range. `None` when `query` is not a
/// subsequence of `text`; an empty `Vec` for an empty query. Used to highlight matches.
///
/// ```
/// use guiraffe_overlays::search::{fuzzy_score, match_ranges};
/// assert_eq!(match_ranges("gc", "Git: Commit"), Some(vec![0..1, 5..6]));
/// assert_eq!(match_ranges("zz", "Git"), None);
/// assert!(fuzzy_score("gc", "Git: Commit").is_some());
/// ```
pub fn match_ranges(query: &str, text: &str) -> Option<Vec<std::ops::Range<usize>>> {
    let mut needle = Vec::new();
    normalize_into(query.trim(), &mut needle);
    let (_, positions) = align(&needle, text)?;
    // Lowered characters back to source byte ranges (one source char can lower to several).
    let mut sources = Vec::with_capacity(text.len());
    for (offset, character) in text.char_indices() {
        for _ in character.to_lowercase() {
            sources.push(offset..offset + character.len_utf8());
        }
    }
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(positions.len());
    for position in positions {
        let Some(source) = sources.get(position).cloned() else {
            continue;
        };
        match ranges.last_mut() {
            Some(last) if last.end == source.start => last.end = source.end,
            Some(last) if last.start == source.start => {}
            _ => ranges.push(source),
        }
    }
    Some(ranges)
}

/// Optimal alignment: the [`fuzzy_score`] of `query` against `text` and the matched
/// positions (indices into the lowered characters of `text`). Same recurrence as
/// [`PreparedText::score`], with the full table kept for backtracking.
pub(crate) fn align(query: &[char], text: &str) -> Option<(i64, Vec<usize>)> {
    let prepared = PreparedText::new(text);
    let n = prepared.chars.len();
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    if query.len() > n {
        return None;
    }
    const NONE: i64 = i64::MIN / 4;
    let m = query.len();
    let mut table = vec![NONE; m * n];
    // For each cell, the previous row's position it extends (usize::MAX for the first row).
    let mut parent = vec![usize::MAX; m * n];
    for (j, (cell, (character, bonus))) in table
        .iter_mut()
        .zip(prepared.chars.iter().zip(&prepared.bonuses))
        .enumerate()
    {
        if *character == query[0] {
            *cell = 10 + bonus - j as i64;
        }
    }
    for i in 1..m {
        let (before, rest) = table.split_at_mut(i * n);
        let previous = &before[(i - 1) * n..];
        let current = &mut rest[..n];
        let mut prefix = NONE;
        let mut prefix_at = usize::MAX;
        for j in 0..n {
            if j >= 2 {
                let candidate = previous[j - 2] + (j - 2) as i64;
                if candidate > prefix {
                    prefix = candidate;
                    prefix_at = j - 2;
                }
            }
            if prepared.chars[j] != query[i] {
                continue;
            }
            let adjacent = if j > 0 { previous[j - 1] + 20 } else { NONE };
            let separated = prefix - j as i64 + 1;
            let (best, from) = if adjacent >= separated {
                (adjacent, j.wrapping_sub(1))
            } else {
                (separated, prefix_at)
            };
            if best > NONE / 2 {
                current[j] = best + 10 + prepared.bonuses[j];
                parent[i * n + j] = from;
            }
        }
    }
    let last = &table[(m - 1) * n..];
    let (mut at, best) = last
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, score)| *score > NONE / 2)
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?;
    let mut positions = vec![0; m];
    for i in (0..m).rev() {
        positions[i] = at;
        at = parent[i * n + at];
        if i > 0 && at >= n {
            return None;
        }
    }
    Some((best - n as i64 / 4, positions))
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
    fn repeated_query_is_served_from_the_cache() {
        let mut index = SearchIndex::new(&[Command::new("a", "Open")]);
        assert_eq!(index.search("op", 5), [0]);
        let evaluated = evaluations();
        assert_eq!(index.search(" op ", 5), [0]);
        assert_eq!(evaluations(), evaluated);
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
#[cfg(test)]
mod property_tests {
    //! The DP scorer and its backtracked alignment against an exhaustive reference.
    use super::*;
    use proptest::prelude::*;

    /// Score one alignment with the documented rules: +10 per match plus the word-boundary
    /// bonus, minus the first match's offset, +20 for each adjacent pair, minus the gap
    /// length between separated matches, minus a quarter of the text length.
    fn alignment_score(text: &PreparedText, positions: &[usize]) -> i64 {
        let mut score = 0;
        for (i, &at) in positions.iter().enumerate() {
            score += 10 + text.bonuses[at];
            if i == 0 {
                score -= at as i64;
            } else if positions[i - 1] + 1 == at {
                score += 20;
            } else {
                score -= (at - positions[i - 1] - 1) as i64;
            }
        }
        score - text.chars.len() as i64 / 4
    }

    /// Enumerate every alignment (exponential; inputs are tiny).
    pub(crate) fn naive(query: &[char], text: &PreparedText) -> Option<i64> {
        fn walk(
            query: &[char],
            text: &PreparedText,
            from: usize,
            taken: &mut Vec<usize>,
            best: &mut Option<i64>,
        ) {
            if taken.len() == query.len() {
                let score = alignment_score(text, taken);
                *best = Some(best.map_or(score, |b: i64| b.max(score)));
                return;
            }
            for at in from..text.chars.len() {
                if text.chars[at] == query[taken.len()] {
                    taken.push(at);
                    walk(query, text, at + 1, taken, best);
                    taken.pop();
                }
            }
        }
        if query.is_empty() {
            return Some(0);
        }
        let mut best = None;
        walk(query, text, 0, &mut Vec::new(), &mut best);
        best
    }

    pub(crate) fn lowered(s: &str) -> Vec<char> {
        s.chars().flat_map(char::to_lowercase).collect()
    }

    proptest! {
        #[test]
        fn dp_matches_the_exhaustive_scorer(
            text in "[a-cA-C _:]{0,10}",
            query in "[a-c]{0,4}",
        ) {
            let prepared = PreparedText::new(&text);
            let needle = lowered(&query);
            prop_assert_eq!(fuzzy_score(&query, &text), naive(&needle, &prepared));
        }

        #[test]
        fn alignment_is_a_matching_subsequence_with_the_optimal_score(
            text in "[a-cA-C _:é]{0,10}",
            query in "[a-cé]{1,4}",
        ) {
            let prepared = PreparedText::new(&text);
            let needle = lowered(&query);
            match align(&needle, &text) {
                None => prop_assert_eq!(naive(&needle, &prepared), None),
                Some((score, positions)) => {
                    prop_assert!(positions.windows(2).all(|w| w[0] < w[1]));
                    for (at, c) in positions.iter().zip(&needle) {
                        prop_assert_eq!(prepared.chars[*at], *c);
                    }
                    prop_assert_eq!(Some(score), naive(&needle, &prepared));
                    prop_assert_eq!(alignment_score(&prepared, &positions), score);
                    let ranges = match_ranges(&query, &text).unwrap_or_default();
                    let highlighted: usize =
                        ranges.iter().map(|r| text[r.clone()].chars().count()).sum();
                    prop_assert_eq!(highlighted, needle.len());
                }
            }
        }

        #[test]
        fn search_ranks_by_the_exhaustive_score(
            labels in proptest::collection::vec("[a-cA-C ]{0,8}", 0..8),
            query in "[a-c]{1,3}",
        ) {
            let commands: Vec<Command> = labels
                .iter()
                .enumerate()
                .map(|(i, label)| Command::new(i.to_string(), label.clone()))
                .collect();
            let needle = lowered(&query);
            let mut expected: Vec<(i64, usize)> = labels
                .iter()
                .enumerate()
                .filter_map(|(i, label)| naive(&needle, &PreparedText::new(label)).map(|s| (s, i)))
                .collect();
            expected.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let expected: Vec<usize> = expected.into_iter().map(|(_, i)| i).collect();
            prop_assert_eq!(search_indices(&query, &commands, usize::MAX), expected);
        }
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn ranges_map_expanding_lowercase_back_to_source_bytes() {
        // 'İ' lowers to two chars; the highlight still covers the whole source char.
        assert_eq!(match_ranges("i", "İx"), Some(vec![0..2]));
        assert_eq!(match_ranges("", "abc"), Some(vec![]));
        assert_eq!(match_ranges("ab", "xaby"), Some(vec![1..3]));
    }
}
