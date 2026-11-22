# Command ribbon

Status: draft for 0.3, not implemented (see the [roadmap](../../ROADMAP.md)). New component in `guiraffe-commands`.

## 1. Source

Port the prototype at `../gui/research/cmd` (`command-core`, `command-gpui`, demo host; GPUI Kit
0.7.0, about 1.8k lines) and its design notes `../gui/research/notes/command-system.md`. Don't
vendor it wholesale: GUIraffe already has a command registry, keybindings, `When` contexts,
menus and a Spotlight adapter in `guiraffe-commands`. Port the ideas and the widget onto those,
and adapt to Kit 0.9.1. Keep the prototype's tests as the behavioural reference.

## 2. What it is

A keyboard-first command bar that slides in above the status bar (not a modal, not an Office
tab-strip ribbon):

- Toggled by a leader key (default Ctrl+Space) or a centred status-bar button.
- The root level shows **category columns** in registration order: Global, then the current
  page's categories, then the selection's (e.g. Global | Todos | Details). Columns have headers
  with vertical separators; a category with more than six entries spans several columns; rows are
  fixed height with a reserved icon slot, a chord key and a label; submenu entries show a marker.
  Parent and Root buttons share the first category header.
- **Chord navigation**: typing an entry's key activates it (`t` → `e` descends two levels);
  Backspace goes up one level, Escape to root and then closes, the leader closes from anywhere.
  Clicking works the same. A leaf closes the bar and runs, or opens a parameter prompt.
- **Parameters**: required text, static choices and host-provided dynamic choices with stable
  ids; a valid selection fills a target automatically, otherwise the bar prompts. A draft never
  executes until complete and revalidated.
- **Availability**: navigation conditions (`When`) hide categories/branches; semantic availability
  disables entries with a reason, enforced identically from every entry point.

## 3. Integration

- One vocabulary: ribbon entries reference `CommandId`s; a command can appear in several menus or
  none. The ribbon, Spotlight, menus and keybindings are projections of the same registry.
  Spotlight lists the whole vocabulary (discoverable even off-page); the ribbon is contextual.
- Registry validation at build/rebind: ids, references, cycles, parameter schemas, shortcut and
  chord collisions (chord keys unique within a menu across all its categories). Invalid
  replacements leave the active registry untouched.
- Commands dispatch through the the control protocol action bus with `source: Ribbon`, so any control action
  can be placed on the ribbon (`ribbon: "format/b"` in `ui!`, store-first `ui!` §3), recorded, replayed and
  invoked via MCP.
- User overrides (shortcuts and chord keys) persist as a small JSON override file with the
  prototype's safety rules: atomic write, malformed files preserved and writes blocked until
  fixed, `null` unbinds. Reuse the existing keybinding editor if `guiraffe-commands` has one;
  otherwise port the prototype's key-bindings page as a showcase demo.
- Focus: opening captures keys above focused inputs only for registered chords and the leader;
  closing restores the previous focus. Held keys repeat navigation but never repeat execution.

## 4. Builder and DataUi

`CommandRibbon::new(&commands)` with props: leader key, rows per column, row height, placement
(above status bar / bottom / top), show icons, show shortcuts, animate open/close, max columns
before horizontal scroll. Available through the DataUi catalogue and `ui!`.

## 5. Performance

Same rules as everything else: opening the bar must not rebuild the registry (projections are
compiled at registry revision), an unchanged repaint allocates nothing, open/close ≤ one frame of
work, and a 2k-command vocabulary with 200 categories stays within p99 ≤8 ms. Bench in
`guiraffe-bench`.

## 6. Acceptance

- Pure tests ported from the prototype (navigation, reconcile on context change, chord collision
  validation, drafts, overrides parsing).
- GPUI `TestAppContext` tests: leader toggle, chord descent and Backspace/Escape, click and
  keyboard equivalence, prompts with dynamic targets, disabled reasons, focus restore, held-key
  behaviour, identical results to Spotlight and shortcuts for the same command.
- Showcase entry with a realistic vocabulary (editor-like app: File/Edit/View/Selection with page
  and selection contexts), goldens light/dark on both backends, a11y roles and labels.
