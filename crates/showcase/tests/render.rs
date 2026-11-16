//! Renders every showcase entry headlessly in light and dark (catches panics and blank demos)
//! and keeps a golden per entry so visual regressions in any component show up here.
use gpui_kit::{AppContext, px, size};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_showcase::{catalog::entries, shell::Showcase};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[test]
fn every_entry_renders_in_both_themes() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    let only = std::env::var("SHOWCASE_ONLY").ok();
    let mut failures = Vec::new();
    for e in entries() {
        if only.as_deref().is_some_and(|o| !slug(e.name).contains(o)) {
            continue;
        }
        for scheme in [Scheme::Light, Scheme::Dark] {
            let name = e.name;
            let mut s = Snapshotter::new().unwrap_or_else(|err| panic!("{err}"));
            let img = s
                .render(size(px(1100.), px(760.)), move |window, cx| {
                    guiraffe_showcase::init(cx);
                    apply(
                        GuiraffeTheme {
                            scheme,
                            ..Default::default()
                        },
                        cx,
                    );
                    cx.new(|cx| Showcase::new(Some(name), window, cx))
                })
                .unwrap_or_else(|err| panic!("{name}: {err:#}"));
            let tag = format!(
                "{}/{}",
                slug(e.name),
                if scheme == Scheme::Light {
                    "light"
                } else {
                    "dark"
                }
            );
            if let Err(err) = assert_snapshot(&tag, &img, &cfg) {
                failures.push(format!("{tag}: {err:#}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
