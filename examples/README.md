# Examples

| Directory | What it is | Run |
|---|---|---|
| [getting-started](getting-started) | Quickstart and the six tutorial apps | `cargo run -p getting-started-examples --example 00_quickstart --features gpui-fast` |
| [team-tasks](team-tasks) | A small app written entirely with `ui!` | `cargo run -p team-tasks --features gpui-fast` |
| [desktop](desktop) | Notifications, notification windows and the system tray | `cargo run -p desktop --features gpui-fast` |

Every component, live: `cargo run --release -p guiraffe-showcase --features gpui-fast`.
The app used for the Electron comparison is in [bench/electron](../bench/electron).
