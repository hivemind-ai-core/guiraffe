# Flow review #2 follow-ups

Source baseline: `aa82bb4c0c9ee98310fcc9dee8696fb59d19a6f7` on release/0.1.
Implementation/benchmark source: `a0dae350e54c565dcae9b532442ff1cc77631098`.
The rebase was clean and preserves the foreman's app-clock/reduced-motion edge-animation fix.
No merge or push was performed.

## Per-item status

| Item | Status | Evidence |
|---|---|---|
| 1: notification/event noise | Fixed | Change stamps compare document/selection snapshot identity, camera, visible gesture feedback and errors; idle pan start/release, sub-threshold movement, no-op edits and repeated selection do not emit redundant events/notifies. Camera controls preserve selection. No-op and real-control regressions pass. |
| 2: entity split | Fixed | `FlowState` owns canonical editor/history/selection; `FlowViewport` owns a transactional editor snapshot and camera interaction; `FlowGesture` draws pointer feedback; `FlowCanvas::new(&state)` is the element. `Flow` aliases the lightweight model/view host for existing compositions. Native camera frames do not write document state. |
| 3: per-frame work | Fixed for listed costs | Theme is cached/compared once per viewport render, node props are reconciled on changes outside render, and cards read the shared configuration theme. Drag origins/root offsets are immutable shared maps. Selection elevation is cached on edits/selection, visible z-order only sorts on admission/scene/selection changes. Edge transforms are batched by colour, not individually allocated per edge. Reserved-buffer/geometry and origin-sharing tests pass. |
| 4: panic/indexing safety | Fixed | Public layered layout validates its graph and returns no positions for invalid parent chains. Map lookup and geometry helpers use checked lookups/iterators instead of panicking indexing. Missing-parent and cycle cases are tested; original cycle execution was deadline-bounded. |
| 5: child deletion depth rebuild | Fixed | Deletion removes affected depth entries; unrelated persistent depth storage survives. Reparenting updates the affected descendants. A dedicated 5k/10k delete-child + undo example measures this path. |
| 6: BVH degradation | Fixed | Refits maintain a normalized perimeter-cost aggregate; exceeding 1.8 times build quality triggers a balanced rebuild, in addition to the depth-48 insertion safeguard. The adversarial movement regression previously visited 511 branches; it now passes the fewer-than-100-branches gate and preserves correct hits. |
| 7: Scene/Editor hit policy | Documented | Scene's Rustdoc explicitly describes structural-only picking, which has no selection; interactive clients use Editor's selection-aware shared paint/hit z-order. |
| 8: selection-only undo step | Fixed | Changed-node keys are recomputed after removing import-only selected flags; selection-only `edit` keeps Scene identity and history untouched. |
| 9: selection persistence | Retained/documented | Selection remains view state outside serialized documents/history. Round-2 serialization coverage verifies the documented contract. Hosts can persist the selection separately. |
| 10: arbitrary edit validation | Retained/documented | Arbitrary `edit` closures still validate the whole resulting graph; rejected edits leave history/routes unchanged. Direct moves/deletions use the specialized validated update paths. |
| 11: box selection modes | Fixed | Shift-drag on empty canvas replaces selection; Control/platform + Shift-drag adds. Shift-click still toggles nodes. Pure replacement/addition regression passes. |
| Known issue #2 | Fixed | First nonempty measured layout fits the graph. Explicit cameras or fit_on_init=false opt out; later edits/resizes preserve the camera. The pending-builder/explicit-camera regression protects initialization order. |
| §6.2 pure notifications | Fixed | Bounds-only redraw uses `cx.notify(entity_id)`. Other entity updates mutate real model/camera/card/gesture data; no update closure exists solely to notify another view. No node-card notify occurs in render. |

Strengthened acceptance includes nonoverlapping arranged root groups (including pinned nodes),
a real cold/warm GPUI frame with **3,000 visible edges** (3,000 then zero cache misses; one
normal-colour transform batch), and toolbar deletion using ordinary pointer clicks.

## Red/green and backend validation

The first regressions reproduced a malformed-parent panic, empty undo transaction,
511-branch degraded query, rebuilt unrelated depth storage, additive-only box selection,
no-op notification and missing first-layout fit. The original parent-cycle layout test
was isolated with a timeout rather than allowed to hang the suite.

A portable native-wheel isolation test was also run against the original GUI sources:
five pan frames changed the host render count from **4 to 9**, while its sibling stayed
at 4. The same test passes on the implementation using `gpui_kit::GPUI_FAST`, with zero
additional host/sibling renders on gpui-fast. Upstream is tested without asserting a
retained-rendering guarantee that its backend does not provide. A second test exercises
the new FlowCanvas host with an explicit document read, and a public-model test verifies
native drag followed by model undo/redo.

Both backend suites pass **90 tests**: 43 flow-core tests and 47 flow tests (28 GPUI tests).
No tests are ignored. Required checks:

```sh
cargo test -p guiraffe-flow-core -p guiraffe-flow
cargo test -p guiraffe-flow-core -p guiraffe-flow --features gpui-kit/gpui-fast
cargo clippy -p guiraffe-flow-core -p guiraffe-flow --all-targets -- -D warnings
cargo clippy -p guiraffe-flow-core -p guiraffe-flow --all-targets --features gpui-kit/gpui-fast -- -D warnings
cargo fmt -p guiraffe-flow-core -p guiraffe-flow -- --check
```

Production source has no unwrap/expect calls, unsafe code or unchecked map/geometry
indexing. Tests use ordinary fixtures/assertions. Formatting and diff checks pass.

## Before/after measurements

`review-2-benchmark.json` contains 80 process outputs: ten alternating AB/BA runs of
old/new scene build+query, move, child deletion and headless frame workloads. CPU 15
was pinned with taskset. Both sides use gpui-kit 0.9.1; core is release and GUI frames
use dev opt-level 2, matching the preceding review's protocol. The new child-delete
example was copied unchanged into the old-source benchmark worktree. Shared local
package caches were cleared between revision builds after an older package artifact
was observed; distinct executable copies were used during all measurements.

Values are medians across runs, with min–max run ranges; percentile rows describe the
runs' reported percentiles, not a pooled latency distribution.

| Measurement | Before | After |
|---|---:|---:|
| Scene build | 13.349 ms (13.135–15.235) | 13.427 ms (12.969–15.035) |
| Query p50 | 8 us (7–8) | 8 us (8–8) |
| Move p50 | 58 us (55–76) | 57.5 us (55–63) |
| Child delete + undo p50 | 1.085 ms (1.078–1.111) | 0.105 ms (0.103–0.107) |
| Child delete + undo p95 | 1.165 ms (1.135–1.221) | 0.124 ms (0.116–0.131) |
| Child delete + undo p99 | 2.832 ms (2.650–2.906) | 0.141 ms (0.125–1.131) |
| Headless frame p50 | 4.788 ms (4.336–5.836) | 4.610 ms (4.537–6.938) |
| Headless frame p95 | 6.734 ms (6.093–7.923) | 6.537 ms (5.703–10.740) |
| Headless frame p99 | 7.783 ms (7.396–9.491) | 7.713 ms (6.977–13.581) |

Child deletion's median is about **10.3x faster**; scene/query/move costs are essentially
unchanged in this sample. Frame medians overlap and the new tail includes an outlier;
this is not a formal frame-budget or GPU-presentation claim.

The requested `guiraffe-bench compare` command was exercised separately at ten pairs
per existing suite. Its registry currently has only core-variants and window-placement:
`review-2-harness-smoke.json` records both as pass. These suites do **not** measure flow,
and their result is not presented as flow-performance evidence. Flow suite registration
requires approval under the crate boundary; `review2-bench.patch` is the prepared,
unapplied proposal. It also includes a regression/check for fresh second-revision
checkout ordering when shared build caches are used. No benchmark crate was edited.

## API/integration notes

Camera subscriptions move to `state.viewport_entity()` so their events never require a
document-entity write. FlowState emits document, selection and error events. The aliases
and existing graph/edit/history/metrics methods preserve the basic Flow host API.
The gallery can consume default first-fit behaviour without source edits; its flow
snapshot goldens may need updating as part of foreman integration. KNOWN_ISSUES.md #2
can be closed there after gallery validation. Native rendering/pixels, OS trackpads,
all-5k-nodes overview performance and GPU presentation remain outside these tests.
