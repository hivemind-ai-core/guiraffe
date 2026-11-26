//! Review state: one `store!` store and its events. The reducer logic is unit-tested
//! without a window.
use crate::{
    diff::{DiffSummary, Pixels},
    discover::Golden,
};
use gpui_kit::SharedString;
use guiraffe_state::{Event, Fx, KeyedList, store};
use std::{collections::BTreeMap, sync::Arc};

/// Shared payload compared by identity, so large images never get compared byte by byte.
#[derive(Debug)]
pub struct Shared<T>(pub Arc<T>);
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// A golden after its two sides were read and compared (off the UI thread).
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub summary: DiffSummary,
    pub before: Option<Shared<Pixels>>,
    pub after: Option<Shared<Pixels>>,
    pub heat: Shared<Pixels>,
    /// GPU-ready images (BGRA), built with the pixels.
    pub images: Shared<crate::view::Images>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Before,
    After,
    #[default]
    Diff,
    SideBySide,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Before, Mode::After, Mode::Diff, Mode::SideBySide];
    pub fn label(self) -> &'static str {
        match self {
            Mode::Before => "Before",
            Mode::After => "After",
            Mode::Diff => "Diff",
            Mode::SideBySide => "Side by side",
        }
    }
    /// Command-id segment.
    pub fn slug(self) -> &'static str {
        match self {
            Mode::Before => "before",
            Mode::After => "after",
            Mode::Diff => "diff",
            Mode::SideBySide => "side-by-side",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub decision: Decision,
    pub reason: SharedString,
}

/// One list row. The verdict lives on the row, so a store-bound row view re-renders alone
/// when it changes. Rows read the selection through a derived binding.
#[derive(Debug, Clone, PartialEq, guiraffe_state::Lens)]
pub struct Row {
    pub golden: Golden,
    pub verdict: Option<Verdict>,
}

pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 16.;
pub const ZOOM_STEP: f32 = 0.25;

store! {
    pub Review {
        /// Keyed by position (the list never reorders).
        goldens: KeyedList<u32, Row> = KeyedList::new(),
        selected: u32 = 0,
        /// The reason typed for the next verdict.
        reason: SharedString = SharedString::default(),
        mode: Mode = Mode::Diff,
        zoom: f32 = 1.0 where 0.25..=16.0,
        /// The inspected pixel, in image coordinates.
        inspect: Option<(u32, u32)> = None #[no_undo],
        loaded: Arc<BTreeMap<Arc<str>, Loaded>> = Arc::default() #[no_undo],
        /// Discovery or load errors, shown in the status line.
        error: Option<SharedString> = None #[no_undo],
    }
}

impl Review {
    pub fn current(&self) -> Option<&Row> {
        self.goldens.get(&self.selected)
    }
    pub fn current_loaded(&self) -> Option<&Loaded> {
        self.loaded.get(&self.current()?.golden.path)
    }
    pub fn decided(&self) -> usize {
        self.goldens
            .iter()
            .filter(|(_, row)| row.verdict.is_some())
            .count()
    }

    /// A commit-message summary of the verdicts: a subject line, then accepted and
    /// rejected goldens with their reasons, then any still undecided.
    pub fn commit_summary(&self) -> String {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut open = Vec::new();
        for (_, row) in self.goldens.iter() {
            let g = &row.golden;
            let what = format!("{} ({}, {})", g.name, g.backend.label(), g.change.label());
            match &row.verdict {
                Some(v) => {
                    let line = if v.reason.is_empty() {
                        what
                    } else {
                        format!("{what}: {}", v.reason)
                    };
                    if v.decision == Decision::Accept {
                        accepted.push(line)
                    } else {
                        rejected.push(line)
                    }
                }
                None => open.push(format!("{} ({})", g.name, g.backend.label())),
            }
        }
        let mut out = format!(
            "Review goldens: {} accepted, {} rejected, {} undecided\n",
            accepted.len(),
            rejected.len(),
            open.len()
        );
        for (title, rows) in [
            ("Accepted", accepted),
            ("Rejected", rejected),
            ("Undecided", open),
        ] {
            if !rows.is_empty() {
                out.push_str(&format!("\n{title}:\n"));
                for row in rows {
                    out.push_str(&format!("- {row}\n"));
                }
            }
        }
        out
    }
}

/// Effect: read and compare these goldens off the UI thread (the current one first,
/// then its neighbours, so stepping through the list never waits).
#[derive(Debug, Clone, PartialEq)]
pub struct LoadRequest(pub Vec<Golden>);

fn request_load(fx: &mut Fx<'_, Review>) {
    let s = fx.state();
    let wanted: Vec<Golden> = [
        Some(s.selected),
        s.selected.checked_add(1),
        s.selected.checked_sub(1),
    ]
    .into_iter()
    .flatten()
    .filter_map(|k| s.goldens.get(&k))
    .filter(|row| !s.loaded.contains_key(&row.golden.path))
    .map(|row| row.golden.clone())
    .collect();
    if !wanted.is_empty() {
        fx.effect(LoadRequest(wanted));
    }
}

/// Move the selection to `key`: one field write. Only the two rows whose derived selection
/// changed render again.
fn select(fx: &mut Fx<'_, Review>, key: u32) {
    let old = fx.state().selected;
    if !fx.state().goldens.contains(&key) {
        return;
    }
    if old != key {
        fx.set_at(Review::selected, key);
        fx.set_at(Review::inspect, None);
    }
    request_load(fx);
}

#[derive(Debug)]
pub struct LoadGoldens(pub Vec<Golden>);
impl Event<Review> for LoadGoldens {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.update(|s| {
            s.goldens = self
                .0
                .into_iter()
                .enumerate()
                .map(|(i, golden)| {
                    (
                        i as u32,
                        Row {
                            golden,
                            verdict: None,
                        },
                    )
                })
                .collect();
            s.selected = 0;
            s.inspect = None;
        });
        request_load(fx);
    }
}

/// Select a golden by key (its position).
#[derive(Debug)]
pub struct Select(pub u32);
impl Event<Review> for Select {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let last = (fx.state().goldens.len() as u32).saturating_sub(1);
        select(fx, self.0.min(last));
    }
}

#[derive(Debug)]
pub struct Step(pub i64);
impl Event<Review> for Step {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let target = (i64::from(fx.state().selected) + self.0).max(0) as u32;
        Select(target).handle(fx);
    }
}

#[derive(Debug)]
pub struct Inspect(pub Option<(u32, u32)>);
impl Event<Review> for Inspect {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.set_at(Review::inspect, self.0);
    }
}

/// Effect: put the commit-message summary on the clipboard.
#[derive(Debug, Clone, PartialEq)]
pub struct CopySummary(pub String);

/// Export the verdicts: the summary goes to the clipboard (an effect, so the handler stays
/// pure and testable without a window).
#[derive(Debug)]
pub struct Export;
impl Event<Review> for Export {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let summary = fx.state().commit_summary();
        fx.effect(CopySummary(summary));
    }
    fn undoable(&self) -> bool {
        false
    }
}

/// Record a verdict for the selected golden (the typed reason, unless one is given),
/// clear the reason and move to the next undecided golden.
#[derive(Debug)]
pub struct Decide {
    pub decision: Decision,
    pub reason: Option<SharedString>,
}
impl Event<Review> for Decide {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let key = fx.state().selected;
        if !fx.state().goldens.contains(&key) {
            return;
        }
        let reason = self.reason.unwrap_or_else(|| fx.state().reason.clone());
        fx.set_at(
            Review::goldens.by_key(key).then(Row::verdict),
            Some(Verdict {
                decision: self.decision,
                reason,
            }),
        );
        fx.set_at(Review::reason, SharedString::default());
        let s = fx.state();
        let len = s.goldens.len() as u32;
        let next = (1..len)
            .map(|offset| (key + offset) % len)
            .find(|k| s.goldens.get(k).is_some_and(|row| row.verdict.is_none()));
        match next {
            Some(next) => select(fx, next),
            None => request_load(fx),
        }
    }
}

#[derive(Debug)]
pub struct LoadedGolden(pub Arc<str>, pub Loaded);
impl Event<Review> for LoadedGolden {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let mut loaded = (*fx.state().loaded).clone();
        loaded.insert(self.0, self.1);
        fx.set_at(Review::loaded, Arc::new(loaded));
    }
    fn undoable(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct Failed(pub SharedString);
impl Event<Review> for Failed {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.set_at(Review::error, Some(self.0));
    }
    fn undoable(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::*;
    use crate::discover::{Change, classify};
    use guiraffe_state::{Coeffects, Engine};

    fn run(engine: &mut Engine<Review>, event: impl Event<Review>) {
        engine.dispatch(event);
        engine.process(&Coeffects::default());
    }

    fn goldens() -> Vec<Golden> {
        ["a-dark", "a-light", "b-dark"]
            .iter()
            .map(|n| {
                classify(
                    &format!("c/tests/snapshots/upstream/{n}.png"),
                    Change::Modified,
                )
                .unwrap()
            })
            .collect()
    }

    fn selected_rows(s: &Review) -> Vec<u32> {
        s.goldens
            .keys()
            .filter(|k| **k == s.selected)
            .copied()
            .collect()
    }

    #[test]
    fn decide_records_reason_and_moves_to_the_next_undecided() {
        let mut engine = Engine::new(Review::default());
        run(&mut engine, LoadGoldens(goldens()));
        run(&mut engine, Review::set().reason("font hinting".into()));
        run(
            &mut engine,
            Decide {
                decision: Decision::Accept,
                reason: None,
            },
        );
        let s = engine.state().clone();
        assert_eq!(s.selected, 1);
        assert_eq!(selected_rows(&s), [1], "exactly one row is selected");
        assert_eq!(&*s.reason, "");
        assert_eq!(
            s.goldens
                .get(&0)
                .and_then(|r| r.verdict.as_ref())
                .map(|v| &*v.reason),
            Some("font hinting")
        );
        run(&mut engine, Step(1));
        run(
            &mut engine,
            Decide {
                decision: Decision::Reject,
                reason: Some("clipped".into()),
            },
        );
        // Wraps around to the one still undecided.
        assert_eq!(engine.state().selected, 1);
        run(
            &mut engine,
            Decide {
                decision: Decision::Accept,
                reason: None,
            },
        );
        assert_eq!(engine.state().decided(), 3);
        let summary = engine.state().commit_summary();
        assert!(
            summary.starts_with("Review goldens: 2 accepted, 1 rejected, 0 undecided"),
            "{summary}"
        );
        assert!(
            summary.contains("- b-dark (upstream, modified): clipped"),
            "{summary}"
        );
        assert!(
            summary.contains("- a-dark (upstream, modified): font hinting"),
            "{summary}"
        );
    }

    #[test]
    fn navigation_and_zoom_clamp() {
        let mut engine = Engine::new(Review::default());
        run(&mut engine, Step(1));
        assert_eq!(engine.state().selected, 0, "empty list stays put");
        run(&mut engine, LoadGoldens(goldens()));
        run(&mut engine, Step(-5));
        assert_eq!(engine.state().selected, 0);
        run(&mut engine, Select(99));
        assert_eq!(engine.state().selected, 2);
        assert_eq!(selected_rows(engine.state()), [2]);
        run(&mut engine, Review::set().zoom(99.));
        assert_eq!(engine.state().zoom, MAX_ZOOM, "the store clamps");
        assert_eq!(Mode::SideBySide.index(), 3);
        let summary = engine.state().commit_summary();
        assert!(
            summary.contains("0 accepted, 0 rejected, 3 undecided"),
            "{summary}"
        );
    }
}
