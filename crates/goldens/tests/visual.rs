//! Goldens of the review tool itself, in both themes: the main window reviewing the demo
//! set with one verdict recorded and the diff of a modified golden open.
#![allow(clippy::unwrap_used, reason = "test assertions")]
use gpui_kit::{AppContext, px, size};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_goldens::{
    review::{Decide, Decision, LoadGoldens, LoadedGolden, Select, SetZoom},
    view::{ReviewApp, load},
};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use std::sync::Arc;

#[test]
fn review_window_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "review-light"),
        (Scheme::Dark, "review-dark"),
    ] {
        let mut snap = Snapshotter::new().unwrap().with_setup(move |cx| {
            guiraffe_commands::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let image = snap
            .render(size(px(960.), px(560.)), |window, cx| {
                let (source, goldens) = guiraffe_goldens::demo();
                let source = Arc::new(source);
                let store = guiraffe_goldens::store(source.clone(), cx);
                store.dispatch_now(LoadGoldens(goldens.clone()), cx);
                // Loads run in the background in the app; done up front here so the first
                // frame is complete.
                for golden in &goldens {
                    store.dispatch_now(
                        LoadedGolden(golden.path.clone(), load(&*source, golden).unwrap()),
                        cx,
                    );
                }
                store.dispatch_now(
                    Decide {
                        decision: Decision::Accept,
                        reason: Some("new badge".into()),
                    },
                    cx,
                );
                store.dispatch_now(Select(1), cx);
                store.dispatch_now(SetZoom(4.), cx);
                let app = cx.new(|cx| ReviewApp::new(store, window, cx));
                guiraffe_overlays::root(app, window, cx)
            })
            .unwrap();
        assert_snapshot(name, &image, &cfg).unwrap();
    }
}
