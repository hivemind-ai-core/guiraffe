# PropertyGrid (and TreeList) polish

Make PropertyGrid feel like a mature inspector (VS/Blender/Unity/Figma-style). Owner: codex.

- **Resizable split**: draggable divider between name and value columns (hover cursor, keyboard
  resize, min widths, double-click to auto-fit names), split ratio persisted via `.persist_key()`.
- **Row styling options**: alternating row colours (theme tokens, on/off), row height presets
  (compact/default/comfortable) plus a per-row override, optional grid lines, group header styling,
  indentation guides, sticky group headers while scrolling.
- **Value editors**: auto-sizing inputs (fill the value column; multi-line text grows to a cap then
  scrolls), number fields with drag-to-scrub, units and min/max/step, sliders for ranges, colour
  swatch + picker, enum select, flags (multi-select), vector/rect fields (x/y/w/h), path/file picker
  hook, read-only and mixed-value ("—") states for multi-selection, reset-to-default affordance,
  validation errors inline.
- **Interaction**: search/filter box with match highlighting, expand/collapse all, keyboard: arrows,
  Enter to edit, Esc to cancel, Tab between editors; copy/paste values; context menu (reset, copy).
- **Tooltips/descriptions** per property; category/advanced toggles; favourites/pinning optional.
- Keep it within budgets (1M nodes / 100k expanded: mount ≤200 ms, p99 ≤8 ms), a11y roles/labels,
  goldens for each option in both themes/backends, showcase knobs for every option.
- TreeList gets the shared pieces: alternating rows, row heights, indent guides, sticky headers.
