# GPUI patches

The root Cargo manifest patches the crates.io packages `gpui-pre 0.3.12`
(Zed snapshot `d2a4fb6`) and `gpui-fast 0.1.15` (Longbridge). Each directory
retains the published crate sources, manifests and Apache 2.0 license.
Registry extraction markers are omitted. The GPUI Kit pin is `=0.9.1`.

The root and standalone comparison app also patch `gpui-fast-linux 0.1.15`.
On Wayland, a previously presented surface with callback demand can request
another compositor tick through a commit that keeps its current buffer.
The platform still parks when demand ends and uses its timer for bootstrap
and outstanding presentation retries where a callback may never arrive.
The change uses the existing single pending callback slot per window.
With `GPUI_WAYLAND_PRESENTATION_TRACE=1`, the Wayland backend also
requests `wp_presentation` feedback for buffer submissions. Set
`WAYLAND_DEBUG=client` to see those timestamps in the protocol log.

| File | Local change |
| --- | --- |
| Both `src/scene.rs` | Changes `Path::vertices` to an `Arc<Vec<_>>`-backed `PathVertices` wrapper. Scene replay, insertion and fast gather share vertices. Mutation detaches via `Arc::make_mut`. Scaling still computes device-space coordinates in the same order. |
| Both `src/elements/list.rs` | Retains the measurement tree when the viewport produces identical sizes and readiness. Changed records still follow the existing measurement update path. |
| Both `src/arena.rs` | Invalidates the token before dropping values. Resets it in place when no handles survive and allocates a new token when a stale handle still owns the old one. |
| Both `src/window.rs` | Exposes a test-support observer at the completed scene swap, so frame regressions capture drawn and retained paint. |
| Both `src/window.rs` | Exposes the rendered path count under test support so the allocation test verifies replay produced all 1,000 paths. |
| `gpui-pre/src/scene.rs` | Sorts paths in place by draw order and sequential `PathId`. Other primitive kinds share a reusable stable sort permutation capped at 65,536 entries per scene. |
| `gpui-pre/src/window.rs`, `gpui-pre/src/view.rs`, `gpui-base/src/root.rs` | Kit opts its viewport root into cached layout and paint. Refresh and child invalidation rebuild as usual. The root prepaints live when accessibility is active. |
| `gpui-pre/src/window.rs`, `gpui-pre/src/element.rs` | Reuses exact global element ID paths from 64 direct-mapped slots per window. Collisions compare complete paths before reusing a value. IDs still use their original backing and equality. |
| `gpui-pre/src/key_dispatch.rs` | Keeps the replay stack inline for up to 32 dispatch ancestors. Deeper trees spill into a temporary buffer. |
| Both `src/keymap/context.rs` | Shares immutable key context entries during retained dispatch replay and detaches on mutation. |
| Both `src/window.rs` and `src/elements/div.rs` | Shares test-only bounds selector strings across replay records and lookup maps. |
| `gpui-fast/src/window.rs`, `gpui-fast/src/fast/rounded_clip*.rs` | Reuses the preceding compiled scene after a complete replay with matching layer metadata. |
| `gpui-pre/src/tab_stop.rs`, `gpui-fast/src/fast/tab_stop.rs` | Upstream reuses tab lookup and order buffers across frames. Fast builds its query index on demand and drops it at frame clear. Both retain tab insertion history capacity. Group paths and insertion order resolve ties. |
| `gpui-pre/src/text_system/font_features.rs` | Shares one empty font feature vector across default styles. |
| `gpui-fast/src/fast/layers/paint.rs` | Keeps shared backing when all vertex masks are empty and preserves clipping for custom masks. |
| `gpui-fast/src/fast/path_cache.rs` | Converts the path-cache test fixture from a vector to the wrapper. |
| Both `src/inherited.rs`, `src/gpui.rs` | Add `InheritedState` / `set_inherited_state`: a one-word hook for state an embedder keeps outside the window while an element lays out its children (GUIraffe's style context stack). Upstream only declares it; it renders every view inside its ancestors. |
| `gpui-fast/src/fast/retained.rs`, `fast/splice.rs`, `fast/layers/reuse.rs` | Capture the hook's word in each retained layout (beside the text style), carry it in `Rebuild`, enter/leave it around a view built again on its own in a splice gap, and refuse to reuse a retained or kept layout whose word changed. Without this, a notified view re-rendered in isolation read the default style context (losing `focus_scope` tab stops, provided size and density). Capture is one thread-local read; nothing allocates. |

Existing callers can borrow, index, reserve, extend and iterate by reference.
Use `vec.into()` when assigning a vector to the public field. The wrapper
dereferences to `Vec` so existing mutating APIs retain copy-on-write semantics.
Code that consumes a raw vector by value needs a separate owned copy or an
iterator over borrowed vertices.

Each shared buffer follows the lives of its path owners. The change adds one
Arc header per independent buffer and no vertex cache. The ID cache retains
at most 64 paths per window. Each cached path holds at most 32 element IDs. The font cache retains one process-wide empty vector.

Sprite primitives are copied without allocating. Cached text already reuses
`Arc<CacheKey>` and shaped layout handles in both backends. Tab-stop replay retains two frame-owned order buffers per window, each sized to the
largest number of focus registrations seen by that frame. Unusually deep groups
can still spill a tab path past its six inline entries.

# GPUI Kit 0.9.1 patches

The crates are from the crates.io releases of `gpui-base` and
`gpui-component`, both published from longbridge/gpui-kit commit
`39ab849a93b5f15507b969164d49891db8d5b470`. Each copy retains its Apache
license, published manifest, and sources. The Kit patch in the root Cargo manifest replaces
these two crates. The workspace pin remains `gpui-kit =0.9.1`.

| File | Local changes and reason |
|---|---|
| `gpui-base/Cargo.toml` | Declares `im`, already locked by GPUI, for persistent top-level block storage. |
| `gpui-base/src/text/state.rs` | Adds `markdown_with_estimated_block_height` and threads the estimate into list creation and population. List growth reseeds estimates and restores the logical anchor because neither backend exposes a height-hinted splice. The constructor also publishes a bounded, complete initial chunk when available while the existing revisioned worker parses the full document. The worker checks for a newer replacement or view drop between parse sections. Exposes the existing selection predicate so a host can preserve window-owned selection endpoints during eviction. Appends replace the last block and join the new tail while sharing settled leaves. The scaling test checks sharing, continuation text, and stable source spans. The opt-in worker stores large Markdown’s settled text paragraphs as cold descriptions. The initial background parser can hand raw AST sections to a one-shot metadata observer before conversion. Parses borrowed pending input and builds the appended source with exact capacity. Estimated-height views overdraw 512 px instead of 1,000 px to bound retained memory; full-measure views keep 1,000 px. |
| `gpui-base/src/text/document.rs` | Seeds unknown heights on list population and lets the existing viewport and overscan layout replace them with measurements. The default full-measure policy still uses exact heights. Tracks up to 256 recently visited top-level blocks for decoration and metric caching. Uses an `im::Vector` for persistent block snapshots and an indexed flow helper for both vectors and slices. Reuses the viewport cache to evict materialized paragraphs and tests cross-window copy and eviction. |
| `gpui-base/src/text/mod.rs` | Re-exports the section visitor for metadata consumers that need the same source positions. |
| `gpui-base/src/text/format/html.rs` | Routes paragraph edits and ownership transfers through the copy-on-write inline-run storage and builds the root block vector. Moves boxed image payloads through HTML inline merges. |
| `gpui-base/src/text/format/markdown.rs` | Splits eligible Markdown at unindented ATX heading boundaries and translates chunk positions to document coordinates and consumes each raw AST independently. Falls back to the one-shot parser around document-wide references, footnotes, and ambiguous block constructs. Equivalence tests cover mixed GFM blocks, UTF-8 positions, and the fallback. Uses the new paragraph ownership helper when merging inline content and stores parsed roots in the persistent vector. Reserves inline runs from the child count, trims excess capacity, and moves compact source maps and boxed image payloads through inline merges. Keeps the final parsed block to preserve streaming fades. Builds the persistent block index directly, sizes tables to their parsed rows and columns, and omits linear source maps that the source ranges reconstruct. |
| `gpui-base/src/text/node.rs` | Releases cached decorations, table metrics, and fence styles for a distant block independently of selection. Shares immutable inline runs when appending clones existing paragraphs and detaches them on mutation. Passes indexed nested blocks to the flow helper and adapts test documents to the root vector. Heap-allocates image and custom payloads only when present and stores source maps in exact-size slices. Enforces the inline run footprint. Stores inactive text paragraphs as shared source ranges, inline metadata and selection handles. Only layout within the viewport and overscan retains materialized runs. Stores whole-run marks inline (at most 48 bytes for a source-backed run, enforced by a test) and allocates per-run selection state only for images and custom inline objects. |
| `gpui-base/src/text/range_highlight.rs` | Reads shared inline runs through the paragraph slice accessor and visits root and nested blocks through iterators. Explicit full-text indexing can materialize one cold paragraph at a time. |
| `gpui-base/src/text/editor_host.rs`, `gpui-base/src/text/block_surface_tests.rs` | Collects parsed roots into the editor host’s existing vector interface and adapts tests for the unchanged interface. |
| `gpui-base/src/text/inline.rs` | Caps retained shaped layouts at 4,096 entries per thread and discards older entries in batches. Keeps measured text in the element until paint, so eviction cannot remove text needed by the current frame. Selection remains in the corresponding inline state. Includes a live-state cache bound test. |
| `gpui-component/src/highlighter/highlighter.rs` | Shares compiled Tree-sitter queries via `Arc`, keyed by canonical language and query purpose, with grammar and source validation. The process-wide cache is capped at 128 entries. The separate combined, injection-discovery, and injected-highlight queries retain their existing semantics. |
| `gpui-component/src/text/mod.rs` | Keeps up to eight recent canonical fence parsers per thread, limits cached input to 64 KiB per parser, skips unknown languages, and invalidates on registry changes. |

The viewport policy bounds measurement work by the viewport and overscan
except for the list index rebuild when the item count changes. The index
stores one estimate or measurement per block. Paragraph decoration, table metrics, and fence highlight results for
scrollable Markdown are retained for at most 256 top-level blocks per view.
Materialized cold paragraphs share that bound. Chat
retains selected rows until the window releases their selection endpoints.

The standalone GPUI Kit 0.9.1 Fast text suite has five pre-existing
`gpui-base` failures, reproduced against the published crates with GPUI
Fast 0.1.15. They cover two long-press selection interactions, ordered-list
markers, table row corner radius, and an unsuccessful range reveal. The
standalone `gpui-component` text suite passes all 78 checks on both stock
and patched Kit. The GUIraffe workspace gate passes on both backends.

# Taffy 0.15.1 patches

Both GPUI backends use the vendored crates.io release from Taffy commit
`0064bd54cbe38bf1d8bcd6ec6ab0704d45fb048a`. The local patch reuses
temporary block items, out-of-flow work entries, and hoisted IDs through
`TaffyTree::clear`. Each pool keeps up to sixteen cleared buffers of up to
16 KiB each. The layout algorithms, result ownership, and ordering stay the
same.
