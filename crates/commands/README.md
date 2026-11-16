# guiraffe-commands

A command catalog shared by native keyboard dispatch, an in-app menu, and the existing overlays Spotlight. The pure `context`, `keybinding`, and `model` modules have no GPUI imports; `Registry<Run>` accepts application-specific handler data. The GPUI `Commands` service specializes that registry with reference-counted callbacks.

## Native setup

Call `init(cx)` once, then create `Commands::new(cx)` for each window. Mount the view inside `guiraffe_overlays::root` and wrap focused content in `CommandScope::new(&commands)`. Register `Command::new(CommandId::parse("file.save")?, "Save", handler)` with a callback of type `Fn(&Payload, &mut Window, &mut App)`. Use `.keybinding(Keybinding::parse("ctrl+s")?)`, `.category("File")`, `.when(When::parse("Editor && !Readonly")?)`, and `.enabled(false)` as needed. Handlers receive the command ID and invocation source.

`Menu::new("commands-menu", &commands)` renders the same registry. Ctrl+Shift+P opens Spotlight within `CommandScope`. The `register`, `unregister`, `registry`, and `execute` free functions use a separate application-wide default service; calling `init` again preserves it. Prefer explicit services for window-local state. The complete runnable example is:

```sh
cargo run -p guiraffe-commands --example commands
cargo run -p guiraffe-commands --example commands --features gpui-kit/gpui-fast
```

## Contexts and resolution

`WhenContext` represents one GPUI-style context frame, including flags and key/value entries. `ContextStack` orders frames from ancestors to focused descendants; `scoped` pushes a frame and restores the original depth when its guard drops, including unwinding. `When` supports identifiers, `==`, `!=`, `!`, `&&`, `||`, `>`, and parentheses. Its precedence follows GPUI: `!`, equality, `&&`, `||`, then `>` (lowest). Equality compares identifiers; negate a comparison with `!(mode == vim)`. Canonical output is accepted by GPUI's predicate parser. Identifiers support Unicode alphanumeric characters, underscores, and hyphens; malformed or oversized expressions return errors.

Matching follows GPUI's context-prefix semantics. `Editor && writable` requires both flags in the same matching frame. `Workspace > Editor` selects the first matching ancestor, then matches the descendant against its remaining suffix. Native parity tests exhaust all 512 three-frame combinations for three flags across twelve expressions, in addition to named value and negation cases.

Use `set_contexts` for host state such as the active editor or document mode. Menu visibility and activation use those explicit host contexts, which remain stable as focus moves into the menu. Shortcut execution combines those frames with the focused window's actual GPUI contexts. Programmatic callers pass an explicit stack to `Commands::execute`, or use the default-service convenience function to capture the current window stack.

The catalog orders commands by lowercase category, lowercase title, then validated ID, independently of registration order. Disabled commands remain visible but cannot execute. For an exact binding conflict, the first enabled command whose predicate matches wins; `binding_conflicts` reports the winner and every shadowed ID. Disabled or inactive commands do not hide eligible alternatives. Distinct chord-prefix behavior is delegated to GPUI's native keymap.

## Keybindings and edits

Bindings use plus-separated modifiers and single-space-separated chords, for example `CTRL+SHIFT+P`, `cmd+k`, `alt+enter`, or `ctrl+k ctrl+s`. Aliases are normalized and modifiers use `ctrl`, `alt`, `shift`, `cmd`, `fn` order. Native conversion uses owned strings and GPUI's `KeyBinding::load`, including multi-stroke shortcuts. `plus` and `minus` name the literal keys. Empty strokes, unknown modifiers, duplicate modifiers, and malformed keys are rejected.

Use `lookup`, `set_enabled`, `set_keybinding`, and `replace` for a keymap editor. Clone a stored descriptor before changing its metadata; cloning a `Handler` preserves its identity so an unchanged replacement is a true no-op. All failed and no-op writes are checked before entering `Entity::update`; successful writes increment `RegistryState::revision` and notify once. Menu rows and ordering are prepared on mutation, rather than rebuilt during rendering. A consumer can read `entity()` and `catalog()` to derive other UI projections. Callbacks run after releasing registry borrows and may safely register or remove commands themselves.

GPUI's keymap is application-global. Installed routes are deduplicated, and services resolve them against their own current registries. Removing or rebinding a command leaves its previous native route installed but inert; no global keymap clearing occurs. A route with no current enabled match propagates to other action listeners. Applications should avoid assigning Ctrl+Shift+P to another command if they want to retain the built-in palette shortcut.

## Spotlight compatibility and limits

`spotlight_commands` returns the existing `guiraffe_overlays::search::Command` shape without changing overlays. `open_spotlight` snapshots the descriptors and originating native focus contexts, then rechecks the current registry, explicit host contexts, enabled state, and predicate before activation. Removed, disabled, or newly inactive commands cannot execute through a stale result. Displayed descriptors are a snapshot: observe the registry and call the existing `Spotlight::set_commands` with a fresh projection to update an already-open palette.

The crate supplies an in-app menu and metadata access for future adapters. Platform menu bars, toolbar integration, gallery registration, and IR command consumers require follow-up changes in their owning crates.

## Validation

`cargo test -p guiraffe-commands` exercises pure parsing/resolution, GPUI interaction and render invalidation, and light/dark menu snapshots. Repeat with `--features gpui-kit/gpui-fast`. Clippy runs with `--all-targets -- -D warnings` on both backends. Snapshot fixtures use the real asset source and four forced frames before capture, with separate upstream and gpui-fast goldens.
