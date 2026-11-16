use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px, rgb, size,
};
use guiraffe_test::snapshot::{SnapshotConfig, SnapshotOutcome, Snapshotter, assert_snapshot};

struct Scene {
    accent: u32,
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0xf8f9fa)).p_4().child(
            div()
                .w(px(120.))
                .h(px(40.))
                .rounded_md()
                .bg(rgb(self.accent)),
        )
    }
}

fn shot(accent: u32) -> image::RgbaImage {
    let mut s = Snapshotter::new().unwrap_or_else(|e| panic!("{e}"));
    s.render(size(px(200.), px(100.)), move |_, cx| {
        cx.new(|_| Scene { accent })
    })
    .unwrap_or_else(|e| panic!("{e:#}"))
}

#[test]
fn golden_lifecycle_on_the_headless_renderer() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let cfg = SnapshotConfig::new(dir.path());
    let first = shot(0x228be6);
    assert_eq!(first.dimensions(), (400, 200), "test window scale 2");
    assert!(
        first.pixels().any(|p| p.0[..3] == [0x22, 0x8b, 0xe6]),
        "scene actually rendered"
    );
    let out = assert_snapshot("button/blue", &first, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    assert!(matches!(out, SnapshotOutcome::Written(_)));
    // Deterministic: a fresh app renders identically and matches the golden.
    let again = shot(0x228be6);
    assert!(matches!(
        assert_snapshot("button/blue", &again, &cfg),
        Ok(SnapshotOutcome::Matched(_))
    ));
    // A visual change fails and leaves actual + diff images for review.
    let changed = shot(0xfa5252);
    let err = assert_snapshot("button/blue", &changed, &cfg)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("differs"), "{err}");
    let backend = if gpui_kit::GPUI_FAST { "gpui-fast" } else { "upstream" };
    assert!(dir.path().join(backend).join("button/blue.actual.png").exists());
    assert!(dir.path().join(backend).join("button/blue.diff.png").exists());
    // Accepting updates the golden.
    let accept = SnapshotConfig {
        update: true,
        ..cfg.clone()
    };
    assert!(matches!(
        assert_snapshot("button/blue", &changed, &accept),
        Ok(SnapshotOutcome::Written(_))
    ));
    assert!(matches!(
        assert_snapshot("button/blue", &changed, &cfg),
        Ok(SnapshotOutcome::Matched(_))
    ));
}

#[test]
fn unsafe_names_are_rejected() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let cfg = SnapshotConfig::new(dir.path());
    let img = image::RgbaImage::new(1, 1);
    for bad in ["", "../escape", "/abs", "a//b", "sp ace"] {
        assert!(assert_snapshot(bad, &img, &cfg).is_err(), "{bad:?}");
    }
}
