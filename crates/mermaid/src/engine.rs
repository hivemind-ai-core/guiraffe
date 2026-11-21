//! Diagram rendering and its content cache.
//!
//! An [`Engine`] owns the SVG rasterizer and the cache, and is handed to a
//! background task for the duration of a render, so nothing here touches the
//! UI thread while parsing, layout or rasterization runs. Every entry point
//! is panic-isolated: the native Mermaid parsers are young, and a malformed
//! diagram must show an error block, not take down the window.

use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use gpui_kit::{DevicePixels, ParsedSvg, RenderImage, Size, SvgRenderer, SvgSize};
use mermaid_rs_renderer::{
    DiagramKind, Layout, LayoutConfig, Theme, compute_flowchart_staged, compute_layout,
    measure_svg_dimensions, parse_mermaid_strict, render_svg,
};

use crate::{
    sizing::{dimensions_are_usable, zoom_raster_size},
    source::{CacheKey, SourceError, admit},
};

/// Most cached diagrams kept, regardless of size.
pub const CACHE_ENTRIES: usize = 16;
/// Most bytes of SVG and rasters kept across cached diagrams.
pub const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Why a diagram could not be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// The source was rejected before parsing (see [`SourceError`]).
    Source(SourceError),
    /// The Mermaid parser rejected the source, with its message.
    Parse(String),
    /// The native renderer panicked. The input is preserved so the reader can
    /// edit it, and the message says so.
    Panic,
    /// The renderer produced dimensions that cannot be displayed.
    Dimensions,
    /// The rasterizer rejected the SVG it was given.
    Raster(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) => write!(f, "{error}"),
            Self::Parse(message) => write!(f, "{message}"),
            Self::Panic => write!(
                f,
                "The Mermaid renderer panicked on this diagram. Edit the source to recover."
            ),
            Self::Dimensions => write!(f, "The diagram rendered to an unusable size."),
            Self::Raster(message) => write!(f, "The diagram could not be rasterized: {message}"),
        }
    }
}

/// A diagram drawn once, before it is scaled for display.
#[derive(Debug, Clone)]
pub struct Drawn {
    pub svg: Arc<str>,
    /// Natural size in logical pixels at the theme's font size.
    pub natural: (f32, f32),
}

/// Parse, lay out and render a diagram to SVG.
///
/// Pure with respect to GPUI: no window, no raster. Panic-isolated.
pub fn draw(source: &str, theme: &Theme) -> Result<Drawn, RenderError> {
    match catch_unwind(AssertUnwindSafe(|| {
        draw_layout(source, theme, None, &|| false)
    })) {
        Ok(result) => result?
            .map(|frame| frame.drawing)
            .ok_or(RenderError::Dimensions),
        Err(_) => Err(RenderError::Panic),
    }
}

struct Frame {
    drawing: Drawn,
    anchor: Option<(f32, f32)>,
}

fn frame(layout: &Layout, theme: &Theme, config: &LayoutConfig) -> Result<Frame, RenderError> {
    let dims = measure_svg_dimensions(layout, config, None);
    if !dimensions_are_usable(dims.width, dims.height) {
        return Err(RenderError::Dimensions);
    }

    let anchor = if layout.kind == DiagramKind::Flowchart {
        layout
            .nodes
            .values()
            .find(|node| !node.hidden)
            .map(|node| (node.x, node.y))
    } else {
        None
    };
    Ok(Frame {
        drawing: Drawn {
            svg: render_svg(layout, theme, config).into(),
            natural: (dims.width, dims.height),
        },
        anchor,
    })
}

struct RenderStage<'a> {
    cancelled: &'a AtomicBool,
    publish: &'a mut dyn FnMut(StagedImage),
}

pub(crate) struct StagedImage {
    pub(crate) image: Image,
    /// Origin of the same node in both frames, measured in logical display pixels.
    pub(crate) anchor: Option<(f32, f32)>,
    pub(crate) scale: f32,
}

fn draw_layout(
    source: &str,
    theme: &Theme,
    stage: Option<&mut dyn FnMut(&Layout)>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<Frame>, RenderError> {
    let config = LayoutConfig::default();
    let parsed =
        parse_mermaid_strict(source).map_err(|error| RenderError::Parse(error.to_string()))?;
    let graph = &parsed.graph;
    let layout = if let Some(preview) = stage
        && graph.kind == DiagramKind::Flowchart
        && (graph.nodes.len() >= 256 || graph.edges.len() >= 256)
    {
        compute_flowchart_staged(graph, theme, &config, preview, cancelled)
    } else {
        (!cancelled()).then(|| compute_layout(graph, theme, &config))
    };
    layout
        .as_ref()
        .map(|layout| frame(layout, theme, &config))
        .transpose()
}

/// One render request: content identity, the source to draw, and what the
/// view currently needs in pixels.
#[derive(Clone)]
pub struct Request {
    pub key: CacheKey,
    /// Normalized source, already admitted by [`admit`].
    pub source: String,
    pub theme: Theme,
    /// Logical width available to the diagram. The diagram is never drawn
    /// wider than this.
    pub display_width: f32,
    pub zoom: f32,
    pub density: f32,
}

impl Request {
    /// A request for `source`, admitted and normalized, or the reason it was
    /// rejected.
    pub fn new(
        source: &str,
        theme: Theme,
        dark: bool,
        display_width: f32,
        zoom: f32,
        density: f32,
    ) -> Result<Self, SourceError> {
        let normalized = admit(source)?;
        let key = CacheKey::new(&normalized, dark, theme.font_size);
        Ok(Self {
            key,
            source: normalized,
            theme,
            display_width,
            zoom,
            density,
        })
    }
}

/// A drawn and rasterized diagram, ready to paint.
#[derive(Clone)]
pub struct Image {
    pub key: CacheKey,
    pub image: Arc<RenderImage>,
    pub svg: Arc<str>,
    /// Logical display size.
    pub width: f32,
    pub height: f32,
    /// Whether the layout in this image was reused from the cache.
    pub cached: bool,
}

struct Entry {
    key: CacheKey,
    source: String,
    svg: Arc<str>,
    natural: (f32, f32),
    anchor: Option<(f32, f32)>,
    parsed: ParsedSvg,
    raster: Option<((u32, u32), Arc<RenderImage>)>,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.svg.len()
            + self.source.len()
            + self
                .raster
                .as_ref()
                .map_or(0, |((w, h), _)| *w as usize * *h as usize * 4)
    }
}

/// Renderer and content cache. One instance serves as many diagrams as its
/// entry limit allows, and travels to whichever background task is drawing.
pub struct Engine {
    svg: SvgRenderer,
    cache: VecDeque<Entry>,
    rasterizations: usize,
    layouts: usize,
}

impl Engine {
    pub fn new(svg: SvgRenderer) -> Self {
        Self {
            svg,
            cache: VecDeque::new(),
            rasterizations: 0,
            layouts: 0,
        }
    }

    /// Number of times a diagram had to be laid out (not served from cache).
    pub fn layouts(&self) -> usize {
        self.layouts
    }

    /// Number of times a raster had to be produced (not served from cache).
    pub fn rasterizations(&self) -> usize {
        self.rasterizations
    }

    /// Cached entry count, for tests and diagnostics.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Bytes retained by the cache.
    pub fn bytes(&self) -> usize {
        self.cache.iter().map(Entry::bytes).sum()
    }

    /// Draw and rasterize one request.
    ///
    /// The whole body is panic-isolated. A panic leaves the cache as it was
    /// before the request, so the next frame retries cleanly.
    pub fn render(&mut self, request: &Request) -> Result<Image, RenderError> {
        match catch_unwind(AssertUnwindSafe(|| self.render_inner(request, None))) {
            Ok(result) => result?
                .map(|frame| frame.image)
                .ok_or(RenderError::Dimensions),
            Err(_) => Err(RenderError::Panic),
        }
    }

    pub(crate) fn render_staged(
        &mut self,
        request: &Request,
        cancelled: &AtomicBool,
        mut publish: impl FnMut(StagedImage),
    ) -> Result<Option<StagedImage>, RenderError> {
        let stage = RenderStage {
            cancelled,
            publish: &mut publish,
        };
        match catch_unwind(AssertUnwindSafe(|| self.render_inner(request, Some(stage)))) {
            Ok(result) => result,
            Err(_) => Err(RenderError::Panic),
        }
    }

    fn render_inner(
        &mut self,
        request: &Request,
        stage: Option<RenderStage<'_>>,
    ) -> Result<Option<StagedImage>, RenderError> {
        let reused = self
            .cache
            .iter()
            .position(|entry| entry.key == request.key)
            .and_then(|index| self.cache.remove(index));
        let cached = reused.is_some();
        let mut entry = match reused {
            Some(entry) => entry,
            None => {
                self.layouts += 1;
                let config = LayoutConfig::default();
                let rendered = if let Some(stage) = stage {
                    let cancelled = || stage.cancelled.load(Ordering::Relaxed);
                    let mut preview = |layout: &Layout| {
                        if !cancelled()
                            && let Ok(frame) = frame(layout, &request.theme, &config)
                            && let Ok(mut entry) = self.entry(request, frame)
                            && let Ok(image) = self.rasterize(request, &mut entry, false)
                            && !cancelled()
                        {
                            (stage.publish)(image);
                        }
                    };
                    draw_layout(
                        &request.source,
                        &request.theme,
                        Some(&mut preview),
                        &cancelled,
                    )?
                } else {
                    draw_layout(&request.source, &request.theme, None, &|| false)?
                };
                let Some(frame) = rendered else {
                    return Ok(None);
                };
                self.entry(request, frame)?
            }
        };
        let image = self.rasterize(request, &mut entry, cached)?;
        self.cache.push_front(entry);
        while self.cache.len() > CACHE_ENTRIES || self.bytes() > CACHE_BYTES {
            self.cache.pop_back();
        }
        Ok(Some(image))
    }

    fn entry(&self, request: &Request, frame: Frame) -> Result<Entry, RenderError> {
        let parsed = self
            .svg
            .parse_svg(frame.drawing.svg.as_bytes())
            .map_err(|error| RenderError::Raster(error.to_string()))?;
        Ok(Entry {
            key: request.key,
            source: request.source.clone(),
            svg: frame.drawing.svg,
            natural: frame.drawing.natural,
            anchor: frame.anchor,
            parsed,
            raster: None,
        })
    }

    fn rasterize(
        &mut self,
        request: &Request,
        entry: &mut Entry,
        cached: bool,
    ) -> Result<StagedImage, RenderError> {
        let (nw, nh) = entry.natural;
        let width = nw.min(request.display_width.max(100.0));
        let height = width * nh / nw;
        let scale = width / nw;
        let pixels = zoom_raster_size(width, height, request.zoom, request.density);
        let image = match &entry.raster {
            Some((size, image)) if *size == pixels => image.clone(),
            _ => {
                self.rasterizations += 1;
                let image = self
                    .svg
                    .render_parsed(
                        &entry.parsed,
                        SvgSize::ExactSize(Size::new(
                            DevicePixels(pixels.0 as i32),
                            DevicePixels(pixels.1 as i32),
                        )),
                    )
                    .map_err(|error| RenderError::Raster(error.to_string()))?;
                entry.raster = Some((pixels, image.clone()));
                image
            }
        };
        Ok(StagedImage {
            anchor: entry.anchor.map(|(x, y)| (x * scale, y * scale)),
            scale,
            image: Image {
                key: entry.key,
                image,
                svg: entry.svg.clone(),
                width,
                height,
                cached,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::Theme;

    const FLOW: &str = "flowchart LR\n  A[Start] --> B{Go?}\n  B -->|yes| C[Done]\n  B -->|no| A";

    fn engine() -> Engine {
        Engine::new(SvgRenderer::new(Arc::new(())))
    }

    fn request(source: &str) -> Request {
        Request::new(source, Theme::modern(), false, 800.0, 1.0, 1.0).expect("admitted")
    }

    #[test]
    fn flowcharts_draw_to_svg_with_text_and_edges() {
        let drawn = draw(FLOW, &Theme::modern()).expect("draws");
        assert!(drawn.svg.contains("<svg"));
        assert!(drawn.svg.contains("<text"));
        assert!(drawn.svg.contains("Start"));
        assert!(dimensions_are_usable(drawn.natural.0, drawn.natural.1));
    }

    #[test]
    fn sequence_and_state_diagrams_draw() {
        for source in [
            "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi",
            "stateDiagram-v2\n  [*] --> Ready\n  Ready --> Done",
        ] {
            let drawn = draw(source, &Theme::modern()).unwrap_or_else(|e| panic!("{e}"));
            assert!(drawn.svg.starts_with("<svg"), "{source}");
        }
    }

    #[test]
    fn invalid_source_is_a_parse_error_not_a_panic() {
        let error = draw("notADiagram\n  ???", &Theme::modern()).expect_err("rejected");
        assert!(matches!(error, RenderError::Parse(_)), "{error:?}");
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn engine_rasterizes_and_the_second_render_reuses_layout_and_raster() {
        let mut engine = engine();
        let request = request(FLOW);
        let first = engine.render(&request).expect("first");
        assert!(!first.cached);
        assert!(first.image.size(0).width.0 > 1 && first.image.size(0).height.0 > 1);
        assert_eq!(engine.layouts(), 1);
        assert_eq!(engine.rasterizations(), 1);

        let pixels = first.image.as_bytes(0).expect("raster bytes");
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0 && p[..3].iter().any(|c| *c < 220))
                .count()
                > 100,
            "a successful raster must contain diagram ink, not only an opaque background"
        );

        let second = engine.render(&request).expect("second");
        assert!(second.cached);
        assert!(Arc::ptr_eq(&first.image, &second.image));
        assert_eq!(engine.layouts(), 1, "layout reused");
        assert_eq!(engine.rasterizations(), 1, "raster reused");
    }

    #[test]
    fn zoom_reuses_the_layout_and_redraws_only_the_raster() {
        let mut engine = engine();
        let mut request = request(FLOW);
        let first = engine.render(&request).expect("first");
        request.zoom = 2.0;
        let zoomed = engine.render(&request).expect("zoomed");
        assert!(zoomed.cached, "same layout");
        assert!(Arc::ptr_eq(&first.svg, &zoomed.svg));
        assert!(!Arc::ptr_eq(&first.image, &zoomed.image));
        // Logical size is zoom-independent (the viewport scales it); the raster gets denser.
        assert!((zoomed.width - first.width).abs() < 1e-3);
        let px = |i: &Image| i.image.size(0).width.0;
        assert!(
            px(&zoomed).abs_diff(px(&first) * 2) <= 1,
            "2x zoom rasterizes 2x the pixels"
        );
        // Zooming out never rasterizes below 1x.
        request.zoom = 0.5;
        let out = engine.render(&request).expect("zoomed out");
        assert_eq!(px(&out), px(&first));
        assert_eq!(engine.layouts(), 1);
        assert_eq!(
            engine.rasterizations(),
            3,
            "1x again after 2x: one raster size is cached per entry"
        );
    }

    #[test]
    fn cancelling_after_the_preview_does_not_cache_a_partial_layout() {
        use std::fmt::Write as _;

        let mut source = String::from("flowchart TD\n  n0 --> n1\n");
        for i in 0..256 {
            writeln!(source, "  n{i}[Node {i}]").expect("write node");
        }
        let mut engine = engine();
        let request = request(&source);
        let cancelled = AtomicBool::new(false);
        let mut previews = 0;
        let result = engine
            .render_staged(&request, &cancelled, |frame| {
                previews += 1;
                assert!(frame.anchor.is_some());
                assert!(frame.image.image.size(0).width.0 > 1);
                cancelled.store(true, Ordering::Relaxed);
            })
            .expect("staged render");
        assert_eq!(previews, 1);
        assert!(result.is_none());
        assert!(engine.is_empty());
        assert_eq!(engine.rasterizations(), 1);
    }

    #[test]
    fn theme_change_is_a_different_cache_entry() {
        let mut engine = engine();
        let light = request(FLOW);
        let dark = Request::new(FLOW, Theme::dark(), true, 800.0, 1.0, 1.0).expect("admitted");
        assert_ne!(light.key, dark.key);
        engine.render(&light).expect("light");
        let drawn = engine.render(&dark).expect("dark");
        assert!(!drawn.cached);
        assert_eq!(engine.len(), 2);
        assert!(drawn.svg.contains("#1f2020"), "dark page color");
    }

    #[test]
    fn display_width_caps_a_wide_diagram_and_preserves_its_aspect() {
        let mut engine = engine();
        let wide = "flowchart LR\n  A --> B --> C --> D --> E --> F";
        let natural = draw(wide, &Theme::modern()).expect("draws").natural;
        let drawn = engine.render(&request(wide)).expect("draws");
        assert!(
            drawn.width <= 800.0 + 1e-3,
            "width {} exceeds the viewport",
            drawn.width
        );
        if natural.0 > 800.0 {
            assert!(
                (drawn.width - 800.0).abs() < 1e-3,
                "clamped to the viewport"
            );
            let expected_height = 800.0 * natural.1 / natural.0;
            assert!((drawn.height - expected_height).abs() < 1e-3);
        }
    }

    #[test]
    fn oversized_diagrams_are_rejected_before_parsing() {
        let source = "x".repeat(crate::source::MAX_SOURCE_BYTES + 1);
        let Err(error) = Request::new(&source, Theme::modern(), false, 800.0, 1.0, 1.0) else {
            panic!("oversized source must be rejected");
        };
        assert_eq!(
            error,
            SourceError::TooLarge {
                bytes: source.len()
            }
        );
    }

    #[test]
    fn cache_evicts_beyond_its_entry_limit() {
        let mut engine = engine();
        for index in 0..(CACHE_ENTRIES + 3) {
            let source = format!("flowchart LR\n  A{index} --> B{index}");
            engine.render(&request(&source)).expect("draws");
        }
        assert_eq!(engine.len(), CACHE_ENTRIES);
        assert!(engine.bytes() <= CACHE_BYTES);
    }

    #[test]
    fn cache_eviction_keeps_the_most_recent_entry() {
        let mut engine = engine();
        for index in 0..(CACHE_ENTRIES + 1) {
            engine
                .render(&request(&format!("flowchart LR\n  A{index} --> B{index}")))
                .expect("draws");
        }
        let newest = format!("flowchart LR\n  A{CACHE_ENTRIES} --> B{CACHE_ENTRIES}");
        assert!(engine.render(&request(&newest)).expect("draws").cached);
    }

    #[test]
    fn every_render_error_has_a_readable_message() {
        let errors = [
            RenderError::Source(SourceError::Empty),
            RenderError::Parse("bad".into()),
            RenderError::Panic,
            RenderError::Dimensions,
            RenderError::Raster("bad svg".into()),
        ];
        for error in errors {
            let message = error.to_string();
            assert!(!message.is_empty(), "every error explains itself");
            assert!(
                !message.ends_with('.') || message.len() > 1,
                "no bare punctuation: {message}"
            );
        }
        // The messages the view shows verbatim are full sentences.
        for error in [RenderError::Panic, RenderError::Dimensions] {
            assert!(error.to_string().ends_with('.'), "{error}");
        }
    }
}
