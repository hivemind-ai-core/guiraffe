//! Command palette: Controls Input + Dialog, with independent fuzzy matching.
use crate::{
    OverlayError, OverlayHost,
    search::{Command, SearchIndex},
    stack::{OverlayId, OverlayKind, Policy},
};
use gpui_kit::StatefulInteractiveElement;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    button::Button,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use std::rc::Rc;
gpui_kit::actions!(
    guiraffe_spotlight,
    [NextCommand, PreviousCommand, SubmitCommand]
);

pub(crate) fn init(cx: &mut App) {
    for context in ["GrSpotlight", "GrSpotlight > Input"] {
        cx.bind_keys([
            gpui_kit::KeyBinding::new("down", NextCommand, Some(context)),
            gpui_kit::KeyBinding::new("up", PreviousCommand, Some(context)),
            gpui_kit::KeyBinding::new("enter", SubmitCommand, Some(context)),
        ]);
    }
}

type Handler = Rc<dyn Fn(&str, &mut Window, &mut App)>;
pub struct Spotlight {
    input: Entity<InputState>,
    commands: Vec<Command>,
    index: SearchIndex,
    labels: Vec<gpui_kit::SharedString>,
    selected: usize,
    matches: Vec<usize>,
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
                    let matches = this.index.search(&this.input.read(cx).value(), 50).to_vec();
                    if this.selected != 0 || this.matches != matches {
                        this.selected = 0;
                        this.matches = matches;
                        cx.notify();
                    }
                }
                InputEvent::PressEnter { .. } => this.activate(window, cx),
                _ => {}
            });
        let mut index = SearchIndex::new(&commands);
        let matches = index.search("", 50).to_vec();
        let labels = commands.iter().map(|c| c.label.clone().into()).collect();
        Self {
            input,
            matches,
            labels,
            commands,
            index,
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
        let host = OverlayHost::entity(window, cx)?;
        if let Some(id) = palette.read(cx).overlay
            && host.read(cx).stack().entries().iter().any(|e| e.id == id)
        {
            return Ok(id);
        }
        let view = palette.clone();
        let id = OverlayHost::open_inner(
            window,
            cx,
            Policy::default(),
            OverlayKind::Spotlight,
            None,
            move |dialog, _, _| {
                let view = view.clone();
                dialog
                    .title("Commands")
                    .content(move |content, _, _| content.child(view.clone()))
            },
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
        if self.commands == commands {
            return;
        }
        self.labels = commands.iter().map(|c| c.label.clone().into()).collect();
        self.index = SearchIndex::new(&commands);
        self.commands = commands;
        self.matches = self.index.search(&self.input.read(cx).value(), 50).to_vec();
        self.selected = 0;
        cx.notify();
    }
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let matches = self.index.search(&self.input.read(cx).value(), 50).to_vec();
        if self.matches != matches {
            self.selected = 0;
            self.matches = matches;
        }
        let id = self
            .matches
            .get(self.selected)
            .and_then(|index| self.commands.get(*index))
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
        let count = self.matches.len();
        let surface = GuiraffeTheme::global(cx).surfaces();
        div()
            .id("guiraffe-spotlight")
            .test_support()
            .flex()
            .flex_col()
            .gap(px(Size::Sm.spacing()))
            .key_context("GrSpotlight")
            .on_action(cx.listener(move |this, _: &NextCommand, _, cx| {
                if count > 0 {
                    let next = (this.selected + 1) % count;
                    if next != this.selected {
                        this.selected = next;
                        cx.notify();
                    }
                }
            }))
            .on_action(cx.listener(move |this, _: &PreviousCommand, _, cx| {
                if count > 0 {
                    let next = (this.selected + count - 1) % count;
                    if next != this.selected {
                        this.selected = next;
                        cx.notify();
                    }
                }
            }))
            .on_action(
                cx.listener(move |this, _: &SubmitCommand, window, cx| this.activate(window, cx)),
            )
            .child(Input::new(&self.input))
            .child(
                div()
                    .id("spotlight-results")
                    .role(gpui_kit::Role::ListBox)
                    .aria_label("Results")
                    .max_h(px(Size::Xs.container_width() / 2.))
                    .overflow_y_scroll()
                    .children(
                        self.matches
                            .iter()
                            .filter_map(|index| self.labels.get(*index))
                            .enumerate()
                            .map(|(index, label)| {
                                // The input owns the keyboard (↑/↓/Enter); rows are not
                                // tab stops and a click never moves focus off the input.
                                let button = Button::new(("command", index))
                                    .label(label.clone())
                                    .tab_stop(false)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.selected = index;
                                        this.activate(window, cx);
                                    }));
                                let active = index == self.selected;
                                div()
                                    .id(("spotlight-row", index))
                                    .test_support()
                                    .role(gpui_kit::Role::ListBoxOption)
                                    .aria_label(label.clone())
                                    .aria_selected(active)
                                    .rounded(px(Size::Sm.radius()))
                                    .when(active, |row| row.bg(surface.hover.to_hsla()))
                                    .child(button)
                            }),
                    ),
            )
    }
}
