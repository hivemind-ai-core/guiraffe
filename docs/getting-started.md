# Getting started with GUIraffe

GUIraffe is a Rust toolkit for native desktop apps. It's built on GPUI (the GPU-rendered UI
framework from Zed) and GPUI Kit, which bundles GPUI with GPUI Controls, a widget library
with Button, Input, Checkbox, Select and more. GUIraffe adds a theme system, Mantine-style
components, a data grid, typed routing, an event-driven store and much more.

This guide takes you from an empty machine to a multi-page app in six small examples. Each
one is a runnable file in [examples/getting-started](../examples/getting-started/examples).
The code shown here is copied from those files, and a test in that crate fails if they drift
apart.

Coming from React or Electron? Read [GUIraffe for React and Electron developers](from-react-electron.md)
alongside this guide.

- [Prerequisites](#prerequisites)
- [Run the examples](#run-the-examples)
- [Create your own app](#create-your-own-app)
- [1. Hello, window](#1-hello-window)
- [2. Counter: state, events and `ui!`](#2-counter-state-events-and-ui)
- [3. A form with validation](#3-a-form-with-validation)
- [4. A data table](#4-a-data-table)
- [5. Theming and dark mode](#5-theming-and-dark-mode)
- [6. Pages, a router and a store](#6-pages-a-router-and-a-store)
- [The two GPUI backends](#the-two-gpui-backends)
- [Debugging](#debugging)
- [Where to go next](#where-to-go-next)

## Prerequisites

- **Rust 1.90 or newer** (the workspace's `rust-version`). Install it with
  [rustup](https://rustup.rs), or run `rustup update stable`.
- **A GPU with a Vulkan driver.** GPUI renders on the GPU. Install your driver's Vulkan
  loader, for example `libvulkan1 mesa-vulkan-drivers` on Debian or Ubuntu.
- **`pkg-config` and these system libraries** on Linux:

  | Library | Debian / Ubuntu | Fedora | Arch |
  |---|---|---|---|
  | xkbcommon | `libxkbcommon-dev` | `libxkbcommon-devel` | `libxkbcommon` |
  | xkbcommon (X11) | `libxkbcommon-x11-dev` | `libxkbcommon-x11-devel` | `libxkbcommon-x11` |
  | Wayland client | `libwayland-dev` | `wayland-devel` | `wayland` |
  | XCB | `libxcb1-dev` | `libxcb-devel` | `libxcb` |
  | fontconfig | `libfontconfig-dev` | `fontconfig-devel` | `fontconfig` |
  | FreeType | `libfreetype-dev` | `freetype-devel` | `freetype2` |

  On Debian or Ubuntu, for example:

  ```sh
  sudo apt install pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
    libxcb1-dev libfontconfig-dev libfreetype-dev libvulkan1 mesa-vulkan-drivers
  ```

- **A local GUIraffe checkout.** GUIraffe isn't published to crates.io yet, so apps depend
  on the crates by path.

`guiraffe doctor` checks all of this. It reports the Rust version, each library (via
`pkg-config`), the Vulkan loader, and, when run inside an app, the dependency pins.

```sh
cargo run -p guiraffe-cli -- doctor
```

## Run the examples

From the checkout:

```sh
cargo run -p getting-started-examples --example 01_hello --features gpui-fast
```

The examples are `01_hello`, `02_counter`, `03_form`, `04_datagrid`, `05_theme` and
`06_router_store`. The first build compiles GPUI and takes a while. Dependencies build with
`opt-level = 2` even in dev builds, because interactive graphics need optimised code.

`--features gpui-fast` selects the GPUI Fast backend. Leave it off to run on upstream GPUI.
See [the two GPUI backends](#the-two-gpui-backends).

The examples in `03_form` and `06_router_store` include unit tests:

```sh
cargo test -p getting-started-examples
```

## Create your own app

### With `guiraffe new`

The `guiraffe` CLI scaffolds a standalone app that uses your checkout. Build it once, then
run it from the directory you want the app in:

```sh
cargo build -p guiraffe-cli          # in the checkout; the binary is target/debug/guiraffe
cd ~/projects
/path/to/anteater/target/debug/guiraffe new my-app --path /path/to/anteater
cd my-app && cargo run
```

`guiraffe new` refuses to overwrite an existing directory. It writes:

- `Cargo.toml` with path dependencies on `guiraffe-core`, `-components`, `-motion`,
  `-state`, `-router`, `-windows`, `-overlays` and `-icons`, the pinned `gpui-kit =0.9.1`
  with the `gpui-fast` backend, and an empty `[workspace]` table so the app never joins an
  enclosing workspace.
- `src/main.rs`: a shell with a sidebar, typed routes, a store with undo, and a window
  opened through the window manager.
- `Cargo.lock`, copied from the checkout, so you build with the dependency versions
  GUIraffe was tested with.

Then **add the `[patch.crates-io]` block** from the next section to the generated
`Cargo.toml`. The scaffold doesn't include it yet. The checkout patches GPUI, GPUI Fast and
GPUI Kit with fixes and performance work ([vendor/README.md](../vendor/README.md)), and a
standalone app doesn't inherit the workspace's patches.

`guiraffe add <crate>…` adds more GUIraffe crates to the app, for example
`guiraffe add datagrid inputs`. Run it with no arguments to list them.
`guiraffe dev` watches the app, rebuilds and relaunches it.

### By hand

Any Cargo binary works. This is the `Cargo.toml` for an app in a directory next to the
checkout (`../anteater`). Adjust the paths to your layout.

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"

# A standalone app: don't join an enclosing workspace.
[workspace]

[dependencies]
gpui-kit = { version = "=0.9.1", features = ["component", "assets", "gpui-fast"] }
# gpui-kit 0.9.1 needs the pre-0.1.16 GPUI Fast API (KNOWN_ISSUES #3).
gpui-fast = "=0.1.15"
guiraffe-core = { path = "../anteater/crates/core" }
guiraffe-components = { path = "../anteater/crates/components" }
guiraffe-ui = { path = "../anteater/crates/ui" }
guiraffe-icons = { path = "../anteater/crates/icons" }
# Add what you use: guiraffe-inputs, -datagrid, -router, -state, -layout, -overlays, ...

# Interactive graphics need optimised dependencies even in dev builds.
[profile.dev.package."*"]
opt-level = 2

# The same patches as the GUIraffe workspace. Apps outside the workspace must repeat them.
[patch.crates-io]
gpui-fast = { path = "../anteater/vendor/gpui-fast" }
gpui-fast-linux = { path = "../anteater/vendor/gpui-fast-linux" }
gpui-pre = { path = "../anteater/vendor/gpui-pre" }
gpui-base = { path = "../anteater/vendor/gpui-base" }
gpui-component = { path = "../anteater/vendor/gpui-component" }
mermaid-rs-renderer = { path = "../anteater/vendor/mermaid-rs-renderer" }
```

Copy the checkout's `Cargo.lock` into your app before the first build, as `guiraffe new`
does. A fresh resolve can pick newer upstream patch releases that don't work with
`gpui-kit 0.9.1`. [examples/compare/guiraffe-app/Cargo.toml](../examples/compare/guiraffe-app/Cargo.toml)
is a complete, working standalone manifest.

Cargo warns that a patch "was not used in the crate graph" when your app doesn't depend on
that crate (for example `mermaid-rs-renderer` without `guiraffe-markdown`). The warning is
harmless, and keeping the full block means adding crates later needs no manifest changes.

Then copy any example below into `src/main.rs` and run `cargo run`. Examples 1, 2 and 5 need
only the crates above. Add `guiraffe-inputs` for example 3, `guiraffe-datagrid` for
example 4, and `guiraffe-router` and `guiraffe-state` for example 6.

## 1. Hello, window

[01_hello.rs](../examples/getting-started/examples/01_hello.rs)

```rust
use gpui_kit::{
    App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, WindowOptions,
    div, px,
};
use guiraffe_components::{Text, Title};
use guiraffe_core::GuiraffeTheme;

/// A view: a struct that implements `Render`. GPUI keeps it alive in an `Entity`.
struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Colours come from the theme, never from constants.
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .bg(s.body.to_hsla())
            .child(Title::new("Hello, GUIraffe").order(1))
            .child(Text::new("A native window, drawn on the GPU.").dimmed(true))
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            // Installs GPUI Controls and the GUIraffe theme (idempotent).
            guiraffe_components::init(cx);
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let hello = cx.new(|_| Hello);
                cx.new(|cx| gpui_kit::component::Root::new(hello, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the window: {error}");
                cx.quit();
            }
        });
}
```

The pieces you'll see in every GUIraffe app:

- **A view** is a struct that implements `Render`. `render` returns elements built with
  GPUI's Tailwind-like methods (`flex`, `items_center`, `gap`, `bg`).
- **`cx.new(|_| Hello)`** creates an `Entity<Hello>`. GPUI owns the struct and keeps it
  alive between frames.
- **`guiraffe_components::init(cx)`** installs GPUI Controls and the default GUIraffe theme.
  Every GUIraffe crate has an idempotent `init`, so call the ones for the crates you use.
- **`Root`** from GPUI Controls wraps the window's top view. Controls need it for dialogs,
  notifications and focus handling.
- **`GuiraffeTheme::global(cx).surfaces()`** gives the scheme's semantic colours (`body`,
  `surface`, `text`, `dimmed`, `border` and more). Don't hard-code colours. Take them from
  the theme so light and dark both work.
- **`Title` and `Text`** are GUIraffe components. Their props are builder methods named
  after Mantine's: `.order(1)` for the heading level, `.dimmed(true)` for secondary text.
- **`GuiraffeAssets`** serves the full Lucide icon set plus GPUI Kit's own assets.
- `cx.open_window` returns a `Result`. GUIraffe avoids `unwrap` and `expect` outside tests
  (clippy warns on `unwrap`), so the example reports the error and quits.

## 2. Counter: state, events and `ui!`

[02_counter.rs](../examples/getting-started/examples/02_counter.rs)

The `ui!` macro is a declarative syntax for the same GPUI elements. It's optional, but it
makes nested layouts easier to read. A reusable piece with props is a `#[component]`:

```rust
/// A stateless component: props in, elements out. `#[component]` generates
/// `Stat::new(label, value)` plus a builder method for each optional prop.
#[component]
fn Stat(
    label: &'static str,
    value: SharedString,
    #[prop(default)] hint: Option<&'static str>,
) -> impl IntoElement {
    ui! {
        div {
            flex flex_col items_center gap_1
            Text(label) { dimmed: true }
            Title(value) { order: 1 }
            if let Some(hint) = hint { Text(hint) { dimmed: true } }
        }
    }
}
```

`#[component]` generates `Stat::new(label, value)`, a `hint(..)` builder for the optional
prop, and GPUI's `RenderOnce` implementation. Inside `ui!`, `Text(label) { dimmed: true }`
means `Text::new(label).dimmed(true)`, and `flex flex_col` calls the no-argument methods
`.flex().flex_col()`.

The view owns its state and changes it in event handlers:

```rust
/// The view owns its state. Rendering reads it; listeners mutate it.
#[derive(Default)]
struct Counter {
    count: i64,
}

impl Counter {
    fn change(&mut self, delta: i64, cx: &mut Context<Self>) {
        self.count = self.count.saturating_add(delta);
        // Ask GPUI to render this view again on the next frame.
        cx.notify();
    }
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.count;
        ui! {
            div {
                size_full flex items_center justify_center
                Card {
                    div {
                        flex flex_col items_center gap_4 p_4
                        Stat("Count", format!("{count}").into()) {
                            hint: (count == 0).then_some("Press +1 to start")
                        }
                        div {
                            flex gap_2
                            Button("decrement") {
                                label: "−1"
                                disabled: count == 0
                                on_click: cx.listener(|this, _, _, cx| this.change(-1, cx))
                            }
                            Button("increment") {
                                label: "+1";
                                .primary()
                                on_click: cx.listener(|this, _, _, cx| this.change(1, cx))
                            }
                            Button("reset") {
                                label: "Reset";
                                .ghost()
                                on_click: cx.listener(|this, _, _, cx| {
                                    this.count = 0;
                                    cx.notify();
                                })
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- `cx.listener(|this, event, window, cx| …)` turns a closure into a handler with mutable
  access to the view (`this`).
- **Nothing redraws until you call `cx.notify()`.** It schedules a render of this view.
- `.primary()` and `.ghost()` are button variants from GPUI Controls. To call a builder
  method after a `name: value` prop, end the prop with `;`, as in `label: "+1";`. Otherwise
  the method would apply to the value.

[docs/ui-macro.md](ui-macro.md) covers the rest of the macro: `for … key …` loops,
`match`, embedding entities with `view(..)`, and cached subtrees.

## 3. A form with validation

[03_form.rs](../examples/getting-started/examples/03_form.rs)

Text fields are **Controls**: an `Entity<InputState>` owns the text, cursor and selection,
and the `Input` element draws it. Keep the validation rules in plain functions over plain
data so you can test them without a window:

```rust
/// What the user typed. Plain data, so validation is testable without a window.
#[derive(Debug, Clone, Default, PartialEq)]
struct Values {
    name: String,
    email: String,
    password: String,
    terms: bool,
}

/// One optional message per field. `None` means the field is valid.
#[derive(Debug, Clone, Default, PartialEq)]
struct Errors {
    name: Option<&'static str>,
    email: Option<&'static str>,
    password: Option<&'static str>,
    terms: Option<&'static str>,
}

impl Errors {
    fn is_empty(&self) -> bool {
        *self == Errors::default()
    }
}

fn validate(v: &Values) -> Errors {
    let email = v.email.trim();
    Errors {
        name: v.name.trim().is_empty().then_some("Enter your name."),
        email: if email.is_empty() {
            Some("Enter your email address.")
        } else if !email
            .split_once('@')
            .is_some_and(|(user, host)| !user.is_empty() && host.contains('.'))
        {
            Some("That doesn't look like an email address.")
        } else {
            None
        },
        password: (password_strength(&v.password) < 3)
            .then_some("Use at least 10 characters with mixed case, digits or symbols."),
        terms: (!v.terms).then_some("Accept the terms to continue."),
    }
}
```

The view creates the inputs once and subscribes to their events. Dropping a `Subscription`
unsubscribes, so keep them in a field:

```rust
impl SignupForm {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Ada Lovelace"));
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("ada@example.com"));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        // Inputs are Controls: they own their text and emit events. Re-render the form
        // when any of them changes so the error states and strength meter stay live.
        let _subscriptions = [&name, &email, &password]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            name,
            email,
            password,
            terms: false,
            show_errors: false,
            submitted: None,
            _subscriptions,
        }
    }
```

In `render`, invalid fields get the theme's red outline colour and a message underneath.
`PasswordStrength` comes from `guiraffe-inputs`:

```rust
impl Render for SignupForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let values = self.values(cx);
        let errors = if self.show_errors {
            validate(&values)
        } else {
            Errors::default()
        };
        // The theme's red ring colour for invalid fields (the same token the DataUi
        // renderer uses), so it has the right contrast in light and dark schemes.
        let invalid: Hsla = GuiraffeTheme::global(cx)
            .state(StateKind::FocusRing, Variant::Outline, Some(ColorName::Red))
            .border
            .to_hsla();
        let border = |error: Option<&str>| error.map(|_| invalid);
        let (name, email, password) = (&self.name, &self.email, &self.password);
        let terms = self.terms;
        let submitted = self.submitted.clone();

        let form = ui! {
            div {
                w: px(380.)
                flex flex_col gap_3
                Title("Create an account") { order: 2 }

                Text("Name") { size: Size::Sm }
                Input(name) { .when_some(border(errors.name), |i, c| i.border_color(c)) }
                FieldError(errors.name)

                Text("Email") { size: Size::Sm }
                Input(email) { .when_some(border(errors.email), |i, c| i.border_color(c)) }
                FieldError(errors.email)

                Text("Password") { size: Size::Sm }
                Input(password) { .when_some(border(errors.password), |i, c| i.border_color(c)) }
                PasswordStrength(&values.password)
                FieldError(errors.password)

                Checkbox("terms") {
                    label: "I accept the terms"
                    checked: terms
                    on_click: cx.listener(|this, checked: &bool, _, cx| {
                        this.terms = *checked;
                        cx.notify();
                    })
                }
                FieldError(errors.terms)

                ScopedButton(
                    Button::new("submit")
                        .label("Sign up")
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
                )
                if let Some(message) = submitted { Text(message) { color: ColorName::Teal } }
            }
        };

        ui! {
            div {
                size_full flex items_center justify_center
                // Inside a focus scope, GUIraffe's opt-in controls such as ScopedButton
                // become Tab stops. (Kit inputs and checkboxes are Tab stops anyway.)
                Card { {focus_scope(form)} }
            }
        }
```

- Errors appear after the first submit and then update as you type, because each input
  change notifies the form.
- The border colour comes from `GuiraffeTheme::state(..)`, so it keeps its contrast in
  both schemes. `FieldError` is a small `#[component]` that renders nothing for a valid
  field.
- `focus_scope(..)` sets `keyboard_nav` in the inherited style context. GUIraffe's opt-in
  controls, like `ScopedButton` here, become Tab stops inside it and never take focus on a
  pointer click outside one.

The unit tests at the bottom of the file run with `cargo test -p getting-started-examples`.

## 4. A data table

[04_datagrid.rs](../examples/getting-started/examples/04_datagrid.rs)

`guiraffe-datagrid` virtualises rows *and* columns, so it builds only the cells you can
see, whether you have 5,000 rows or a million. Data comes from a `GridSource`. `VecSource`
adapts a `Vec` of your own rows: each `.column(..)` returns a borrowed `CellRef`, and
nothing is copied.

```rust
/// Column definitions point at source columns by index (the order of `.column` calls).
fn grid(cx: &mut App) -> GridHandle {
    let source = VecSource::new(orders())
        .column(|o: &Order| CellRef::Int(o.id))
        .column(|o: &Order| CellRef::Text(&o.customer))
        .column(|o: &Order| CellRef::Text(o.city))
        .column(|o: &Order| CellRef::Text(o.status))
        .column(|o: &Order| CellRef::Number(o.amount))
        .column(|o: &Order| CellRef::Bool(o.paid))
        .key(|o: &Order| o.id as u64);
    let columns = vec![
        ColumnDef::new("id", "ID", 0).width(80.),
        ColumnDef::new("customer", "Customer", 1).width(180.),
        ColumnDef::new("city", "City", 2),
        ColumnDef::new("status", "Status", 3).width(110.),
        ColumnDef::new("amount", "Amount", 4)
            .align(Align::End)
            .formatter(|cell, out| {
                if let CellRef::Number(n) = cell {
                    // Formatters append to a buffer the grid passes in.
                    let _ = write!(out, "${n:.2}");
                }
            }),
        ColumnDef::new("paid", "Paid", 5)
            .width(80.)
            .formatter(|cell, out| {
                out.push_str(if matches!(cell, CellRef::Bool(true)) {
                    "yes"
                } else {
                    "no"
                })
            }),
    ];
    let handle = GridHandle::new(source, columns, cx);
    handle.set_options(
        GridOptions {
            focusable: Some(true),
            label: "Orders".into(),
            ..Default::default()
        },
        cx,
    );
    handle
}
```

A `ColumnDef` is presentation: title, width, alignment, a formatter, and whether it's
sortable (the default) or resizable. Its third argument is the source column it shows.

`GridHandle` is the Control. Clicking a header sorts by that column, and Shift+click adds
another sort key. Filters are predicates on a column, set with `set_filter`. Sorting and
filtering produce a row permutation and never copy the data. Above 50,000 rows
(`GridOptions::sync_below`) they run on the background executor.

```rust
impl Orders {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let grid = grid(cx);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by customer"));
        let status = SegmentedControl::new(["all"].into_iter().chain(STATUSES), cx);
        let _subscriptions = vec![
            // Typing in the search box filters the Customer column (definition 1).
            cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let needle = input.read(cx).value().trim().to_lowercase();
                    let filter: Option<Filter> = (!needle.is_empty()).then(|| {
                        Arc::new(move |cell: CellRef<'_>| {
                            matches!(cell, CellRef::Text(s) if s.to_lowercase().contains(&needle))
                        }) as Filter
                    });
                    this.grid.set_filter(1, filter, cx);
                    cx.notify();
                }
            }),
            // The segmented control filters the Status column (definition 3).
            cx.subscribe(&status, |this, _, event: &SegmentedEvent, cx| {
                let SegmentedEvent::Changed(index) = *event;
                let filter: Option<Filter> = index
                    .checked_sub(1)
                    .and_then(|i| STATUSES.get(i).copied())
                    .map(|wanted| {
                        Arc::new(move |cell: CellRef<'_>| cell == CellRef::Text(wanted)) as Filter
                    });
                this.grid.set_filter(3, filter, cx);
                cx.notify();
            }),
            // Clicking a header sorts (Shift+click adds a key); the grid tells us.
            cx.subscribe(grid.events(), |this, _, event: &GridEvent, cx| {
                if let GridEvent::SortChanged(keys) = event {
                    let columns = this.grid.columns();
                    this.sort = keys
                        .iter()
                        .filter_map(|k| {
                            let title = &columns.get(k.column)?.title;
                            let arrow = if k.dir == SortDir::Asc { "↑" } else { "↓" };
                            Some(format!("{title} {arrow}"))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                        .into();
                    cx.notify();
                }
            }),
        ];
        Self {
            grid,
            search,
            status,
            sort: SharedString::default(),
            _subscriptions,
        }
    }
}
```

Render the grid with `DataGrid::new(&handle)` inside a container with a definite height:

```rust
impl Render for Orders {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let shown = self.grid.view_row_count();
        let sort = if self.sort.is_empty() {
            "click a header to sort".into()
        } else {
            format!("sorted by {}", self.sort)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(Size::Lg.spacing()))
            .bg(s.body.to_hsla())
            .child(Title::new("Orders").order(2))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(div().w(px(240.)).child(Input::new(&self.search)))
                    .child(self.status.clone()),
            )
            .child(Text::new(format!("{shown} of {ROWS} orders · {sort}")).dimmed(true))
            // The grid fills the remaining space and builds only the visible rows.
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(DataGrid::new(&self.grid)),
            )
    }
}
```

`GridEvent` also reports selection changes, edits, copies, column resizes and reorders. The
showcase's data grid demo adds pinned columns, editing and a million computed rows
(`FnSource`).

## 5. Theming and dark mode

[05_theme.rs](../examples/getting-started/examples/05_theme.rs)

Styling has two layers.

**The theme** is an app-wide `GuiraffeTheme` global: scheme (light or dark), primary
colour, shades, default radius, font scale, contrast and layout recipes. `apply(theme, cx)`
installs it, updates GPUI Controls' theme to match and refreshes every window.
`set_scheme(..)` changes just the scheme:

```rust
Switch::new("dark")
    .label("Dark mode")
    .checked(dark)
    .on_click(|checked: &bool, _, cx| {
        set_scheme(
            if *checked {
                Scheme::Dark
            } else {
                Scheme::Light
            },
            cx,
        )
    }),
```

```rust
cx.subscribe(&primary, |_, _, event: &SegmentedEvent, cx| {
    let SegmentedEvent::Changed(index) = *event;
    if let Some((_, color)) = PRIMARIES.get(index) {
        let theme = GuiraffeTheme {
            primary: *color,
            ..GuiraffeTheme::global(cx).clone()
        };
        apply(theme, cx);
    }
}),
```

Start from your own theme in `main`:

```rust
apply(
    GuiraffeTheme {
        primary: ColorName::Blue,
        default_radius: Size::Md,
        ..Default::default()
    },
    cx,
);
```

**The style context** is inherited down the tree. Containers set density, control size,
surface and emphasis, and widgets read them, so widgets don't carry their own spacing
([spec/14-style-context.md](../spec/14-style-context.md)). `provide(StylePatch, child)`
patches the context for a subtree:

```rust
// Everything inside `provide` sees the patched context: Card padding follows the
// density, segmented controls and `SaveButton` follow the control size.
let preview = provide(
    self.patch,
    Card::new().child(Preview {
        sample: self.sample.clone(),
    }),
);
```

Your own components read the context with `StyleContext::current()` while they render.
Here's a Kit button that follows the inherited control size:

```rust
/// A widget of your own that follows the inherited context instead of taking size props:
/// it reads `StyleContext::current()` while it renders and sizes a Kit button to match.
#[derive(IntoElement)]
struct SaveButton;

impl RenderOnce for SaveButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let ctx = StyleContext::current();
        Button::new("save")
            .label(format!("Save ({:?})", ctx.control_size))
            .primary()
            .with_size(kit_size(ctx.control_size))
    }
}

/// The preview body. It reads the context in `render`, which runs while the enclosing
/// `provide` lays out its children. Reading `StyleContext::current()` in the parent's
/// `render` instead would see the parent's context, not the patched one.
#[derive(IntoElement)]
struct Preview {
    sample: Entity<SegmentedControl>,
}

impl RenderOnce for Preview {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let ctx = StyleContext::current();
        div()
            .flex()
            .flex_col()
            .gap(px(ctx.density.space(Size::Sm)))
            .child(Title::new("Preview").order(4))
            .child(Text::new(format!("Density {:?}", ctx.density)).dimmed(true))
            .child(self.sample)
            .child(div().flex().child(SaveButton))
    }
}
```

Read the context inside a component's `render`. Code that runs eagerly in the parent's
`render` executes before the provider lays out its children, so it sees the parent's
context. The theme's `recipes` field holds the patches for each kind of container (`page`,
`toolbar`, `form`, `card`, `sidebar`, `table`). Edit the recipes to restyle an app globally
instead of changing call sites.

## 6. Pages, a router and a store

[06_router_store.rs](../examples/getting-started/examples/06_router_store.rs)

**Routes** are a typed enum. `routes!` generates parsing and formatting, so
`Page::Todo { id: 7 }` round-trips through `/todos/7` for deep links and saved history:

```rust
routes! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Page {
        Todos = "/",
        Todo { id: u64 } = "/todos/:id",
        Settings = "/settings",
    }
}
```

**The store** holds app state and processes typed events in order. Handlers are pure: they
read the state through `Fx` and describe what should happen, a new state, more events, or
effects as data that the store runs. That makes them testable without a window.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    id: u64,
    title: SharedString,
    done: bool,
}

/// The whole app state. It must be `Clone + PartialEq`; the `Arc` keeps cloning for undo
/// cheap and lets unchanged lists compare by pointer first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppState {
    todos: Arc<Vec<Todo>>,
    next_id: u64,
    status: Option<SharedString>,
}

#[derive(Debug)]
struct AddTodo(SharedString);
#[derive(Debug)]
struct ToggleTodo(u64);
#[derive(Debug)]
struct RemoveTodo(u64);
#[derive(Debug)]
struct ClearStatus;

/// An effect is plain data; the store runs the handler registered for its type.
#[derive(Debug)]
struct Log(String);

impl Event<AppState> for AddTodo {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        let title = self.0.trim().to_string();
        if title.is_empty() {
            return; // no state change: no re-render, no undo entry
        }
        fx.update(|s| {
            s.next_id += 1;
            let id = s.next_id;
            Arc::make_mut(&mut s.todos).push(Todo {
                id,
                title: title.into(),
                done: false,
            });
        });
    }
}

impl Event<AppState> for ToggleTodo {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        fx.update(|s| {
            if let Some(todo) = Arc::make_mut(&mut s.todos)
                .iter_mut()
                .find(|t| t.id == self.0)
            {
                todo.done = !todo.done;
            }
        });
    }
}

impl Event<AppState> for RemoveTodo {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        let Some(title) = fx
            .state()
            .todos
            .iter()
            .find(|t| t.id == self.0)
            .map(|t| t.title.clone())
        else {
            return;
        };
        fx.update(|s| {
            Arc::make_mut(&mut s.todos).retain(|t| t.id != self.0);
            s.status = Some(format!("Removed “{title}”. Undo brings it back.").into());
        });
        fx.effect(Log(format!("removed todo {}", self.0)));
        // A timer, described as data: the store dispatches ClearStatus in 4 s.
        fx.dispatch_debounced("status", Duration::from_secs(4), ClearStatus);
    }
}

impl Event<AppState> for ClearStatus {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        fx.update(|s| s.status = None);
    }
    fn undoable(&self) -> bool {
        false
    }
}
```

Views **select** what they need. A selection notifies its view only when the selected value
changes. `select_memo` also skips the computation when its input is unchanged:

```rust
/// The shell: navigation plus the current page. It only re-renders when the route or a
/// value it selected from the store changes.
struct Shell {
    router: Router<Page>,
    page: Selected<Page>,
    store: Store<AppState>,
    todos: Selected<Arc<Vec<Todo>>>,
    status: Selected<Option<SharedString>>,
    open_count: Selected<usize>,
    new_todo: Entity<InputState>,
    #[cfg(feature = "devtools")]
    devtools: guiraffe_devtools::Devtools,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    fn new(store: Store<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let router = Router::new(Page::Todos, cx);
        let page = router.select(cx, |p| p.clone());
        let todos = store.select(cx, |s| s.todos.clone());
        let status = store.select(cx, |s| s.status.clone());
        // A memoised selection: the count is recomputed only when the list changes.
        let open_count = store.select_memo(
            cx,
            |s| s.todos.clone(),
            |todos| todos.iter().filter(|t| !t.done).count(),
        );
        let new_todo = cx.new(|cx| InputState::new(window, cx).placeholder("What needs doing?"));
        let _subscriptions = vec![cx.subscribe_in(
            &new_todo,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let title = input.read(cx).value();
                    this.store.dispatch(AddTodo(title), cx);
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                }
            },
        )];
        Self {
            #[cfg(feature = "devtools")]
            devtools: devtools(&store, cx),
            router,
            page,
            store,
            todos,
            status,
            open_count,
            new_todo,
            _subscriptions,
        }
    }
```

Dispatch events and navigate from handlers:

```rust
fn todos_page(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
    let rows = self.todos.get(cx).iter().map(|todo| {
        let (id, store, router) = (todo.id, self.store.clone(), self.router.clone());
        let toggle = self.store.clone();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(
                Checkbox::new(("done", id))
                    .label(todo.title.clone())
                    .checked(todo.done)
                    .on_click(move |_, _, cx| toggle.dispatch(ToggleTodo(id), cx)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(Button::new(("open", id)).label("Open").ghost().on_click(
                        move |_, _, cx| {
                            router.navigate(Page::Todo { id }, cx);
                        },
                    ))
                    .child(
                        Button::new(("remove", id))
                            .label("Remove")
                            .danger()
                            .on_click(move |_, _, cx| store.dispatch(RemoveTodo(id), cx)),
                    ),
            )
    });
    let undo = self.store.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(Title::new("Todos").order(2))
        .child(Text::new(format!("{} open", self.open_count.get(cx))).dimmed(true))
        .child(Input::new(&self.new_todo))
        .child(Card::new().child(div().flex().flex_col().gap(px(8.)).children(rows)))
        .child(
            div().flex().child(
                Button::new("undo")
                    .label("Undo")
                    .disabled(!self.store.can_undo(cx))
                    .on_click(move |_, _, cx| undo.dispatch(UndoEvent, cx)),
            ),
        )
        .into_any_element()
}
```

Render the current page by matching on the route:

```rust
impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let page = self.page.get(cx).clone();
        let content = match page {
            Page::Todos => self.todos_page(cx),
            Page::Todo { id } => self.todo_page(id, cx),
            Page::Settings => self.settings_page(cx),
        };
```

And wire it up in `main`. `enable_undo` makes undoable events (the default) restorable with
`UndoEvent`, and `on_effect` registers the handler for an effect type:

```rust
let store = Store::new(seed(), cx);
store.enable_undo(50, cx);
store.on_effect(cx, |Log(line): Log, _, _| eprintln!("[todos] {line}"));
```

Handler tests use `Engine`, the store's GPUI-free core:

```rust
/// Handlers are pure: test them with an `Engine`, no window or GPUI needed.
#[test]
fn add_toggle_remove_and_undo() {
    let mut engine = Engine::new(AppState::default());
    engine.enable_undo(10);
    engine.dispatch(AddTodo("  write docs ".into()));
    engine.dispatch(AddTodo("   ".into()));
    engine.process(&Coeffects::default());
    assert_eq!(engine.state().todos.len(), 1);
    assert_eq!(engine.state().todos[0].title, "write docs");

    engine.dispatch(ToggleTodo(1));
    engine.process(&Coeffects::default());
    assert!(engine.state().todos[0].done);

    engine.dispatch(RemoveTodo(1));
    let outcome = engine.process(&Coeffects::default());
    assert!(engine.state().todos.is_empty());
    assert!(engine.state().status.is_some());
    // The log line and the delayed ClearStatus come back as effects (data).
    assert_eq!(outcome.effects.len(), 2);

    assert!(engine.undo());
    assert_eq!(engine.state().todos.len(), 1);
}
```

The router is itself built on a store: `router.store()` exposes it for interceptors and
devtools. `router.add_guard(..)` can block navigation, for example to ask about unsaved
changes. For multiple windows, see `guiraffe-windows` and the scaffold from `guiraffe new`.

## The two GPUI backends

GUIraffe runs on two GPUI implementations, chosen with a cargo feature and no code changes:

- **Upstream GPUI** (`gpui-pre`, Zed's GPUI) is the default for `gpui-kit`.
- **GPUI Fast** (`gpui-kit/gpui-fast`) records what each view read and skips views whose
  inputs didn't change. When a view calls `cx.notify()`, upstream GPUI also re-renders its
  ancestors and their uncached children. GPUI Fast doesn't. Apps and the `guiraffe new`
  template use GPUI Fast.

In this repository, crates expose a `gpui-fast` feature that forwards to
`gpui-kit/gpui-fast`:

```sh
cargo run -p getting-started-examples --example 04_datagrid                       # upstream GPUI
cargo run -p getting-started-examples --example 04_datagrid --features gpui-fast  # GPUI Fast
```

Library code should stay backend-neutral and be tested on both. To give your own app the
same switch, leave `gpui-fast` out of `gpui-kit`'s features and add
`[features] gpui-fast = ["gpui-kit/gpui-fast"]`, like this crate's
[Cargo.toml](../examples/getting-started/Cargo.toml). The performance rules in
[spec/00-api-conventions.md](../spec/00-api-conventions.md) §6 get the most out of both
backends: small views for fast-changing state, notify only when output changes, and
virtualise long lists.

On Linux, GPUI uses Wayland when `WAYLAND_DISPLAY` is set and X11 otherwise. To try X11
under a Wayland session, run with `env -u WAYLAND_DISPLAY`.

## Debugging

**The showcase** shows every GUIraffe component, live, with knobs, in both themes. It's the
quickest way to see what a component can do and how its demo is written
(`crates/showcase/src/demos/`). Pass an entry name to open it directly:

```sh
cargo run -p guiraffe-showcase --features gpui-fast
cargo run -p guiraffe-showcase --features gpui-fast -- "Data grid"
```

**Devtools** (`guiraffe-devtools`) add render counters, an inspector that shows each
tracked view's bounds and inherited style context, a frame-time profiler, and a store event
log with time travel. Example 6 wires them up behind a feature:

```sh
cargo run -p getting-started-examples --example 06_router_store --features gpui-fast,devtools
```

Press Ctrl-Shift-I to open the panel. The setup is in the example's `devtools` function.
Call `guiraffe_devtools::init(cx)`, create a `Devtools` per window, connect stores with
`inspect_store(..)` and `add_store(..)`, and in the view's `render` return
`tools.layer(tools.track("view-id", element))`:

```rust
// With `--features devtools`, Ctrl-Shift-I opens the inspector over the app.
#[cfg(feature = "devtools")]
return self.devtools.layer(self.devtools.track("shell", shell));
#[cfg(not(feature = "devtools"))]
shell.into_any_element()
```

Keep devtools an optional dependency. Without its `devtools` feature the crate compiles to
nothing. [crates/devtools/README.md](../crates/devtools/README.md) explains what each panel
measures.

More tips:

- **A view doesn't update?** Something changed without `cx.notify()`, or a subscription was
  dropped. Keep `Subscription`s in a field, like `_subscriptions` in the examples.
- **A view updates too often?** Turn on the devtools render counters. Move fast-changing
  state into a smaller child view, and select narrower values from the store.
- **Test without a window.** Keep logic in plain functions (example 3) and store handlers
  (example 6). `guiraffe-test` adds headless snapshot rendering in both themes. See
  `pages_in_both_themes` in [examples/ui-macro-demo/src/main.rs](../examples/ui-macro-demo/src/main.rs).
- **A build or link error after updating?** Check the pins with `guiraffe doctor`, and make
  sure your app has the `[patch.crates-io]` block and the checkout's `Cargo.lock`.
- [KNOWN_ISSUES.md](../KNOWN_ISSUES.md) lists known problems and workarounds.

## Where to go next

- **The showcase** (`cargo run -p guiraffe-showcase`) and the **gallery**
  (`cargo run -p guiraffe-gallery`), a reference app with state, routing, motion, flow,
  overlays and windows.
- **[docs/ui-macro.md](ui-macro.md)**: the `ui!` macro and `#[component]` in depth, and the
  team tasks demo (`cargo run -p ui-macro-demo --features gpui-fast`).
- **[GUIraffe for React and Electron developers](from-react-electron.md)**: concept mapping,
  the performance model, and the measured comparison with Electron.
- **Specs**, the design of each part: [00-overview](../spec/00-overview.md),
  [00-api-conventions](../spec/00-api-conventions.md) (how every crate's API is shaped),
  [14-style-context](../spec/14-style-context.md),
  [15-performance-architecture](../spec/15-performance-architecture.md),
  [12-app-framework](../spec/12-app-framework.md),
  [22-table-grid-sheet](../spec/22-table-grid-sheet.md) and
  [24-ui-macro](../spec/24-ui-macro.md).
- **Component crates** in [crates/](../crates):

  | Crate | What it gives you |
  |---|---|
  | `guiraffe-core` | Theme, palettes, sizes, variants, style context |
  | `guiraffe-layout` | Stack, Group, Flex, Grid, SimpleGrid, Container, Center, AppShell, Splitter |
  | `guiraffe-components` | Mantine-style components GPUI Controls lacks: Card, Text, Title, SegmentedControl, Stepper, Timeline, RingProgress, ... |
  | `guiraffe-inputs` | ChipGroup, TagsInput, RangeSlider, PasswordStrength, Dropzone |
  | `guiraffe-overlays` | Modals, drawers, popovers, context menus, toasts, spotlight, desktop notifications |
  | `guiraffe-datagrid` | Virtualised data grid: sort, filter, selection, inline editing |
  | `guiraffe-tree` | Virtualised trees and property grids |
  | `guiraffe-charts`, `guiraffe-flamegraph`, `guiraffe-timeline` | Charts, flame graphs, Gantt and time-based editors |
  | `guiraffe-markdown`, `guiraffe-mermaid`, `guiraffe-code`, `guiraffe-chat` | Markdown, Mermaid diagrams, code views, streaming agent chat |
  | `guiraffe-flow` | Node-graph editor in the style of React Flow |
  | `guiraffe-state`, `guiraffe-router`, `guiraffe-windows`, `guiraffe-commands` | Store, typed routing, multi-window management, commands and keybindings |
  | `guiraffe-motion`, `guiraffe-dnd`, `guiraffe-icons` | Animation, drag and drop, the Lucide icon set |
  | `guiraffe-ui` | `ui!`, `#[component]`, and DataUi, which renders JSON screen documents ([README](../README.md#authoring-dataui-screens)) |
