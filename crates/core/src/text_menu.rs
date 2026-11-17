//! A shared right-click / keyboard context menu for read-only selectable text surfaces.
//!
//! Surfaces (code viewer, Markdown, chat messages) keep their own selection. They describe
//! what the menu can do with a [`TextMenuSpec`] and hold one [`TextMenu`] handle that opens
//! a Controls `PopupMenu` at the pointer (or at a keyboard anchor for Shift+F10 / the Menu
//! key). Escape and an outside click dismiss it through the popup's own handling; focus
//! returns to the surface's focus handle.
use gpui_kit::{
    AnyElement, App, DismissEvent, Entity, EntityId, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, ParentElement, Pixels, Point, SharedString, Subscription, Window, anchored,
    component::menu::PopupMenu, component::menu::PopupMenuItem, deferred, px,
};
use std::{cell::RefCell, rc::Rc};

/// A menu action. Receives the window and app when the item is chosen.
pub type TextMenuHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// One menu entry.
#[derive(Clone)]
pub struct TextMenuItem {
    pub label: SharedString,
    pub enabled: bool,
    pub handler: TextMenuHandler,
}

impl TextMenuItem {
    pub fn new(
        label: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            handler: Rc::new(handler),
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// What a text surface offers: Copy (disabled without a selection), Select all, then any
/// surface-specific extras after a separator.
#[derive(Clone)]
pub struct TextMenuSpec {
    pub has_selection: bool,
    pub copy: TextMenuHandler,
    pub select_all: TextMenuHandler,
    pub extra: Vec<TextMenuItem>,
}

/// Labels of the standard entries.
pub const COPY: &str = "Copy";
pub const SELECT_ALL: &str = "Select all";

impl TextMenuSpec {
    pub fn new(
        has_selection: bool,
        copy: impl Fn(&mut Window, &mut App) + 'static,
        select_all: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            has_selection,
            copy: Rc::new(copy),
            select_all: Rc::new(select_all),
            extra: Vec::new(),
        }
    }

    pub fn item(mut self, item: TextMenuItem) -> Self {
        self.extra.push(item);
        self
    }

    /// Every entry in display order; `None` marks a separator.
    pub fn items(&self) -> Vec<Option<TextMenuItem>> {
        let mut items = vec![
            Some(TextMenuItem {
                label: COPY.into(),
                enabled: self.has_selection,
                handler: self.copy.clone(),
            }),
            Some(TextMenuItem {
                label: SELECT_ALL.into(),
                enabled: true,
                handler: self.select_all.clone(),
            }),
        ];
        if !self.extra.is_empty() {
            items.push(None);
            items.extend(self.extra.iter().cloned().map(Some));
        }
        items
    }
}

/// Whether a key press should open the context menu: Shift+F10 or the Menu key.
pub fn is_menu_key(event: &KeyDownEvent) -> bool {
    let k = &event.keystroke;
    let m = k.modifiers;
    match k.key.as_str() {
        "f10" => m.shift && !m.control && !m.alt && !m.platform,
        "menu" | "contextmenu" | "apps" => true,
        _ => false,
    }
}

struct Open {
    position: Point<Pixels>,
    menu: Entity<PopupMenu>,
    items: Vec<Option<TextMenuItem>>,
    owner: EntityId,
    restore: FocusHandle,
    _dismiss: Subscription,
}

#[derive(Default)]
struct Inner {
    open: Option<Open>,
}

/// One open-or-closed context menu, owned by a text surface. Cheap to clone.
#[derive(Clone, Default)]
pub struct TextMenu {
    inner: Rc<RefCell<Inner>>,
}

impl TextMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.inner.borrow().open.is_some()
    }

    /// Where the open menu is anchored, in window coordinates.
    pub fn position(&self) -> Option<Point<Pixels>> {
        self.inner.borrow().open.as_ref().map(|o| o.position)
    }

    /// `(label, enabled)` of the open menu's entries; separators are omitted.
    pub fn entries(&self) -> Vec<(SharedString, bool)> {
        self.inner
            .borrow()
            .open
            .as_ref()
            .map(|o| {
                o.items
                    .iter()
                    .flatten()
                    .map(|i| (i.label.clone(), i.enabled))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Run the enabled entry labelled `label` and close the menu, as a click would.
    /// Returns whether an enabled entry ran.
    pub fn activate(&self, label: &str, window: &mut Window, cx: &mut App) -> bool {
        let handler = self.inner.borrow().open.as_ref().and_then(|o| {
            o.items
                .iter()
                .flatten()
                .find(|i| i.label.as_ref() == label && i.enabled)
                .map(|i| i.handler.clone())
        });
        let Some(handler) = handler else {
            return false;
        };
        self.dismiss(window, cx);
        handler(window, cx);
        true
    }

    /// Open the menu at `position`. `owner` renders [`Self::element`] and is notified when
    /// the menu opens or closes; `restore` regains focus when it is dismissed.
    pub fn open(
        &self,
        position: Point<Pixels>,
        spec: &TextMenuSpec,
        owner: EntityId,
        restore: FocusHandle,
        window: &mut Window,
        cx: &mut App,
    ) {
        let items = spec.items();
        let restore_focus = restore.clone();
        let built = items.clone();
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            menu = menu.action_context(restore.clone()).min_w(px(160.));
            for item in &built {
                menu = match item {
                    None => menu.separator(),
                    Some(item) => {
                        let handler = item.handler.clone();
                        menu.item(
                            PopupMenuItem::new(item.label.clone())
                                .disabled(!item.enabled)
                                .on_click(move |_, window, cx| handler(window, cx)),
                        )
                    }
                };
            }
            menu
        });
        let weak = Rc::downgrade(&self.inner);
        let dismiss = window.subscribe(&menu, cx, move |_, _: &DismissEvent, _, cx| {
            if let Some(inner) = weak.upgrade() {
                // Dropping the subscription inside its own callback is deferred by GPUI.
                let closed = inner.borrow_mut().open.take();
                drop(closed);
                cx.notify(owner);
            }
        });
        menu.focus_handle(cx).focus(window, cx);
        self.inner.borrow_mut().open = Some(Open {
            position,
            menu,
            items,
            owner,
            restore: restore_focus,
            _dismiss: dismiss,
        });
        cx.notify(owner);
    }

    /// Close without running anything, returning focus to the surface.
    pub fn dismiss(&self, window: &mut Window, cx: &mut App) {
        let closed = self.inner.borrow_mut().open.take();
        if let Some(open) = closed {
            window.focus(&open.restore, cx);
            cx.notify(open.owner);
        }
    }

    /// The deferred popup layer while open; place it as a child of the owner's root.
    pub fn element(&self) -> Option<AnyElement> {
        let inner = self.inner.borrow();
        let open = inner.open.as_ref()?;
        Some(
            deferred(
                anchored()
                    .position(open.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(open.menu.clone()),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_disabled_without_selection_and_extras_follow_separator() {
        let spec = TextMenuSpec::new(false, |_, _| {}, |_, _| {});
        let items = spec.items();
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .flatten()
                .any(|i| i.label == COPY && !i.enabled)
        );
        let spec = TextMenuSpec::new(true, |_, _| {}, |_, _| {})
            .item(TextMenuItem::new("Copy with line numbers", |_, _| {}).enabled(true));
        let labels: Vec<_> = spec
            .items()
            .iter()
            .map(|i| i.as_ref().map(|i| i.label.to_string()))
            .collect();
        assert_eq!(
            labels,
            vec![
                Some(COPY.into()),
                Some(SELECT_ALL.into()),
                None,
                Some("Copy with line numbers".into())
            ]
        );
        assert!(spec.items()[0].as_ref().is_some_and(|i| i.enabled));
    }
}
