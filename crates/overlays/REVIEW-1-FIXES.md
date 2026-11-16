# Overlays review 1 fixes

Scope: all items in `reviews/overlays-review-1.md`, rebased onto release base
`1f9e2afded01bfcb111c230e2545a7917d7fc0d1`. No merges or other-crate edits.

## Red tests and resulting behavior

Each numbered item has a regression recorded failing before its implementation.
The logs below are the local red-run artifacts; test names remain in the crate.

| Item | Observed red failure | Fix / regression coverage |
| --- | --- | --- |
| 1 | System delivery stored 0 notes instead of 1; unmanaged occupancy admitted hidden work | Force InApp, reconcile available native capacity, queue blocked work, drop/log missing admissions; tests also cover later unmanaged arrival/removal and persistent successors after admission failure |
| 2 | Update changed/reordered native entity IDs | Mutate the existing native entity, keeping its list position and animation identity |
| 3 | Updated hovered note lost `paused` | Preserve hover state while resetting its lifetime |
| 4 | First stacked confirm stayed Pending when its own OK action ran | Bind acceptance to the originating ID; sibling confirmation cannot be accepted |
| 5 | Non-top close returned NotTopmost, or popped an unrelated native dialog | Independent per-ID Base dialog scopes, Controls surface parts, arbitrary owned removal, repaired focus chains; tests include three mixed-policy modals, Drawer plus two modals, native preservation, native/managed opening order, child-input restoration and focus restoration |
| 6 | All-paused notes retained the timer task | Count only unpaused timed notes; hover cancels/restarts the task at the pause boundary |
| 7 | `clean` emitted no observer notification | Notify overlay observers after clearing |
| 8 | Pure shift moved Bottom Y from 104 to 62, covering its anchor; native UI positioning also violated the disabled-flip contract | Cross-axis shift only; measured `FloatingTooltip` calls the solver during prepaint, with a runnable hover example |
| 9 | Greedy score missed a better later contiguous match (51 versus 58 for the weaker candidate) | Optimal subsequence DP with adjacency/boundary bonuses and linear rejection preflight |
| 10 | Typing normalized labels eight more times in the two-command fixture | Prepared Unicode text, reused DP buffers and cached queries; submission does not perform another search; 10k benchmark below |
| 11 | Delivery ran inline before awaiting its task | `show_async(cx)` schedules blocking platform delivery on the GPUI background executor |
| 12 | Strong context capture kept the closed Root/host alive | Window-close subscription clears contexts, managed views, confirmations and idle resources; WeakEntity guidance is documented |
| 13 | Loading allowed background input value `x` | Element-owned, window-scoped keystroke interception; unmount drops the subscription and background input resumes |

Red artifacts: `/tmp/overlays-review-red-1-3.log`, `-4.log`, `-5.log`,
`-6-7-12.log`, `-8.log`, `-8-ui.log`, `-9.log`, `-10.log`, `-11.log`,
`-13.log`, `/tmp/overlays-review-red-admission.log`, and
`/tmp/overlays-review-red-child-focus.log` (same prefix).
Additional adapter regressions exposed lost scroll occlusion and overlapping
exit/enter priorities; the existing interaction assertions caught both before
handoff. The final implementation retains those assertions.

## API and motion

The native Controls window API exposes only top-pop dialog operations and keeps
its per-dialog state private. Owned dialogs therefore use the exported `Modal`
builder plus Base focus/actions and Controls title/content/footer/close parts.
The current repository builder call sites compile through inference; callers explicitly annotating
`gpui_kit::component::dialog::Dialog` must change that annotation to `Modal`.

Managed enter/exit retention now uses guiraffe-motion `Presence`, `Transition`,
`MotionPrefs`, `sample_presence`, and `framed`. Native Sheets and notification
stacks retain their upstream presentation. Focused native dialogs cover owned
layers; newly focused managed dialogs can open above existing native dialogs.
Owned close operations never pop native dialogs. Native `WindowExt` dialog queries
now describe native dialogs only; use the managed host stack to inspect owned ones.

## Validation

- `cargo test -p guiraffe-overlays`: **58 passed**.
- `cargo test -p guiraffe-overlays --all-features`: **61 passed**.
- `cargo test -p guiraffe-overlays --all-features --features gpui-kit/gpui-fast`:
  **61 passed** on gpui-fast.
- `cargo clippy -p guiraffe-overlays --all-targets -- -D warnings`: passed.
- The same all-targets clippy gate with all features, and with all features plus
  `gpui-kit/gpui-fast`: passed.
- `cargo fmt -p guiraffe-overlays -- --check` and `git diff --check`: passed.

These are headless interaction tests, including the existing application render
counter. Examples compile through the all-targets lint gate. No live desktop
notification daemon, cross-platform desktop runtime, pixel snapshots or frame-time
benchmark is claimed.

## 10k-command search evidence

`benchmarks/search-10k-review-1.json` contains the raw runs, source/harness hashes,
compiler version and commands. Old and new binaries alternated ten times each,
pinned to CPU 15. Each run performed 100 changing queries over 10,000 command
labels, with candidate preparation measured separately.

| Metric | Old greedy/per-call normalization | Prepared optimal DP |
| --- | ---: | ---: |
| Median of run query medians | 1.558 ms | 0.1485 ms |
| Run-median range | 1.531–1.684 ms | 0.143–0.166 ms |
| Median one-time preparation | — | 3.060 ms |

This measures the complete synthetic query paths, not an isolated scoring kernel:
the candidate also fixes match quality, and real command distributions may differ.
No GUI frame-time or general speedup guarantee follows from these measurements.

Reproduce the candidate without compiling GPUI:

```sh
rustc --edition=2024 -O crates/overlays/examples/search_bench.rs -o /tmp/search-bench
/tmp/search-bench
```
