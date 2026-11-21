//! Spotlight: a keyboard-first command bar (Mantine Spotlight, VS Code's command palette,
//! macOS Spotlight).
//!
//! [`SpotlightState`] is the Control (spec/00 §1): it owns the actions, the query, the
//! selection and recents, emits [`SpotlightEvent`]s and renders the panel. Open it as a
//! window overlay (centered in the upper third over the theme scrim, focus restored on
//! close) with [`SpotlightState::open`], or embed it with [`Spotlight::new`].
//!
//! ```no_run
//! use gpui_kit::{AppContext, Window, App};
//! use gpui_kit::assets::IconName;
//! use guiraffe_overlays::spotlight::{SpotlightAction, SpotlightState};
//!
//! fn open_palette(window: &mut Window, cx: &mut App) {
//!     let spotlight = cx.new(|cx| {
//!         SpotlightState::new(window, cx)
//!             .group("File", [
//!                 SpotlightAction::new("file.new", "New file")
//!                     .icon(IconName::FilePlus)
//!                     .shortcut("Ctrl+N"),
//!             ])
//!             .group("Git", [
//!                 SpotlightAction::new("git.push", "Push")
//!                     .description("Upload local commits"),
//!             ])
//!             .pinned(["git.push"])
//!             .on_activate(|id, _window, _cx| println!("run {id}"))
//!     });
//!     let _ = SpotlightState::open(&spotlight, window, cx);
//! }
//! ```
//!
//! Keyboard (key context `GrSpotlight`, rebindable): ↑/↓ (and Ctrl+P/Ctrl+N) move, PageUp/
//! PageDown jump, Tab/Shift+Tab cycle groups in the overlay (inline they move focus as
//! usual), Enter runs, Escape closes. The search input
//! keeps keyboard focus throughout; pointer hover and clicks on rows never take it.
mod action;
#[cfg(test)]
mod gpui_tests;
mod model;

pub use action::{Shortcut, SpotlightAction, SpotlightLabels};
pub use model::{ActionIndex, FilterFn, Row, RowHeights, SpotlightModel, visible_rows};

use crate::{
    OverlayError, OverlayHost, search,
    stack::{OverlayId, OverlayKind, Policy},
};
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, BoxShadow, ClickEvent, Context, Entity, EventEmitter, FontWeight,
    HighlightStyle, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Render,
    RenderOnce, ScrollHandle, SharedString, StatefulInteractiveElement, StyleRefinement, Styled,
    StyledText, Subscription, Task, Window, div, point, px, svg,
};
use guiraffe_core::{A11yDisabledExt, GuiraffeTheme, Rgb, Size, StateKind, Variant};
use guiraffe_motion::TransitionKind;
use std::rc::Rc;

gpui_kit::actions!(
    guiraffe_spotlight,
    [
        NextCommand,
        PreviousCommand,
        SubmitCommand,
        NextGroup,
        PreviousGroup,
        PageDown,
        PageUp
    ]
);

/// Key context of the panel; the search input inside it is `GrSpotlight > Input`.
pub const KEY_CONTEXT: &str = "GrSpotlight";

pub(crate) fn init(cx: &mut App) {
    for context in ["GrSpotlight", "GrSpotlight > Input"] {
        let context = Some(context);
        cx.bind_keys([
            gpui_kit::KeyBinding::new("down", NextCommand, context),
            gpui_kit::KeyBinding::new("ctrl-n", NextCommand, context),
            gpui_kit::KeyBinding::new("up", PreviousCommand, context),
            gpui_kit::KeyBinding::new("ctrl-p", PreviousCommand, context),
            gpui_kit::KeyBinding::new("pagedown", PageDown, context),
            gpui_kit::KeyBinding::new("pageup", PageUp, context),
            gpui_kit::KeyBinding::new("tab", NextGroup, context),
            gpui_kit::KeyBinding::new("shift-tab", PreviousGroup, context),
            gpui_kit::KeyBinding::new("enter", SubmitCommand, context),
        ]);
    }
}

/// Typed events (spec/00 §3). Subscribe with `cx.subscribe(&spotlight, ..)`.
#[derive(Debug, Clone, PartialEq)]
pub enum SpotlightEvent {
    /// Opened as an overlay.
    Opened,
    /// The overlay closed: Escape, outside press, activation or [`SpotlightState::close`].
    Closed,
    /// The (trimmed) query changed; async sources can fetch and call `set_actions`.
    QueryChanged(SharedString),
    /// The highlighted action changed (keyboard, pointer or a new query).
    SelectionChanged(Option<SharedString>),
    /// An action ran (Enter or click). Emitted before `on_activate` runs.
    Activated(SharedString),
}

type ActivateFn = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// Row heights from the size scale (the virtualised list needs them up front).
fn row_heights() -> RowHeights {
    RowHeights {
        header: Size::Xs.control_height(),
        action: Size::Md.control_height(),
        action_with_description: Size::Lg.control_height() + Size::Xs.radius(),
    }
}

/// State and view of the command bar. Create with `cx.new(|cx| SpotlightState::new(window, cx)...)`.
pub struct SpotlightState {
    model: SpotlightModel,
    input: Entity<InputState>,
    labels: SpotlightLabels,
    on_activate: Option<ActivateFn>,
    overlay: Option<OverlayId>,
    loading: bool,
    width: Option<Pixels>,
    max_list_height: Option<Pixels>,
    transition: TransitionKind,
    scroll: ScrollHandle,
    /// Height of the result viewport at the last render (for scroll-into-view).
    list_height: f32,
    last_selected: Option<SharedString>,
    load: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SpotlightEvent> for SpotlightState {}

impl SpotlightState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let labels = SpotlightLabels::default();
        let placeholder = labels.placeholder.clone();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(&input, window, |this, input, event, _, cx| {
            if let InputEvent::Change = event {
                let query = input.read(cx).value();
                this.apply_query(&query, cx);
            }
        });
        let mut model = SpotlightModel::new(row_heights());
        model.set_section_labels(labels.pinned.clone(), labels.recent.clone());
        Self {
            model,
            input,
            labels,
            on_activate: None,
            overlay: None,
            loading: false,
            width: None,
            max_list_height: None,
            transition: TransitionKind::Pop,
            scroll: ScrollHandle::new(),
            list_height: 0.0,
            last_selected: None,
            load: None,
            _subscriptions: vec![subscription],
        }
    }

    // ------------------------------------------------------------------ builder

    /// Append actions (each keeps its own `group`).
    pub fn actions(mut self, actions: impl IntoIterator<Item = SpotlightAction>) -> Self {
        self.model.extend_actions(actions);
        self.model.select_first();
        self.sync_selection_marker();
        self
    }
    /// Append actions under the section `label`.
    pub fn group(
        self,
        label: impl Into<SharedString>,
        actions: impl IntoIterator<Item = SpotlightAction>,
    ) -> Self {
        let label = label.into();
        self.actions(actions.into_iter().map(|a| a.group(label.clone())))
    }
    /// Action ids listed first under "Pinned" while the query is empty.
    pub fn pinned<S: Into<SharedString>>(mut self, ids: impl IntoIterator<Item = S>) -> Self {
        self.model
            .set_pinned(ids.into_iter().map(Into::into).collect());
        self.model.select_first();
        self.sync_selection_marker();
        self
    }
    /// Initial recents (most recent first), e.g. restored from the app's settings.
    pub fn recents<S: Into<SharedString>>(mut self, ids: impl IntoIterator<Item = S>) -> Self {
        self.model
            .set_recents(ids.into_iter().map(Into::into).collect());
        self.model.select_first();
        self.sync_selection_marker();
        self
    }
    /// How many recents to keep (default 5; 0 disables the Recent section).
    pub fn recent_limit(mut self, limit: usize) -> Self {
        self.model.set_recent_limit(limit);
        self
    }
    /// Cap the listed actions (default: all, virtualised).
    pub fn limit(mut self, limit: usize) -> Self {
        self.model.set_limit(Some(limit));
        self
    }
    /// Replace the fuzzy filter (spec: Mantine `filter`). Matches still highlight where the
    /// query is a subsequence of the title.
    pub fn filter(
        mut self,
        filter: impl Fn(&str, &[SpotlightAction]) -> Vec<usize> + 'static,
    ) -> Self {
        self.model.set_filter(Some(Rc::new(filter)));
        self
    }
    pub fn labels(
        mut self,
        labels: SpotlightLabels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = labels.placeholder.clone();
        self.model
            .set_section_labels(labels.pinned.clone(), labels.recent.clone());
        self.labels = labels;
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
        self
    }
    pub fn placeholder(
        mut self,
        text: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = text.into();
        self.labels.placeholder = text.clone();
        self.input
            .update(cx, |input, cx| input.set_placeholder(text, window, cx));
        self
    }
    /// Runs after an action is chosen (and after the overlay closed and focus returned).
    pub fn on_activate(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_activate = Some(Rc::new(handler));
        self
    }
    /// Panel width (default: a little wider than the `Xs` container).
    pub fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.width = Some(width.into());
        self
    }
    /// Maximum height of the result list before it scrolls.
    pub fn max_list_height(mut self, height: impl Into<Pixels>) -> Self {
        self.max_list_height = Some(height.into());
        self
    }
    /// Open/close transition (default `Pop`; reduced motion makes it instant).
    pub fn transition(mut self, kind: TransitionKind) -> Self {
        self.transition = kind;
        self
    }
    /// Show the loading state (e.g. while an async source fetches).
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    // ------------------------------------------------------------------ runtime API

    /// Replace the actions of a live palette. Keeps the highlighted action when it is
    /// still listed.
    pub fn set_actions(&mut self, actions: Vec<SpotlightAction>, cx: &mut Context<Self>) {
        if self.model.actions() == actions.as_slice() {
            return;
        }
        self.model.set_actions(actions);
        self.after_rows_changed(false, cx);
    }
    pub fn set_pinned(&mut self, ids: Vec<SharedString>, cx: &mut Context<Self>) {
        self.model.set_pinned(ids);
        self.after_rows_changed(false, cx);
    }
    pub fn set_recents(&mut self, ids: Vec<SharedString>, cx: &mut Context<Self>) {
        self.model.set_recents(ids);
        self.after_rows_changed(false, cx);
    }
    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        if self.loading != loading {
            self.loading = loading;
            cx.notify();
        }
    }
    /// Load actions from an async source: shows the loading state, prepares the search
    /// index on a background thread and installs the result. A newer call cancels an
    /// older one.
    pub fn load(
        &mut self,
        source: impl Future<Output = Vec<SpotlightAction>> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.set_loading(true, cx);
        self.load = Some(cx.spawn(async move |this, cx| {
            let actions = source.await;
            let prepared = cx
                .background_spawn(async move {
                    let index = ActionIndex::new(&actions);
                    (actions, index)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let (actions, index) = prepared;
                this.model.install(actions, index);
                this.loading = false;
                this.load = None;
                this.after_rows_changed(false, cx);
                cx.notify();
            });
        }));
    }
    /// Replace the query (updates the search input too).
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let value = SharedString::from(query.to_owned());
        self.input
            .update(cx, |input, cx| input.set_value(value, window, cx));
        self.apply_query(query, cx);
    }
    pub fn query(&self) -> &str {
        self.model.query()
    }
    /// Recently run action ids, most recent first (persist them to restore with
    /// [`SpotlightState::recents`]).
    pub fn recent_ids(&self) -> &[SharedString] {
        self.model.recents()
    }
    pub fn selected_action(&self) -> Option<&SpotlightAction> {
        self.model.selected_action()
    }
    pub fn model(&self) -> &SpotlightModel {
        &self.model
    }
    pub fn is_open(&self) -> bool {
        self.overlay.is_some()
    }
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Open as a window overlay: clears the query, focuses the input and restores the
    /// previous focus on close. Opening an already open palette returns its id.
    pub fn open(
        this: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<OverlayId, OverlayError> {
        let host = OverlayHost::entity(window, cx)?;
        if let Some(id) = this.read(cx).overlay
            && host.read(cx).stack().entries().iter().any(|e| e.id == id)
        {
            return Ok(id);
        }
        let view = this.clone();
        let weak = this.downgrade();
        let (transition, label) = {
            let state = this.read(cx);
            (state.transition, state.labels.name.clone())
        };
        let id = OverlayHost::open_layer(
            window,
            cx,
            Policy::default(),
            OverlayKind::Spotlight,
            None,
            transition,
            move |modal, window, cx| {
                let width = view.read(cx).panel_width();
                let viewport = window.viewport_size();
                let weak = weak.clone();
                modal
                    .close_button(false)
                    .width(width)
                    .offset_top(viewport.height * 0.12)
                    .accessibility_label(label.clone())
                    .p_0()
                    .gap_0()
                    .border_0()
                    .bg(gpui_kit::transparent_black())
                    .rounded(px(Size::Md.radius()))
                    .on_close(move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if this.overlay.take().is_some() {
                                cx.emit(SpotlightEvent::Closed);
                            }
                        });
                    })
                    .child(view.clone())
            },
        )?;
        this.update(cx, |this, cx| {
            this.overlay = Some(id);
            this.input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            this.apply_query("", cx);
            if this.model.select_first() {
                this.after_selection_changed(cx);
            }
            this.scroll.set_offset(point(px(0.), px(0.)));
            cx.emit(SpotlightEvent::Opened);
            cx.notify();
        });
        Ok(id)
    }
    /// Close the overlay (focus returns to where it was before opening).
    pub fn close(this: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let overlay = this.update(cx, |this, _| this.overlay.take());
        if let Some(overlay) = overlay {
            let _ = OverlayHost::close(window, cx, overlay);
            this.update(cx, |_, cx| cx.emit(SpotlightEvent::Closed));
        }
    }
    pub fn toggle(
        this: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), OverlayError> {
        if this.read(cx).is_open() {
            Self::close(this, window, cx);
            Ok(())
        } else {
            Self::open(this, window, cx).map(|_| ())
        }
    }

    // ------------------------------------------------------------------ internals

    fn panel_width(&self) -> Pixels {
        self.width
            .unwrap_or(px(Size::Xs.container_width() + 2. * Size::Xl.spacing()))
    }

    fn apply_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.model.set_query(query) {
            cx.emit(SpotlightEvent::QueryChanged(SharedString::from(
                self.model.query().to_owned(),
            )));
            self.after_rows_changed(true, cx);
        }
    }

    fn after_rows_changed(&mut self, to_top: bool, cx: &mut Context<Self>) {
        if to_top {
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
        self.after_selection_changed(cx);
        cx.notify();
    }

    fn sync_selection_marker(&mut self) {
        self.last_selected = self.model.selected_action().map(|a| a.id.clone());
    }

    fn after_selection_changed(&mut self, cx: &mut Context<Self>) {
        let selected = self.model.selected_action().map(|a| a.id.clone());
        if selected != self.last_selected {
            self.last_selected = selected.clone();
            cx.emit(SpotlightEvent::SelectionChanged(selected));
        }
        self.reveal_selected();
        cx.notify();
    }

    /// Scroll the virtualised list so the selected row is fully visible.
    fn reveal_selected(&mut self) {
        let Some(row) = self.model.selected_row() else {
            return;
        };
        let height = self.list_height;
        if height <= 0.0 {
            return;
        }
        // A section header directly above the first action of a group comes into view too.
        let top = if row > 0 && matches!(self.model.rows().get(row - 1), Some(Row::Header(_))) {
            self.model.row_top(row - 1)
        } else {
            self.model.row_top(row)
        };
        let bottom = self.model.row_bottom(row);
        let current = -f32::from(self.scroll.offset().y);
        let next = if top < current {
            top
        } else if bottom > current + height {
            bottom - height
        } else {
            return;
        };
        self.scroll.set_offset(point(px(0.), px(-next.max(0.))));
    }

    fn navigate(&mut self, moved: bool, cx: &mut Context<Self>) {
        if moved {
            self.after_selection_changed(cx);
        }
    }

    /// Apply the input's current text. Input `Change` events are delivered when effects
    /// flush; a key pressed right after typing (in the same update) must see the new query.
    fn sync_query(&mut self, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value();
        if value.trim() != self.model.query() {
            self.apply_query(&value, cx);
        }
    }

    fn activate_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_query(cx);
        let Some(id) = self
            .model
            .selected_action()
            .filter(|a| !a.disabled)
            .map(|a| a.id.clone())
        else {
            return;
        };
        if let Some(overlay) = self.overlay.take() {
            if OverlayHost::close(window, cx, overlay).is_err() {
                return;
            }
            cx.emit(SpotlightEvent::Closed);
        }
        self.model.record_recent(&id);
        self.sync_selection_marker();
        cx.emit(SpotlightEvent::Activated(id.clone()));
        if let Some(callback) = self.on_activate.clone() {
            window.defer(cx, move |window, cx| callback(&id, window, cx));
        }
        cx.notify();
    }

    fn activate_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.select_row(row) {
            self.after_selection_changed(cx);
        }
        if self.model.selected_row() == Some(row) {
            self.activate_selected(window, cx);
        }
    }
}

// ---------------------------------------------------------------------- rendering

/// Theme colors of the panel, resolved once per render.
struct Palette {
    body: Rgb,
    surface: Rgb,
    border: Rgb,
    text: Rgb,
    dimmed: Rgb,
    hover: Rgb,
    selected: Rgb,
    accent: Rgb,
    highlight: Rgb,
    disabled: Rgb,
}

impl Palette {
    fn new(theme: &GuiraffeTheme) -> Self {
        let s = theme.surfaces();
        let light = theme.variant(None, Variant::Light);
        Self {
            body: s.body,
            surface: s.surface,
            border: s.border,
            text: s.text,
            dimmed: s.dimmed,
            hover: s.hover,
            selected: light.background.over(s.body),
            accent: theme.primary_color(),
            highlight: light.foreground,
            disabled: theme
                .state(StateKind::Disabled, Variant::Subtle, None)
                .foreground,
        }
    }
}

fn shadows(theme: &GuiraffeTheme) -> Vec<BoxShadow> {
    theme
        .shadow(Size::Xl)
        .iter()
        .map(|l| BoxShadow {
            color: Rgb::BLACK.alpha(l.alpha).to_hsla(),
            offset: point(px(0.), px(l.y)),
            blur_radius: px(l.blur),
            spread_radius: px(l.spread),
            inset: false,
        })
        .collect()
}

/// A keycap chip (`Ctrl`, `K`, `↵`).
fn key_chip(label: SharedString, p: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(Size::Xs.font_size() + Size::Xs.spacing() * 0.8))
        .min_w(px(Size::Xs.font_size() + Size::Xs.spacing() * 0.8))
        .px(px(Size::Xs.radius() * 2.))
        .rounded(px(Size::Sm.radius()))
        .border_1()
        .border_color(p.border.to_hsla())
        .bg(p.surface.to_hsla())
        .text_color(p.dimmed.to_hsla())
        .text_size(px(Size::Xs.font_size()))
        .line_height(px(Size::Xs.font_size()))
        .child(label)
}

fn shortcut_chips(shortcut: &Shortcut, p: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(Size::Xs.spacing() * 0.6))
        .children(shortcut.chords().iter().map(|keys| {
            div()
                .flex()
                .items_center()
                .gap(px(Size::Xs.radius()))
                .children(keys.iter().map(|key| key_chip(key.clone(), p)))
        }))
}

fn icon(name: IconName, size: f32, color: Rgb) -> gpui_kit::Svg {
    svg()
        .path(name.path())
        .flex_none()
        .size(px(size))
        .text_color(color.to_hsla())
}

/// A centered state block: icon, title and one line of help.
fn state_block(
    id: &'static str,
    glyph: IconName,
    title: SharedString,
    description: Option<SharedString>,
    p: &Palette,
) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(Size::Xs.spacing() * 0.6))
        .py(px(Size::Xl.spacing()))
        .px(px(Size::Md.spacing()))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(Size::Xl.control_height() * 0.8))
                .rounded_full()
                .bg(p.surface.to_hsla())
                .border_1()
                .border_color(p.border.to_hsla())
                .child(icon(glyph, Size::Xl.icon_size(), p.dimmed)),
        )
        .child(
            div()
                .mt(px(Size::Xs.spacing() * 0.4))
                .text_size(px(Size::Sm.font_size()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text.to_hsla())
                .child(title),
        )
        .children(description.map(|d| {
            div()
                .text_size(px(Size::Xs.font_size()))
                .text_color(p.dimmed.to_hsla())
                .text_center()
                .child(d)
        }))
        .into_any_element()
}

impl SpotlightState {
    fn render_row(
        &self,
        row: usize,
        p: &Palette,
        any_icon: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let top = self.model.row_top(row);
        let height = self.model.row_bottom(row) - top;
        let inset = Size::Xs.spacing() * 0.6;
        let frame = div()
            .absolute()
            .left(px(inset))
            .right(px(inset))
            .top(px(top))
            .h(px(height));
        match *self.model.rows().get(row)? {
            Row::Header(section) => {
                let label = self.model.section_label(section)?.clone();
                Some(
                    frame
                        .flex()
                        .items_end()
                        .pb(px(Size::Xs.radius() * 2.))
                        .px(px(Size::Xs.spacing()))
                        .text_size(px(Size::Xs.font_size()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.dimmed.to_hsla())
                        .child(
                            div()
                                .id(("spotlight-group", section as usize))
                                .test_support()
                                .role(gpui_kit::Role::Heading)
                                .aria_label(label.clone())
                                .child(label),
                        )
                        .into_any_element(),
                )
            }
            Row::Action(index) => {
                let action = self.model.action(index)?;
                let selected = self.model.selected_row() == Some(row);
                let disabled = action.disabled;
                let title_color = if disabled { p.disabled } else { p.text };
                let query = self.model.query();
                let title: AnyElement = match (!query.is_empty() && !disabled)
                    .then(|| search::match_ranges(query, &action.title))
                    .flatten()
                {
                    Some(ranges) if !ranges.is_empty() => {
                        // Color, weight and an underline: the match is not shown by color alone.
                        let style = HighlightStyle {
                            color: Some(p.highlight.to_hsla()),
                            font_weight: Some(FontWeight::BOLD),
                            underline: Some(gpui_kit::UnderlineStyle {
                                thickness: px(1.),
                                color: Some(p.highlight.to_hsla()),
                                wavy: false,
                            }),
                            ..Default::default()
                        };
                        StyledText::new(action.title.clone())
                            .with_highlights(ranges.into_iter().map(|r| (r, style)))
                            .into_any_element()
                    }
                    _ => action.title.clone().into_any_element(),
                };
                let glyph_color = if selected { p.accent } else { p.dimmed };
                let mut option = div()
                    .id(("spotlight-row", row))
                    .a11y_disabled(disabled)
                    .test_support()
                    .role(gpui_kit::Role::ListBoxOption)
                    .aria_label(action.title.clone())
                    .aria_selected(selected)
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .gap(px(Size::Sm.spacing()))
                    .px(px(Size::Xs.spacing()))
                    .rounded(px(Size::Sm.radius()))
                    .text_color(title_color.to_hsla())
                    // Like Kit buttons: a pointer press never moves focus off the input.
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default());
                if selected {
                    option = option.bg(p.selected.to_hsla()).child(
                        // Selection is not conveyed by color alone: an accent bar marks it.
                        div()
                            .absolute()
                            .left_0()
                            .top(px(height * 0.25))
                            .h(px(height * 0.5))
                            .w(px(Size::Xs.radius() * 1.5))
                            .rounded(px(Size::Xs.radius()))
                            .bg(p.accent.to_hsla()),
                    );
                }
                if disabled {
                    option = option.cursor_not_allowed();
                } else {
                    let hover = p.hover.to_hsla();
                    option = option
                        .cursor_pointer()
                        .when(!selected, |o| o.hover(move |s| s.bg(hover)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.activate_row(row, window, cx);
                        }));
                }
                let option = option
                    .when(any_icon, |o| {
                        o.child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .size(px(Size::Xs.control_height() - Size::Xs.radius()))
                                .rounded(px(Size::Sm.radius()))
                                .bg(p.surface.to_hsla())
                                .border_1()
                                .border_color(p.border.to_hsla())
                                .children(
                                    action
                                        .icon
                                        .map(|g| icon(g, Size::Md.icon_size(), glyph_color)),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(Size::Sm.font_size()))
                                    .child(title),
                            )
                            .children(action.description.clone().map(|d| {
                                div()
                                    .truncate()
                                    .text_size(px(Size::Xs.font_size()))
                                    .text_color(
                                        if disabled { p.disabled } else { p.dimmed }.to_hsla(),
                                    )
                                    .child(d)
                            })),
                    )
                    .children(action.shortcut.as_ref().map(|s| shortcut_chips(s, p)));
                Some(frame.child(option).into_any_element())
            }
        }
    }

    fn render_results(
        &mut self,
        p: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let max = self
            .max_list_height
            .map(f32::from)
            .unwrap_or(Size::Lg.control_height() * 8.)
            .min(f32::from(window.viewport_size().height) * 0.6)
            .max(Size::Lg.control_height());
        let total = self.model.total_height();
        let height = total.min(max);
        self.list_height = height;
        let top = (-f32::from(self.scroll.offset().y)).clamp(0., (total - height).max(0.));
        // Overdraw one viewport above and below so wheel scrolling shows rows before the
        // re-render that follows it.
        let range = self.model.visible_rows(top - height, height * 3.);
        let any_icon = self.model.any_icon();
        let rows: Vec<AnyElement> = range
            .filter_map(|row| self.render_row(row, p, any_icon, cx))
            .collect();
        div()
            .id("spotlight-results")
            .test_support()
            .role(gpui_kit::Role::ListBox)
            .aria_label(self.labels.name.clone())
            .h(px(height))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .child(div().relative().w_full().h(px(total)).children(rows))
            .into_any_element()
    }

    fn render_footer(&self, p: &Palette) -> AnyElement {
        let hint = |keys: &[&'static str], label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(Size::Xs.radius() * 2.))
                .children(keys.iter().map(|k| key_chip(SharedString::from(*k), p)))
                .child(label)
        };
        let count = self.model.result_count();
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(Size::Md.spacing()))
            .px(px(Size::Md.spacing()))
            .py(px(Size::Xs.spacing() * 0.8))
            .border_t_1()
            .border_color(p.border.to_hsla())
            .bg(p.surface.to_hsla())
            .text_size(px(Size::Xs.font_size()))
            .text_color(p.dimmed.to_hsla())
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .items_center()
                    .gap(px(Size::Md.spacing()))
                    .child(hint(&["↑", "↓"], "Navigate"))
                    .child(hint(&["↵"], "Run"))
                    .child(hint(&["Tab"], "Next group"))
                    .child(hint(&["Esc"], "Close")),
            )
            .child(
                div()
                    .id("spotlight-count")
                    .test_support()
                    .flex_none()
                    .child(SharedString::from(if count == 1 {
                        "1 result".to_owned()
                    } else {
                        format!("{count} results")
                    })),
            )
            .into_any_element()
    }
}

impl Render for SpotlightState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let p = Palette::new(theme);
        let shadow = shadows(theme);
        let radius = px(Size::Md.radius());
        let body = if self.loading && self.model.rows().is_empty() {
            state_block(
                "spotlight-loading",
                IconName::LoaderCircle,
                self.labels.loading.clone(),
                None,
                &p,
            )
        } else if self.model.actions().is_empty() {
            state_block(
                "spotlight-empty",
                IconName::Inbox,
                self.labels.empty_title.clone(),
                Some(self.labels.empty_description.clone()),
                &p,
            )
        } else if self.model.rows().is_empty() {
            let title = SharedString::from(format!(
                "{} for “{}”",
                self.labels.no_results_title,
                self.model.query()
            ));
            state_block(
                "spotlight-no-results",
                IconName::SearchX,
                title,
                Some(self.labels.no_results_description.clone()),
                &p,
            )
        } else {
            div()
                .py(px(Size::Xs.spacing() * 0.6))
                .child(self.render_results(&p, window, cx))
                .into_any_element()
        };
        div()
            .id("guiraffe-spotlight")
            .test_support()
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &NextCommand, _, cx| {
                this.sync_query(cx);
                let moved = this.model.move_by(1);
                this.navigate(moved, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousCommand, _, cx| {
                this.sync_query(cx);
                let moved = this.model.move_by(-1);
                this.navigate(moved, cx);
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                this.sync_query(cx);
                let moved = this.model.move_clamped(8);
                this.navigate(moved, cx);
            }))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                this.sync_query(cx);
                let moved = this.model.move_clamped(-8);
                this.navigate(moved, cx);
            }))
            .on_action(cx.listener(|this, _: &NextGroup, window, cx| {
                // Inline, Tab keeps its usual meaning so the page has no keyboard trap;
                // inside the overlay (a focus-trapped dialog) it cycles groups.
                if this.overlay.is_none() {
                    window.focus_next(cx);
                    return;
                }
                this.sync_query(cx);
                let moved = this.model.cycle_section(true);
                this.navigate(moved, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousGroup, window, cx| {
                // Inline, Tab keeps its usual meaning so the page has no keyboard trap;
                // inside the overlay (a focus-trapped dialog) it cycles groups.
                if this.overlay.is_none() {
                    window.focus_prev(cx);
                    return;
                }
                this.sync_query(cx);
                let moved = this.model.cycle_section(false);
                this.navigate(moved, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SubmitCommand, window, cx| {
                    this.activate_selected(window, cx)
                }),
            )
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(p.body.to_hsla())
            .text_color(p.text.to_hsla())
            .border_1()
            .border_color(p.border.to_hsla())
            .rounded(radius)
            .shadow(shadow)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(Size::Xs.spacing()))
                    .px(px(Size::Md.spacing()))
                    .h(px(Size::Xl.control_height()))
                    .border_b_1()
                    .border_color(p.border.to_hsla())
                    .child(icon(IconName::Search, Size::Lg.icon_size(), p.dimmed))
                    .child(
                        div().flex_1().text_size(px(Size::Lg.font_size())).child(
                            Input::new(&self.input)
                                .appearance(false)
                                .with_size(guiraffe_core::kit_size(Size::Lg))
                                .aria_label(self.labels.placeholder.clone()),
                        ),
                    )
                    .when(self.overlay.is_some(), |header| {
                        header.child(key_chip("Esc".into(), &p))
                    }),
            )
            .child(body)
            // Keyboard hints only help when there is something to move through.
            .when(!self.model.rows().is_empty(), |panel| {
                panel.child(self.render_footer(&p))
            })
    }
}

/// Embeds a [`SpotlightState`] inline (the overlay uses the same panel). Implements
/// [`Styled`] so callers can size it.
#[derive(IntoElement)]
pub struct Spotlight {
    state: Entity<SpotlightState>,
    style: StyleRefinement,
}

impl Spotlight {
    pub fn new(state: &Entity<SpotlightState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Spotlight {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Spotlight {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let width = self.state.read(cx).panel_width();
        div()
            .w(width)
            .max_w_full()
            .refine_style(&self.style)
            .child(self.state)
    }
}
