//! Visual snapshot testing on GPUI's headless wgpu renderer.
//!
//! ```ignore
//! let mut snap = Snapshotter::new()?.with_setup(|cx| guiraffe_core::init(cx));
//! let image = snap.render(size(px(320.), px(200.)), |_, cx| cx.new(|_| MyView))?;
//! assert_snapshot("button/filled-light", &image, &SnapshotConfig::new("tests/snapshots"))?;
//! ```
//! Goldens live at `<dir>/<name>.png`. When a golden is missing (or `update` is set) the image
//! is written and the check passes; on mismatch `<name>.actual.png` and `<name>.diff.png` are
//! written next to it for review.
//!
//! GPUs round anti-aliasing differently. Where a named renderer can't match the shared golden,
//! `<name>.<renderer>.png` replaces it for that renderer only, compared with the same tolerance
//! (CI sets `GUIRAFFE_SNAPSHOT_RENDERER=lavapipe`).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result, bail};
use gpui_kit::{App, Entity, HeadlessAppContext, Pixels, Render, Size, Window};
use image::RgbaImage;

use crate::diff::{DiffReport, Tolerance, diff, diff_image};

type Setup = Box<dyn Fn(&mut App)>;

pub struct Snapshotter {
    setup: Option<Setup>,
    assets: Arc<dyn gpui_kit::AssetSource>,
}

impl Snapshotter {
    /// A snapshotter with the full icon set (guiraffe-icons), so icon components render.
    pub fn new() -> Result<Self> {
        Ok(Self {
            setup: None,
            assets: Arc::new(guiraffe_icons::GuiraffeAssets),
        })
    }

    /// Use a custom asset source (e.g. the app's own icons and images).
    pub fn with_assets(mut self, assets: impl gpui_kit::AssetSource) -> Self {
        self.assets = Arc::new(assets);
        self
    }

    /// Run `f` in every fresh app before building the view (init themes, fonts, globals).
    pub fn with_setup(mut self, f: impl Fn(&mut App) + 'static) -> Self {
        self.setup = Some(Box::new(f));
        self
    }

    /// Render `build` into a fresh headless window of `size` (logical pixels; the test window
    /// scale is 2, so the image is twice as large) and capture it.
    pub fn render<V: Render + 'static>(
        &mut self,
        size: Size<Pixels>,
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
    ) -> Result<RgbaImage> {
        self.render_after(size, build, |_, _| {})
    }

    /// [`Self::render`], running `interact` after the first frame (hover an element, press a
    /// key) and capturing once the window has settled.
    pub fn render_after<V: Render + 'static>(
        &mut self,
        size: Size<Pixels>,
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
        interact: impl FnOnce(&mut Window, &mut App),
    ) -> Result<RgbaImage> {
        let mut interact = Some(interact);
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            self.assets.clone(),
            gpui_kit::platform::current_headless_renderer,
        );
        if let Some(setup) = &self.setup {
            cx.update(|cx| setup(cx));
        }
        let window = cx
            .open_window(size, build)
            .context("open headless window")?;
        // Two frames: assets (SVG icons, images) load during the first and paint in the second.
        for _ in 0..2 {
            cx.update_window(window.into(), |_, window, cx| {
                let _ = window.draw(cx);
            })?;
            cx.run_until_parked();
            if let Some(interact) = interact.take() {
                cx.update_window(window.into(), |_, window, cx| interact(window, cx))?;
                cx.run_until_parked();
            }
        }
        cx.capture_screenshot(window.into())
            .context("capture (is a wgpu adapter available?)")
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    pub dir: PathBuf,
    pub tolerance: Tolerance,
    /// Overwrite goldens instead of comparing (accepting new output). With a `renderer`, only
    /// that renderer's goldens are written, and only where the shared golden doesn't match.
    pub update: bool,
    /// The renderer whose own goldens take precedence over the shared ones.
    pub renderer: Option<String>,
}

impl SnapshotConfig {
    /// Goldens live in `<dir>/<backend>/`, where backend is `upstream` or `gpui-fast`: the two
    /// GPUI backends anti-alias curves slightly differently, so each keeps its own goldens
    /// rather than hiding real differences behind a loose tolerance.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let backend = if gpui_kit::GPUI_FAST {
            "gpui-fast"
        } else {
            "upstream"
        };
        Self {
            dir: dir.into().join(backend),
            tolerance: Tolerance::DEFAULT,
            update: false,
            renderer: None,
        }
    }

    /// Opt-in convenience for test binaries: `update` when `GUIRAFFE_UPDATE_SNAPSHOTS=1`, and
    /// `renderer` from `GUIRAFFE_SNAPSHOT_RENDERER`.
    /// (Read explicitly by the caller, never implicitly by the library.)
    pub fn update_from_env(mut self) -> Self {
        self.update = std::env::var("GUIRAFFE_UPDATE_SNAPSHOTS").is_ok_and(|v| v == "1");
        self.renderer = std::env::var("GUIRAFFE_SNAPSHOT_RENDERER")
            .ok()
            .filter(|v| !v.is_empty());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotOutcome {
    Matched(DiffReport),
    Written(PathBuf),
}

fn safe_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && !name.starts_with('/')
        && name
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'));
    if ok {
        Ok(())
    } else {
        bail!("invalid snapshot name {name:?}: use [A-Za-z0-9-_./], no '..'")
    }
}

/// Compare `image` against the golden `name`, writing it if absent or when updating.
pub fn assert_snapshot(
    name: &str,
    image: &RgbaImage,
    cfg: &SnapshotConfig,
) -> Result<SnapshotOutcome> {
    safe_name(name)?;
    let shared = cfg.dir.join(format!("{name}.png"));
    // The renderer's own golden. Without a shared golden there is nothing to override.
    let own = match &cfg.renderer {
        Some(renderer) if shared.exists() => {
            let name = format!("{name}.{renderer}");
            safe_name(&name)?;
            Some(cfg.dir.join(format!("{name}.png")))
        }
        _ => None,
    };
    let golden = match &own {
        Some(own) if own.exists() => own.clone(),
        _ => shared,
    };
    if !golden.exists() || (cfg.update && own.is_none()) {
        write(&golden, image)?;
        return Ok(SnapshotOutcome::Written(golden));
    }
    let expected = image::open(&golden)
        .with_context(|| format!("read golden {}", golden.display()))?
        .to_rgba8();
    let report = diff(
        (expected.as_raw(), expected.width(), expected.height()),
        (image.as_raw(), image.width(), image.height()),
        cfg.tolerance,
    );
    if report.matches(cfg.tolerance) {
        return Ok(SnapshotOutcome::Matched(report));
    }
    if cfg.update
        && let Some(own) = own
    {
        write(&own, image)?;
        return Ok(SnapshotOutcome::Written(own));
    }
    let actual = cfg.dir.join(format!("{name}.actual.png"));
    write(&actual, image)?;
    if report.size_mismatch.is_none() {
        let d = diff_image(expected.as_raw(), image.as_raw(), cfg.tolerance);
        if let Some(img) = RgbaImage::from_raw(image.width(), image.height(), d) {
            write(&cfg.dir.join(format!("{name}.diff.png")), &img)?;
        }
    }
    bail!(
        "snapshot {name:?} differs: {report:?}\n  golden: {}\n  actual: {}\n  accept with SnapshotConfig::update",
        golden.display(),
        actual.display()
    )
}

fn write(path: &Path, image: &RgbaImage) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    image
        .save(path)
        .with_context(|| format!("write {}", path.display()))
}
