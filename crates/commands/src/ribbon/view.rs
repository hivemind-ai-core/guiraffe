//! The GPUI command ribbon: a keyboard-first command bar that slides in above the status bar.
//!
//! [`RibbonState`] is the per-window control (navigation, prompt, focus). [`ribbon_scope`] is a
//! window's root (the leader works inside it); [`CommandRibbon`] is the bar itself and
//! [`RibbonButton`] the status-bar toggle. While closed, the bar renders one empty element and
//! does no other work; its rows are projected once per open, navigation or registry change.
use super::model::{KeyOutcome, NavError, Navigator, RibbonKey, RibbonModel, Step};
use crate::{Choices, Commands, ContextStack, InvocationSource, Param, ParamKind, params::Draft};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, RenderOnce, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Subscription, Window,
    component::{
        Disableable, Icon, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        input::{Input, InputEvent, InputState},
    },
    div,
    prelude::*,
    px,
};
use guiraffe_core::{
    GuiraffeTheme, Size, StateKind, StyleContext, Variant,
    protocol::{Action, ActionBus, ControlId, Source},
};
use std::rc::Rc;

/// The default leader: Ctrl+Space on every platform. See [`RibbonState::set_leader`].
pub const RIBBON_LEADER: &str = "ctrl-space";
/// Key context of [`RibbonScope`]: the leader works while focus is inside it.
pub const RIBBON_CONTEXT: &str = "GrRibbon";

/// Ribbon props: data, so they serialize for DataUi and `ui!`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct RibbonProps {
    /// Where the open bar goes. Overlay (the default) draws it over the bottom of the
    /// window, so the content above never moves; slotted takes layout space.
    pub placement: RibbonPlacement,
    /// Rows per column; a longer category spans several columns.
    pub rows_per_column: usize,
    pub show_icons: bool,
    pub show_shortcuts: bool,
    /// Columns that fit before the bar scrolls horizontally (sets the bar's width budget).
    pub max_columns: usize,
    /// Row height in pixels; `None` derives a compact height from the theme's size scale.
    pub row_height: Option<f32>,
}

/// Where the open ribbon is drawn.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum RibbonPlacement {
    /// Over the bottom of the window, in a deferred layer: opening and closing never
    /// re-layout the content.
    #[default]
    Overlay,
    /// As a child of its parent, taking layout space (the content above shrinks).
    Slotted,
}

fn props_row_height(props: &RibbonProps) -> f32 {
    props
        .row_height
        .filter(|h| h.is_finite() && *h > 0.)
        .unwrap_or(Size::Xs.control_height() * 0.8)
}
impl Default for RibbonProps {
    fn default() -> Self {
        Self {
            placement: RibbonPlacement::Overlay,
            rows_per_column: 6,
            show_icons: true,
            show_shortcuts: true,
            max_columns: 8,
            row_height: None,
        }
    }
}

/// What the ribbon did, for hosts that log or record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RibbonEvent {
    Opened,
    Closed,
    /// A command or action ran (or was handed to the action bus) with this outcome.
    Ran {
        step: Step,
        result: Result<(), String>,
    },
    /// An activation was refused, with the reason shown in the bar.
    Refused(String),
}

#[derive(Clone)]
struct Row {
    entry: usize,
    element_id: SharedString,
    key: SharedString,
    label: SharedString,
    icon: Option<SharedString>,
    shortcut: SharedString,
    submenu: bool,
    disabled: Option<SharedString>,
}
struct Column {
    title: SharedString,
    rows: Vec<Row>,
}

struct ChoiceRow {
    element_id: SharedString,
    key: SharedString,
    value: String,
    label: SharedString,
}
enum PromptKind {
    Choice(Vec<ChoiceRow>),
    Text {
        input: Entity<InputState>,
        _enter: Subscription,
    },
}
struct Prompt {
    draft: Draft,
    param: String,
    title: SharedString,
    kind: PromptKind,
}

const CHOICE_KEYS: &str = "123456789abcdefghijklmnopqrstuvwxyz";

/// Per-window ribbon state. Share one [`RibbonModel`] (an `Rc`) between windows.
pub struct RibbonState {
    commands: Commands,
    model: Rc<RibbonModel>,
    nav: Navigator,
    props: RibbonProps,
    choices: Choices,
    open: bool,
    /// The window's native contexts when the ribbon opened.
    origin: ContextStack,
    contexts: ContextStack,
    columns: Rc<Vec<Column>>,
    crumbs: SharedString,
    prompt: Option<Prompt>,
    message: Option<SharedString>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    /// The leader keystroke, matched by this window's keystroke interceptor.
    leader: gpui_kit::Keystroke,
    window: gpui_kit::WindowId,
    _leader: Subscription,
    /// Parsed once: building a key context per frame would allocate.
    bar_context: gpui_kit::KeyContext,
    _registry: Subscription,
}

impl EventEmitter<RibbonEvent> for RibbonState {}
impl Focusable for RibbonState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl RibbonState {
    /// Create the ribbon of one window over `commands` and a compiled `model`.
    ///
    /// The leader is handled by one keystroke interceptor registered here for `window`, not by
    /// per-frame listeners, so a closed ribbon adds no work to any frame.
    pub fn new(
        commands: &Commands,
        model: Rc<RibbonModel>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let window_id = window.window_handle().window_id();
        let weak = cx.weak_entity();
        let leader = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() != window_id {
                return;
            }
            let Some(ribbon) = weak.upgrade() else {
                return;
            };
            let state = ribbon.read(cx);
            let is_leader = event.keystroke.modifiers == state.leader.modifiers
                && event.keystroke.key == state.leader.key;
            if !is_leader {
                return;
            }
            if !event
                .context_stack
                .iter()
                .any(|context| context.contains(RIBBON_CONTEXT))
            {
                return;
            }
            // An auto-repeating (held) leader never toggles again; every new press does, however
            // quickly it follows the last one and wherever its key-up went.
            let held = window.is_dispatching_held_key();
            ribbon.update(cx, |state, cx| state.leader_pressed(held, window, cx));
            cx.stop_propagation();
        });
        // The registry changing (enable, rebind, host contexts) reprojects an open ribbon only.
        let registry = cx.observe(commands.entity(), |this: &mut Self, _, cx| {
            if this.open {
                this.reproject(cx);
            }
        });
        Self {
            commands: commands.clone(),
            model,
            nav: Navigator::default(),
            props: RibbonProps::default(),
            choices: Choices::default(),
            open: false,
            origin: ContextStack::default(),
            contexts: ContextStack::default(),
            columns: Rc::new(Vec::new()),
            crumbs: SharedString::default(),
            prompt: None,
            message: None,
            focus: cx.focus_handle(),
            restore: None,
            leader: default_leader(cx),
            window: window_id,
            _leader: leader,
            bar_context: gpui_kit::KeyContext::parse("GrRibbonBar").unwrap_or_default(),
            _registry: registry,
        }
    }
    pub fn props(mut self, props: RibbonProps) -> Self {
        self.props = props;
        self
    }
    /// Change props in place; notifies only when they differ.
    pub fn set_props(&mut self, props: RibbonProps, cx: &mut Context<Self>) {
        if self.props != props {
            self.props = props;
            cx.notify();
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn depth(&self) -> usize {
        self.nav.depth()
    }
    pub fn breadcrumbs(&self) -> Vec<&str> {
        self.nav.breadcrumbs(&self.model)
    }
    pub fn model(&self) -> &Rc<RibbonModel> {
        &self.model
    }
    /// The parameter currently prompted for, if any.
    pub fn prompting(&self) -> Option<&str> {
        self.prompt.as_ref().map(|p| p.param.as_str())
    }
    /// The last refusal or error shown in the bar.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    /// Replace the layout (after a rebind of chord keys, say). Keeps the ribbon open.
    pub fn set_model(&mut self, model: Rc<RibbonModel>, cx: &mut Context<Self>) {
        self.model = model;
        self.nav.root();
        if self.open {
            self.reproject(cx);
        }
    }
    /// Set a host-provided dynamic choice catalogue (stable ids, unique per catalogue).
    pub fn set_choices(
        &mut self,
        key: impl Into<String>,
        choices: Vec<crate::Choice>,
        cx: &mut Context<Self>,
    ) {
        self.choices.set(key, choices);
        if let Some(prompt) = &mut self.prompt
            && let Some(command) = self.commands.lookup(prompt.draft.command(), cx)
        {
            prompt.draft.refresh(&command, &self.choices);
        }
        if self.open {
            self.reproject(cx);
        }
    }

    fn compute_contexts(&self, cx: &App) -> ContextStack {
        let mut contexts = self.commands.entity().read(cx).contexts().clone();
        contexts.extend(&self.origin);
        contexts
    }

    /// Rebuild the projection: once per open, navigation, prompt or registry change.
    fn reproject(&mut self, cx: &mut Context<Self>) {
        self.contexts = self.compute_contexts(cx);
        self.nav.reconcile(&self.model, &self.contexts);
        let state = self.commands.entity().read(cx);
        let sections = self
            .model
            .sections(&self.nav, state.catalog(), &self.contexts);
        let columns = sections
            .into_iter()
            .map(|section| Column {
                title: section.title.into(),
                rows: section
                    .items
                    .into_iter()
                    .map(|item| Row {
                        entry: item.entry,
                        element_id: format!(
                            "ribbon-entry-{}",
                            self.model.entry_id(item.entry).unwrap_or_default()
                        )
                        .into(),
                        key: item.key.into(),
                        label: item.label.into(),
                        icon: item.icon.map(Into::into),
                        shortcut: item.shortcut.unwrap_or_default().into(),
                        submenu: item.submenu,
                        disabled: item.disabled_reason.map(Into::into),
                    })
                    .collect(),
            })
            .collect();
        self.columns = Rc::new(columns);
        self.crumbs = self.nav.breadcrumbs(&self.model).join(" › ").into();
        if let Some(prompt) = &mut self.prompt
            && let Some(command) = self.commands.lookup(prompt.draft.command(), cx)
        {
            prompt.draft.refresh(&command, &self.choices);
        }
        cx.notify();
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.origin = crate::service::native_contexts(window);
        self.restore = window.focused(cx).filter(|f| f != &self.focus);
        self.nav.root();
        self.prompt = None;
        self.message = None;
        self.open = true;
        self.reproject(cx);
        self.focus.focus(window, cx);
        cx.emit(RibbonEvent::Opened);
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.prompt = None;
        self.message = None;
        self.nav.root();
        self.columns = Rc::new(Vec::new());
        if let Some(previous) = self.restore.take() {
            previous.focus(window, cx);
        }
        cx.emit(RibbonEvent::Closed);
        cx.notify();
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
        } else {
            self.open(window, cx);
        }
    }

    /// Change the leader (for example `"alt-space"`; `secondary-` resolves per OS).
    pub fn set_leader(&mut self, keys: &str, cx: &App) -> Result<(), crate::Error> {
        self.leader = gpui_kit::Keystroke::parse_with_os(keys, cx.platform_info().os)
            .map_err(|e| crate::Error::InvalidBinding(e.to_string()))?;
        Ok(())
    }

    /// The window this ribbon belongs to.
    pub fn window_id(&self) -> gpui_kit::WindowId {
        self.window
    }

    /// The leader: opens, or closes from any depth (mid-chord too), except for auto-repeat
    /// while the leader is held.
    fn leader_pressed(&mut self, held: bool, window: &mut Window, cx: &mut Context<Self>) {
        if held {
            return;
        }
        if self.open {
            let _ = self.nav.key(RibbonKey::Leader, self.prompt.is_some());
            self.close(window, cx);
        } else {
            self.open(window, cx);
        }
    }

    /// Up one level (Backspace / the Parent button). In a prompt, back to the menu.
    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompting = self.prompt.take().is_some();
        let _ = self.nav.key(RibbonKey::Backspace, prompting);
        self.message = None;
        self.refocus(window, cx);
        self.reproject(cx);
    }

    /// Keys come back to the bar after a text prompt held focus.
    fn refocus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open && !self.focus.is_focused(window) {
            self.focus.focus(window, cx);
        }
    }

    /// To the root (the Root button and Escape).
    pub fn root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt = None;
        self.message = None;
        self.nav.root();
        self.refocus(window, cx);
        self.reproject(cx);
    }

    /// Escape discards a partial chord or prompt and returns to the root. It never closes;
    /// at the root it does nothing (close with the leader or the status-bar button).
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav.key(RibbonKey::Escape, self.prompt.is_some()) == KeyOutcome::Root {
            self.root(window, cx);
        }
    }

    /// Activate an entry by index (a click). Same checks and results as its chord.
    pub fn activate(&mut self, entry: usize, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.commands.entity().read(cx);
        let result = self
            .nav
            .activate(entry, &self.model, state.catalog(), &self.contexts);
        self.step(result, window, cx);
    }

    /// Handle one chord character. Returns whether the ribbon used the key.
    pub fn press(
        &mut self,
        key: &str,
        held: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.open {
            return false;
        }
        if self.prompt.is_some() {
            if held {
                return true;
            }
            return self.choose_by_key(key, window, cx);
        }
        if held && self.nav.would_execute(key, &self.model, &self.contexts) {
            // Held keys repeat navigation, never execution.
            return true;
        }
        let state = self.commands.entity().read(cx);
        let result = self
            .nav
            .press(key, &self.model, state.catalog(), &self.contexts);
        self.step(result, window, cx);
        true
    }

    fn step(
        &mut self,
        result: Result<Step, NavError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.message = None;
        match result {
            Ok(Step::Descended) => self.reproject(cx),
            Ok(Step::Command(id)) => {
                let Some(command) = self.commands.lookup(&id, cx) else {
                    return self.refuse(NavError::Unreachable.to_string(), cx);
                };
                let draft = Draft::prepare(&command, &self.contexts, &self.choices);
                match draft.next(&command).cloned() {
                    Some(param) => self.ask(draft, &param, window, cx),
                    None => self.run(draft, window, cx),
                }
            }
            Ok(Step::Action { control, action }) => {
                self.close(window, cx);
                let result = ControlId::lookup(&control, cx)
                    .ok_or(guiraffe_core::protocol::ActionError::UnknownTarget)
                    .and_then(|id| {
                        let action = Action {
                            name: action.clone().into(),
                            payload: Default::default(),
                        };
                        ActionBus::dispatch(id, &action, Source::Ribbon, window, cx)
                    })
                    .map(|_| ())
                    .map_err(|e| format!("{e:?}"));
                cx.emit(RibbonEvent::Ran {
                    step: Step::Action { control, action },
                    result,
                });
            }
            Err(error) => self.refuse(error.to_string(), cx),
        }
    }

    fn refuse(&mut self, reason: String, cx: &mut Context<Self>) {
        self.message = Some(reason.clone().into());
        self.reproject(cx);
        cx.emit(RibbonEvent::Refused(reason));
    }

    fn ask(&mut self, draft: Draft, param: &Param, window: &mut Window, cx: &mut Context<Self>) {
        let title: SharedString = match self.commands.lookup(draft.command(), cx) {
            Some(command) => format!("{} › {}", command.title, param.label).into(),
            None => param.label.clone().into(),
        };
        let kind = match &param.kind {
            ParamKind::Text { .. } => {
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder(param.label.clone()));
                let enter = cx.subscribe_in(
                    &input,
                    window,
                    |this: &mut Self, input, event: &InputEvent, window, cx| {
                        if let InputEvent::PressEnter { .. } = event {
                            let value = input.read(cx).value().to_string();
                            this.submit(&value, window, cx);
                        }
                    },
                );
                input.read(cx).focus_handle(cx).focus(window, cx);
                PromptKind::Text {
                    input,
                    _enter: enter,
                }
            }
            _ => PromptKind::Choice(
                param
                    .choices(&self.choices)
                    .iter()
                    .zip(CHOICE_KEYS.chars())
                    .enumerate()
                    .map(|(index, (choice, key))| ChoiceRow {
                        element_id: format!("ribbon-choice-{index}").into(),
                        key: key.to_string().into(),
                        value: choice.value.clone(),
                        label: choice.label.clone().into(),
                    })
                    .collect(),
            ),
        };
        self.prompt = Some(Prompt {
            draft,
            param: param.id.clone(),
            title,
            kind,
        });
        cx.notify();
    }

    fn choose_by_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(Prompt {
            kind: PromptKind::Choice(rows),
            ..
        }) = &self.prompt
        else {
            return false;
        };
        let Some(value) = rows
            .iter()
            .find(|row| row.key.as_ref() == key)
            .map(|row| row.value.clone())
        else {
            return true;
        };
        self.submit(&value, window, cx);
        true
    }

    /// Fill the prompted parameter; runs the command when the draft is complete.
    pub fn submit(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut prompt) = self.prompt.take() else {
            return;
        };
        let Some(command) = self.commands.lookup(prompt.draft.command(), cx) else {
            return self.refuse(NavError::Unreachable.to_string(), cx);
        };
        if let Err(reason) = prompt
            .draft
            .set(&command, &prompt.param, value, &self.choices)
        {
            self.message = Some(reason.into());
            self.prompt = Some(prompt);
            cx.notify();
            return;
        }
        let draft = prompt.draft;
        match draft.next(&command).cloned() {
            Some(param) => {
                self.focus.focus(window, cx);
                self.ask(draft, &param, window, cx)
            }
            None => self.run(draft, window, cx),
        }
    }

    fn run(&mut self, draft: Draft, window: &mut Window, cx: &mut Context<Self>) {
        // Close first, so focus is back where the command expects it.
        let id = draft.command().clone();
        self.close(window, cx);
        let contexts = self.compute_contexts(cx);
        let result = self
            .commands
            .execute_with(
                &id,
                draft.args().clone(),
                &self.choices,
                &contexts,
                InvocationSource::Ribbon,
                window,
                cx,
            )
            .map_err(|e| e.to_string());
        cx.emit(RibbonEvent::Ran {
            step: Step::Command(id),
            result,
        });
    }

    fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return false;
        }
        match keystroke.key.as_str() {
            "escape" => {
                self.escape(window, cx);
                true
            }
            "backspace" => {
                if matches!(
                    self.prompt.as_ref().map(|p| &p.kind),
                    Some(PromptKind::Text { .. })
                ) {
                    // Backspace edits the text; Escape leaves the prompt.
                    return false;
                }
                self.back(window, cx);
                true
            }
            "tab" | "enter" | "up" | "down" | "left" | "right" => false,
            _ => {
                if matches!(
                    self.prompt.as_ref().map(|p| &p.kind),
                    Some(PromptKind::Text { .. })
                ) {
                    return false;
                }
                let key = keystroke.key_char.as_deref().unwrap_or(&keystroke.key);
                self.press(key, event.is_held, window, cx)
            }
        }
    }
}

thread_local! {
    static SCOPE_CONTEXT: gpui_kit::KeyContext =
        gpui_kit::KeyContext::parse(RIBBON_CONTEXT).unwrap_or_default();
}

/// The root of a window that has a ribbon: a plain `div` carrying [`RIBBON_CONTEXT`], so the
/// leader works while focus is inside it. Add the bar with
/// `.children(CommandRibbon::when_open(&ribbon, cx))` and a [`RibbonButton`] in the status bar.
///
/// No listeners, no wrapper element and a cached key context: with the ribbon closed, the
/// window renders exactly what it would without one (gated in `tests/ribbon_alloc.rs`).
pub fn ribbon_scope(_state: &Entity<RibbonState>) -> gpui_kit::Div {
    div().key_context(SCOPE_CONTEXT.with(Clone::clone))
}

/// The ribbon bar. Use [`CommandRibbon::when_open`] so a closed ribbon adds no element.
#[derive(IntoElement)]
pub struct CommandRibbon {
    state: Entity<RibbonState>,
    style: StyleRefinement,
}
impl CommandRibbon {
    /// The bar while the ribbon is open, otherwise nothing: pass to `.children(..)`.
    pub fn when_open(state: &Entity<RibbonState>, cx: &App) -> Option<Self> {
        state.read(cx).open.then(|| Self::new(state))
    }
    pub fn new(state: &Entity<RibbonState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
        }
    }
}
impl Styled for CommandRibbon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for CommandRibbon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let placement = self.state.read(cx).props.placement;
        let bar = self.bar(cx);
        match placement {
            RibbonPlacement::Slotted => bar.into_any_element(),
            // A zero-height slot where the bar would go; the bar is drawn in a deferred layer
            // growing upward from it. It takes no layout space, so the content above never
            // re-lays out, and whatever sits below the slot (a status bar) stays visible.
            RibbonPlacement::Overlay => {
                let width = window.viewport_size().width;
                div()
                    .w_full()
                    .h_0()
                    .flex_shrink_0()
                    .child(
                        gpui_kit::deferred(
                            gpui_kit::anchored()
                                .position_mode(gpui_kit::AnchoredPositionMode::Local)
                                .anchor(gpui_kit::Anchor::BottomLeft)
                                .position(gpui_kit::point(px(0.), px(0.)))
                                .child(bar.w(width).shadow_lg()),
                        )
                        .with_priority(1),
                    )
                    .into_any_element()
            }
        }
    }
}

impl CommandRibbon {
    fn bar(self, cx: &mut App) -> gpui_kit::Stateful<gpui_kit::Div> {
        let state = self.state.read(cx);
        if !state.open {
            return div().id("command-ribbon-closed");
        }
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let context = StyleContext::current();
        // Compact rows: four fifths of the smallest control height (24 px at the defaults).
        let height = props_row_height(&state.props);
        let gap = context.density.space(Size::Xs);
        let props = &state.props;
        let rows_per_column = props.rows_per_column.max(1);
        let dimmed = theme
            .state(StateKind::Dimmed, Variant::Subtle, None)
            .foreground
            .to_hsla();
        let accent = theme.primary_color().to_hsla();
        let column_width = height * 11.;
        let key_handler = self.state.clone();
        let escape_handler = self.state.clone();
        let mut bar = div()
            .id("command-ribbon")
            .track_focus(&state.focus)
            .key_context(state.bar_context.clone())
            .w_full()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(gap))
            .py(px(gap))
            .border_t_1()
            .border_color(accent)
            .bg(surfaces.surface.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .text_size(px(Size::Sm.font_size()))
            .capture_key_down(move |event: &KeyDownEvent, window, cx| {
                // Escape leaves a text prompt before the input sees it.
                if event.keystroke.key == "escape" && escape_handler.read(cx).prompt.is_some() {
                    escape_handler.update(cx, |state, cx| state.escape(window, cx));
                    cx.stop_propagation();
                }
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if key_handler.update(cx, |state, cx| state.key_down(event, window, cx)) {
                    cx.stop_propagation();
                }
            });
        let row_height = px(height);
        if let Some(prompt) = &state.prompt {
            bar = bar.child(prompt_view(
                prompt,
                &self.state,
                height,
                accent,
                dimmed,
                gap,
                cx,
            ));
        } else {
            let mut categories = div()
                .id("command-ribbon-categories")
                .flex()
                .items_start()
                .overflow_x_scroll()
                .max_w(px(column_width * props.max_columns.max(1) as f32 + gap * 4.))
                .h(px(height * (rows_per_column as f32 + 1.)));
            if state.columns.is_empty() {
                categories = categories.child(
                    div()
                        .px(px(gap * 2.))
                        .text_color(dimmed)
                        .child("No commands here"),
                );
            }
            for (index, column) in state.columns.iter().enumerate() {
                let mut header = div()
                    .h(row_height)
                    .flex()
                    .items_center()
                    .gap(px(gap))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(Size::Xs.font_size()))
                            .text_color(dimmed)
                            .text_ellipsis()
                            .child(column.title.clone()),
                    );
                if index == 0 {
                    let back = self.state.clone();
                    let root = self.state.clone();
                    let top = state.nav.depth() == 0 && state.prompt.is_none();
                    header = header
                        .child(
                            Button::new("ribbon-parent")
                                .ghost()
                                .xsmall()
                                .icon(gpui_kit::assets::IconName::ArrowLeft)
                                .tooltip("Parent menu · Backspace")
                                .accessibility_label("Parent menu")
                                .tab_stop(false)
                                .disabled(top)
                                .on_click(move |_, window, cx| {
                                    back.update(cx, |s, cx| s.back(window, cx))
                                }),
                        )
                        .child(
                            Button::new("ribbon-root")
                                .ghost()
                                .xsmall()
                                .icon(gpui_kit::assets::IconName::ArrowUp)
                                .tooltip("Root menu · Escape")
                                .accessibility_label("Root menu")
                                .tab_stop(false)
                                .disabled(top)
                                .on_click(move |_, window, cx| {
                                    root.update(cx, |s, cx| s.root(window, cx))
                                }),
                        );
                }
                let mut columns = div().flex().items_start().gap(px(gap));
                for chunk in column.rows.chunks(rows_per_column) {
                    let mut list = div().flex().flex_col().w(px(column_width));
                    for row in chunk {
                        list = list.child(row_view(
                            row,
                            &self.state,
                            props,
                            height,
                            accent,
                            dimmed,
                            gap,
                            cx,
                        ));
                    }
                    columns = columns.child(list);
                }
                categories = categories.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .px(px(gap * 2.))
                        .when(index > 0, |s| {
                            s.border_l_1().border_color(surfaces.border.to_hsla())
                        })
                        .child(header)
                        .child(columns),
                );
            }
            bar = bar.child(categories);
        }
        let footer = state
            .message
            .clone()
            .unwrap_or_else(|| state.crumbs.clone());
        bar = bar.child(
            div()
                .id("ribbon-status")
                .px(px(gap * 2.))
                .text_size(px(Size::Xs.font_size()))
                .text_color(dimmed)
                .child(footer),
        );
        bar.style().refine(&self.style);
        bar
    }
}

#[allow(clippy::too_many_arguments)]
fn row_view(
    row: &Row,
    state: &Entity<RibbonState>,
    props: &RibbonProps,
    height: f32,
    accent: gpui_kit::Hsla,
    dimmed: gpui_kit::Hsla,
    gap: f32,
    cx: &App,
) -> impl IntoElement {
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    let hover = theme
        .state(StateKind::Hover, Variant::Subtle, None)
        .background
        .to_hsla();
    let pressed = theme
        .state(StateKind::Pressed, Variant::Subtle, None)
        .background
        .to_hsla();
    let disabled_fg = theme
        .state(StateKind::Disabled, Variant::Subtle, None)
        .foreground
        .to_hsla();
    let enabled = row.disabled.is_none();
    let entry = row.entry;
    let target = state.clone();
    let mut button = Button::new(row.element_id.clone())
        .w_full()
        .h(px(height))
        .custom(
            ButtonCustomVariant::new(cx)
                .color(surfaces.surface.to_hsla())
                .hover(hover)
                .active(pressed)
                .foreground(if enabled {
                    surfaces.text.to_hsla()
                } else {
                    disabled_fg
                }),
        )
        .role(gpui_kit::Role::MenuItem)
        .accessibility_label(row.label.clone())
        .tab_stop(false)
        .disabled(!enabled)
        .on_click(move |_, window, cx| {
            target.update(cx, |state, cx| state.activate(entry, window, cx));
        });
    if let Some(reason) = &row.disabled {
        button = button.tooltip(reason.clone());
    }
    let icon_slot = div().w(px(height * 0.75)).flex_shrink_0().when_some(
        row.icon.clone().filter(|_| props.show_icons),
        |slot, path| slot.child(Icon::empty().path(path).xsmall()),
    );
    button.child(
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(gap))
            .when(props.show_icons, |r| r.child(icon_slot))
            .child(
                div()
                    .w(px(height * 0.75))
                    .flex_shrink_0()
                    .font_weight(gpui_kit::FontWeight::BOLD)
                    .text_color(if enabled { accent } else { disabled_fg })
                    .child(row.key.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(row.label.clone()),
            )
            .when(props.show_shortcuts && !row.shortcut.is_empty(), |r| {
                r.child(
                    div()
                        .text_size(px(Size::Xs.font_size()))
                        .text_color(dimmed)
                        .child(row.shortcut.clone()),
                )
            })
            .child(
                div()
                    .w(px(height * 0.5))
                    .flex_shrink_0()
                    .when(row.submenu, |m| {
                        m.child(
                            Icon::new(gpui_kit::assets::IconName::ChevronRight)
                                .xsmall()
                                .text_color(dimmed),
                        )
                    }),
            ),
    )
}

fn prompt_view(
    prompt: &Prompt,
    state: &Entity<RibbonState>,
    height: f32,
    accent: gpui_kit::Hsla,
    dimmed: gpui_kit::Hsla,
    gap: f32,
    cx: &App,
) -> impl IntoElement {
    let theme = GuiraffeTheme::global(cx);
    let surfaces = theme.surfaces();
    let hover = theme
        .state(StateKind::Hover, Variant::Subtle, None)
        .background
        .to_hsla();
    let body = match &prompt.kind {
        PromptKind::Text { input, .. } => div()
            .w(px(height * 14.))
            .child(Input::new(input).small())
            .into_any_element(),
        PromptKind::Choice(rows) if rows.is_empty() => div()
            .text_color(dimmed)
            .child("Nothing to choose from")
            .into_any_element(),
        PromptKind::Choice(rows) => div()
            .flex()
            .flex_wrap()
            .gap(px(gap))
            .children(rows.iter().map(|row| {
                let value = row.value.clone();
                let target = state.clone();
                Button::new(row.element_id.clone())
                    .with_size(px(height))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(surfaces.surface.to_hsla())
                            .hover(hover)
                            .foreground(surfaces.text.to_hsla()),
                    )
                    .role(gpui_kit::Role::MenuItem)
                    .accessibility_label(row.label.clone())
                    .tab_stop(false)
                    .on_click(move |_, window, cx| {
                        target.update(cx, |state, cx| state.submit(&value, window, cx));
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(gap))
                            .child(
                                div()
                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                    .text_color(accent)
                                    .child(row.key.clone()),
                            )
                            .child(row.label.clone()),
                    )
            }))
            .into_any_element(),
    };
    div()
        .id("ribbon-prompt")
        .px(px(gap * 2.))
        .flex()
        .flex_col()
        .gap(px(gap))
        .child(
            div()
                .text_size(px(Size::Xs.font_size()))
                .text_color(dimmed)
                .child(prompt.title.clone()),
        )
        .child(body)
}

/// The centred status-bar button that toggles the ribbon.
#[derive(IntoElement)]
pub struct RibbonButton {
    state: Entity<RibbonState>,
    label: SharedString,
}
impl RibbonButton {
    pub fn new(state: &Entity<RibbonState>) -> Self {
        Self {
            state: state.clone(),
            label: "Commands".into(),
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
}
impl RenderOnce for RibbonButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = self.state.read(cx).open;
        let state = self.state;
        Button::new("ribbon-toggle")
            .ghost()
            .xsmall()
            .label(self.label)
            .tooltip("Command ribbon · Ctrl+Space")
            .accessibility_label("Command ribbon")
            .tab_stop(false)
            .toggled(open)
            .on_click(move |_, window, cx| state.update(cx, |s, cx| s.toggle(window, cx)))
    }
}

fn default_leader(cx: &App) -> gpui_kit::Keystroke {
    gpui_kit::Keystroke::parse_with_os(RIBBON_LEADER, cx.platform_info().os).unwrap_or_default()
}

impl Render for RibbonState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        CommandRibbon::new(&cx.entity())
    }
}
