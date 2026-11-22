# Examples

These commands run from a checkout. To run the examples without one, see the
[install commands](../README.md#how-to-use-it).

| Directory | What it is | Run |
|---|---|---|
| [getting-started](getting-started) | Quickstart and the six tutorial apps | `cargo run -p getting-started-examples --example 00_quickstart --features gpui-fast` |
| [gallery](gallery) | Integration reference app: eight pages using the shell, router, store, overlays, windows, flow and chat together | `cargo run --release -p guiraffe-gallery --features gpui-fast` |
| [team-tasks](team-tasks) | A small app written entirely with `ui!` | `cargo run -p team-tasks --features gpui-fast` |
| [desktop](desktop) | Notifications, notification windows and the system tray | `cargo run -p desktop --features gpui-fast` |

Every component, live: `cargo run --release -p guiraffe-showcase --features gpui-fast`.
The app used for the Electron comparison is in [bench/electron](../bench/electron).
