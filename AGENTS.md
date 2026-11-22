Work in worktrees

Merge changes into a release branch. After multiple changes are ready, test them together in batch, once tests pass, merge to main. Then open a new release branch for upcoming changes.
Only the foreman should merge to main. The foreman also controls who can merge to the release branch at any given time.
Its the foremans responsibility to make sure agents have appropriate work, and that the quality of the work is high.

## GUIraffe specifics
- Read docs/architecture.md and docs/contributing/api-conventions.md first. gpui-kit is pinned to =0.9.1 (workspace dep).
- Worktrees live in `worktrees/<branch>`; branch off the current release branch; never merge yourself, report to the foreman.
- Gate: `cargo test -p <crate>` and `cargo clippy -p <crate> --all-targets -- -D warnings` pass, `cargo fmt`.
- No `unwrap`/`expect` in library code; split pure logic from GPUI and test it; add GPUI TestAppContext tests for interaction.
- Don't edit other crates' files except to add workspace deps; ask the foreman.
- Build isolation: each worktree builds into its OWN target dir (the default `target/` inside the worktree). Never point CARGO_TARGET_DIR at another worktree's or the main checkout's target dir; mixing branches in one target dir links stale crate versions and produces false gate failures. Keep TMPDIR and build output out of /tmp (small RAM disk).
- Gates must include: tests on both backends, `cargo test -p guiraffe-bench` (allocation gates), per-crate doctests, clippy -D warnings, fmt; look at every changed golden. Run `scripts/gate.sh` (add `--regen` to regenerate goldens first); it does all of this.
- Don't commit test reports, logs, review write-ups or handover notes. Durable facts go in docs/, KNOWN_ISSUES.md or ROADMAP.md.
