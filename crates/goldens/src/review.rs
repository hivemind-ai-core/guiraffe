//! Review state (one store) and its events. Pure: no GPUI types beyond the decoded images
//! the view shows, so the reducer logic is unit-tested without a window.
use crate::{
    diff::{DiffSummary, Pixels},
    discover::Golden,
};
use guiraffe_state::{Event, Fx};
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
    pub fn wire(self) -> &'static str {
        match self {
            Mode::Before => "before",
            Mode::After => "after",
            Mode::Diff => "diff",
            Mode::SideBySide => "side-by-side",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Mode::Before => "Before",
            Mode::After => "After",
            Mode::Diff => "Diff",
            Mode::SideBySide => "Side by side",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.wire() == value)
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
    pub reason: Arc<str>,
}

pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 16.;

#[derive(Debug, Clone, PartialEq)]
pub struct ViewSettings {
    pub mode: Mode,
    pub zoom: f32,
    /// The inspected pixel, in image coordinates.
    pub inspect: Option<(u32, u32)>,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Diff,
            zoom: 1.,
            inspect: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Review {
    pub goldens: Arc<[Golden]>,
    pub selected: usize,
    /// Verdicts by golden path.
    pub verdicts: Arc<BTreeMap<Arc<str>, Verdict>>,
    /// The reason typed for the next verdict.
    pub reason: Arc<str>,
    pub view: ViewSettings,
    pub loaded: Arc<BTreeMap<Arc<str>, Loaded>>,
    /// Discovery or load errors, shown in the status line.
    pub error: Option<Arc<str>>,
}

impl Review {
    pub fn current(&self) -> Option<&Golden> {
        self.goldens.get(self.selected)
    }
    pub fn current_loaded(&self) -> Option<&Loaded> {
        self.loaded.get(&self.current()?.path)
    }
    pub fn verdict(&self, golden: &Golden) -> Option<&Verdict> {
        self.verdicts.get(&golden.path)
    }
    pub fn decided(&self) -> usize {
        self.goldens
            .iter()
            .filter(|g| self.verdicts.contains_key(&g.path))
            .count()
    }

    /// A commit-message summary of the verdicts: a subject line, then accepted and
    /// rejected goldens with their reasons, then any still undecided.
    pub fn commit_summary(&self) -> String {
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut open = Vec::new();
        for golden in self.goldens.iter() {
            let line = |verdict: &Verdict| {
                let what = format!(
                    "{} ({}, {})",
                    golden.name,
                    golden.backend.label(),
                    golden.change.label()
                );
                if verdict.reason.is_empty() {
                    what
                } else {
                    format!("{what}: {}", verdict.reason)
                }
            };
            match self.verdict(golden) {
                Some(v) if v.decision == Decision::Accept => accepted.push(line(v)),
                Some(v) => rejected.push(line(v)),
                None => open.push(format!("{} ({})", golden.name, golden.backend.label())),
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
    .filter_map(|i| s.goldens.get(i))
    .filter(|g| !s.loaded.contains_key(&g.path))
    .cloned()
    .collect();
    if !wanted.is_empty() {
        fx.effect(LoadRequest(wanted));
    }
}

#[derive(Debug)]
pub struct SetGoldens(pub Vec<Golden>);
impl Event<Review> for SetGoldens {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.update(|s| {
            s.goldens = self.0.into();
            s.selected = 0;
            s.view.inspect = None;
        });
        request_load(fx);
    }
}

#[derive(Debug)]
pub struct Select(pub usize);
impl Event<Review> for Select {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let len = fx.state().goldens.len();
        let index = self.0.min(len.saturating_sub(1));
        if index != fx.state().selected {
            fx.update(|s| {
                s.selected = index;
                s.view.inspect = None;
            });
        }
        request_load(fx);
    }
}

#[derive(Debug)]
pub struct Step(pub i64);
impl Event<Review> for Step {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let target = (fx.state().selected as i64 + self.0).max(0) as usize;
        Select(target).handle(fx);
    }
}

#[derive(Debug)]
pub struct SetMode(pub Mode);
impl Event<Review> for SetMode {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.set_field(self.0, |s| &s.view.mode, |s, v| s.view.mode = v);
    }
}

#[derive(Debug)]
pub struct SetZoom(pub f32);
impl Event<Review> for SetZoom {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let zoom = if self.0.is_finite() {
            self.0.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.
        };
        fx.set_field(zoom, |s| &s.view.zoom, |s, v| s.view.zoom = v);
    }
}

/// Zoom in or out by one power-of-two step.
#[derive(Debug)]
pub struct ZoomStep(pub i32);
impl Event<Review> for ZoomStep {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let zoom = fx.state().view.zoom * 2f32.powi(self.0);
        SetZoom(zoom).handle(fx);
    }
}

#[derive(Debug)]
pub struct Inspect(pub Option<(u32, u32)>);
impl Event<Review> for Inspect {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.set_field(self.0, |s| &s.view.inspect, |s, v| s.view.inspect = v);
    }
}

#[derive(Debug)]
pub struct SetReason(pub Arc<str>);
impl Event<Review> for SetReason {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.set_field(self.0, |s| &s.reason, |s, v| s.reason = v);
    }
}

/// Record a verdict for the selected golden (the typed reason, unless one is given),
/// clear the reason and move to the next undecided golden.
#[derive(Debug)]
pub struct Decide {
    pub decision: Decision,
    pub reason: Option<Arc<str>>,
}
impl Event<Review> for Decide {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        let Some(path) = fx.state().current().map(|g| g.path.clone()) else {
            return;
        };
        let reason = self.reason.unwrap_or_else(|| fx.state().reason.clone());
        fx.update(|s| {
            Arc::make_mut(&mut s.verdicts).insert(
                path,
                Verdict {
                    decision: self.decision,
                    reason,
                },
            );
            s.reason = "".into();
            let len = s.goldens.len();
            if let Some(next) = (1..len)
                .map(|offset| (s.selected + offset) % len)
                .find(|&i| !s.verdicts.contains_key(&s.goldens[i].path))
            {
                s.selected = next;
                s.view.inspect = None;
            }
        });
        request_load(fx);
    }
}

#[derive(Debug)]
pub struct LoadedGolden(pub Arc<str>, pub Loaded);
impl Event<Review> for LoadedGolden {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.update(|s| {
            Arc::make_mut(&mut s.loaded).insert(self.0, self.1);
        });
    }
    fn undoable(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct Failed(pub Arc<str>);
impl Event<Review> for Failed {
    fn handle(self, fx: &mut Fx<'_, Review>) {
        fx.update(|s| s.error = Some(self.0));
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

    #[test]
    fn decide_records_reason_and_moves_to_the_next_undecided() {
        let mut engine = Engine::new(Review::default());
        run(&mut engine, SetGoldens(goldens()));
        run(&mut engine, SetReason("font hinting".into()));
        run(
            &mut engine,
            Decide {
                decision: Decision::Accept,
                reason: None,
            },
        );
        let s = engine.state().clone();
        assert_eq!(s.selected, 1);
        assert_eq!(&*s.reason, "");
        assert_eq!(
            s.verdicts
                .get("c/tests/snapshots/upstream/a-dark.png")
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
    fn navigation_zoom_and_mode_clamp() {
        let mut engine = Engine::new(Review::default());
        run(&mut engine, Step(1));
        assert_eq!(engine.state().selected, 0, "empty list stays put");
        run(&mut engine, SetGoldens(goldens()));
        run(&mut engine, Step(-5));
        assert_eq!(engine.state().selected, 0);
        run(&mut engine, Select(99));
        assert_eq!(engine.state().selected, 2);
        run(&mut engine, ZoomStep(10));
        assert_eq!(engine.state().view.zoom, MAX_ZOOM);
        run(&mut engine, SetZoom(f32::NAN));
        assert_eq!(engine.state().view.zoom, 1.);
        run(&mut engine, SetMode(Mode::SideBySide));
        assert_eq!(Mode::parse("side-by-side"), Some(engine.state().view.mode));
        assert!(Mode::parse("sideways").is_none());
        let summary = engine.state().commit_summary();
        assert!(
            summary.contains("0 accepted, 0 rejected, 3 undecided"),
            "{summary}"
        );
    }
}
