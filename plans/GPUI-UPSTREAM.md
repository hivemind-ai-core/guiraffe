# Draft pull request for zed-industries/zed

## Share tessellated path vertices between cached frames

Cached path replay currently clones a vector when copying a primitive from
the previous scene and again when retaining it in the new scene. Share the
backing allocation and detach on mutation. Preserve indexing, vector methods
and borrowed iteration through a wrapper. Direct assignments to
`Path::vertices` now use `Vec::into`.

The local `gpui-pre 0.3.12` chart fixture recorded 263 allocations and 498,444
requested bytes in one warm frame. DHAT found two groups of 124 vertex-buffer
allocations, each requesting 232,896 bytes. Together those accounted for
94.3% of the frame's allocations.

Add a copy-on-write check and a cached canvas regression that draws 1,000
triangles, warms both frame buffers, verifies every path is replayed, and
counts allocations in the full CPU draw plus arena clear.

A separate follow-up addresses remaining full-frame allocation sites in the
published snapshot. It reuses unescaped arena validity tokens, avoids empty
tab-stop resets, sorts paths in place with insertion order as a tie-breaker,
and reuses shallow dispatch stacks, recent element IDs and empty font
features.

# Draft pull request for longbridge/gpui-fast

## Pace callback-only Wayland ticks from the compositor

The Wayland loop arms a 16.667 ms local timer when a tick consumes a
next-frame callback, schedules another, and leaves the current buffer in
place. That interleaves software timers with native callback delivery.
Request a `wl_surface.frame` on a mapped surface and commit its current
state instead, reusing the one pending callback slot. Preserve the timer
retry before the first successful presentation and preserve parking when
demand ends. An obscured mapped surface can pause on the compositor callback,
as it already can after a buffer submission.

The state transition tests cover callback demand before and after the first
present, idle parking, and both presentation failure states.

## Share tessellated path vertices through retained replay and gather

The cached scene replays paths into its paint operations and gathers them
into renderer order. Let all of these owners share vertex backing. Make
writes copy-on-write so layer translations and clipping continue to update
only the affected copy. Keep the buffer shared when all vertex masks are empty, as
they are for ordinary paint paths. Backend renderer code retains borrowed
access to vertices.

The local `gpui-fast 0.1.15` chart fixture recorded 373 allocations and
698,712 requested bytes in one warm frame. DHAT found 124 vertex-buffer
allocations in each of replay, primitive insertion, and gather. Together
these were 99.7% of the allocations and 698,688 requested bytes. The remaining
24 bytes came from the arena validity token. Keep that token when no boxes
survive the clear, and retain the existing invalidation for escaped boxes.

Verify with the same cached 1,000-path canvas and copy-on-write checks. The
API migration for a direct vector assignment is `Vec::into`.


## Reuse tab-stop order buffers during paint replay

Both GPUI snapshots now append focus registrations to a reusable frame vector.
The first Tab or Shift-Tab traversal sorts it in place by group path and
insertion order and binary-searches the currently focused entry. Group
nesting, duplicate indices, non-tab stops, and wrapping keep their existing
ordering rules. Storage follows the two window frames and retains capacity up
to the largest registration count previously seen by each frame. The
six-entry inline group path can still spill for deeper nesting.

In the upstream release soak probe, the tab-stop change reduced Gantt from
215 to 80 allocation requests per unchanged draw and from 322,504 to
74,104 requested bytes. The vendor regression replays grouped tab operations,
checks forward and reverse traversal, and verifies the same vector allocation
is reused after a clear.

# Draft pull request for DioxusLabs/taffy

## Retain bounded scratch for block and out-of-flow layout

Let `LayoutPartialTree` opt into scratch storage and have `TaffyTree`
retain it when the node tree is cleared. Recursive block sizing and
out-of-flow layout take their vectors by ownership and return them once
the pass completes, so descendants can use disjoint buffers. Custom trees
get the existing temporary-vector behavior by default.

Recycle block items, the out-of-flow worklist and temporary hoisted IDs.
Each of the three pools retains at most sixteen buffers of at most 16 KiB
each. Returning a buffer drops its contents, including any state attached
to a block item. The bound is 768 KiB per tree, plus spare vector headers.
Owned result vectors and per-node child lists keep their existing lifetimes.

In the upstream mounted release probe, this removed 17 requests per retained
draw in five fixtures and 20 in Charts, after tab-stop reuse. Taffy's 171
library tests include a nested scratch reuse check.
