//! Pure Spotlight state: prepared search index, ranking, grouping into sections, keyboard
//! selection, recents/pinned and the row geometry used by the virtualised list. No GPUI
//! window or entity, so every behaviour is unit-tested headlessly.
//!
//! Per keystroke the default filter allocates nothing once its buffers are warm: candidate
//! text is prepared once per action set, scores/rows/offsets live in reused vectors, and a
//! query that extends the previous one only rescans the previous matches.
use super::action::SpotlightAction;
use crate::search::{PreparedText, Workspace, normalize_into};
use gpui_kit::SharedString;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

/// Replaces the default fuzzy filter: return the indices of matching actions, best first.
/// Grouping, selection and highlighting still apply to the result.
pub type FilterFn = Rc<dyn Fn(&str, &[SpotlightAction]) -> Vec<usize>>;

/// Matches in keywords, description or group count, but below an equal title match.
const SECONDARY_PENALTY: i64 = 30;

/// Prepared (lowercased, boundary-annotated) text of every action. `Send`, so large action
/// sets can be prepared on a background thread ([`ActionIndex::new`] is pure).
#[derive(Debug, Default)]
pub struct ActionIndex {
    title: Vec<PreparedText>,
    secondary: Vec<PreparedText>,
    /// Section of each action: 0 is "ungrouped", then groups in order of first appearance.
    group: Vec<u32>,
    groups: Vec<Option<SharedString>>,
    /// Action indices ordered by group (stable), for the browse (empty query) listing.
    by_group: Vec<u32>,
    ids: HashMap<SharedString, u32>,
}

impl ActionIndex {
    pub fn new(actions: &[SpotlightAction]) -> Self {
        let mut groups: Vec<Option<SharedString>> = vec![None];
        let mut group = Vec::with_capacity(actions.len());
        let mut title = Vec::with_capacity(actions.len());
        let mut secondary = Vec::with_capacity(actions.len());
        let mut ids = HashMap::with_capacity(actions.len());
        let mut text = String::new();
        for (index, action) in actions.iter().enumerate() {
            let g = match &action.group {
                None => 0,
                Some(label) => match groups.iter().position(|g| g.as_ref() == Some(label)) {
                    Some(found) => found,
                    None => {
                        groups.push(Some(label.clone()));
                        groups.len() - 1
                    }
                },
            };
            group.push(g as u32);
            title.push(PreparedText::new(&action.title));
            text.clear();
            text.push_str(&action.keywords);
            for extra in [action.description.as_ref(), action.group.as_ref()]
                .into_iter()
                .flatten()
            {
                text.push(' ');
                text.push_str(extra);
            }
            secondary.push(PreparedText::new(&text));
            // First occurrence wins for duplicate ids.
            ids.entry(action.id.clone()).or_insert(index as u32);
        }
        let mut by_group: Vec<u32> = (0..actions.len() as u32).collect();
        by_group.sort_by_key(|&i| group.get(i as usize).copied().unwrap_or(0));
        Self {
            title,
            secondary,
            group,
            groups,
            by_group,
            ids,
        }
    }
    fn len(&self) -> usize {
        self.title.len()
    }
}

/// One line of the result list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// Section header; the index into [`SpotlightModel::section_label`].
    Header(u32),
    /// An action, by index into [`SpotlightModel::actions`].
    Action(u32),
}

/// Heights of the row kinds, so the list can be virtualised with exact offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowHeights {
    pub header: f32,
    pub action: f32,
    pub action_with_description: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct Section {
    label: Option<SharedString>,
    first_row: u32,
}

/// The pure state behind [`super::SpotlightState`].
pub struct SpotlightModel {
    actions: Vec<SpotlightAction>,
    index: ActionIndex,
    pinned: Vec<SharedString>,
    recents: Vec<SharedString>,
    recent_limit: usize,
    /// Rank of each action in `recents` (`u32::MAX` when not recent), a ranking tie-break.
    recent_rank: Vec<u32>,
    pinned_label: SharedString,
    recent_label: SharedString,
    limit: Option<usize>,
    filter: Option<FilterFn>,
    heights: RowHeights,

    query: String,
    needle: Vec<char>,
    previous_needle: Vec<char>,
    /// Ranked (score, action) of the current query, untruncated (the limit applies to rows).
    matches: Vec<(i64, u32)>,
    scratch: Vec<(i64, u32)>,
    matches_valid: bool,
    group_rank: Vec<u32>,
    shown: Vec<bool>,
    work: Workspace,

    any_icon: bool,
    rows: Vec<Row>,
    sections: Vec<Section>,
    offsets: Vec<f32>,
    selectable: Vec<u32>,
    selected: Option<usize>,
    #[cfg(test)]
    pub(crate) scored: usize,
}

impl SpotlightModel {
    pub fn new(heights: RowHeights) -> Self {
        Self {
            actions: Vec::new(),
            index: ActionIndex::default(),
            pinned: Vec::new(),
            recents: Vec::new(),
            recent_limit: 5,
            recent_rank: Vec::new(),
            pinned_label: "Pinned".into(),
            recent_label: "Recent".into(),
            limit: None,
            filter: None,
            heights,
            query: String::new(),
            needle: Vec::new(),
            previous_needle: Vec::new(),
            matches: Vec::new(),
            scratch: Vec::new(),
            matches_valid: false,
            group_rank: Vec::new(),
            shown: Vec::new(),
            work: Workspace::default(),
            any_icon: false,
            rows: Vec::new(),
            sections: Vec::new(),
            offsets: vec![0.0],
            selectable: Vec::new(),
            selected: None,
            #[cfg(test)]
            scored: 0,
        }
    }

    // ------------------------------------------------------------------ configuration

    /// Replace every action (prepares the search index). Keeps the selected action when it
    /// is still listed, so a live source can update an open palette without jumping.
    pub fn set_actions(&mut self, actions: Vec<SpotlightAction>) {
        let index = ActionIndex::new(&actions);
        self.install(actions, index);
    }
    /// Append actions (re-prepares the index).
    pub fn extend_actions(&mut self, more: impl IntoIterator<Item = SpotlightAction>) {
        let mut all = std::mem::take(&mut self.actions);
        all.extend(more);
        self.set_actions(all);
    }
    /// [`Self::set_actions`] with an index prepared elsewhere (e.g. a background thread).
    pub fn install(&mut self, actions: Vec<SpotlightAction>, index: ActionIndex) {
        debug_assert_eq!(actions.len(), index.len());
        let keep = self.selected_action().map(|a| a.id.clone());
        self.index = if index.len() == actions.len() {
            index
        } else {
            ActionIndex::new(&actions)
        };
        self.actions = actions;
        self.any_icon = self.actions.iter().any(|a| a.icon.is_some());
        self.matches_valid = false;
        self.update_recent_rank();
        self.rebuild(keep.as_ref());
    }
    pub fn set_pinned(&mut self, ids: Vec<SharedString>) {
        if self.pinned != ids {
            self.pinned = ids;
            self.refresh();
        }
    }
    pub fn set_recents(&mut self, ids: Vec<SharedString>) {
        let mut ids = ids;
        ids.truncate(self.recent_limit);
        if self.recents != ids {
            self.recents = ids;
            self.update_recent_rank();
            self.matches_valid = false;
            self.refresh();
        }
    }
    pub fn set_recent_limit(&mut self, limit: usize) {
        self.recent_limit = limit;
        if self.recents.len() > limit {
            self.recents.truncate(limit);
            self.update_recent_rank();
            self.matches_valid = false;
            self.refresh();
        }
    }
    pub fn set_section_labels(&mut self, pinned: SharedString, recent: SharedString) {
        self.pinned_label = pinned;
        self.recent_label = recent;
        self.refresh();
    }
    pub fn set_limit(&mut self, limit: Option<usize>) {
        if self.limit != limit {
            self.limit = limit;
            self.refresh();
        }
    }
    pub fn set_filter(&mut self, filter: Option<FilterFn>) {
        self.filter = filter;
        self.matches_valid = false;
        self.refresh();
    }
    pub fn set_heights(&mut self, heights: RowHeights) {
        if self.heights != heights {
            self.heights = heights;
            self.layout();
        }
    }
    /// Move `id` to the front of the recents (on activation).
    pub fn record_recent(&mut self, id: &SharedString) {
        if self.recent_limit == 0 {
            return;
        }
        self.recents.retain(|r| r != id);
        self.recents.insert(0, id.clone());
        self.recents.truncate(self.recent_limit);
        self.update_recent_rank();
        // Only the browse listing shows the Recent section; a search keeps its results
        // (recency is a tie-break, applied from the next keystroke on).
        if self.query.is_empty() {
            self.refresh();
        }
    }
    fn update_recent_rank(&mut self) {
        self.recent_rank.clear();
        self.recent_rank.resize(self.actions.len(), u32::MAX);
        for (rank, id) in self.recents.iter().enumerate() {
            if let Some(&i) = self.index.ids.get(id)
                && let Some(slot) = self.recent_rank.get_mut(i as usize)
            {
                *slot = rank as u32;
            }
        }
    }

    // ------------------------------------------------------------------ querying

    /// Set the query; returns whether the listed rows changed (callers notify only then).
    pub fn set_query(&mut self, query: &str) -> bool {
        let query = query.trim();
        if self.query == query {
            return false;
        }
        self.query.clear();
        self.query.push_str(query);
        self.rebuild(None);
        true
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    fn refresh(&mut self) {
        let keep = self.selected_action().map(|a| a.id.clone());
        self.rebuild(keep.as_ref());
    }

    fn score_matches(&mut self) {
        std::mem::swap(&mut self.previous_needle, &mut self.needle);
        normalize_into(&self.query, &mut self.needle);
        if let Some(filter) = self.filter.clone() {
            let ranked = filter(&self.query, &self.actions);
            self.matches.clear();
            let n = ranked.len() as i64;
            self.matches.extend(
                ranked
                    .into_iter()
                    .filter(|&i| i < self.actions.len())
                    .enumerate()
                    .map(|(rank, i)| (n - rank as i64, i as u32)),
            );
            self.matches_valid = true;
            return;
        }
        // A query that extends the previous one can only drop matches: rescan those only.
        let narrowing = self.matches_valid
            && !self.previous_needle.is_empty()
            && self.needle.starts_with(&self.previous_needle);
        self.scratch.clear();
        let candidates = if narrowing {
            self.matches.len()
        } else {
            self.actions.len()
        };
        for k in 0..candidates {
            let i = if narrowing {
                self.matches[k].1 as usize
            } else {
                k
            };
            #[cfg(test)]
            {
                self.scored += 1;
            }
            let (Some(title), Some(secondary)) =
                (self.index.title.get(i), self.index.secondary.get(i))
            else {
                continue;
            };
            let best = title.score(&self.needle, &mut self.work).max(
                secondary
                    .score(&self.needle, &mut self.work)
                    .map(|s| s - SECONDARY_PENALTY),
            );
            if let Some(score) = best {
                self.scratch.push((score, i as u32));
            }
        }
        std::mem::swap(&mut self.matches, &mut self.scratch);
        let recent = &self.recent_rank;
        let rank = |i: u32| recent.get(i as usize).copied().unwrap_or(u32::MAX);
        self.matches.sort_unstable_by(|a, b| {
            b.0.cmp(&a.0)
                .then(rank(a.1).cmp(&rank(b.1)))
                .then(a.1.cmp(&b.1))
        });
        self.matches_valid = true;
    }

    fn rebuild(&mut self, keep: Option<&SharedString>) {
        self.rows.clear();
        self.sections.clear();
        let limit = self.limit.unwrap_or(usize::MAX);
        if self.query.is_empty() {
            self.matches_valid = false;
            self.needle.clear();
            self.browse(limit);
        } else {
            self.score_matches();
            self.ranked(limit);
        }
        self.layout();
        self.selectable.clear();
        for (row, kind) in self.rows.iter().enumerate() {
            if let Row::Action(i) = kind
                && self.actions.get(*i as usize).is_some_and(|a| !a.disabled)
            {
                self.selectable.push(row as u32);
            }
        }
        let kept = keep.and_then(|id| {
            self.selectable.iter().position(|&row| {
                matches!(self.rows.get(row as usize), Some(Row::Action(i))
                    if self.actions.get(*i as usize).is_some_and(|a| &a.id == id))
            })
        });
        self.selected = kept.or((!self.selectable.is_empty()).then_some(0));
    }

    fn push_section(&mut self, label: Option<SharedString>) {
        if label.is_some() {
            self.rows.push(Row::Header(self.sections.len() as u32));
        }
        self.sections.push(Section {
            label,
            first_row: self.rows.len() as u32,
        });
    }

    /// Empty query: Pinned, Recent, then every group in first-appearance order.
    fn browse(&mut self, limit: usize) {
        self.shown.clear();
        self.shown.resize(self.actions.len(), false);
        let mut count = 0;
        for pinned in [true, false] {
            let (len, label) = if pinned {
                (self.pinned.len(), self.pinned_label.clone())
            } else {
                (self.recents.len(), self.recent_label.clone())
            };
            let mut opened = false;
            for k in 0..len {
                let id = if pinned {
                    self.pinned.get(k)
                } else {
                    self.recents.get(k)
                };
                let Some(&i) = id.and_then(|id| self.index.ids.get(id)) else {
                    continue;
                };
                if self.shown.get(i as usize).copied().unwrap_or(true) || count >= limit {
                    continue;
                }
                if !opened {
                    self.push_section(Some(label.clone()));
                    opened = true;
                }
                self.rows.push(Row::Action(i));
                self.shown[i as usize] = true;
                count += 1;
            }
        }
        let mut current = u32::MAX;
        for k in 0..self.index.by_group.len() {
            let i = self.index.by_group[k];
            if self.shown.get(i as usize).copied().unwrap_or(true) || count >= limit {
                continue;
            }
            let g = self.index.group.get(i as usize).copied().unwrap_or(0);
            if g != current {
                current = g;
                let label = self.index.groups.get(g as usize).cloned().flatten();
                self.push_section(label);
            }
            self.rows.push(Row::Action(i));
            count += 1;
        }
    }

    /// Non-empty query: groups ordered by their best match, actions by score within.
    fn ranked(&mut self, limit: usize) {
        self.group_rank.clear();
        self.group_rank.resize(self.index.groups.len(), u32::MAX);
        for (position, &(_, i)) in self.matches.iter().enumerate() {
            let g = self.index.group.get(i as usize).copied().unwrap_or(0) as usize;
            if let Some(slot) = self.group_rank.get_mut(g) {
                *slot = (*slot).min(position as u32);
            }
        }
        // Rows in (group rank, rank) order without a stable-sort buffer: walk the matches
        // once per listed group, in group-rank order. Groups are few; matches are scanned
        // O(groups x matches) but with no allocation.
        let mut order_limit = limit;
        let mut previous_rank = None;
        loop {
            let next = self
                .group_rank
                .iter()
                .enumerate()
                .filter(|(_, r)| **r != u32::MAX && previous_rank.is_none_or(|p| **r > p))
                .min_by_key(|(_, r)| **r)
                .map(|(g, r)| (g as u32, *r));
            let Some((g, rank)) = next else { break };
            previous_rank = Some(rank);
            if order_limit == 0 {
                break;
            }
            let label = self.index.groups.get(g as usize).cloned().flatten();
            self.push_section(label);
            for k in 0..self.matches.len() {
                let i = self.matches[k].1;
                if self.index.group.get(i as usize).copied() != Some(g) {
                    continue;
                }
                if order_limit == 0 {
                    break;
                }
                self.rows.push(Row::Action(i));
                order_limit -= 1;
            }
        }
    }

    fn layout(&mut self) {
        self.offsets.clear();
        let mut y = 0.0;
        self.offsets.push(y);
        for row in &self.rows {
            y += match row {
                Row::Header(_) => self.heights.header,
                Row::Action(i) => {
                    if self
                        .actions
                        .get(*i as usize)
                        .is_some_and(|a| a.description.is_some())
                    {
                        self.heights.action_with_description
                    } else {
                        self.heights.action
                    }
                }
            };
            self.offsets.push(y);
        }
    }

    // ------------------------------------------------------------------ reading

    pub fn actions(&self) -> &[SpotlightAction] {
        &self.actions
    }
    pub fn action(&self, index: u32) -> Option<&SpotlightAction> {
        self.actions.get(index as usize)
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn section_label(&self, section: u32) -> Option<&SharedString> {
        self.sections
            .get(section as usize)
            .and_then(|s| s.label.as_ref())
    }
    pub fn pinned(&self) -> &[SharedString] {
        &self.pinned
    }
    pub fn recents(&self) -> &[SharedString] {
        &self.recents
    }
    /// Number of listed actions (rows minus headers).
    pub fn result_count(&self) -> usize {
        self.rows.len() - self.sections.iter().filter(|s| s.label.is_some()).count()
    }
    /// Whether any action has an icon (rows then reserve the icon column).
    pub fn any_icon(&self) -> bool {
        self.any_icon
    }
    /// Top offset of `row` (and `offsets[rows.len()]` is the total height).
    pub fn row_top(&self, row: usize) -> f32 {
        self.offsets.get(row).copied().unwrap_or(0.0)
    }
    pub fn row_bottom(&self, row: usize) -> f32 {
        self.offsets.get(row + 1).copied().unwrap_or(0.0)
    }
    pub fn total_height(&self) -> f32 {
        self.offsets.last().copied().unwrap_or(0.0)
    }
    /// Rows intersecting `[top, top + height)`.
    pub fn visible_rows(&self, top: f32, height: f32) -> Range<usize> {
        visible_rows(&self.offsets, top, height)
    }
    /// The selected row index.
    pub fn selected_row(&self) -> Option<usize> {
        self.selected
            .and_then(|s| self.selectable.get(s))
            .map(|&r| r as usize)
    }
    pub fn selected_action(&self) -> Option<&SpotlightAction> {
        match self.rows.get(self.selected_row()?) {
            Some(Row::Action(i)) => self.actions.get(*i as usize),
            _ => None,
        }
    }

    // ------------------------------------------------------------------ navigation

    /// Highlight the first selectable action (a fresh listing).
    pub fn select_first(&mut self) -> bool {
        let first = (!self.selectable.is_empty()).then_some(0);
        let changed = self.selected != first;
        self.selected = first;
        changed
    }
    /// Move by `delta` selectable rows, wrapping. Returns whether the selection changed.
    pub fn move_by(&mut self, delta: isize) -> bool {
        let count = self.selectable.len() as isize;
        if count == 0 {
            return false;
        }
        let current = self.selected.map_or(-1, |s| s as isize);
        let next = (current + delta).rem_euclid(count) as usize;
        self.select_index(next)
    }
    /// Move by `delta` without wrapping (PageUp/PageDown).
    pub fn move_clamped(&mut self, delta: isize) -> bool {
        let count = self.selectable.len() as isize;
        if count == 0 {
            return false;
        }
        let current = self.selected.map_or(0, |s| s as isize);
        self.select_index((current + delta).clamp(0, count - 1) as usize)
    }
    /// Select the first selectable action of the next (or previous) section, wrapping
    /// (Tab / Shift+Tab).
    pub fn cycle_section(&mut self, forward: bool) -> bool {
        let n = self.sections.len();
        if n == 0 || self.selectable.is_empty() {
            return false;
        }
        let current = self
            .selected_row()
            .map(|row| self.section_of(row))
            .unwrap_or(0);
        for step in 1..=n {
            let s = if forward {
                (current + step) % n
            } else {
                (current + n * 2 - step) % n
            };
            if let Some(first) = self.first_selectable_in(s) {
                return self.select_index(first);
            }
        }
        false
    }
    fn section_of(&self, row: usize) -> usize {
        self.sections
            .iter()
            .rposition(|s| (s.first_row as usize) <= row)
            .unwrap_or(0)
    }
    fn first_selectable_in(&self, section: usize) -> Option<usize> {
        let start = self.sections.get(section)?.first_row;
        let end = self
            .sections
            .get(section + 1)
            .map_or(u32::MAX, |s| s.first_row);
        self.selectable
            .iter()
            .position(|&row| row >= start && row < end)
    }
    /// Select the action shown at `row` (pointer). Disabled rows and headers are ignored.
    pub fn select_row(&mut self, row: usize) -> bool {
        match self.selectable.iter().position(|&r| r as usize == row) {
            Some(index) => self.select_index(index),
            None => false,
        }
    }
    fn select_index(&mut self, index: usize) -> bool {
        if self.selected == Some(index) {
            return false;
        }
        self.selected = Some(index);
        true
    }
}

/// Rows intersecting `[top, top + height)` given cumulative `offsets` (`offsets[0] == 0`,
/// one more entry than rows). Binary search: O(log rows).
pub fn visible_rows(offsets: &[f32], top: f32, height: f32) -> Range<usize> {
    let rows = offsets.len().saturating_sub(1);
    if rows == 0 || height <= 0.0 {
        return 0..0;
    }
    let bottom = top + height;
    // First row whose bottom is below `top`.
    let start = offsets[1..].partition_point(|&end| end <= top).min(rows);
    // First row whose top is at or below `bottom`.
    let end = offsets[..rows]
        .partition_point(|&begin| begin < bottom)
        .max(start);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: RowHeights = RowHeights {
        header: 30.0,
        action: 40.0,
        action_with_description: 50.0,
    };

    fn model(actions: Vec<SpotlightAction>) -> SpotlightModel {
        let mut m = SpotlightModel::new(H);
        m.set_actions(actions);
        m
    }

    fn sample() -> Vec<SpotlightAction> {
        vec![
            SpotlightAction::new("new", "New file").group("File"),
            SpotlightAction::new("open", "Open file").group("File"),
            SpotlightAction::new("commit", "Commit")
                .group("Git")
                .keywords("save vcs"),
            SpotlightAction::new("push", "Push")
                .group("Git")
                .description("Upload"),
            SpotlightAction::new("theme", "Toggle theme"),
            SpotlightAction::new("off", "Disabled thing")
                .group("File")
                .disabled(true),
        ]
    }

    fn listing(m: &SpotlightModel) -> Vec<String> {
        m.rows()
            .iter()
            .map(|r| match r {
                Row::Header(s) => format!("# {}", m.section_label(*s).map_or("", |l| l.as_ref())),
                Row::Action(i) => m.action(*i).map_or(String::new(), |a| a.id.to_string()),
            })
            .collect()
    }

    #[test]
    fn browse_lists_ungrouped_first_then_groups_in_order() {
        let m = model(sample());
        assert_eq!(
            listing(&m),
            [
                "theme", "# File", "new", "open", "off", "# Git", "commit", "push"
            ]
        );
        assert_eq!(m.selected_action().map(|a| a.id.as_ref()), Some("theme"));
        assert_eq!(m.total_height(), 40.0 * 5.0 + 50.0 + 30.0 * 2.0);
    }

    #[test]
    fn pinned_and_recent_sections_lead_and_are_not_repeated() {
        let mut m = model(sample());
        m.set_pinned(vec!["push".into()]);
        m.set_recents(vec!["open".into(), "push".into(), "missing".into()]);
        assert_eq!(
            listing(&m),
            [
                "# Pinned", "push", "# Recent", "open", "theme", "# File", "new", "off", "# Git",
                "commit"
            ]
        );
        m.record_recent(&"commit".into());
        assert_eq!(m.recents()[0].as_ref(), "commit");
    }

    #[test]
    fn query_ranks_groups_by_best_match_and_skips_disabled_in_navigation() {
        let mut m = model(sample());
        m.set_query("push");
        assert_eq!(listing(&m), ["# Git", "push"]);
        m.set_query("file");
        // "Disabled thing" matches through its group keyword but stays unselectable.
        assert_eq!(listing(&m)[0], "# File");
        assert!(listing(&m).contains(&"off".to_owned()));
        for _ in 0..5 {
            m.move_by(1);
            assert_ne!(m.selected_action().map(|a| a.id.as_ref()), Some("off"));
        }
        m.set_query("save");
        assert_eq!(listing(&m), ["# Git", "commit"], "keywords match");
        m.set_query("zzz");
        assert!(m.rows().is_empty());
        assert_eq!(m.selected_action(), None);
        assert!(!m.move_by(1));
    }

    #[test]
    fn title_matches_outrank_keyword_matches() {
        let mut m = model(vec![
            SpotlightAction::new("k", "Something").keywords("open"),
            SpotlightAction::new("t", "Open"),
        ]);
        m.set_query("open");
        assert_eq!(listing(&m), ["t", "k"]);
    }

    #[test]
    fn recents_break_score_ties() {
        let mut m = model(vec![
            SpotlightAction::new("a", "Open"),
            SpotlightAction::new("b", "Open"),
        ]);
        m.set_query("open");
        assert_eq!(listing(&m), ["a", "b"]);
        m.set_recents(vec!["b".into()]);
        m.set_query("ope");
        m.set_query("open");
        assert_eq!(listing(&m), ["b", "a"]);
    }

    #[test]
    fn narrowing_rescans_previous_matches_only() {
        let mut actions: Vec<_> = (0..100)
            .map(|i| SpotlightAction::new(i.to_string(), format!("Item {i}")))
            .collect();
        actions.push(SpotlightAction::new("x", "Xylophone"));
        let mut m = model(actions);
        m.set_query("x");
        let after_first = m.scored;
        m.set_query("xy");
        assert_eq!(m.scored - after_first, 1, "only the single previous match");
        m.set_query("y");
        assert_eq!(
            m.scored - after_first - 1,
            101,
            "a new query rescans everything"
        );
    }

    #[test]
    fn narrowing_agrees_with_a_fresh_search() {
        let actions: Vec<_> = ["alpha beta", "alphabet", "beta", "gamma alpha", "al"]
            .iter()
            .enumerate()
            .map(|(i, t)| SpotlightAction::new(i.to_string(), *t))
            .collect();
        for query in ["a", "al", "alp", "alph", "alpha", "alphab"] {
            let mut incremental = model(actions.clone());
            for end in 1..=query.len() {
                incremental.set_query(&query[..end]);
            }
            let mut fresh = model(actions.clone());
            fresh.set_query(query);
            assert_eq!(listing(&incremental), listing(&fresh), "{query}");
        }
    }

    #[test]
    fn tab_cycles_sections_and_wraps() {
        let mut m = model(sample());
        let id = |m: &SpotlightModel| m.selected_action().map(|a| a.id.to_string());
        assert_eq!(id(&m).as_deref(), Some("theme"));
        assert!(m.cycle_section(true));
        assert_eq!(id(&m).as_deref(), Some("new"));
        m.cycle_section(true);
        assert_eq!(id(&m).as_deref(), Some("commit"));
        m.cycle_section(true);
        assert_eq!(id(&m).as_deref(), Some("theme"), "wraps");
        m.cycle_section(false);
        assert_eq!(id(&m).as_deref(), Some("commit"));
        m.move_by(1);
        assert_eq!(id(&m).as_deref(), Some("push"));
        m.move_by(1);
        assert_eq!(id(&m).as_deref(), Some("theme"), "arrows wrap");
        m.move_clamped(-10);
        assert_eq!(id(&m).as_deref(), Some("theme"));
        m.move_clamped(10);
        assert_eq!(id(&m).as_deref(), Some("push"));
    }

    #[test]
    fn live_updates_keep_the_selected_action() {
        let mut m = model(sample());
        m.move_by(2);
        assert_eq!(m.selected_action().map(|a| a.id.as_ref()), Some("open"));
        let mut more = sample();
        more.insert(0, SpotlightAction::new("first", "Aaa first"));
        m.set_actions(more);
        assert_eq!(m.selected_action().map(|a| a.id.as_ref()), Some("open"));
    }

    #[test]
    fn custom_filter_order_is_kept_within_groups() {
        let mut m = model(sample());
        let filter: FilterFn = Rc::new(|query, actions| {
            actions
                .iter()
                .enumerate()
                .filter(|(_, a)| a.id.len() == query.len())
                .map(|(i, _)| i)
                .rev()
                .collect()
        });
        m.set_filter(Some(filter));
        m.set_query("xxxx");
        // ids of length 4: open, push. Reversed order puts push (Git) first.
        assert_eq!(listing(&m), ["# Git", "push", "# File", "open"]);
    }

    #[test]
    fn limit_caps_listed_actions() {
        let mut m = model(sample());
        m.set_limit(Some(2));
        assert_eq!(listing(&m), ["theme", "# File", "new"]);
        m.set_query("e");
        assert_eq!(m.result_count(), 2);
    }

    #[test]
    fn visible_rows_binary_search() {
        let offsets = [0.0, 10.0, 20.0, 30.0, 40.0];
        assert_eq!(visible_rows(&offsets, 0.0, 15.0), 0..2);
        assert_eq!(visible_rows(&offsets, 10.0, 10.0), 1..2);
        assert_eq!(visible_rows(&offsets, 35.0, 100.0), 3..4);
        assert_eq!(visible_rows(&offsets, 100.0, 10.0), 4..4);
        assert_eq!(visible_rows(&[0.0], 0.0, 10.0), 0..0);
        let m = model(sample());
        assert_eq!(m.visible_rows(0.0, 45.0), 0..2);
    }
}
