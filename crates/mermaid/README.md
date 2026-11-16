# guiraffe-mermaid

Standalone Mermaid viewer: parse, lay out and rasterize a diagram off the UI
thread, cache by content, and show it in a pan/zoom viewport that follows the
GUIraffe theme.

Ported from `../gui/workbench/crates/mer` (`document`, `renderer`) with
`mermaid-rs-renderer = 0.3.1`.

## Layers

| module | what it owns | testable without a window |
|---|---|---|
| `source` | admission limits, source normalization, content-hash cache key | yes |
| `sizing` | bounded raster size at a density, aspect preserved | yes |
| `theme` | `GuiraffeTheme` surfaces/palette → `mermaid_rs_renderer::Theme` | yes |
| `viewport` | zoom, pan, fit, zoom-about-a-point, clamping | yes |
| `engine` | parse → layout → SVG → raster, LRU cache, panic isolation | yes (rasterizer only) |
| `view` | `MermaidView` / `MermaidViewState`: measure, schedule, paint, interact | GPUI tests |

## Behaviour

- **Off-thread.** The engine travels to a background task for each render and
  returns with the result, so there is no cache mutex and no rasterization on
  the UI thread. A newer source supersedes an in-flight render; only the newest
  revision is published.
- **Cached by content.** Normalized source hash + theme + font size. Trailing
  whitespace and outer blank lines are ignored, so a fence streaming into an
  agent chat keeps one identity as it settles. Zoom reuses the layout and only
  re-rasterizes.
- **Panic-isolated.** The native parsers are young: `catch_unwind` wraps parse,
  layout, rasterization and the whole cache path. A panic or a parse error
  shows an error block with the message, and the diagram recovers as soon as
  the source is fixed.
- **Bounded memory.** 64 MiB / 16 entries, and a raster of at most 4096 px per
  edge and 4 Mpx total; display size may exceed raster size.
- **Input.** Drag to pan, wheel to zoom about the pointer, double-click to fit.
  `MermaidViewState::{zoom_in, zoom_out, fit, reset_zoom, svg}` exist for hosts
  that draw their own controls.

## Known gaps

- **No link callbacks from inside diagrams.** The renderer emits plain SVG text
  and shapes; node `click` directives are not wired to GPUI handlers.
- **Pan uses pointer deltas, not a drag gesture.** A host that places the view
  inside a scroll container will see both pan and scroll; give it a fixed-height
  box (the example and the Markdown embed both do).
- **No virtualized diagram list.** Each `MermaidView` owns one diagram and one
  engine; a page with dozens of diagrams creates one engine per diagram. A
  shared engine behind a service would be the next step if that shows up.
- **Theme changes repaint but do not reparse.** A diagram is re-rendered for the
  new scheme (the cache key includes it), which costs one layout per visible
  diagram on a theme switch.

## Example

```sh
cargo run -p guiraffe-mermaid --example viewer
```

Flowchart, sequence, state and a deliberately broken sample; previous/next,
zoom, fit, 100%, copy SVG, theme toggle.
