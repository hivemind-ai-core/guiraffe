//! `cargo run -p guiraffe-commands --example commands --features gpui-fast`
use gpui_kit::{
    App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Window, WindowOptions, div, px,
};
use guiraffe_commands::{
    Command, CommandId, CommandScope, Commands, ContextStack, Error, Keybinding, Menu, Payload,
    When, WhenContext,
};
use guiraffe_core::{GuiraffeTheme, Size};
struct Status(String);
impl Render for Status {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.0.clone())
    }
}
struct Example {
    commands: Commands,
    status: Entity<Status>,
    focus: FocusHandle,
}
impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CommandScope::new(&self.commands).size_full().child(
            div()
                .size_full()
                .key_context("Editor")
                .track_focus(&self.focus)
                .p(px(Size::Md.spacing()))
                .flex()
                .flex_col()
                .gap(px(Size::Sm.spacing()))
                .child("Workspace commands")
                .child("Try Ctrl+S, Ctrl+K Ctrl+S, or Ctrl+Shift+P for Spotlight.")
                .child(Menu::new("commands-menu", &self.commands))
                .child(self.status.clone()),
        )
    }
}
fn install(commands: &Commands, status: &Entity<Status>, cx: &mut App) -> Result<(), Error> {
    let mut contexts = ContextStack::default();
    contexts.push(WhenContext::new("Editor")?);
    commands.set_contexts(contexts, cx);
    for (id, title, category, binding, when, enabled) in [
        ("file.save", "Save", "File", "ctrl+s", When::always(), true),
        (
            "editor.format",
            "Format selection",
            "Editor",
            "alt+enter",
            When::parse("Editor && !Readonly")?,
            true,
        ),
        (
            "app.settings",
            "Settings",
            "Application",
            "ctrl+k ctrl+s",
            When::always(),
            true,
        ),
        (
            "file.delete",
            "Delete protected file",
            "File",
            "ctrl+shift+delete",
            When::always(),
            false,
        ),
    ] {
        let status = status.downgrade();
        commands.register(
            Command::new(
                CommandId::parse(id)?,
                title,
                move |payload: &Payload, _: &mut Window, cx: &mut App| {
                    let _ = status.update(cx, |status, cx| {
                        status.0 = format!("Ran {} from {:?}", payload.id, payload.source);
                        cx.notify();
                    });
                },
            )
            .category(category)
            .keybinding(Keybinding::parse(binding)?)
            .when(when)
            .enabled(enabled),
            cx,
        )?;
    }
    Ok(())
}
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            guiraffe_commands::init(cx);
            if let Err(error) = cx.open_window(WindowOptions::default(), |window, cx| {
                let commands = Commands::new(cx);
                let status = cx.new(|_| Status("Choose a command or press its shortcut.".into()));
                if let Err(error) = install(&commands, &status, cx) {
                    status.update(cx, |status, _| status.0 = error.to_string());
                }
                let view = cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    Example {
                        commands,
                        status,
                        focus,
                    }
                });
                let wrapper = cx.new(|_| Frame { view });
                guiraffe_overlays::root(wrapper, window, cx)
            }) {
                eprintln!("Cannot open commands example: {error}");
                cx.quit();
            }
        });
}
struct Frame {
    view: Entity<Example>,
}
impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .child(self.view.clone())
    }
}
