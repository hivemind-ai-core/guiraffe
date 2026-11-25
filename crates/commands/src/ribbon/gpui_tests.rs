use super::*;
use crate::{
    Choice, Command, CommandId, CommandScope, Commands, InvocationSource, Keybinding, Param,
    Payload, When, init,
};
use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, Keystroke, ParentElement, PlatformInput, Render, Styled, TestAppContext, Window,
    WindowBounds, WindowHandle, WindowOptions, base::Root, div, point, px, size,
    test::TestWindowExt,
};
use guiraffe_core::protocol::{
    self, ActionBus, ActionError, Capabilities, ControlSnapshot, Drive, Interceptor, Observe,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

type Log = Rc<RefCell<Vec<String>>>;

struct Host {
    ribbon: Entity<RibbonState>,
    commands: Commands,
    focus: FocusHandle,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ribbon_scope(&self.ribbon)
            .size_full()
            .flex()
            .flex_col()
            .child(
                CommandScope::new(&self.commands).flex_1().child(
                    div()
                        .id("content")
                        .key_context("Editor")
                        .track_focus(&self.focus)
                        .size_full()
                        .child("content"),
                ),
            )
            .children(CommandRibbon::when_open(&self.ribbon, cx))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .child(RibbonButton::new(&self.ribbon)),
            )
    }
}

fn id(value: &str) -> CommandId {
    CommandId::parse(value).expect("id")
}

fn logged(log: &Log) -> impl Fn(&Payload, &mut Window, &mut App) + 'static {
    let log = log.clone();
    move |payload: &Payload, _: &mut Window, _: &mut App| {
        let args: Vec<_> = payload
            .args
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        log.borrow_mut().push(format!(
            "{} {:?} {}",
            payload.id,
            payload.source,
            args.join(",")
        ));
    }
}

fn register(commands: &Commands, log: &Log, cx: &mut App) {
    let commands_list = [
        Command::new(id("file.save"), "Save", logged(log))
            .keybinding(Keybinding::parse("ctrl+s").expect("key")),
        Command::new(id("text.upper"), "Upper case", logged(log)),
        Command::new(id("text.italic"), "Italic", logged(log)),
        Command::new(id("records.delete"), "Delete record", logged(log))
            .enabled(false)
            .disabled_reason("Read-only workspace"),
        Command::new(id("records.archive"), "Archive record", logged(log))
            .param(Param::dynamic("record", "Record", "records").from_context("selected")),
        Command::new(id("records.rename"), "Rename record", logged(log))
            .param(Param::text("name", "New name", 12)),
    ];
    for command in commands_list {
        commands.register(command, cx).expect("register");
    }
}

fn layout() -> RibbonLayout {
    RibbonLayout::new("root")
        .menu(
            RibbonMenu::new("root", "Commands")
                .category(
                    RibbonCategory::new("Global")
                        .entry(RibbonEntry::command("save", "s", id("file.save")))
                        .entry(RibbonEntry::menu("text", "t", "text")),
                )
                .category(
                    RibbonCategory::new("Records")
                        .visible(When::parse("Records").expect("when"))
                        .entry(RibbonEntry::command("archive", "a", id("records.archive")))
                        .entry(RibbonEntry::command("rename", "n", id("records.rename")))
                        .entry(RibbonEntry::command("delete", "x", id("records.delete"))),
                ),
        )
        .menu(
            RibbonMenu::new("text", "Text").category(
                RibbonCategory::new("Text")
                    .entry(RibbonEntry::menu("emphasis", "e", "emphasis"))
                    .entry(RibbonEntry::command("upper", "u", id("text.upper"))),
            ),
        )
        .menu(
            RibbonMenu::new("emphasis", "Emphasis").category(
                RibbonCategory::new("Emphasis")
                    .entry(RibbonEntry::command("italic", "i", id("text.italic")))
                    .entry(RibbonEntry::action(
                        "bold",
                        "b",
                        "Bold",
                        "format/bold",
                        "toggle",
                    )),
            ),
        )
}

struct Opened {
    window: WindowHandle<Root>,
    ribbon: Entity<RibbonState>,
    commands: Commands,
    focus: FocusHandle,
    log: Log,
}

fn open(cx: &mut TestAppContext, host_context: &str) -> Opened {
    cx.update(init);
    let log: Log = Rc::default();
    let host_context = host_context.to_owned();
    let (window, ribbon, commands, focus) = cx.update(|cx| {
        let commands = Commands::new(cx);
        register(&commands, &log, cx);
        let mut contexts = crate::ContextStack::default();
        contexts.push(crate::WhenContext::parse(&host_context).expect("frame"));
        commands.set_contexts(contexts, cx);
        let model = {
            let state = commands.entity().read(cx);
            Rc::new(
                RibbonModel::build(&layout(), state.catalog(), &BTreeMap::new()).expect("model"),
            )
        };
        let slot = Rc::new(RefCell::new(None));
        let out = slot.clone();
        let service = commands.clone();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(900.), px(500.)),
                    ))),
                    ..Default::default()
                },
                move |window, cx| {
                    let ribbon = cx.new(|cx| RibbonState::new(&service, model, window, cx));
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    *out.borrow_mut() = Some((ribbon.clone(), focus.clone()));
                    let host = cx.new(|_| Host {
                        ribbon,
                        commands: service,
                        focus,
                    });
                    guiraffe_overlays::root(host, window, cx)
                },
            )
            .expect("window");
        let (ribbon, focus) = slot.borrow_mut().take().expect("built");
        (window, ribbon, commands, focus)
    });
    Opened {
        window,
        ribbon,
        commands,
        focus,
        log,
    }
}

fn press(cx: &mut TestAppContext, o: &Opened, key: &str) {
    cx.update_window(o.window.into(), |_, window, cx| window.press(key, cx))
        .expect("press");
    cx.run_until_parked();
}
fn held(cx: &mut TestAppContext, o: &Opened, key: &'static str) {
    let keystroke = Keystroke::parse(key).unwrap_or_else(|_| panic!("bad key {key}"));
    cx.update_window(o.window.into(), move |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(
            PlatformInput::KeyDown(KeyDownEvent {
                keystroke,
                is_held: true,
                prefer_character_input: false,
            }),
            cx,
        );
    })
    .expect("held");
    cx.run_until_parked();
}
fn click(cx: &mut TestAppContext, o: &Opened, element: &str) {
    let element = element.to_owned();
    cx.update_window(o.window.into(), move |_, window, cx| {
        window.render_frame(cx);
        window.click(gpui_kit::SharedString::from(element), cx);
    })
    .expect("click");
    cx.run_until_parked();
}
fn visible(cx: &mut TestAppContext, o: &Opened, element: &str) -> bool {
    let element = element.to_owned();
    cx.update_window(o.window.into(), move |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::SharedString::from(element))
            .is_some()
    })
    .expect("find")
}
fn is_open(cx: &mut TestAppContext, o: &Opened) -> bool {
    cx.update(|cx| o.ribbon.read(cx).is_open())
}
fn content_focused(cx: &mut TestAppContext, o: &Opened) -> bool {
    let focus = o.focus.clone();
    cx.update_window(o.window.into(), move |_, window, _| {
        focus.is_focused(window)
    })
    .expect("focus")
}
fn log(o: &Opened) -> Vec<String> {
    o.log.borrow().clone()
}

#[gpui_kit::test]
fn leader_and_button_toggle_and_closing_restores_focus(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    assert!(
        !visible(cx, &o, "ribbon-entry-save"),
        "closed renders no rows"
    );
    press(cx, &o, "ctrl-space");
    assert!(is_open(cx, &o));
    assert!(
        !content_focused(cx, &o),
        "the ribbon takes the keys while open"
    );
    assert!(visible(cx, &o, "ribbon-entry-save"));
    press(cx, &o, "ctrl-space");
    assert!(!is_open(cx, &o));
    assert!(
        content_focused(cx, &o),
        "closing restores the previous focus"
    );
    click(cx, &o, "ribbon-toggle");
    assert!(is_open(cx, &o));
    click(cx, &o, "ribbon-toggle");
    assert!(!is_open(cx, &o));
    assert!(log(&o).is_empty());
}

#[gpui_kit::test]
fn chords_descend_backspace_goes_up_and_escape_goes_to_root_then_closes(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    press(cx, &o, "ctrl-space");
    press(cx, &o, "t");
    press(cx, &o, "e");
    assert_eq!(
        cx.update(|cx| o.ribbon.read(cx).breadcrumbs().join("/")),
        "Commands/Text/Emphasis"
    );
    assert!(visible(cx, &o, "ribbon-entry-italic"));
    press(cx, &o, "backspace");
    assert_eq!(cx.update(|cx| o.ribbon.read(cx).depth()), 1);
    press(cx, &o, "e");
    press(cx, &o, "escape");
    assert_eq!(cx.update(|cx| o.ribbon.read(cx).depth()), 0);
    assert!(is_open(cx, &o));
    press(cx, &o, "escape");
    assert!(!is_open(cx, &o));
    assert!(content_focused(cx, &o));
    // The leader closes from any depth.
    press(cx, &o, "ctrl-space");
    press(cx, &o, "t");
    press(cx, &o, "ctrl-space");
    assert!(!is_open(cx, &o));
}

#[gpui_kit::test]
fn a_leaf_closes_the_bar_and_runs_like_its_shortcut_and_spotlight(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    press(cx, &o, "ctrl-space");
    press(cx, &o, "s");
    assert!(!is_open(cx, &o));
    assert!(content_focused(cx, &o));
    press(cx, &o, "ctrl-s");
    let commands = o.commands.clone();
    cx.update_window(o.window.into(), move |_, window, cx| {
        let contexts = commands.window_contexts(window, cx);
        commands
            .execute(
                &id("file.save"),
                &contexts,
                InvocationSource::Spotlight,
                window,
                cx,
            )
            .expect("spotlight path");
    })
    .expect("run");
    assert_eq!(
        log(&o),
        [
            "file.save Ribbon ",
            "file.save Keyboard ",
            "file.save Spotlight "
        ]
    );
}

#[gpui_kit::test]
fn clicking_and_typing_reach_the_same_entries(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    press(cx, &o, "ctrl-space");
    click(cx, &o, "ribbon-entry-text");
    click(cx, &o, "ribbon-entry-upper");
    press(cx, &o, "ctrl-space");
    press(cx, &o, "t");
    press(cx, &o, "u");
    assert_eq!(log(&o), ["text.upper Ribbon ", "text.upper Ribbon "]);
    // The Parent and Root buttons match Backspace and Escape.
    press(cx, &o, "ctrl-space");
    press(cx, &o, "t");
    press(cx, &o, "e");
    click(cx, &o, "ribbon-parent");
    assert_eq!(cx.update(|cx| o.ribbon.read(cx).depth()), 1);
    press(cx, &o, "e");
    click(cx, &o, "ribbon-root");
    assert_eq!(cx.update(|cx| o.ribbon.read(cx).depth()), 0);
}

#[gpui_kit::test]
fn page_categories_follow_context_and_disabled_entries_explain_and_refuse(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    press(cx, &o, "ctrl-space");
    assert!(
        !visible(cx, &o, "ribbon-entry-delete"),
        "Records is off-page"
    );
    press(cx, &o, "x");
    assert!(is_open(cx, &o));
    cx.update(|cx| {
        let mut contexts = crate::ContextStack::default();
        contexts.push(crate::WhenContext::parse("Records").expect("frame"));
        o.commands.set_contexts(contexts, cx);
    });
    cx.run_until_parked();
    assert!(
        visible(cx, &o, "ribbon-entry-delete"),
        "open bar follows context"
    );
    let disabled = cx
        .update_window(o.window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.find("ribbon-entry-delete").disabled()
        })
        .expect("find");
    assert_eq!(disabled, Some(true));
    press(cx, &o, "x");
    click(cx, &o, "ribbon-entry-delete");
    assert!(is_open(cx, &o));
    assert_eq!(
        cx.update(|cx| o.ribbon.read(cx).message().map(str::to_owned)),
        Some("Read-only workspace".to_owned())
    );
    assert!(log(&o).is_empty());
}

#[gpui_kit::test]
fn prompts_collect_dynamic_targets_and_text_and_valid_selection_fills_itself(
    cx: &mut TestAppContext,
) {
    let o = open(cx, "Records");
    cx.update(|cx| {
        o.ribbon.update(cx, |state, cx| {
            state.set_choices(
                "records",
                vec![Choice::new("r1", "First"), Choice::new("r2", "Second")],
                cx,
            )
        })
    });
    press(cx, &o, "ctrl-space");
    press(cx, &o, "a");
    assert_eq!(
        cx.update(|cx| o.ribbon.read(cx).prompting().map(str::to_owned)),
        Some("record".to_owned())
    );
    assert!(visible(cx, &o, "ribbon-choice-1"));
    press(cx, &o, "2");
    assert!(!is_open(cx, &o));
    assert_eq!(log(&o), ["records.archive Ribbon record=r2"]);
    // A valid selection fills the target; a stale one would prompt instead.
    cx.update(|cx| {
        let mut contexts = crate::ContextStack::default();
        contexts.push(crate::WhenContext::parse("Records selected=r1").expect("frame"));
        o.commands.set_contexts(contexts, cx);
    });
    press(cx, &o, "ctrl-space");
    press(cx, &o, "a");
    assert_eq!(
        log(&o).last().map(String::as_str),
        Some("records.archive Ribbon record=r1")
    );
    // Text prompt: typing goes to the input, Enter submits, Escape leaves the prompt.
    press(cx, &o, "ctrl-space");
    press(cx, &o, "n");
    assert_eq!(
        cx.update(|cx| o.ribbon.read(cx).prompting().map(str::to_owned)),
        Some("name".to_owned())
    );
    press(cx, &o, "escape");
    assert!(is_open(cx, &o));
    assert!(cx.update(|cx| o.ribbon.read(cx).prompting().is_none()));
    press(cx, &o, "n");
    cx.update_window(o.window.into(), |_, window, cx| window.input("Notes", cx))
        .expect("type");
    press(cx, &o, "enter");
    assert!(!is_open(cx, &o));
    assert_eq!(
        log(&o).last().map(String::as_str),
        Some("records.rename Ribbon name=Notes")
    );
    // No entry point can run a command with parameters without valid arguments.
    let commands = o.commands.clone();
    let result = cx
        .update_window(o.window.into(), move |_, window, cx| {
            let contexts = commands.window_contexts(window, cx);
            commands.execute(
                &id("records.rename"),
                &contexts,
                InvocationSource::Spotlight,
                window,
                cx,
            )
        })
        .expect("run");
    assert!(matches!(result, Err(crate::Error::InvalidArguments(_))));
}

#[gpui_kit::test]
fn held_keys_repeat_navigation_but_never_execution(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    press(cx, &o, "ctrl-space");
    held(cx, &o, "s");
    assert!(is_open(cx, &o), "a held leaf key does not run");
    held(cx, &o, "t");
    assert_eq!(
        cx.update(|cx| o.ribbon.read(cx).depth()),
        1,
        "held navigation repeats"
    );
    held(cx, &o, "backspace");
    assert_eq!(cx.update(|cx| o.ribbon.read(cx).depth()), 0);
    press(cx, &o, "escape");
    assert!(!is_open(cx, &o));
    // An auto-repeating leader toggles once.
    held(cx, &o, "ctrl-space");
    held(cx, &o, "ctrl-space");
    held(cx, &o, "ctrl-space");
    assert!(is_open(cx, &o));
    assert!(log(&o).is_empty());
}

#[gpui_kit::test]
fn every_window_has_its_own_ribbon(cx: &mut TestAppContext) {
    let first = open(cx, "Workspace");
    let second = open(cx, "Workspace");
    press(cx, &second, "ctrl-space");
    assert!(is_open(cx, &second));
    assert!(!is_open(cx, &first));
    press(cx, &first, "ctrl-space");
    press(cx, &first, "s");
    assert_eq!(log(&first), ["file.save Ribbon "]);
    assert!(log(&second).is_empty());
    assert!(is_open(cx, &second));
}

struct Bold(bool);
impl Observe for Bold {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            checked: Some(self.0),
            ..Default::default()
        }
    }
}
impl Drive for Bold {
    type Action = ();
    type Output = ();
    fn decode(action: &protocol::Action) -> Result<(), ActionError> {
        (action.name == protocol::names::TOGGLE)
            .then_some(())
            .ok_or(ActionError::Unsupported)
    }
    fn apply(&mut self, _: (), _: &mut Window, cx: &mut Context<Self>) -> Result<(), ActionError> {
        self.0 = !self.0;
        cx.notify();
        Ok(())
    }
}
struct Sources(RefCell<Vec<protocol::Source>>);
impl Interceptor for Sources {
    fn before(
        &self,
        _: protocol::ControlId,
        _: &protocol::Action,
        source: protocol::Source,
        _: &Window,
        _: &App,
    ) -> Result<(), ActionError> {
        self.0.borrow_mut().push(source);
        Ok(())
    }
    fn after(
        &self,
        _: protocol::ControlId,
        _: &protocol::Action,
        _: protocol::Source,
        _: &Result<protocol::Outcome, ActionError>,
        _: &Window,
        _: &App,
    ) {
    }
}

#[gpui_kit::test]
fn action_entries_drive_controls_through_the_bus_as_ribbon(cx: &mut TestAppContext) {
    let o = open(cx, "Workspace");
    let sources = Rc::new(Sources(RefCell::new(Vec::new())));
    let hook = sources.clone();
    let bold = cx
        .update_window(o.window.into(), move |_, window, cx| {
            let bold = cx.new(|_| Bold(false));
            ActionBus::register(
                "format/bold",
                &bold,
                "toggle",
                Capabilities::TOGGLE,
                window,
                cx,
            );
            ActionBus::add_interceptor(hook, window, cx);
            let menus = control_menus(window, cx);
            assert_eq!(menus.len(), 1);
            assert_eq!(menus[0].1.categories[0].entries[0].key, "1");
            bold
        })
        .expect("register");
    press(cx, &o, "ctrl-space");
    press(cx, &o, "t");
    press(cx, &o, "e");
    press(cx, &o, "b");
    assert!(!is_open(cx, &o));
    assert!(cx.update(|cx| bold.read(cx).0));
    assert_eq!(*sources.0.borrow(), [protocol::Source::Ribbon]);
}
