use gpui_kit::{
    App, AppContext, Context, HeadlessAppContext, IntoElement, ParentElement, Render, Styled,
    Window, div, px, size,
};
use guiraffe_commands::{
    Command, CommandId, CommandScope, Commands, ContextStack, Keybinding, Menu, Payload, When,
    WhenContext,
};
use guiraffe_core::{GuiraffeTheme, Scheme, Size, apply};
use guiraffe_test::snapshot::{SnapshotConfig, assert_snapshot};
use std::sync::Arc;
struct Scene {
    commands: Commands,
}
impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        CommandScope::new(&self.commands).size_full().child(
            div()
                .size_full()
                .p(px(Size::Md.spacing()))
                .bg(surfaces.body.to_hsla())
                .text_color(surfaces.text.to_hsla())
                .flex()
                .flex_col()
                .gap(px(Size::Sm.spacing()))
                .child(
                    div()
                        .text_size(px(Size::Lg.font_size()))
                        .child("Workspace commands"),
                )
                .child(Menu::new("menu", &self.commands)),
        )
    }
}
fn noop(_: &Payload, _: &mut Window, _: &mut App) {}
#[test]
fn menu_in_both_schemes() {
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            guiraffe_commands::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let window = cx
            .open_window(
                size(
                    px(Size::Xs.container_width()),
                    px(Size::Xl.control_height() * 6.),
                ),
                |_, cx| {
                    let commands = Commands::new(cx);
                    let mut contexts = ContextStack::default();
                    contexts.push(WhenContext::new("Editor").expect("context"));
                    commands.set_contexts(contexts, cx);
                    for (id, title, category, key, enabled) in [
                        (
                            "editor.format",
                            "Format selection",
                            "Editor",
                            "alt+enter",
                            true,
                        ),
                        ("file.save", "Save file", "File", "ctrl+s", true),
                        (
                            "file.delete",
                            "Delete protected file",
                            "File",
                            "ctrl+shift+delete",
                            false,
                        ),
                        ("view.settings", "Settings", "View", "ctrl+k ctrl+s", true),
                    ] {
                        commands
                            .register(
                                Command::new(CommandId::parse(id).expect("slug"), title, noop)
                                    .category(category)
                                    .keybinding(Keybinding::parse(key).expect("key"))
                                    .when(When::always())
                                    .enabled(enabled),
                                cx,
                            )
                            .expect("register");
                    }
                    cx.new(|_| Scene { commands })
                },
            )
            .expect("headless window");
        // Explicit redraws settle first-scene glyphs/assets independently of
        // retained-rendering assertions in the interaction suite.
        for _ in 0..4 {
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .expect("frame");
            cx.run_until_parked();
        }
        let image = cx.capture_screenshot(window.into()).expect("capture");
        assert_snapshot(
            if scheme == Scheme::Light {
                "menu-light"
            } else {
                "menu-dark"
            },
            &image,
            &config,
        )
        .unwrap_or_else(|e| panic!("{e:#}"));
    }
}
