//! Gate: registered controls cost the command system nothing until a shortcut or a
//! Spotlight query touches one. A warm frame with 1,000 registered controls and a live
//! commands service allocates exactly as one without them.
#![allow(clippy::unwrap_used, reason = "test assertions")]

use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext, Window,
    div,
};
use guiraffe_commands::{Command, CommandId, Commands, Keybinding};
use guiraffe_core::protocol::{
    Action, ActionBus, ActionError, Capabilities, ControlSnapshot, Drive, Observe,
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Switch(bool);
impl Observe for Switch {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            checked: Some(self.0),
            ..Default::default()
        }
    }
}
impl Drive for Switch {
    type Action = ();
    type Output = ();
    fn decode(_: &Action) -> Result<(), ActionError> {
        Ok(())
    }
    fn apply(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) -> Result<(), ActionError> {
        self.0 = !self.0;
        Ok(())
    }
}
struct Host(Vec<Entity<Switch>>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().children(
            self.0
                .iter()
                .map(|e| div().child(if e.read(cx).0 { "on" } else { "off" })),
        )
    }
}

fn warm_frame(cx: &mut TestAppContext, registered: bool) -> guiraffe_test::alloc::Counts {
    cx.update(guiraffe_commands::init);
    let (_, window) =
        cx.add_window_view(|_, cx| Host((0..1000).map(|_| cx.new(|_| Switch(false))).collect()));
    window.update(|window, cx| {
        let commands = Commands::new(cx);
        commands
            .register(
                Command::new(
                    CommandId::parse("file.save").unwrap(),
                    "Save",
                    |_: &_, _: &mut Window, _: &mut App| {},
                )
                .keybinding(Keybinding::parse("ctrl+s").unwrap()),
                cx,
            )
            .unwrap();
        let switches: Vec<Entity<Switch>> = if registered {
            (0..1000).map(|_| cx.new(|_| Switch(false))).collect()
        } else {
            Vec::new()
        };
        for (i, switch) in switches.iter().enumerate() {
            ActionBus::register(
                &format!("page/{i}"),
                switch,
                "switch",
                Capabilities::TOGGLE,
                window,
                cx,
            );
        }
        assert_eq!(
            commands.entity().read(cx).catalog().len(),
            1,
            "no control was surfaced"
        );
        for _ in 0..4 {
            window.refresh();
            window.draw(cx).clear(cx);
        }
        let ((), counts) = count_allocations(|| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        drop((commands, switches));
        counts
    })
}

#[gpui_kit::test]
fn registered_controls_add_no_idle_command_work(cx: &mut TestAppContext) {
    let baseline = warm_frame(cx, false);
    let registered = warm_frame(cx, true);
    eprintln!("idle frame: plain {baseline:?}, 1,000 registered controls {registered:?}");
    assert_eq!(registered, baseline);
}
