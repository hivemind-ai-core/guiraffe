# Linux visual snapshots and e2e probe spike

**Result:** deterministic Linux frame capture is feasible with the pinned GPUI.
Five fresh native app launches under Xvfb/llvmpipe produced identical pixels,
and five fresh offscreen contexts did so in each of two adapter setups.
Software Vulkan/lavapipe and Weston remain unverified because they are absent.
This is an experimental probe, not a golden-image suite or production service.
Investigation date: 2026-11-16. Starting checkout: `f96e93a` on `spike-visual`;
Implementation commit: `e87cd30`. The
workspace dependency is exactly `gpui-kit =0.9.1`, lockfile resolves the upstream
GPUI family to `gpui-pre* 0.3.12`. Sources below refer to those exact cached versions.

## What the installed sources actually support

The design's macOS-only warning needs qualification for the currently pinned dependencies:

- `gpui-pre-0.3.12/src/app/test_context.rs::VisualTestContext` drives layout and
  interaction on a test window; it is not itself a screenshot renderer.
- `app/visual_test_context.rs::VisualTestAppContext` still documents a real macOS
  platform and offscreen compositor windows. Do not assume that constructing it
  with a Linux platform adds screenshot support to native Linux windows.
- `Window::render_to_image` (behind `test-support`) delegates to
  `PlatformWindow::render_to_image`. The default implementation errors; neither
  the Linux X11 nor Wayland native window implements it. The ordinary Linux
  headless platform window also discards scenes and has a mock atlas: setting
  `ZED_HEADLESS` alone does **not** produce pixels.
- A separate, usable API exists in the inspected source:
  `HeadlessAppContext::with_platform(text_system, assets, renderer_factory)`.
  `gpui_kit::platform::current_headless_renderer()` selects
  `gpui-pre-wgpu::WgpuHeadlessRenderer` on Linux. Test windows then get a real
  sprite atlas and `render_to_image` implementation. `window.draw(cx)` must run
  before capture. `crates/test/examples/offscreen.rs` exercises this path.
  WGPU's factory enables both Vulkan and OpenGL, so a successful capture alone
  does not prove that Vulkan, lavapipe, or a particular GPU was selected.
- The inspected `/tmp/gf-src/gpui-fast-0.1.15` tar extraction has the same
  `HeadlessAppContext`, test-window renderer integration and screenshot API;
  the cached companion `gpui-fast-platform-0.1.15` selects its WGPU headless
  renderer on Linux. This is source evidence; this spike does not build or
  claim runtime parity for the Fast backend.

`screen-capture` is a different facility: screen-sharing sources, not a
synchronous renderer snapshot. GPUI core's feature enables `zed-scap`; the
platform and Linux crates forward it. The `gpui-kit 0.9.1` facade does not expose
this feature directly. X11 `screen_capture_sources` calls
`scap_screen_capture::scap_screen_sources`; Wayland advertises support but returns
`Wayland screen capture not yet implemented.` Linux headless explicitly rejects
screen capture. Therefore this spike uses ImageMagick's X11 window capture for
real-app e2e and the WGPU renderer API for the offscreen experiment, without
adding direct GPUI implementation dependencies or scap/portal requirements.

## Host tooling and fixed conditions

Present: `Xvfb`, `xvfb-run`, `xdotool`, ImageMagick `import`, `identify`, `compare`,
`glxinfo`, `eglinfo`, and `timeout`. Missing: `weston`, `xwd`, and `vulkaninfo`.
`/usr/share/vulkan/icd.d` contains only `radeon_icd.json`; neither a lavapipe
manifest nor `libvulkan_lvp.so` was found. No system packages were installed.
The available physical render node is `/dev/dri/renderD128`. Host: Linux
7.2.2-arch1-1 x86_64; rustc 1.98.0 (88d9e12ae 2026-08-18).
`fc-match sans` reports Noto Sans Regular; fonts are system-provided, not bundled
or pinned by this spike.

`xvfb-run -a -s '-screen 0 800x600x24 -nolisten tcp' glxinfo -B`
reports Mesa 26.2.1, llvmpipe (LLVM 22.1.8, 256 bits), accelerated **no**, OpenGL 4.6.
This is software **OpenGL**, not lavapipe/software Vulkan. Weston headless and
its screenshooter cannot be tested on this host because Weston is absent.

The tiny scene has a fixed 320 x 200 window, a 200 x 64 blue button, an orange
rectangle, a solid background, and system-font text. It has no animation,
blinking caret, external assets, network access, or time-dependent content.
The real-window run uses Xvfb without a window manager, scale 1, and no
compositor decoration. The GPUI test window defaults to scale **2**, so the
offscreen output is **640 x 400**, not the native window's 320 x 200. Captures must contain both exact scene colors and the
expected dimensions, so five identical blank images cannot pass.

## Reproduction

Run from this worktree. The examples read explicit command-line paths; the
library never implicitly reads environment variables or paths.

```sh
cargo fmt -p guiraffe-test --check
cargo test -p guiraffe-test
cargo test -p guiraffe-test --all-features
cargo clippy -p guiraffe-test --all-targets --all-features -- -D warnings
cargo build -p guiraffe-test --all-features --examples

# Real app, five independent process launches and decoded RGBA equality.
timeout 120s xvfb-run -a -s '-screen 0 800x600x24 -nolisten tcp' \
  env -u WAYLAND_DISPLAY LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p guiraffe-test --all-features --test e2e \
  -- --ignored --nocapture --test-threads=1

# In-process offscreen WGPU: no X server required by the API.
timeout 60s env -u DISPLAY -u WAYLAND_DISPLAY \
  target/debug/examples/offscreen /tmp/guiraffe-offscreen

# Requested software-Vulkan setup: this manifest is MISSING on this host.
# If this works via GL fallback, it is not lavapipe evidence.
timeout 60s xvfb-run -a -s '-screen 0 800x600x24 -nolisten tcp' \
  env -u WAYLAND_DISPLAY LIBGL_ALWAYS_SOFTWARE=1 \
  VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
  target/debug/examples/offscreen /tmp/guiraffe-offscreen-no-lvp
```

The ignored e2e test requires a separately built `probe_app` example.
It polls an explicit visual readiness condition (both scene colors) with a
10-second deadline before comparing each capture; the paint-quiet heuristic
alone is insufficient for a cold renderer. It fails
if tools, the renderer, the app, or the display are unavailable; it does not
silently skip a missing prerequisite. It retains PNGs under a PID-specific
`/tmp/guiraffe-visual-*` directory and prints the artifact paths. It compares
**decoded RGBA pixels**, not PNG file bytes (PNG timestamps can differ).

## Probe contract and prototype boundaries

`guiraffe-test` has no default features. Linux module `e2e` is opt-in, and the
additional `offscreen` feature enables GPUI's `test-support` for investigation.
`Probe::start(Config, Registry)` starts the server inside the example app;
`Client::call` is the tiny synchronous Rust client. Transport is one newline-
delimited JSON-RPC 2.0 request/response per Unix connection, numeric request IDs,
64 KiB maximum messages, local socket mode 0600, and bounded I/O/subprocess waits.
It refuses to replace an occupied socket path. Dropping the probe stops its
listener and removes its own socket. The example is killed and reaped by the
test's child guard even on assertion failure.

| Method | Parameters | Result / behavior |
|---|---|---|
| `windows` | `{}` | configured app window ID and exact title; X11 lookup restricted to the app PID |
| `query` | `{"selector":"save"}` | window key, actual paint bounds `[x,y,w,h]`, published state |
| `click` | `{"selector":"save"}` | XTest pointer move to window-local center, button down/up |
| `type` | `{"text":"hello"}` | focus app window and send real XTest keyboard events |
| `key` | `{"key":"BackSpace"}` | xdotool key syntax, real keyboard events |
| `wait_idle` | `{}` | a paint exists and is quiet for 100 ms; response explicitly says no GPU fence |
| `capture` | `{}` | unique PNG path in explicitly configured output directory, via `import -window XID` |

The real GPUI debug-selector map is crate-private; only `VisualTestContext`
exposes its bounds accessor. The example sets `.debug_selector("save")` and
mirrors that same test ID using a canvas paint callback over the button's
actual bounds. `Registry::publish` replaces the entire frame, including the
state and removed IDs. This is **manual instrumentation**, not generic native
window selector traversal. A future shared `TestId` element must publish
observation data; reading the debug-selector map requires an upstream API change.
There is also a public opt-in observation facility in gpui-kit 0.9.1:
`base::TestSupportExt` (`.test_support()`) and `test::TestWindowExt::try_find`.
That is the preferred next integration point for identified elements; this
spike does not yet bridge its window-local observation into the socket server.
The canvas is throwaway and is not a production wrapper for arbitrary widgets.

The integration test queries actual button geometry, captures the initial
frame, clicks it, types `hello`, presses BackSpace, and asserts the paint-
published application state is one click and `hell`. A missing selector must
return an error. The keyboard listener demonstrates physical key delivery; it
is not a Controls text input/IME test. The example's explicit focus handle owns
keyboard focus. No test substitutes a model-only counter for real X11 input.

Known limits: one configured window, scale 1, serial clients, no selector
hierarchy, no visibility/overlap checks, no drag API, and no Wayland support.
`wait_idle` is a paint-quiet heuristic, not a guarantee of executor quiescence,
asset completion, layout stability, or GPU presentation. There is no rendering
fence between that heuristic and external capture; fresh-process consistency
is evidence only for this static scene. This developer-only socket can inject
input and write captures; keep it in an explicitly chosen private runtime
directory and never ship the feature enabled. A hung client can delay other
clients until its read deadline; teardown may wait for the active request.

## Measurements and evidence

The final native log is [visual-evidence/e2e.log](visual-evidence/e2e.log).
[results.json](visual-evidence/results.json) records every accepted capture path,
its decoded 8-bit RGBA SHA-256 and the differing-pixel count against its first
capture. PNG samples are retained beside it. Original `/tmp` paths are run
artifacts and may be cleaned later; the committed samples and hashes are durable.

| Experiment | Images | Dimensions | Differing pixels within setup | Outcome |
|---|---:|---|---:|---|
| Native app, five fresh processes, Xvfb + Mesa llvmpipe GL | 5 | 320 x 200 | 0 in all 5 | pass; real input assertions pass on every launch |
| WGPU offscreen, no DISPLAY or WAYLAND_DISPLAY, default adapter selection | 5 | 640 x 400 | 0 in all 5 | pass; adapter identity not exposed by headless factory |
| WGPU offscreen, Xvfb + forced software GL + missing Vulkan manifest | 5 | 640 x 400 | 0 in all 5 | pass via GL fallback; **not** a lavapipe run |
| Native `Window::render_to_image` | each app launch | — | — | returns `render_to_image not implemented for this platform` |
| Weston headless + screenshooter | 0 | — | — | not run: Weston missing |
| lavapipe/software Vulkan | 0 | — | — | not run: ICD/library missing |

Hashes of decoded RGBA bytes (all five match within each row):

```text
native Xvfb:        3aa32ce0fb110c3a91986fefe6f2b6dbfd1ab54c4069cd36aceced897e5982a8
offscreen default: b98058a33ffb2383656dc2f2b8f2053caa07813980009a0d228459333c9532d0
offscreen GL:      aec56d0ccd24595748d0f974216fa0c27fed5c1a3b56f7f44068f422175a1336
```

Recompute a sample's hash with:

```sh
magick spikes/visual-evidence/native-xvfb.png -depth 8 rgba:- | sha256sum
```

The two offscreen adapter setups differ at **302 of 256,000 pixels**, with a
maximum channel difference of **1/255**. This is measured cross-setup variation,
not nondeterminism within a setup. Do not share exact-match goldens between
these profiles; a perceptual diff policy would tolerate these small differences.
The native app reports `is_software_emulated: true`, device `llvmpipe (LLVM
22.1.8, 256 bits)`, driver `4.6 (Core Profile) Mesa 26.2.1-arch1.1`, scale 1.
The offscreen fallback's GL/software attribution follows from disabling Vulkan
with a nonexistent explicit ICD and forcing Mesa software GL; its factory does
not return adapter metadata. No claim is made that the default offscreen
adapter is software.

A first attempt captured a **black image** after a CPU paint callback and the
100 ms quiet period. An eight-second diagnostic delay then captured the scene
and verified input. The final test replaces that delay with bounded visual
readiness polling; it rejects blank images and still requires pixel equality
across all five independent launches. The earlier black sample is retained as
[visual-evidence/native-first-paint-race.png](visual-evidence/native-first-paint-race.png).
This proves that `wait_idle` needs a real presentation barrier before it can
be the sole readiness condition. Do not turn five black frames into a passing
snapshot test. The native capture test also asserts the paint-published target
origin `(16,16)` and size `(200,64)`; its absolute canvas is anchored at `(0,0)`
inside the target to avoid the text line shifting its observed bounds.

## Validation and recommendation

Final checks passed:

- `cargo test -p guiraffe-test --no-default-features`: 0 unit tests, 0 doc tests;
  verifies the disabled feature surface compiles.
- `cargo test -p guiraffe-test --all-features`: **3 unit tests passed**, **1 real
  e2e test explicitly ignored**, 0 doc tests. Unit coverage includes malformed
  protocol input and round trip, frame replacement, real-socket client/error
  round trips, socket permissions, collision refusal and teardown.
- The separate Xvfb command above: **1 e2e test passed**, **5 fresh processes**,
  **5 accepted matching captures**, and click/type/key assertions in each.
- Both offscreen commands above: **5 matching captures each**, successful exit.
- `cargo clippy -p guiraffe-test --all-targets --all-features -- -D warnings`.
- `cargo fmt --all --check` and `git diff --check`.

Prefer the in-process WGPU headless renderer for widget golden images: it
avoids native window placement, desktop overlap, external capture and the
first-presentation race. Use native Xvfb + external capture for real-app e2e,
where process startup, native focus and XTest input are the behavior under test.
Keep this opt-in until the probe's manual registration and quiet-period
heuristic are replaced by shared observation and a reliable readiness barrier.

Before making a visual CI gate, freeze the renderer/backend, driver/Mesa image,
fonts, scale, locale, theme, viewport and assets; disable motion/carets and
wait for async asset completion. Provision lavapipe on a dedicated runner and
rerun the exact experiments with a real existing ICD before selecting software
Vulkan as the standard profile. Independently validate the Fast backend and
Controls widgets, light/dark themes, SVGs/images, clipping/shadows, resize and
multi-window scenes. The source-level Fast API match is not runtime evidence.
Five repetitions of one static scene establish feasibility, not a flake-rate
bound or universal visual determinism. Native capture needs no perceptual
threshold for this profile today; cross-adapter goldens already demonstrate
why a later reviewable perceptual policy is necessary.
