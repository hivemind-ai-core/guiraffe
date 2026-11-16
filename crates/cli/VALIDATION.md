# CLI hand-in

Base: `release/0.1` at `aa82bb4`. The unfinished Claude scaffold/preview/import work was continued in `worktrees/feat-cli`.

Validation:

- `cargo test -p guiraffe-cli`: 7 unit/GPUI tests and 1 command integration test passed.
- `cargo test -p guiraffe-cli --features gpui-fast`: the same 8 tests passed.
- `cargo clippy -p guiraffe-cli --all-targets -- -D warnings`: passed.
- `cargo clippy -p guiraffe-cli --features gpui-fast --all-targets -- -D warnings`: passed.
- `cargo fmt -p guiraffe-cli --check` and `git diff --check`: passed.
- Ran the built `guiraffe new hello-guiraffe --path <checkout>` in a fresh directory outside the workspace, then `cargo build --offline` in the generated project with its independently resolved lockfile: passed. Its gpui-fast family resolved to 0.1.15.

The GPUI test verifies hot-reload state application: invalid input retains the host and live slots, then a valid reload clears the error and preserves slot values. It does not exercise a real desktop window or the polling timer end to end. File change detection uses modification time and length; template compilation still runs on the UI thread. No performance improvement is claimed from timing measurements.

The generated manifest pins gpui-kit 0.9.1 and gpui-fast 0.1.15 because resolving gpui-fast 0.1.16 caused the scaffold's IntoElement conflicts. Checkout paths are validated and escaped for TOML; malformed options fail instead of being silently ignored. Preview polling/file parsing runs in the background and only applies an entity update after a detected file change.

The separate required-component documentation adds a read-only syntax-highlighted code viewer and requires both code and Mermaid viewers to be usable standalone and embedded in Markdown through shared implementations. These are requirements, not claims that the new code viewer is implemented.

No release/main merge was performed. Independent review and release batch integration remain with the foreman. Existing worker branches and dirty worktrees were left untouched; Orca refused coordinator takeover from this structured session (`legacy_read_only`), so no lifecycle mutation or mail acknowledgment was performed.
