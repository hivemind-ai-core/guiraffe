# GUIraffe 🦒

Rich, beautiful and FAST desktop UI for Rust, built on GPUI Kit + GPUI Controls (gpui-component), heavily inspired by Mantine.

## New to GUIraffe?

- [Getting started](docs/getting-started.md): prerequisites, creating an app, and six runnable
  examples (hello window, counter, form validation, data grid, theming, router and store).
- [GUIraffe for React and Electron developers](docs/from-react-electron.md): how React concepts
  map to GUIraffe, the rendering model, and the measured comparison with Electron.

## Build a small desktop tool

Start with the [GUIraffe quickstart](docs/ui-macro.md#start-a-small-desktop-tool) for a runnable
counter, dependencies for your own Rust binary, and examples of `ui!` and `#[component]`.

## Calendar views

Use `guiraffe-calendar` for year, month, week, and day schedules with shared multi-source events,
chrono time zones, editing callbacks, keyboard access, and virtualized hours. See the
[calendar API and interaction specification](spec/27-calendar.md), or run the four live showcase
pages with `cargo run -p guiraffe-showcase --features gpui-fast`.

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
