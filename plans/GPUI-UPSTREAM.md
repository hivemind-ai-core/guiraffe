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
