# GUIraffe 🦒

Rich, beautiful and FAST desktop UI for Rust, built on GPUI Kit + GPUI Controls (gpui-component), heavily inspired by Mantine.

## Authoring DataUi screens

Run the catalogue commands from a checkout, or use an installed `guiraffe` binary.

```sh
cargo run -p guiraffe-cli -- catalog --json > catalogue.json
cargo run -p guiraffe-cli -- catalog --schema > screen.schema.json
cargo run -p guiraffe-cli -- catalog --markdown > widget-reference.md
cargo run -p guiraffe-cli -- preview crates/showcase/recipes/settings.json
```

The catalogue, schema and widget reference come from the DataUi widget descriptors.
The [showcase recipes](crates/showcase/recipes) provide screen documents validated against
the generated schema and rendered in both themes by the headless snapshot suite. See
[coverage](crates/showcase/COVERAGE.md#authoring-catalogue-coverage) for the static widget inventory.

Bump the catalogue major version for breaking changes. Record the new catalogue fingerprint
in the changelog when publishing a catalogue revision. The CLI test prints the fingerprint.
