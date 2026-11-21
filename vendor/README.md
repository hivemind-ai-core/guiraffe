# GPUI patches

The root Cargo manifest patches the crates.io packages `gpui-pre 0.3.12`
(Zed snapshot `d2a4fb6`) and `gpui-fast 0.1.15` (Longbridge). Each directory
retains the published crate sources, manifests and Apache 2.0 license.
Registry extraction markers are omitted. The GPUI Kit pin is `=0.9.1`.

| File | Local change |
| --- | --- |
| Both `src/scene.rs` | Changes `Path::vertices` to an `Arc<Vec<_>>`-backed `PathVertices` wrapper. Scene replay, insertion and fast gather share vertices. Mutation detaches via `Arc::make_mut`. Scaling still computes device-space coordinates in the same order. |
| Both `src/arena.rs` | Invalidates the token before dropping values. Resets it in place when no handles survive and allocates a new token when a stale handle still owns the old one. |
| Both `src/window.rs` | Exposes the rendered path count under test support so the allocation test verifies replay produced all 1,000 paths. |
| `gpui-pre/src/scene.rs` | Sorts paths in place by draw order and sequential `PathId`. The ID preserves insertion order for ties. |
| `gpui-pre/src/window.rs`, `gpui-pre/src/element.rs` | Reuses exact global element ID paths from 64 direct-mapped slots per window. Collisions compare complete paths before reusing a value. IDs still use their original backing and equality. |
| `gpui-pre/src/key_dispatch.rs` | Keeps the replay stack inline for up to 32 dispatch ancestors. Deeper trees spill into a temporary buffer. |
| `gpui-pre/src/tab_stop.rs` | Clears existing frame buffers and keeps the empty tab-stop tree when no nodes were added. |
| `gpui-pre/src/text_system/font_features.rs` | Shares one empty font feature vector across default styles. |
| `gpui-fast/src/fast/layers/paint.rs` | Checks whether clipping changes any vertex mask before requesting mutable backing. |
| `gpui-fast/src/fast/path_cache.rs` | Converts the path-cache test fixture from a vector to the wrapper. |

Existing callers can borrow, index, reserve, extend and iterate by reference.
Use `vec.into()` when assigning a vector to the public field. The wrapper
dereferences to `Vec` so existing mutating APIs retain copy-on-write semantics.
Code that consumes a raw vector by value needs a separate owned copy or an
iterator over borrowed vertices.

Each shared buffer follows the lives of its path owners. The change adds one
Arc header per independent buffer and no vertex cache. The ID cache retains
at most 64 paths per window. Each cached path holds at most 32 element IDs. The font cache retains one process-wide empty vector.

Sprite primitives are copied without allocating. Cached text already reuses
`Arc<CacheKey>` and shaped layout handles in both backends. Tab-stop replay
for focusable widgets may still allocate.
