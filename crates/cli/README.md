# guiraffe CLI

Build with `cargo build -p guiraffe-cli`; the binary is `target/debug/guiraffe`.

- `guiraffe new my-app --path /absolute/path/to/guiraffe` creates a standalone Cargo app using the local GUIraffe checkout. The app includes typed routes, an event-driven store, a themed shell, overlays and the window manager. Existing directories are never overwritten.
- `guiraffe preview screen.guiraffe.json` opens a data-UI document and reloads it when saved. `--screen <id|name>` selects a screen in a Mocket document. Invalid saves show an error while keeping the last valid document; compatible live slot values survive a reload.
- `guiraffe import design.mocket.json --screen 'Order detail'` writes GUIraffe JSON to stdout and coverage diagnostics to stderr.

Generated apps use `gpui-kit =0.9.1` and default to gpui-fast. The template pins `gpui-fast =0.1.15`: 0.1.16 introduces an IntoElement implementation incompatible with gpui-kit 0.9.1's component stack. The generated app must be built independently of this workspace to check dependency resolution.

A standalone app does not inherit this workspace's `[patch.crates-io]` table, so the generated `Cargo.toml` repeats it: the vendored GPUI, GPUI Kit and Mermaid crates (`vendor/README.md`) are patched with paths into the same checkout as the `guiraffe-*` path dependencies, and the checkout's `Cargo.lock` is copied alongside. The scaffold tests check that the list matches the root manifest and that `cargo metadata` resolves the vendored sources.

Preview checks modification time and length every 300 ms on a background executor, where file reading and JSON parsing also run. UI template compilation currently happens when applying the loaded document on the UI thread. An idle poll does not update the preview entity. Changes that preserve both file length and modification time are not detected.

This is the initial `new`/`preview`/`import` tool. Code generation, packaging, devtools and additional app templates remain roadmap work. The source named by the generated path dependencies must remain available: the checkout, or Cargo's git cache for a CLI installed with `cargo install --git`. Published crate distribution is deferred.
