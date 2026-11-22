# DataUi documents

DataUi renders screens described as JSON. Applications keep the logic: documents emit events, and
the application replies with slot patches.

## Discover what a document can use

Install the CLI (no checkout needed), then export what it knows:

```sh
cargo install --locked --git https://github.com/hivemind-ai-core/guiraffe \
  --rev 716d957e4cdd3c950099800dcee5d4210508117e \
  --debug guiraffe-cli
guiraffe catalog --json > catalogue.json
guiraffe catalog --schema > screen.schema.json
guiraffe catalog --markdown > widget-reference.md
```

All three are generated from the widget descriptors in each crate, so they match that revision.

## Start from a recipe

[Recipes](../../crates/showcase/recipes) are complete screens (dashboard, form, settings, editor,
chat, wizard and more), validated against the schema and screenshot-tested.

```sh
curl -fsSLO https://raw.githubusercontent.com/hivemind-ai-core/guiraffe/716d957e4cdd3c950099800dcee5d4210508117e/crates/showcase/recipes/settings.json
guiraffe preview settings.json
```

Preview reloads when you save, keeps live values, and shows errors without losing the last valid
screen.

## Host a document

See the [guiraffe-ui README](../../crates/ui/README.md) for loading, validating, binding slots and
handling events.
