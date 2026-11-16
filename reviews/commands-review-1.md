# Commands review #1: BLOCKED (owner: commands author, after data-ui-2)
Must fix:
1. Specificity: the deepest matching `when` context wins; catalog order only breaks ties.
2. A disabled winner consumes the keystroke and never falls through to a shadowed command.
3. A platform-neutral `mod`/`secondary` modifier; the palette uses it; tests per OS.
4. Report chord-prefix conflicts (is_prefix_of is unused).
Same pass:
5. A binding → command index; no per-keystroke parsing, cloning or scanning; lazy menu rows.
6. Refcount native keymap routes and remove them; test that app bindings still fire.
Follow-ups: document the CommandScope requirement or use window-level routing (7); WeakEntity guidance plus a scoped registration guard (8); Spotlight observes the registry (9); a gallery story and tests (10).
