# Data UI review #1: merged after rework
Fixed:
1. Granular re-render: bound nodes are their own entities, and patches notify only them. Measured on gpui-fast: root 1→1, sibling 1→1, patched node 1→2.
   - Root-cause discovery: `entity.update(cx, |_, cx| cx.notify())` counts as a *write* on gpui-fast and invalidates ancestors. We now use `cx.notify(entity_id)`, recorded in conventions §6.2. The same pattern was fixed in flow, and reported to layout, where it is the likely cause of its gpui-fast failures.
   - Layout-changing values may still re-layout ancestors (documented).
2. Hot reload: `Logic::bind(names, out)` can fill new slots. `migrate` keeps versions, bumps the version of new slots and the generation, and runs in O(n).
3. Stale events: emits are stamped with the template generation, so mismatches are dropped and counted.
4. Widget schemas: props and events are stored by position at compile time, with no string lookups at render. Unknown props and events are diagnosed.
5. Duplicate ids: an Error diagnostic, and `find` is O(1).
8. JSON objects as prop values are diagnosed.
9. Value equality: NaN equals itself by bit pattern, and lists short-circuit on `Arc::ptr_eq`. Extreme numbers display in exponent form.
10. One depth limit (60), consistent between JSON and builders and tested both ways. The render-time duplicate check was removed.
11. Template fields are private, and `children()` cannot panic.
12. The emit closure is built once. The event path never writes the host entity; `DataUiHandle` serves external pushes.
Literal values on bound props seed slots (designer defaults). Templates are Send + Sync and compile on any thread.
Follow-ups (open spec/10 work):
- 6: event payloads and two-way inputs.
- 7: `each`/`when` plus splice patches.
- 13: a typed bindings helper (derive).
- An allocation benchmark for instantiate and patch.
- Parsing size/color props at compile time.
