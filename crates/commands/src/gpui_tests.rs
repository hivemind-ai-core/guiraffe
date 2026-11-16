use crate::*;
use gpui_kit::{
    App, AppContext, Bounds, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, TestAppContext, Window, WindowBounds, WindowHandle, WindowOptions, base::Root,
    div, point, px, size, test::TestWindowExt,
};
use std::{cell::Cell, rc::Rc};
struct Host {
    commands: Commands,
    focus: FocusHandle,
    context: &'static str,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CommandScope::new(&self.commands).size_full().child(
            div()
                .size_full()
                .key_context(self.context)
                .track_focus(&self.focus)
                .child(Menu::new("command-menu", &self.commands)),
        )
    }
}
fn id(value: &str) -> CommandId {
    CommandId::parse(value).expect("valid id")
}
fn open(cx: &mut TestAppContext, context: &'static str) -> (WindowHandle<Root>, Commands) {
    cx.update(init);
    cx.update(|cx| {
        let commands = Commands::new(cx);
        let service = commands.clone();
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(600.), px(500.)),
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let host = cx.new(|cx| {
                        let focus = cx.focus_handle();
                        focus.focus(window, cx);
                        Host {
                            commands: service,
                            focus,
                            context,
                        }
                    });
                    guiraffe_overlays::root(host, window, cx)
                },
            )
            .expect("window");
        (handle, commands)
    })
}
fn counted(
    id_text: &str,
    title: &str,
    counter: Rc<Cell<usize>>,
) -> Command<impl Fn(&Payload, &mut Window, &mut App)> {
    Command::new(
        id(id_text),
        title,
        move |_: &Payload, _: &mut Window, _: &mut App| counter.set(counter.get() + 1),
    )
}
#[gpui_kit::test]
fn native_shortcut_runs_once_and_chords_wait_for_completion(cx: &mut TestAppContext) {
    let (handle, commands) = open(cx, "Editor");
    let single = Rc::new(Cell::new(0));
    let chord = Rc::new(Cell::new(0));
    cx.update(|cx| {
        commands
            .register(
                counted("save", "Save", single.clone())
                    .keybinding(Keybinding::parse("CTRL+SHIFT+S").expect("binding")),
                cx,
            )
            .expect("register");
        commands
            .register(
                counted("settings", "Settings", chord.clone())
                    .keybinding(Keybinding::parse("ctrl+k ctrl+s").expect("chord")),
                cx,
            )
            .expect("register");
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.press("ctrl-shift-s", cx);
        window.press("ctrl-k", cx);
        assert_eq!(chord.get(), 0);
        window.press("ctrl-s", cx);
    })
    .expect("keys");
    cx.run_until_parked();
    assert_eq!(single.get(), 1);
    assert_eq!(chord.get(), 1);
}
#[gpui_kit::test]
fn native_focus_context_and_disabled_guard_block_execution(cx: &mut TestAppContext) {
    let (handle, commands) = open(cx, "Editor");
    let blocked = Rc::new(Cell::new(0));
    let allowed = Rc::new(Cell::new(0));
    cx.update(|cx| {
        commands
            .register(
                counted("terminal", "Terminal", blocked.clone())
                    .keybinding(Keybinding::parse("ctrl+t").expect("binding"))
                    .when(When::parse("Terminal").expect("when")),
                cx,
            )
            .expect("register");
        commands
            .register(
                counted("disabled", "Disabled", blocked.clone())
                    .keybinding(Keybinding::parse("ctrl+d").expect("binding"))
                    .enabled(false),
                cx,
            )
            .expect("register");
        commands
            .register(
                counted("editor", "Editor", allowed.clone())
                    .keybinding(Keybinding::parse("alt+enter").expect("binding"))
                    .when(When::parse("GrCommand > Editor && !Readonly").expect("when")),
                cx,
            )
            .expect("register");
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.press("ctrl-t", cx);
        window.press("ctrl-d", cx);
        window.press("alt-enter", cx);
    })
    .expect("keys");
    cx.run_until_parked();
    assert_eq!(blocked.get(), 0);
    assert_eq!(allowed.get(), 1);
}
#[gpui_kit::test]
fn menu_click_runs_handler_and_disabled_entry_cannot_run(cx: &mut TestAppContext) {
    let (handle, commands) = open(cx, "Editor");
    let count = Rc::new(Cell::new(0));
    let disabled = Rc::new(Cell::new(0));
    cx.update(|cx| {
        commands
            .register(
                counted("file.save", "Save", count.clone()).category("File"),
                cx,
            )
            .expect("register");
        commands
            .register(
                counted("file.close", "Close", disabled.clone())
                    .category("File")
                    .enabled(false),
                cx,
            )
            .expect("register");
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("file.save", cx);
        window.click("file.close", cx);
    })
    .expect("click");
    cx.run_until_parked();
    assert_eq!(count.get(), 1);
    assert_eq!(disabled.get(), 0);
}
#[gpui_kit::test]
fn noop_and_failed_mutations_do_not_notify_registry(cx: &mut TestAppContext) {
    let (_, commands) = open(cx, "Editor");
    cx.update(|cx| commands.register(counted("save", "Save", Rc::default()), cx))
        .expect("register");
    cx.run_until_parked();
    let notifications = Rc::new(Cell::new(0));
    let out = notifications.clone();
    let _subscription =
        cx.update(|cx| cx.observe(commands.entity(), move |_, _| out.set(out.get() + 1)));
    cx.update(|cx| {
        assert!(!commands.set_enabled(&id("save"), true, cx).expect("noop"));
        assert!(
            !commands
                .set_keybinding(&id("save"), None, cx)
                .expect("noop")
        );
        assert!(!commands.set_contexts(ContextStack::default(), cx));
        assert!(!commands.unregister(&id("missing"), cx));
        let stored = commands.lookup(&id("save"), cx).expect("stored");
        assert!(!commands.replace(stored, cx).expect("noop"));
        assert!(
            commands
                .register(counted("save", "Duplicate", Rc::default()), cx)
                .is_err()
        );
        assert!(commands.set_enabled(&id("missing"), false, cx).is_err());
    });
    cx.run_until_parked();
    assert_eq!(notifications.get(), 0);
    cx.update(|cx| {
        assert!(
            commands
                .set_enabled(&id("save"), false, cx)
                .expect("changed")
        )
    });
    cx.run_until_parked();
    assert_eq!(notifications.get(), 1);
    cx.update(|cx| assert!(!commands.set_enabled(&id("save"), false, cx).expect("noop")));
    cx.run_until_parked();
    assert_eq!(notifications.get(), 1);
}
#[gpui_kit::test]
fn rebinding_unregister_and_conflict_fallback_resolve_current_catalog(cx: &mut TestAppContext) {
    let (handle, commands) = open(cx, "Editor");
    let a = Rc::new(Cell::new(0));
    let b = Rc::new(Cell::new(0));
    cx.update(|cx| {
        for command in [
            counted("a", "Alpha", a.clone()),
            counted("b", "Beta", b.clone()),
        ] {
            commands
                .register(
                    command.keybinding(Keybinding::parse("ctrl+s").expect("binding")),
                    cx,
                )
                .expect("register");
        }
        assert_eq!(
            commands
                .binding_conflicts(&ContextStack::default(), cx)
                .len(),
            1
        );
    });
    cx.update_window(handle.into(), |_, window, cx| window.press("ctrl-s", cx))
        .expect("first");
    assert_eq!(a.get(), 1);
    assert_eq!(b.get(), 0);
    cx.update(|cx| {
        commands
            .set_enabled(&id("a"), false, cx)
            .expect("disable winner");
    });
    cx.update_window(handle.into(), |_, window, cx| window.press("ctrl-s", cx))
        .expect("fallback");
    assert_eq!(b.get(), 1);
    cx.update(|cx| {
        commands
            .set_keybinding(
                &id("b"),
                Some(Keybinding::parse("ctrl+o").expect("binding")),
                cx,
            )
            .expect("rebind");
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.press("ctrl-s", cx);
        window.press("ctrl-o", cx);
    })
    .expect("rebind keys");
    assert_eq!(b.get(), 2);
    cx.update(|cx| assert!(commands.unregister(&id("b"), cx)));
    cx.update_window(handle.into(), |_, window, cx| window.press("ctrl-o", cx))
        .expect("stale route");
    assert_eq!(b.get(), 2);
}
#[gpui_kit::test]
fn handlers_can_mutate_the_registry_reentrantly(cx: &mut TestAppContext) {
    let (handle, commands) = open(cx, "Editor");
    let service = commands.clone();
    let counter = Rc::new(Cell::new(0));
    let output = counter.clone();
    cx.update(|cx| {
        commands.register(
            Command::new(
                id("once"),
                "Once",
                move |payload: &Payload, _: &mut Window, cx: &mut App| {
                    assert_eq!(payload.source, InvocationSource::Keyboard);
                    output.set(output.get() + 1);
                    assert!(service.unregister(&payload.id, cx));
                },
            )
            .keybinding(Keybinding::parse("ctrl+r").expect("binding")),
            cx,
        )
    })
    .expect("register");
    cx.update_window(handle.into(), |_, window, cx| {
        window.press("ctrl-r", cx);
        window.press("ctrl-r", cx);
    })
    .expect("reentrant handler");
    assert_eq!(counter.get(), 1);
}
#[gpui_kit::test]
fn services_are_window_local_and_init_preserves_the_default(cx: &mut TestAppContext) {
    let (a, first) = open(cx, "Editor");
    let (b, second) = open(cx, "Editor");
    let one = Rc::new(Cell::new(0));
    let two = Rc::new(Cell::new(0));
    cx.update(|cx| {
        first
            .register(
                counted("save", "One", one.clone())
                    .keybinding(Keybinding::parse("ctrl+s").expect("binding")),
                cx,
            )
            .expect("first");
        second
            .register(
                counted("save", "Two", two.clone())
                    .keybinding(Keybinding::parse("ctrl+s").expect("binding")),
                cx,
            )
            .expect("second");
        let default = registry(cx).expect("default");
        init(cx);
        assert_eq!(
            default.entity(),
            registry(cx).expect("same default").entity()
        );
    });
    cx.update_window(a.into(), |_, window, cx| window.press("ctrl-s", cx))
        .expect("first key");
    assert_eq!(one.get(), 1);
    assert_eq!(two.get(), 0);
    cx.update_window(b.into(), |_, window, cx| window.press("ctrl-s", cx))
        .expect("second key");
    assert_eq!(two.get(), 1);
}
#[gpui_kit::test]
fn spotlight_snapshot_uses_legacy_type_and_live_execution_guard(cx: &mut TestAppContext) {
    let (handle, commands) = open(cx, "Editor");
    let count = Rc::new(Cell::new(0));
    cx.update(|cx| {
        commands.register(
            counted("save", "Save", count.clone())
                .category("File")
                .keywords("write disk"),
            cx,
        )
    })
    .expect("register");
    cx.update_window(handle.into(), |_, window, cx| {
        window.press("ctrl-shift-p", cx);
        window.input("sav", cx);
        commands
            .set_enabled(&id("save"), false, cx)
            .expect("disable after opening");
        window.press("enter", cx);
    })
    .expect("palette");
    cx.run_until_parked();
    assert_eq!(count.get(), 0);
    cx.update(|cx| {
        let projected: Vec<guiraffe_overlays::search::Command> =
            commands.spotlight_commands(&ContextStack::default(), cx);
        let command = projected.first().expect("entry");
        assert_eq!(command.id, "save");
        assert_eq!(command.label, "Save");
        assert!(command.keywords.contains("disk"));
        assert!(command.disabled);
    });
}
#[gpui_kit::test]
fn pure_predicates_agree_with_native_gpui_for_supported_contexts(cx: &mut TestAppContext) {
    let _ = cx;
    for frames in [
        vec!["Workspace", "Editor mode=vim"],
        vec!["Workspace Readonly", "Editor"],
        vec!["Terminal"],
        vec!["Workspace", "Pane", "Editor"],
    ] {
        let mut pure = ContextStack::default();
        let mut native = Vec::new();
        for frame in frames {
            pure.push(WhenContext::parse(frame).expect("pure frame"));
            native.push(gpui_kit::KeyContext::parse(frame).expect("native frame"));
        }
        for query in [
            "Editor",
            "!Readonly",
            "Editor && !Readonly",
            "Workspace > Editor",
            "Workspace > Pane > Editor",
            "Editor || Terminal",
            "mode == vim",
            "mode != vim",
            "(Editor || Terminal) && !Readonly",
        ] {
            let expected = gpui_kit::KeyBindingContextPredicate::parse(query)
                .expect("native predicate")
                .depth_of(&native)
                .is_some();
            assert_eq!(
                When::parse(query).expect("pure predicate").matches(&pure),
                expected,
                "{query}"
            );
        }
    }
}

struct Consumer {
    commands: Commands,
    renders: guiraffe_test::render::RenderCounter,
}
impl Render for Consumer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.hit();
        Menu::new("probe-menu", &self.commands)
    }
}
struct Leaf(usize);
impl Render for Leaf {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(self.0.to_string())
    }
}
struct Probe {
    consumer: gpui_kit::Entity<Consumer>,
    leaf: gpui_kit::Entity<Leaf>,
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.consumer.clone())
            .child(self.leaf.clone())
    }
}
#[gpui_kit::test]
fn noop_registry_writes_do_not_invalidate_consumers_on_unrelated_frames(cx: &mut TestAppContext) {
    let commands = cx.update(Commands::new);
    cx.update(|cx| commands.register(counted("save", "Save", Rc::default()), cx))
        .expect("register");
    let renders = guiraffe_test::render::RenderCounter::new();
    let counter = renders.clone();
    let (probe, vcx) = cx.add_window_view(|_, cx| Probe {
        consumer: cx.new(|_| Consumer {
            commands: commands.clone(),
            renders: counter,
        }),
        leaf: cx.new(|_| Leaf(0)),
    });
    vcx.run_until_parked();
    let before = renders.get();
    vcx.update(|_, cx| {
        assert!(!commands.set_enabled(&id("save"), true, cx).expect("noop"));
        let leaf = probe.read(cx).leaf.clone();
        leaf.update(cx, |leaf, cx| {
            leaf.0 += 1;
            cx.notify();
        });
    });
    vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            renders.get(),
            before,
            "no write leaked to registry consumers"
        );
    }
    vcx.update(|_, cx| {
        assert!(
            commands
                .set_enabled(&id("save"), false, cx)
                .expect("actual change")
        )
    });
    vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    assert!(renders.get() > before, "a changed command redraws its menu");
}

#[gpui_kit::test]
fn native_predicate_precedence_and_ancestor_matching_exhaustive(cx: &mut TestAppContext) {
    let _ = cx;
    let queries = [
        "Root && A > Editor",
        "Root > Editor && A",
        "Root > A || B",
        "A > !B && Editor",
        "(Root > Pane) > Editor",
        "Root > (A || B) > Editor",
        "!(A || B)",
        "Root > (Editor && !A)",
        "Root > !A || B",
        "mode == vim && Editor",
        "A || B && C",
        "(A || B) && C",
    ];
    for bits in 0u16..512 {
        let mut pure = ContextStack::default();
        let mut native = Vec::new();
        for (i, name) in ["Root", "Pane", "Editor"].into_iter().enumerate() {
            let mut frame = WhenContext::new(name).expect("context");
            let mut text = name.to_owned();
            for (j, flag) in ["A", "B", "C"].into_iter().enumerate() {
                if bits & (1 << (i * 3 + j)) != 0 {
                    frame.insert(flag, None).expect("flag");
                    text.push(' ');
                    text.push_str(flag);
                }
            }
            if i == 2 {
                frame.insert("mode", Some("vim")).expect("mode");
                text.push_str(" mode=vim");
            }
            pure.push(frame);
            native.push(gpui_kit::KeyContext::parse(&text).expect("native frame"));
        }
        for query in queries {
            let expression = When::parse(query).expect("pure query");
            let expected = gpui_kit::KeyBindingContextPredicate::parse(query)
                .expect("native query")
                .depth_of(&native)
                .is_some();
            assert_eq!(expression.matches(&pure), expected, "{query}: {bits}");
            let canonical = gpui_kit::KeyBindingContextPredicate::parse(expression.as_str())
                .expect("canonical native syntax");
            assert_eq!(canonical.depth_of(&native).is_some(), expected);
        }
    }
    for invalid in ["!A == B", "A == B == C", "(A && B) == C"] {
        assert!(When::parse(invalid).is_err());
        assert!(gpui_kit::KeyBindingContextPredicate::parse(invalid).is_err());
    }
}
