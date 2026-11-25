//! Golden screenshots of the open command ribbon in both schemes.
use gpui_kit::{
    App, AppContext, Context, Entity, HeadlessAppContext, IntoElement, ParentElement, Render,
    Styled, Window, div, px, size,
};
use guiraffe_commands::{
    Command, CommandId, Commands, ContextStack, Keybinding, Payload, When, WhenContext,
    ribbon::{
        CommandRibbon, RibbonButton, RibbonCategory, RibbonEntry, RibbonLayout, RibbonMenu,
        RibbonModel, RibbonScope, RibbonState,
    },
};
use guiraffe_core::{GuiraffeTheme, Scheme, Size, apply};
use guiraffe_test::snapshot::{SnapshotConfig, assert_snapshot};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

struct Scene {
    ribbon: Entity<RibbonState>,
}
impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        RibbonScope::new(&self.ribbon)
            .size_full()
            .flex()
            .flex_col()
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .child(
                div()
                    .flex_1()
                    .p(px(Size::Md.spacing()))
                    .child("document.rs"),
            )
            .child(CommandRibbon::new(&self.ribbon))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .border_t_1()
                    .border_color(surfaces.border.to_hsla())
                    .child(RibbonButton::new(&self.ribbon)),
            )
    }
}

fn noop(_: &Payload, _: &mut Window, _: &mut App) {}
fn id(value: &str) -> CommandId {
    CommandId::parse(value).expect("id")
}

fn layout() -> RibbonLayout {
    let global = RibbonCategory::new("Global")
        .entry(RibbonEntry::menu("file", "f", "file").icon("icons/file.svg"))
        .entry(RibbonEntry::command("find", "/", id("edit.find")).icon("icons/search.svg"))
        .entry(
            RibbonEntry::command("settings", ",", id("view.settings")).icon("icons/settings.svg"),
        );
    let editor = RibbonCategory::new("Editor")
        .entry(RibbonEntry::command("undo", "u", id("edit.undo")).icon("icons/undo.svg"))
        .entry(RibbonEntry::command("redo", "r", id("edit.redo")).icon("icons/redo.svg"))
        .entry(RibbonEntry::command("copy", "c", id("edit.copy")).icon("icons/copy.svg"))
        .entry(RibbonEntry::command("replace", "h", id("edit.replace")).icon("icons/replace.svg"))
        .entry(RibbonEntry::command("lines", "l", id("view.lines")))
        .entry(RibbonEntry::command("wrap", "w", id("view.wrap")))
        .entry(RibbonEntry::command("format", "=", id("editor.format")));
    let selection = RibbonCategory::new("Selection").entry(
        RibbonEntry::command("delete", "x", id("selection.delete")).icon("icons/delete.svg"),
    );
    RibbonLayout::new("root")
        .menu(
            RibbonMenu::new("root", "Commands")
                .category(global)
                .category(editor)
                .category(selection),
        )
        .menu(RibbonMenu::new("file", "File").category(
            RibbonCategory::new("File").entry(RibbonEntry::command("save", "s", id("file.save"))),
        ))
}

#[test]
fn ribbon_in_both_schemes() {
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
        let slot = Rc::new(std::cell::RefCell::new(None));
        let out = slot.clone();
        let window = cx
            .open_window(size(px(1200.), px(360.)), move |_, cx| {
                let commands = Commands::new(cx);
                let mut contexts = ContextStack::default();
                contexts.push(WhenContext::new("Editor").expect("context"));
                commands.set_contexts(contexts, cx);
                for (id_text, title, key, enabled) in [
                    ("edit.find", "Find in files", Some("ctrl+shift+f"), true),
                    ("view.settings", "Settings", Some("ctrl+,"), true),
                    ("edit.undo", "Undo", Some("ctrl+z"), true),
                    ("edit.redo", "Redo", Some("ctrl+shift+z"), false),
                    ("edit.copy", "Copy", Some("ctrl+c"), true),
                    ("edit.replace", "Replace", Some("ctrl+h"), true),
                    ("view.lines", "Line numbers", None, true),
                    ("view.wrap", "Soft wrap", None, true),
                    (
                        "editor.format",
                        "Format document",
                        Some("alt+shift+f"),
                        true,
                    ),
                    ("selection.delete", "Delete selection", None, true),
                    ("file.save", "Save", Some("ctrl+s"), true),
                ] {
                    let mut command = Command::new(id(id_text), title, noop)
                        .when(When::always())
                        .enabled(enabled);
                    if let Some(key) = key {
                        command = command.keybinding(Keybinding::parse(key).expect("key"));
                    }
                    commands.register(command, cx).expect("register");
                }
                let model = {
                    let state = commands.entity().read(cx);
                    Rc::new(
                        RibbonModel::build(&layout(), state.catalog(), &BTreeMap::new())
                            .expect("model"),
                    )
                };
                let ribbon = cx.new(|cx| RibbonState::new(&commands, model, cx));
                *out.borrow_mut() = Some(ribbon.clone());
                cx.new(|_| Scene { ribbon })
            })
            .expect("headless window");
        let ribbon = slot.borrow_mut().take().expect("ribbon");
        cx.update_window(window.into(), |_, window, cx| {
            ribbon.update(cx, |state, cx| state.open(window, cx));
        })
        .expect("open");
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
                "ribbon-light"
            } else {
                "ribbon-dark"
            },
            &image,
            &config,
        )
        .unwrap_or_else(|e| panic!("{e:#}"));
    }
}
