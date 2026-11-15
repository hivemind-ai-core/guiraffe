# Overlays implementation hand-in

Release base: `8d0f992` (`release/0.1`, GPUI Kit **0.9.1**). The foreman explicitly
replaced the original 0.7.0 target during this dispatch; the initial 0.7.0 inventory
and the 0.9.1 source refresh are recorded in `INVENTORY.md`.

## Delivered

- Pure stack IDs/order/top-only dismissal admission; native Controls supplies
  modal focus trapping, focus restoration, scroll occlusion and transitions.
- Window-owned modal service (`modals`), named context factories, stacked dialogs,
  awaitable boolean confirmations, cancellation on close/window destruction.
- Four-side Drawer wrapping Controls Sheet; Escape/outside policies. Disabling
  Escape preserves the native pointer close button through a separate key action.
- Native Popover, HoverCard, ContextMenu/Menu and Tooltip exports.
- Pure floating flip/shift/alignment/arrow solver with all-side/all-alignment
  coordinate tables, viewport-edge combinations, flip enable/disable and invalid
  geometry coverage. Controls primitives retain their native positioning.
- Pure toast scheduler: six positions, visible limit/FIFO queue, hover pause,
  progress, persistent/timed dismissal, update-by-ID and fresh clocks on promotion.
  Native NotificationList owns close buttons and transitions. Progress invalidates
  only individual cards; the root helper caches application content.
- Spotlight: cached fuzzy results, Unicode/case-insensitive matching, keywords,
  disabled commands, arrow/Enter and pointer activation, overridable key contexts.
- Token-styled LoadingOverlay and six-position Affix, including Styled overrides.
- Optional notify-rust desktop delivery; markup escaping on Linux, platform-native
  delivery elsewhere and explicit unsupported-platform errors.
- Idempotent init that preserves an already-configured GUIraffe theme; idempotent
  Root wrapping; per-window isolation exercised through guiraffe-windows.
- Managed-window runnable example: `cargo run -p guiraffe-overlays --example overlays`.

## Validation

| Command | Result |
| --- | --- |
| `cargo test -p guiraffe-overlays` | 37 passed: 20 pure + 17 GPUI interaction tests |
| `cargo test -p guiraffe-overlays --all-features` | 39 passed, including 2 desktop construction/escaping tests |
| `cargo test -p guiraffe-overlays --all-features --features gpui-kit/gpui-fast` | 39 passed on gpui-fast |
| `cargo clippy -p guiraffe-overlays --all-targets -- -D warnings` | passed, including example |
| `cargo clippy -p guiraffe-overlays --all-targets --all-features -- -D warnings` | passed |
| `cargo clippy -p guiraffe-overlays --all-targets --all-features --features gpui-kit/gpui-fast -- -D warnings` | passed on gpui-fast |
| `cargo fmt -p guiraffe-overlays -- --check` | passed |
| `git diff --check` | passed |

The interaction suite asserts native confirmation, cancellation on destruction,
context/outside policy, all Drawer sides, pointer close with Escape disabled,
focus trapping/restoration, background wheel blocking, toast hover/update/queue/
expiry/native close, Spotlight typing/arrows/click, LoadingOverlay pointer blocking,
all Affix positions, managed-window independence and unchanged application render
counts during progress. Tests use headless TestAppContext; no visible windows or
notification daemon are opened by the test suite.

## Native constraints and validation limits

- Controls supports one Sheet per window. A Drawer requires an empty managed
  stack; dialogs can then stack above it. Only the top managed modal may close.
- Managed operations should use this crate's services; direct Controls window
  operations bypass managed stack IDs. Native primitive wrappers keep native state.
- Toast admission is capped by Controls' theme `notification.max_items`; unrelated
  unmanaged notifications share that native presentation list.
- Use `root` when application-content caching is needed. Kit's raw automatic Root
  still mounts the plugin but leaves application caching to its caller.
- Desktop delivery was compiled and construction tested on Linux; no live daemon,
  Windows/macOS runtime delivery, real-renderer screenshots or frame-time benchmark
  was exercised. The desktop `show` method is blocking and documents background use.

## New-conventions follow-ups

The dispatch adopted service free functions, standard component builders, Styled
support, idempotent initialization, leaf updates, cached content/results, key
contexts and a render-count regression test on both backends.

Shared serializable XProps/derive/IR registration, shared prop traits and motion
adoption, gallery integration and light/dark visual snapshots remain follow-ups:
the referenced guiraffe-macros/motion/test/IR foundations are not present on the
release base, and edits outside overlays were not authorized. The optional shared-foundation scope question `msg_81a14e102999` remains
unanswered; no approval is claimed. These are explicit follow-ups under the
dispatch's original crate-only edit boundary, rather than a reason to withhold
the completed and validated overlay implementation.

No merges were performed. Edits are confined to overlays and its Cargo.lock entry.
