# GUIraffe 0.4 candidates

Collected 2026-11-21; not yet prioritised with the user. Main source: Raph Levien, "Advice for
the next 100 Rust GUIs" (linebender.org/blog/next-100-guis, 2026-11-21), plus "A Critique of
GPUI" (see plans/ROADMAP-0.3.md §E for the parts already in 0.3). Levien works on several of the
projects he recommends (Parley, AccessKit, Vello, Xilem), so check each claim against our own
measurements before acting.

## Rendering

- **Renderer feature gaps.** Levien: GPUI's 2D renderer "lacks full gradient support, blending,
  filters". Audit what our showcase and design recipes need (multi-stop/radial/conic gradients,
  blend modes, blur/backdrop filters, drop shadows beyond the current ones, masks, clip paths,
  stroke styles) and close the gaps in the vendored GPUI renderers, or render those regions
  through Vello into a texture. Decision record first: patch GPUI vs Vello-in-a-layer.
- **System compositor layers.** Hardware layers for video, animation and large static content:
  a CALayer-like API with per-platform polyfills, explicit hints like Flutter's
  `RepaintBoundary`/SwiftUI `.drawingGroup()`. Evaluate forest-rs/subduction. Pairs with the
  0.3 "animate everything" work and multi-window docking. Budget the RAM per layer and gate it.

## Text

- **Text stack review.** Parley is now the de-facto shared Rust text layout engine (Cosmic Text
  the alternative). Compare GPUI's text system against Parley for shaping, bidi, font fallback,
  rich text and IME; we already patch GPUI text for performance. Decide whether to adopt Parley
  for CodeViewer/Markdown/rich text, or contribute missing pieces.
- **Rich text editing** (Levien: not yet fully accessible anywhere): rich text editor component
  with full IME composition and AccessKit text ranges.

## Accessibility

- **Measure the accessibility cost.** egui loses most of its immediate-mode advantage to
  building and diffing the AccessKit tree every frame. Add an a11y-on vs a11y-off bench to the
  0.3 critique bench; the tree must update incrementally from the spec/30 control registry (stable
  ids are the same ids), never rebuilt per frame.
- Native screen-reader passes (Orca, NVDA, VoiceOver) for every component; text ranges for
  CodeViewer/Markdown/inputs.

## Architecture

- **Design vs execution split.** Levien argues the core of a toolkit is an incremental
  computation engine behind a clear spec. Our DataUi documents, catalogue and spec/30/31 already
  head there; 0.4 would formalise a spec'd "UI document → retained tree" boundary, with a
  conformance test suite in the WPT spirit, so alternative front-ends (DataUi, `ui!`, agents)
  share one engine.
- **Styling as an efficient CSS subset.** Theme tokens and StyleContext cover most cases;
  consider selector-like styling compiled to an incremental state machine (no sibling selectors),
  only if real apps need it.
- **Virtualised data-model spec.** A documented, incremental data-source interface shared by
  DataGrid, Sheet, Tree, lists, Timeline and CodeViewer (partly exists per component; unify).

## Platforms

- **Mobile** (only if wanted): Levien recommends native UI plus shared Rust logic; pure-Rust
  Android via android-view shows IME and gestures are workable. Out of scope until desktop
  platforms (macOS/Windows CI) are covered.
- macOS/Windows CI and testing; crates.io publishing (carried from 0.3 "later").
