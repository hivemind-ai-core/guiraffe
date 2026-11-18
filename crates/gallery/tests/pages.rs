//! Every gallery page renders, in light and dark, through the real shell + router.
//! Goldens double as a visual regression suite for the whole toolkit.
//! They capture the default build; the `devtools` feature adds a toolbar button (covered by
//! `tests/devtools.rs`), so this suite is skipped there instead of keeping a second golden set.
#![cfg(not(feature = "devtools"))]

use gpui_kit::{AppContext, px, size};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_gallery::{Page, shell::Gallery};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

#[test]
fn every_page_in_both_schemes() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        for page in Page::ALL {
            let mut snap = Snapshotter::new()
                .unwrap_or_else(|e| panic!("{e}"))
                .with_setup(move |cx| {
                    guiraffe_gallery::init(cx);
                    apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                });
            let image = snap
                .render(size(px(1000.), px(680.)), move |window, cx| {
                    let view = cx.new(|cx| Gallery::new(page, cx));
                    guiraffe_overlays::root(view, window, cx)
                })
                .unwrap_or_else(|e| panic!("{page:?}: {e:#}"));
            let name = format!(
                "{}-{}",
                page.title().to_lowercase().replace(' ', "-"),
                if scheme == Scheme::Dark {
                    "dark"
                } else {
                    "light"
                }
            );
            assert_snapshot(&name, &image, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
        }
    }
}
