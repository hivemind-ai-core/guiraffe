//! Command palette: Controls Input + Dialog, with independent fuzzy matching.
use crate::{
    OverlayError, OverlayHost,
    search::{Command, search},
    stack::{OverlayId, OverlayKind, Policy},
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use std::rc::Rc;
type Handler = Rc<dyn Fn(&str, &mut Window, &mut App)>;
pub struct Spotlight {
    input: Entity<InputState>,
    commands: Vec<Command>,
    selected: usize,
    on_select: Handler,
    overlay: Option<OverlayId>,
    _subscription: Subscription,
}
impl Spotlight {
    pub fn new(
        commands: Vec<Command>,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_select: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search commands…"));
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::Change => {
                    this.selected = 0;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.activate(window, cx),
                _ => {}
            });
        Self {
            input,
            commands,
            selected: 0,
            on_select: Rc::new(on_select),
            overlay: None,
            _subscription: subscription,
        }
    }
    pub fn open(
        palette: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<OverlayId, OverlayError> {
        let view = palette.clone();
        let id = OverlayHost::open_inner(
            window,
            cx,
            Policy::default(),
            OverlayKind::Spotlight,
            None,
            move |dialog, _, _| dialog.title("Commands").child(view.clone()),
        )?;
        palette.update(cx, |this, cx| {
            this.overlay = Some(id);
            this.selected = 0;
            this.input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            cx.notify();
        });
        Ok(id)
    }
    pub fn set_commands(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        self.commands = commands;
        self.selected = 0;
        cx.notify();
    }
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value();
        let id = search(&query, &self.commands, 50)
            .get(self.selected)
            .map(|command| command.id.clone());
        if let Some(id) = id {
            let Some(overlay) = self.overlay else {
                return;
            };
            if OverlayHost::close(window, cx, overlay).is_err() {
                return;
            }
            self.overlay = None;
            let callback = self.on_select.clone();
            window.defer(cx, move |window, cx| callback(&id, window, cx));
        }
    }
}
impl Render for Spotlight {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.input.read(cx).value();
        let matches = search(&query, &self.commands, 50);
        let count = matches.len();
        let surface = GuiraffeTheme::global(cx).surfaces();
        div()
            .id("guiraffe-spotlight")
            .test_support()
            .flex()
            .flex_col()
            .gap(px(Size::Sm.spacing()))
            .capture_key_down(cx.listener(
                move |this, event: &gpui_kit::KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "down" => {
                            if count > 0 {
                                this.selected = (this.selected + 1) % count;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }
                        "up" => {
                            if count > 0 {
                                this.selected = (this.selected + count - 1) % count;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }
                        "enter" => {
                            this.activate(window, cx);
                            cx.stop_propagation();
                        }
                        _ => {}
                    }
                },
            ))
            .child(Input::new(&self.input))
            .children(matches.iter().enumerate().map(|(index, command)| {
                let button = Button::new(("command", index))
                    .label(command.label.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected = index;
                        this.activate(window, cx);
                    }));
                if index == self.selected {
                    div()
                        .bg(surface.hover.to_hsla())
                        .rounded(px(Size::Sm.radius()))
                        .child(button)
                } else {
                    div().child(button)
                }
            }))
    }
}
