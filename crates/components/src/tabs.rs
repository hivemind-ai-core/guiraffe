//! Tabs: a GUIraffe-themed tab row keyed by stable host ids, with icons, badges and closable
//! tabs.
//!
//! The model ([`TabsDoc`]) lives in a [`Store`] ([`tabs_store`]). Hosts write it with
//! [`SetTabs`] and [`SetSelected`], which never come back as user events. Users select and
//! close tabs with the protocol's `select` and `close` actions (payload: the tab id), from the
//! pointer, the keyboard or any bus client alike, once the row is registered
//! ([`Tabs::register`]). Each applied action emits one [`TabsEvent`].
//!
//! Keyboard (roving focus): Left/Right move between tabs, Home/End jump, Enter/Space select,
//! Delete or Ctrl+W closes a closable tab.
//!
//! Overflow is not handled here: [`Tabs::set_hidden`] keeps tabs out of the row and
//! [`TabsOptions::trailing`] renders a slot after the visible tabs. A shared overflow row
//! (issue 19) drives both.

use std::{collections::BTreeSet, rc::Rc, sync::Arc};

use gpui_kit::{
    AnyElement, App, AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Render, Role,
    SharedString, StatefulInteractiveElement, Styled, TestSupportExt, WeakEntity, Window,
    assets::IconName, component::Icon, div, prelude::*, px,
};
use guiraffe_core::{
    GuiraffeTheme, StateKind, Variant,
    protocol::{
        Action, ActionBus, ActionError, Capabilities, ControlId, ControlSnapshot, Drive, Observe,
        Outcome, Source, names,
    },
    widget_value::Value,
};
use guiraffe_state::{Event, Fx, Store};

/// A tab's stable key, provided by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

impl From<u64> for TabId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// One tab.
#[derive(Debug, Clone, PartialEq)]
pub struct TabItem {
    pub id: TabId,
    pub label: SharedString,
    pub icon: Option<IconName>,
    /// A short count or note shown as a pill after the label.
    pub badge: Option<SharedString>,
    pub closable: bool,
    pub disabled: bool,
}

impl TabItem {
    pub fn new(id: impl Into<TabId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            badge: None,
            closable: false,
            disabled: false,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn closable(mut self) -> Self {
        self.closable = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// The tab row's model.
#[derive(Debug, Clone, PartialEq)]
pub struct TabsDoc {
    tabs: Arc<Vec<TabItem>>,
    pub selected: Option<TabId>,
}

/// What a [`TabsAction`] changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabsChange {
    Selected {
        id: TabId,
    },
    /// `selected` is the selection after the close.
    Closed {
        id: TabId,
        selected: Option<TabId>,
    },
}

/// A user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabsAction {
    Select(TabId),
    Close(TabId),
}

impl TabsDoc {
    /// Tabs in order; the first enabled one is selected.
    pub fn new(tabs: Vec<TabItem>) -> Self {
        let mut doc = Self {
            tabs: Arc::new(Vec::new()),
            selected: None,
        };
        doc.set_tabs(tabs);
        doc
    }

    pub fn tabs(&self) -> &[TabItem] {
        &self.tabs
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn tab(&self, id: TabId) -> Option<&TabItem> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// Replace the tabs (later duplicates of an id are dropped). The selection is kept while
    /// its tab exists, else the first enabled tab is selected.
    pub fn set_tabs(&mut self, tabs: Vec<TabItem>) -> bool {
        let mut seen = BTreeSet::new();
        let tabs: Vec<TabItem> = tabs.into_iter().filter(|t| seen.insert(t.id)).collect();
        let changed = *self.tabs != tabs;
        if changed {
            self.tabs = Arc::new(tabs);
        }
        let selected = self
            .selected
            .filter(|id| self.tab(*id).is_some())
            .or_else(|| self.tabs.iter().find(|t| !t.disabled).map(|t| t.id));
        let moved = selected != self.selected;
        self.selected = selected;
        changed || moved
    }

    /// Apply a user action; `None` for a no-op or refused one.
    pub fn apply(&mut self, action: TabsAction) -> Option<TabsChange> {
        match action {
            TabsAction::Select(id) => {
                let tab = self.tab(id).filter(|t| !t.disabled)?;
                (self.selected != Some(tab.id)).then(|| {
                    self.selected = Some(id);
                    TabsChange::Selected { id }
                })
            }
            TabsAction::Close(id) => {
                let i = self.index_of(id)?;
                if !self.tabs[i].closable || self.tabs[i].disabled {
                    return None;
                }
                Arc::make_mut(&mut self.tabs).remove(i);
                if self.selected == Some(id) {
                    // The tab that took its place, else the one before it.
                    let enabled = |t: &&TabItem| !t.disabled;
                    self.selected = self.tabs[i.min(self.tabs.len())..]
                        .iter()
                        .find(enabled)
                        .or_else(|| {
                            self.tabs[..i.min(self.tabs.len())]
                                .iter()
                                .rev()
                                .find(enabled)
                        })
                        .map(|t| t.id);
                }
                Some(TabsChange::Closed {
                    id,
                    selected: self.selected,
                })
            }
        }
    }
}

fn id_payload(action: &Action) -> Result<TabId, ActionError> {
    let v = match &action.payload {
        Value::Num(n) => *n,
        p => p
            .field(&[SharedString::new_static("id")])
            .as_f64()
            .ok_or(ActionError::Invalid("tab id"))?,
    };
    (v >= 0.0 && v.fract() == 0.0 && v <= 9_007_199_254_740_992.0)
        .then_some(TabId(v as u64))
        .ok_or(ActionError::Invalid("tab id"))
}

impl TabsAction {
    /// The erased form: `select` or `close` with the tab id as a number.
    pub fn to_action(self) -> Action {
        let (name, id) = match self {
            Self::Select(id) => (names::SELECT, id),
            Self::Close(id) => (names::CLOSE, id),
        };
        Action {
            name: SharedString::new_static(name),
            payload: Value::Num(id.0 as f64),
        }
    }

    pub fn decode(action: &Action) -> Result<Self, ActionError> {
        match action.name.as_ref() {
            names::SELECT => Ok(Self::Select(id_payload(action)?)),
            names::CLOSE => Ok(Self::Close(id_payload(action)?)),
            _ => Err(ActionError::Unsupported),
        }
    }
}

impl From<TabsChange> for Value {
    fn from(c: TabsChange) -> Self {
        let field = |k: &'static str, v: Value| (SharedString::new_static(k), v);
        let id = |t: TabId| Value::Num(t.0 as f64);
        let fields = match c {
            TabsChange::Selected { id: t } => vec![
                field("kind", Value::Str("selected".into())),
                field("id", id(t)),
            ],
            TabsChange::Closed { id: t, selected } => vec![
                field("kind", Value::Str("closed".into())),
                field("id", id(t)),
                field("selected", selected.map_or(Value::Null, id)),
            ],
        };
        Value::Record(Arc::new(fields.into_iter().collect()))
    }
}

/// A store for a tab row (no undo: selection and closing are view state for most hosts).
pub fn tabs_store(doc: TabsDoc, cx: &mut App) -> Store<TabsDoc> {
    Store::new(doc, cx)
}

#[derive(Debug, Clone, Copy)]
struct ApplyTabsAction(TabsAction);

impl Event<TabsDoc> for ApplyTabsAction {
    fn handle(self, fx: &mut Fx<'_, TabsDoc>) {
        let mut next = fx.state().clone();
        if next.apply(self.0).is_some() {
            fx.set(next);
        }
    }
}

/// Host write: replace the tabs. Never echoes.
#[derive(Debug, Clone)]
pub struct SetTabs(pub Vec<TabItem>);

impl Event<TabsDoc> for SetTabs {
    fn handle(self, fx: &mut Fx<'_, TabsDoc>) {
        let mut next = fx.state().clone();
        if next.set_tabs(self.0) {
            fx.set(next);
        }
    }
    fn undoable(&self) -> bool {
        false
    }
}

/// Host write: the selection (ignored for a missing or disabled tab). Never echoes.
#[derive(Debug, Clone, Copy)]
pub struct SetSelected(pub TabId);

impl Event<TabsDoc> for SetSelected {
    fn handle(self, fx: &mut Fx<'_, TabsDoc>) {
        if fx.state().tab(self.0).is_some_and(|t| !t.disabled)
            && fx.state().selected != Some(self.0)
        {
            fx.update(|d| d.selected = Some(self.0));
        }
    }
    fn undoable(&self) -> bool {
        false
    }
}

/// Emitted per applied user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabsEvent {
    Selected(TabId),
    Closed { id: TabId, selected: Option<TabId> },
}

/// Renders the slot after the visible tabs (the overflow hook).
pub type TabsTrailing = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone, Default)]
pub struct TabsOptions {
    /// Rendered after the last visible tab; an overflow row puts its "…" menu here.
    pub trailing: Option<TabsTrailing>,
}

/// The tab row.
pub struct Tabs {
    store: Store<TabsDoc>,
    doc: guiraffe_state::Selected<TabsDoc>,
    options: TabsOptions,
    hidden: BTreeSet<TabId>,
    /// The tab with roving keyboard focus.
    focused: Option<TabId>,
    focus: FocusHandle,
    id: Option<ControlId>,
    /// Test hook: draw the focus ring without keyboard focus (goldens).
    force_ring: bool,
}

impl EventEmitter<TabsEvent> for Tabs {}

impl Focusable for Tabs {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn route(
    view: &WeakEntity<Tabs>,
    action: TabsAction,
    source: Source,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = view.upgrade() else { return };
    match entity.read(cx).id {
        Some(id) => {
            ActionBus::dispatch(id, &action.to_action(), source, window, cx).ok();
        }
        None => {
            entity.update(cx, |tabs, cx| tabs.perform(action, cx).ok());
        }
    }
}

impl Tabs {
    pub fn new(store: Store<TabsDoc>, options: TabsOptions, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            doc: store.select(cx, |d: &TabsDoc| d.clone()),
            store,
            options,
            hidden: BTreeSet::new(),
            focused: None,
            focus: cx.focus_handle(),
            id: None,
            force_ring: false,
        })
    }

    /// Register on the window's action bus at `path` (`select` and `close`).
    pub fn register(view: &Entity<Self>, path: &str, window: &Window, cx: &mut App) -> ControlId {
        let id = ActionBus::register(
            path,
            view,
            "tabs",
            Capabilities::SELECT | Capabilities::CLOSE,
            window,
            cx,
        );
        view.update(cx, |t, _| t.id = Some(id));
        id
    }

    pub fn store(&self) -> &Store<TabsDoc> {
        &self.store
    }

    pub fn doc<'a>(&self, cx: &'a App) -> &'a TabsDoc {
        self.doc.get(cx)
    }

    pub fn selected(&self, cx: &App) -> Option<TabId> {
        self.doc(cx).selected
    }

    /// Test hook for goldens: give `id` the roving focus and draw its focus ring.
    #[doc(hidden)]
    pub fn focus_for_snapshot(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        self.focused = Some(id);
        self.force_ring = true;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The tab with keyboard focus.
    pub fn focused_tab(&self) -> Option<TabId> {
        self.focused
    }

    /// Keep these tabs out of the row (an overflow row lists them in its menu). They stay
    /// selectable over the bus.
    pub fn set_hidden(&mut self, hidden: impl IntoIterator<Item = TabId>, cx: &mut Context<Self>) {
        let hidden: BTreeSet<TabId> = hidden.into_iter().collect();
        if hidden != self.hidden {
            self.hidden = hidden;
            cx.notify();
        }
    }

    /// Apply a user action; emits one [`TabsEvent`].
    pub fn perform(
        &mut self,
        action: TabsAction,
        cx: &mut Context<Self>,
    ) -> Result<Option<TabsChange>, ActionError> {
        let mut next = self.doc(cx).clone();
        let Some(change) = next.apply(action) else {
            return Ok(None);
        };
        self.store.dispatch_now(ApplyTabsAction(action), cx);
        if let TabsChange::Closed { id, selected } = change {
            if self.focused == Some(id) {
                self.focused = selected;
            }
            cx.emit(TabsEvent::Closed { id, selected });
        } else if let TabsChange::Selected { id } = change {
            cx.emit(TabsEvent::Selected(id));
        }
        cx.notify();
        Ok(Some(change))
    }

    fn visible<'a>(&'a self, doc: &'a TabsDoc) -> impl Iterator<Item = &'a TabItem> + 'a {
        doc.tabs().iter().filter(|t| !self.hidden.contains(&t.id))
    }

    /// The keyboard map; returns the action to route.
    fn key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) -> Option<TabsAction> {
        let doc = self.doc(cx).clone();
        let visible: Vec<&TabItem> = self.visible(&doc).filter(|t| !t.disabled).collect();
        if visible.is_empty() {
            return None;
        }
        let at = self
            .focused
            .or(doc.selected)
            .and_then(|id| visible.iter().position(|t| t.id == id))
            .unwrap_or(0);
        let m = &e.keystroke.modifiers;
        let move_to = |i: usize| visible[i].id;
        let next = match e.keystroke.key.as_str() {
            "left" => Some(move_to(at.saturating_sub(1))),
            "right" => Some(move_to((at + 1).min(visible.len() - 1))),
            "home" => Some(move_to(0)),
            "end" => Some(move_to(visible.len() - 1)),
            "enter" | "space" => return Some(TabsAction::Select(visible[at].id)),
            "delete" => {
                return visible[at]
                    .closable
                    .then_some(TabsAction::Close(visible[at].id));
            }
            "w" if m.control || m.platform => {
                return visible[at]
                    .closable
                    .then_some(TabsAction::Close(visible[at].id));
            }
            _ => None,
        };
        if let Some(id) = next
            && self.focused != Some(id)
        {
            self.focused = Some(id);
            cx.notify();
        }
        None
    }
}

impl Observe for Tabs {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, cx: &App) -> ControlSnapshot {
        let doc = self.doc(cx);
        ControlSnapshot {
            value: doc.selected.map(|id| Value::Num(id.0 as f64)),
            selected: doc
                .selected
                .and_then(|id| doc.index_of(id))
                .map(|i| Rc::from([i].as_slice())),
            ..Default::default()
        }
    }
}

/// The bus outcome of a [`TabsAction`].
pub struct TabsOutcome(pub Option<TabsChange>);

impl From<TabsOutcome> for Outcome {
    fn from(o: TabsOutcome) -> Self {
        o.0.map_or(Outcome::EMPTY, |c| Outcome(c.into()))
    }
}

impl Drive for Tabs {
    type Action = TabsAction;
    type Output = TabsOutcome;
    fn decode(action: &Action) -> Result<TabsAction, ActionError> {
        TabsAction::decode(action)
    }
    fn apply(
        &mut self,
        action: TabsAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TabsOutcome, ActionError> {
        self.perform(action, cx).map(TabsOutcome)
    }
}

/// Element id of tab `id` (for tests and accessibility).
pub fn tab_element_id(id: TabId) -> ElementId {
    ElementId::NamedInteger("tab".into(), id.0)
}

/// Element id of tab `id`'s close button.
pub fn tab_close_id(id: TabId) -> ElementId {
    ElementId::NamedInteger("tab-close".into(), id.0)
}

impl Render for Tabs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let doc = self.doc(cx).clone();
        let theme = GuiraffeTheme::global(cx).clone();
        let s = theme.surfaces();
        let accent = theme.variant(None, Variant::Filled).background.to_hsla();
        let hover = theme
            .state(StateKind::Hover, Variant::Subtle, None)
            .background
            .to_hsla();
        let disabled = theme
            .state(StateKind::Disabled, Variant::Subtle, None)
            .foreground
            .to_hsla();
        let ring = theme
            .state(StateKind::FocusRing, Variant::Default, None)
            .border
            .to_hsla();
        let ring_on = self.force_ring || guiraffe_core::ring_visible(&self.focus, window);
        let badge_bg = theme.variant(None, Variant::Light).background.to_hsla();
        let badge_fg = theme.variant(None, Variant::Light).foreground.to_hsla();
        let view = cx.weak_entity();
        let tabs = self.visible(&doc).map(|tab| {
            let selected = doc.selected == Some(tab.id);
            let focused = ring_on && self.focused.or(doc.selected) == Some(tab.id);
            let id = tab.id;
            let select = {
                let view = view.clone();
                move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
                    route(&view, TabsAction::Select(id), Source::Pointer, window, cx)
                }
            };
            div()
                .id(tab_element_id(tab.id))
                .test_support()
                .role(Role::Tab)
                .aria_label(tab.label.clone())
                .aria_selected(selected)
                .relative()
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(32.))
                .px(px(12.))
                .rounded_t(px(4.))
                .text_color(if tab.disabled {
                    disabled
                } else if selected {
                    s.text.to_hsla()
                } else {
                    s.dimmed.to_hsla()
                })
                .when(selected, |d| d.font_weight(gpui_kit::FontWeight::SEMIBOLD))
                .when(!tab.disabled, |d| {
                    d.cursor_pointer()
                        .hover(move |st| st.bg(hover))
                        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                        .on_click(select)
                })
                .when_some(tab.icon, |d, icon| d.child(Icon::new(icon).size_4()))
                .child(tab.label.clone())
                .when_some(tab.badge.clone(), |d, badge| {
                    d.child(
                        div()
                            .px(px(6.))
                            .rounded(px(8.))
                            .text_size(px(10.))
                            .bg(badge_bg)
                            .text_color(badge_fg)
                            .child(badge),
                    )
                })
                .when(tab.closable && !tab.disabled, |d| {
                    let view = view.clone();
                    d.child(
                        div()
                            .id(tab_close_id(id))
                            .test_support()
                            .role(Role::Button)
                            .aria_label(SharedString::from(format!("Close {}", tab.label)))
                            .rounded(px(3.))
                            .hover(move |st| st.bg(hover))
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                route(&view, TabsAction::Close(id), Source::Pointer, window, cx)
                            })
                            .child(Icon::new(IconName::Close).size_3()),
                    )
                })
                // Selection: an accent underline (a shape cue, not colour alone).
                .when(selected, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(px(2.))
                            .bg(accent),
                    )
                })
                // Focus ring as an overlay: focus never changes layout.
                .when(focused, |d| {
                    d.child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_t(px(4.))
                            .border_1()
                            .border_color(ring),
                    )
                })
        });
        let trailing = self.options.trailing.clone().map(|t| t(window, cx));
        div()
            .id("tabs")
            .test_support()
            .role(Role::TabList)
            .track_focus(&self.focus)
            .key_context("Tabs")
            .on_key_down({
                let view = view.clone();
                move |e: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    let Some(tabs) = view.upgrade() else { return };
                    let action = tabs.update(cx, |t, cx| t.key(e, cx));
                    if let Some(action) = action {
                        route(&view, action, Source::Keyboard, window, cx);
                    }
                }
            })
            .flex()
            .items_end()
            .border_b_1()
            .border_color(s.border.to_hsla())
            .text_size(px(13.))
            .children(tabs)
            .children(trailing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> TabsDoc {
        TabsDoc::new(vec![
            TabItem::new(1, "Mixer").closable(),
            TabItem::new(2, "Piano").closable(),
            TabItem::new(3, "Off").disabled(true),
            TabItem::new(4, "Browser").closable(),
        ])
    }

    #[test]
    fn select_refuses_missing_and_disabled_tabs() {
        let mut d = doc();
        assert_eq!(d.selected, Some(TabId(1)));
        assert_eq!(
            d.apply(TabsAction::Select(TabId(2))),
            Some(TabsChange::Selected { id: TabId(2) })
        );
        assert_eq!(d.apply(TabsAction::Select(TabId(2))), None);
        assert_eq!(d.apply(TabsAction::Select(TabId(3))), None, "disabled");
        assert_eq!(d.apply(TabsAction::Select(TabId(9))), None, "missing");
    }

    #[test]
    fn closing_the_selected_tab_selects_its_right_then_left_neighbour() {
        let mut d = doc();
        d.apply(TabsAction::Select(TabId(2)));
        assert_eq!(
            d.apply(TabsAction::Close(TabId(2))),
            Some(TabsChange::Closed {
                id: TabId(2),
                selected: Some(TabId(4))
            }),
            "the next enabled tab to the right (3 is disabled)"
        );
        assert_eq!(
            d.apply(TabsAction::Close(TabId(4))),
            Some(TabsChange::Closed {
                id: TabId(4),
                selected: Some(TabId(1))
            }),
            "the last tab: the one to the left"
        );
        assert_eq!(d.apply(TabsAction::Close(TabId(3))), None, "not closable");
        assert_eq!(
            d.apply(TabsAction::Close(TabId(1))),
            Some(TabsChange::Closed {
                id: TabId(1),
                selected: None
            })
        );
    }

    #[test]
    fn closing_another_tab_keeps_the_selection() {
        let mut d = doc();
        assert_eq!(
            d.apply(TabsAction::Close(TabId(4))),
            Some(TabsChange::Closed {
                id: TabId(4),
                selected: Some(TabId(1))
            })
        );
    }

    #[test]
    fn set_tabs_keeps_the_selection_by_id() {
        let mut d = doc();
        d.apply(TabsAction::Select(TabId(4)));
        let mut tabs = d.tabs().to_vec();
        tabs.reverse();
        assert!(d.set_tabs(tabs));
        assert_eq!(d.selected, Some(TabId(4)));
        assert!(d.set_tabs(vec![TabItem::new(7, "New"), TabItem::new(7, "Dup")]));
        assert_eq!((d.tabs().len(), d.selected), (1, Some(TabId(7))));
    }

    #[test]
    fn erased_actions_round_trip() {
        for a in [TabsAction::Select(TabId(3)), TabsAction::Close(TabId(12))] {
            assert_eq!(TabsAction::decode(&a.to_action()), Ok(a));
        }
    }
}
