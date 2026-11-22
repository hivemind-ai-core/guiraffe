# 29. Layout animation ("Animate everything", opt-in)

Status: implemented in `guiraffe-motion` (`layout` module), with hooks in `guiraffe-layout` (`Stack`),
`guiraffe-tree` (PropertyGrid/TreeList collapse) and a showcase entry ("Layout animation").

## Goal

When a layout change moves or resizes something (an item added to or removed from a list, a list
reordered, a group collapsed, a panel shown or hidden), the change can *optionally* animate: the
container's size and the items' positions are interpolated from the old layout to the new one
instead of jumping. Off by default. Never costs anything when idle or disabled.

## Rules this design follows (spec/00-api-conventions.md)

- §6.5 *Animate in leaves*: the element that animates requests frames for the view that renders it,
  and only while it is moving. Put the animated container in the smallest view that owns the data.
- §6.7 no per-frame allocations or O(n) scans beyond the children already being laid out.
- §8 motion tokens, reduced motion, `Presence` for exits.

## API (crate `guiraffe-motion`)

### Settings

```rust
pub struct LayoutMotion { pub enabled: bool, pub timing: Timing }
// MotionPrefs { reduced, time_scale, tokens, layout: LayoutMotion }
```

- **Global opt-in**: `MotionPrefs::layout.enabled` (default `false`), next to the motion tokens the
  theme already owns. `LayoutMotion::default().timing` is `Timing::Spring(Spring::SNAPPY)`, the
  spring the accordion uses, so retargeting keeps velocity.
- **Per-container override**: every animated container takes `.enabled(bool)` (wins over the global)
  and `.timing(Timing)`. `None` (not called) means "follow `MotionPrefs::layout`".
- **Reduced motion** (`MotionPrefs::reduced`, which folds in the platform setting): every container
  behaves exactly as if disabled: a plain pass-through layout, new sizes and positions apply in the
  same frame.
- A `StyleContext` field for per-subtree opt-in is deferred: it needs a `guiraffe-core` change; the
  per-container override covers subtrees today.

### `AnimatedSize` — interpolate a container's size

```rust
animate_size("details", child)            // vertical (height), follows the global setting
AnimatedSize::new("panel", child).axis(SizeAxis::Horizontal).enabled(true).timing(t)
```

- An `Element` with an id (its state lives in GPUI element state keyed by the id path, so it
  survives re-renders and reorders of its parent).
- **Measure once per frame**: in prepaint the child is laid out exactly once, as a root
  (`layout_as_root`), at the container's cross size (`Vertical`: definite width, min-content
  height; `Horizontal`: max-content width, definite height; `Both`: max-content both). The child is
  *not* part of the parent's layout tree, so it is never laid out twice.
- The container's own size along the animated axis is the animated value; the cross axis fills its
  parent (100 %). The child is painted at its natural size, clipped to the container (`.clip(false)`
  to let it overflow).
- When the measured size differs from the target, the animation **retargets from the current value**
  (`Animated::set_at`: continuous position, springs keep velocity). The first frame shows the old
  size; there is no one-frame jump.
- The very first frame (no state yet) lays the child out normally and records its size: content
  appears at its natural size, never animates in from zero (use `Presence` for that).
- Frames: `request_animation_frame` only while the value moves or the container lags its target.
  At rest a frame re-render costs one layout of the child (the same as without the wrapper) and no
  frame requests; nothing is allocated in steady state.
- Caveat: the child must have an intrinsic size along the animated axis (a `flex_1`/`h_full` child
  measures as zero). On retained rendering (gpui-fast) a *nested view* whose size changes without
  the wrapper's view re-rendering is measured when the wrapper's view next renders.

### `AnimatedList` — FLIP for keyed children

```rust
AnimatedList::new("todo").gap(px(6.)).keyed(item.id, row(item))   // a flex column
```

- Each child has a key (`impl Into<ElementId>`). The container records each child's origin
  *relative to the container* by key (scrolling or moving the whole list never animates).
- **FLIP**: on a frame where the key sequence differs from the previous frame (add, remove,
  reorder; detected with a 64-bit hash of the keys, O(children)), every child whose relative origin
  moved is painted offset back to where it *was visually* (including any in-flight offset, so
  interruptions continue from the current position) and the offset animates to zero. Layout is never
  changed by the offset (prepaint `with_element_offset`), so hit-testing follows what is painted.
- Layout changes that are not structural (window resize, an item's content growing, an entering
  item's reveal pushing its neighbours) do not start FLIP animations: they are already continuous
  or should apply immediately.
- **Enter/exit** via [`KeyedPresence<K>`]: a pure helper the owning view keeps. `sync(keys, now, prefs)`
  diffs the new key list against the old one, keeps removed keys mounted at their old position with
  an exiting `Presence`, and gives entering keys an entering `Presence`. `entries()` yields
  `(key, visibility)` in display order; `prune(now, prefs)` drops finished exits and says whether a
  frame is needed. The view wraps entering/exiting rows in `reveal(id, visibility, row)` (height
  scales with visibility, opacity follows) so neighbours slide smoothly; when the reveal reaches 0
  the key leaves the sequence with zero displacement, so FLIP has nothing to do.
- Without `KeyedPresence`, a removed item disappears at once and its neighbours FLIP into place.
- Allocation: one `HashMap<ElementId, ItemMotion>` per list in element state, reused; it grows only
  when new keys appear and is pruned on structural frames only.

### `reveal(id, progress, child)`

A clipped vertical reveal whose height is `measured_height × progress` with opacity `progress`
(progress clamped to [0, 1]). Pure function of the progress the caller samples (from `Presence`);
it does not request frames itself except when the measured height changes.

### Pure core (unit-testable without GPUI)

- `SizeTrack` — the `AnimatedSize` state machine: `observe(measured, now, prefs) -> bool` (retarget),
  `value_at(now, prefs) -> (f32, moving)`.
- `FlipTrack` — per-item offset state: `moved(prev_visual, new_origin, now)`, `offset_at(now)`.
- `KeyedPresence<K>` — as above.
- `keys_hash` — order-sensitive key hash.

## Hooks in other crates

- `guiraffe-layout::Stack::animate_layout(id)`: wraps the stack in `AnimatedSize` (vertical), with
  `animate_layout_with(id, enabled: Option<bool>)` for the per-container override.
- `guiraffe-tree` (PropertyGrid and TreeList): `set_animate_layout(Option<bool>)` (default: follow
  `MotionPrefs::layout`). On expand/collapse the rows after the toggled group get an animated vertical
  shift: on expand the new child rows slide down out from under the group header (clipped), on
  collapse the rows below slide up to close the gap. Implemented in `crates/tree/src/collapse_motion.rs`
  plus a few lines in `set_expanded` and the rows container; the virtualized window is extended by
  the shift so no rows are missing at the bottom. Disabled while a filter is active.
- Accordion and ExpandingList already animate panel height with a measured reveal driven by a
  spring, interruptibly and with reduced motion respected (`disclosure_motion`), which is the same
  behaviour; they are left as is.

## Tests

- Pure: interpolation, retarget mid-flight continues from the current value, reduced motion is
  instant, `KeyedPresence` diff (enter, exit at old position, re-add during exit, reorder), hash.
- GPUI (`TestAppContext`, both backends): `AnimatedSize` starts at the old size and ends at the new
  one; retargets mid-flight without a jump; reduced motion and disabled apply the new size in the
  same frame; no frame is requested at rest; FLIP offsets start at the old position and end at zero;
  an animating frame allocates no more than an idle re-render of the same view plus 3
  (`alloc_tests.rs`, counting allocator; the 3 are GPUI's frame scheduling: one boxed next-frame
  callback per animating element and the callback list it re-creates; our state allocates 0;
  on gpui-fast the bound is +8 because an idle re-render reuses retained layout/paint that a
  moving frame cannot). A pure test asserts 0 allocations for 60 steady FLIP/size frames.
- Tree: expanding a group with animation on shifts the following rows and settles; off by default.
- Showcase goldens of the demo at rest, light and dark, both backends.
