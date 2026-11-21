# guiraffe-commands

A command catalog shared by native keyboard dispatch, an in-app menu, and the existing overlays Spotlight. The pure `context`, `keybinding`, and `model` modules have no GPUI imports; `Registry<Run>` accepts application-specific handler data. The GPUI `Commands` service specializes that registry with reference-counted callbacks.

## Native setup

Call `init(cx)` once, then create `Commands::new(cx)` for each window. Mount the view inside `guiraffe_overlays::root` and wrap focused content in `CommandScope::new(&commands)`. Register `Command::new(CommandId::parse("file.save")?, "Save", handler)` with a callback of type `Fn(&Payload, &mut Window, &mut App)`. Use `.keybinding(Keybinding::parse("ctrl+s")?)`, `.category("File")`, `.when(When::parse("Editor && !Readonly")?)`, and `.enabled(false)` as needed. Handlers receive the command ID and invocation source.

`Menu::new("commands-menu", &commands)` renders the same registry. ⌘⇧P on macOS / Ctrl+Shift+P elsewhere (`PALETTE_SHORTCUT`, `secondary-shift-p`) opens Spotlight within `CommandScope`. Shortcuts only route inside a `CommandScope`: place one around the focused content of every window that should react to them. The `register`, `unregister`, `registry`, and `execute` free functions use a separate application-wide default service; calling `init` again preserves it. Prefer explicit services for window-local state. The complete runnable example is:

```sh
cargo run -p guiraffe-commands --example commands
cargo run -p guiraffe-commands --example commands --features gpui-kit/gpui-fast
```

## Contexts and resolution

`WhenContext` represents one GPUI-style context frame, including flags and key/value entries. `ContextStack` orders frames from ancestors to focused descendants; `scoped` pushes a frame and restores the original depth when its guard drops, including unwinding. `When` supports identifiers, `==`, `!=`, `!`, `&&`, `||`, `>`, and parentheses. Its precedence follows GPUI: `!`, equality, `&&`, `||`, then `>` (lowest). Equality compares identifiers; negate a comparison with `!(mode == vim)`. Canonical output is accepted by GPUI's predicate parser. Identifiers support Unicode alphanumeric characters, underscores, and hyphens; malformed or oversized expressions return errors.

Matching follows GPUI's context-prefix semantics. `Editor && writable` requires both flags in the same matching frame. `Workspace > Editor` selects the first matching ancestor, then matches the descendant against its remaining suffix. Native parity tests exhaust all 512 three-frame combinations for three flags across twelve expressions, in addition to named value and negation cases.

Use `set_contexts` for host state such as the active editor or document mode. Menu visibility and activation use those explicit host contexts, which remain stable as focus moves into the menu. Shortcut execution combines those frames with the focused window's actual GPUI contexts. Programmatic callers pass an explicit stack to `Commands::execute`, or use the default-service convenience function to capture the current window stack.

The catalog orders commands by lowercase category, lowercase title, then validated ID, independently of registration order. Disabled commands remain visible but cannot execute.

When several commands share a shortcut, the most specific one wins: the command whose `when` expression matches at the deepest context frame (GPUI's `depth_of`); an unconditional command is the least specific; catalog order breaks ties. Disabled or inactive commands are skipped, so a disabled winner never swallows the key: the next match runs, and if no command matches the key propagates to application bindings. `conflict_report` lists exact conflicts (winner plus every shadowed ID, in precedence order) and chord-prefix conflicts (`ctrl+k` shadowing `ctrl+k ctrl+s`: the shorter binding waits for a second key).

## Keybindings and edits

Use `secondary` (alias `mod`) for the platform's primary modifier: `secondary+s` is ⌘S on macOS and Ctrl+S elsewhere, resolved with the OS GPUI reports, and conflicts with a physical `ctrl+s` there. Menus show `Keybinding::label`: `⇧⌘P` on macOS, `Ctrl+Shift+P` elsewhere. Bindings use plus-separated modifiers and single-space-separated chords, for example `CTRL+SHIFT+P`, `cmd+k`, `alt+enter`, or `ctrl+k ctrl+s`. Aliases are normalized and modifiers use `ctrl`, `alt`, `shift`, `cmd`, `fn` order. Native conversion uses owned strings and GPUI's `KeyBinding::load`, including multi-stroke shortcuts. `plus` and `minus` name the literal keys. Empty strokes, unknown modifiers, duplicate modifiers, and malformed keys are rejected.

Use `lookup`, `set_enabled`, `set_keybinding`, and `replace` for a keymap editor. Clone a stored descriptor before changing its metadata; cloning a `Handler` preserves its identity so an unchanged replacement is a true no-op. All failed and no-op writes are checked before entering `Entity::update`; successful writes increment `RegistryState::revision` and notify once. A binding index (platform-resolved shortcut to commands) is maintained on every mutation, so a keystroke is one hash lookup plus predicate checks; native routes carry the resolved shortcut and are never re-parsed. Menu rows are built lazily on the first render after a change and cached until the next one. A consumer can read `entity()` and `catalog()` to derive other UI projections. Callbacks run after releasing registry borrows and may safely register or remove commands themselves.

GPUI's keymap is application-global. Native routes are installed through `guiraffe_core::keys` (fallible, OS-aware) and reference-counted across services: one route per shortcut, removed from the keymap when the last command using it is unregistered or rebound, or when its service is dropped (an unmounted view). Removal keeps every application binding and its order. A route with no current enabled match propagates to other action listeners. Keep the `Commands` handle in the view that owns the commands and let it drop with the view; capture `WeakEntity` handles in command callbacks to avoid cycles that would keep a service alive.

## Spotlight compatibility and limits

`spotlight_actions` projects visible commands to `guiraffe_overlays::SpotlightAction`s: the category becomes the group and the platform shortcut label (`Keybinding::label`) becomes keycap chips. `spotlight_commands` still returns the plain `guiraffe_overlays::search::Command` shape for other search UIs. `open_spotlight` snapshots the originating native focus contexts, then rechecks the current registry, explicit host contexts, enabled state, and predicate before activation. Removed, disabled, or newly inactive commands cannot execute through a stale result. An open palette observes the registry and re-projects on every change; recently run commands are kept on `RegistryState::spotlight_recents` and listed under "Recent" the next time.

The crate supplies an in-app menu and metadata access for future adapters. Platform menu bars, toolbar integration, gallery registration, and IR command consumers require follow-up changes in their owning crates.

## Validation

`cargo test -p guiraffe-commands` exercises pure parsing/resolution, GPUI interaction and render invalidation, and light/dark menu snapshots. Repeat with `--features gpui-kit/gpui-fast`. Clippy runs with `--all-targets -- -D warnings` on both backends. Snapshot fixtures use the real asset source and four forced frames before capture, with separate upstream and gpui-fast goldens.
