# DataUi documents

DataUi renders screens described as JSON. Applications keep the logic: documents emit events, and
the application replies with slot patches.

## Discover what a document can use

```sh
cargo run -p guiraffe-cli -- catalog --json > catalogue.json
cargo run -p guiraffe-cli -- catalog --schema > screen.schema.json
cargo run -p guiraffe-cli -- catalog --markdown > widget-reference.md
```

All three are generated from the widget descriptors in each crate, so they match your checkout.

## Start from a recipe

[Recipes](../../crates/showcase/recipes) are complete screens (dashboard, form, settings, editor,
chat, wizard and more), validated against the schema and screenshot-tested.

```sh
cargo run -p guiraffe-cli -- preview crates/showcase/recipes/settings.json
```

Preview reloads when you save, keeps live values, and shows errors without losing the last valid
screen.

## Host a document

See the [guiraffe-ui README](../../crates/ui/README.md) for loading, validating, binding slots and
handling events.
