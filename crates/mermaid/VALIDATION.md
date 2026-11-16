# Mermaid visibility fix — 2026-11-16

Validated source: `3f58a34de39c443be741a1a30fca33aa253ca457` on `fix-mermaid-paint`, based on release/0.1 `56e3a5a`.

## Failure and fix

The renderer produced a raster, but the measurement wrapper used block layout.
Its flex child, containing an absolutely positioned image, received zero height.
Adding flex column layout gives the diagram its available height. Bounds updates
also skip unchanged measurements before entering an entity update.

Before the layout fix, the strengthened existing GPUI test
`a_flowchart_renders_to_a_visible_image` failed with:

```text
diagram body must fill the host, got 884 x 0
```

After the fix it passes. The raster test checks diagram ink, and the new scene
test checks visible pixels in standalone and Markdown views in both themes,
excluding status text and the placeholder. This exercises the shared viewer
through the real Markdown wrapper. No second rendering implementation was added.

## Automated checks

All commands ran in this worktree with `ARGV0` unset and
`CARGO_TARGET_DIR=/home/dan/projects/anteater/target`.

| Check | Upstream | gpui-fast |
| --- | --- | --- |
| Mermaid tests | 47 unit, 1 visual integration, 2 doc passed | same |
| Markdown with mermaid,syntect | 42 unit, 2 doc passed | same |
| Mermaid clippy, all targets, warnings denied | passed | passed |
| Showcase Mermaid and Markdown entries, light/dark | passed | passed |

```sh
cargo test -p guiraffe-mermaid
cargo test -p guiraffe-mermaid --features gpui-kit/gpui-fast
cargo clippy -p guiraffe-mermaid --all-targets -- -D warnings
cargo clippy -p guiraffe-mermaid --features gpui-kit/gpui-fast --all-targets -- -D warnings
cargo test -p guiraffe-markdown --features mermaid,syntect
cargo test -p guiraffe-markdown --features mermaid,syntect,gpui-kit/gpui-fast
SHOWCASE_ONLY=mermaid-diagrams cargo test -p guiraffe-showcase --test render
SHOWCASE_ONLY=markdown-viewer cargo test -p guiraffe-showcase --test render
SHOWCASE_ONLY=mermaid-diagrams cargo test -p guiraffe-showcase --test render --features gpui-fast
SHOWCASE_ONLY=markdown-viewer cargo test -p guiraffe-showcase --test render --features gpui-fast
cargo fmt -p guiraffe-mermaid -- --check
git diff --check
```

The new viewer references and four affected Mermaid showcase references were
visually inspected. The viewer and Mermaid showcase PNGs are byte-identical
between backends. All scene checks were rerun successfully with
`GUIRAFFE_UPDATE_SNAPSHOTS` unset. The Markdown showcase references did not change;
the dedicated embedded-view snapshots cover its diagram below the showcase fold.

## Native verification

Built the viewer example and showcase in release mode with gpui-fast, then ran
both on a private Xvfb display at 1400×900. Inspected light/dark captures of the
standalone flowchart, the showcase flowchart plus sequence diagram, and a full
embedded Markdown diagram after scrolling the document. The apps remained open
and rendered after theme changes; only the owned processes/display were stopped.

```sh
cargo build --release -p guiraffe-mermaid --example viewer --features gpui-kit/gpui-fast
cargo build --release -p guiraffe-showcase --features gpui-fast
```

Retained native embedded captures:

- [Light](validation/native-markdown-light.png)
- [Dark](validation/native-markdown-dark.png)

Native checks used gpui-fast only. Upstream was checked through the offscreen
renderer and TestAppContext tests. No full workspace suite, release integration,
merge, or push was performed. Other agents' interaction, flow and showcase-controls
work remains separate.

Claude's appended diagnostic test deliberately panicked. Its exact patch was
saved as `/tmp/claude-mermaid-probe.patch` before removing only that test from the
release checkout; no other release files were changed.
