//! `guiraffe-goldens`: review changed snapshot goldens after a gate.
//!
//! Finds changed golden PNGs (git diff against a base ref, plus untracked files a
//! `scripts/gate.sh --regen` wrote) for both backends and both themes, and shows before,
//! after and a diff heat map with the changed-pixel count and the max channel delta, with
//! zoom and pixel inspection. Accept or reject each golden with a reason, then export the
//! verdicts as a commit-message summary.
//!
//! State lives in one [`guiraffe_state::Store`] ([`review::Review`]). Every control is a
//! registered bus control ([`controls::paths`]), so shortcuts, Spotlight, MCP and scripts
//! drive the tool exactly as the pointer does. Image diffing ([`diff`]) and discovery
//! ([`discover`]) are pure and run off the UI thread.
pub mod controls;
pub mod diff;
pub mod discover;
#[cfg(test)]
mod gpui_tests;
pub mod review;
pub mod view;

use discover::{Golden, Source};
use gpui_kit::{App, AppContext, WindowHandle, WindowOptions};
use std::{collections::HashMap, sync::Arc};

/// Goldens held in memory: fixtures, tests and the demo.
#[derive(Default)]
pub struct MemorySource {
    pub before: HashMap<Arc<str>, Vec<u8>>,
    pub after: HashMap<Arc<str>, Vec<u8>>,
}

impl Source for MemorySource {
    fn before(&self, golden: &Golden) -> Option<Vec<u8>> {
        self.before.get(&golden.path).cloned()
    }
    fn after(&self, golden: &Golden) -> Option<Vec<u8>> {
        self.after.get(&golden.path).cloned()
    }
}

/// The store for a review over `source`, with its background loader installed.
pub fn store(source: Arc<dyn Source>, cx: &mut App) -> guiraffe_state::Store<review::Review> {
    let store = guiraffe_state::Store::new(review::Review::default(), cx);
    view::install_loader(&store, source, cx);
    store
}

/// Open the review window over `goldens`.
pub fn open(
    source: Arc<dyn Source>,
    goldens: Vec<Golden>,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<gpui_kit::component::Root>> {
    guiraffe_commands::init(cx);
    let store = store(source, cx);
    store.dispatch(review::SetGoldens(goldens), cx);
    cx.open_window(WindowOptions::default(), move |window, cx| {
        let app = cx.new(|cx| view::ReviewApp::new(store, window, cx));
        guiraffe_overlays::root(app, window, cx)
    })
}

/// Synthetic goldens for the demo, the tool's own snapshots and tests: one modified pair
/// per backend and theme, one added and one deleted.
pub fn demo() -> (MemorySource, Vec<Golden>) {
    use diff::{Pixels, encode_png};
    use discover::{Change, classify};
    let card = |dark: bool, shift: u32| {
        let (bg, fg) = if dark {
            ([24, 26, 32, 255], [220, 224, 232, 255])
        } else {
            ([248, 248, 250, 255], [40, 44, 52, 255])
        };
        let mut p = Pixels::filled(64, 40, bg);
        for y in 10..30 {
            for x in (8 + shift)..(40 + shift) {
                p.set(x, y, fg);
            }
        }
        p
    };
    let png = |p: &Pixels| encode_png(p).unwrap_or_default();
    let mut source = MemorySource::default();
    let mut goldens = Vec::new();
    for backend in ["upstream", "gpui-fast"] {
        for (theme, dark) in [("light", false), ("dark", true)] {
            let path = format!("crates/demo/tests/snapshots/{backend}/button/{theme}.png");
            if let Some(g) = classify(&path, Change::Modified) {
                source.before.insert(g.path.clone(), png(&card(dark, 0)));
                source.after.insert(
                    g.path.clone(),
                    png(&card(dark, if backend == "upstream" { 2 } else { 1 })),
                );
                goldens.push(g);
            }
        }
    }
    if let Some(g) = classify(
        "crates/demo/tests/snapshots/upstream/badge-light.png",
        Change::Added,
    ) {
        source.after.insert(g.path.clone(), png(&card(false, 4)));
        goldens.push(g);
    }
    if let Some(g) = classify(
        "crates/demo/tests/snapshots/upstream/old-dark.png",
        Change::Deleted,
    ) {
        source.before.insert(g.path.clone(), png(&card(true, 0)));
        goldens.push(g);
    }
    goldens.sort_by(|a, b| (&a.name, a.backend, a.theme).cmp(&(&b.name, b.backend, b.theme)));
    (source, goldens)
}
