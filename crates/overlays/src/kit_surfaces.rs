//! Document adapters for the Controls overlay components re-exported by this crate.

use std::time::Duration;

use gpui_kit::{
    Anchor, App, Context, InteractiveElement, IntoElement, Keystroke, MouseButton, ParentElement,
    StatefulInteractiveElement, StyleRefinement, Styled, Window,
    component::{Icon, Side, button::Button, kbd::Kbd, menu::PopupMenuItem},
    div, px,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{ContextMenuExt, DropdownMenu, HoverCard, Popover, PopupMenu, Tooltip, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: &widget_specs::POPOVER,
                schema: guiraffe_core::widget_schema!(&widget_specs::POPOVER),
                render: |node, children, _, _| {
                    let mut popover = Popover::new(node.state_id())
                        .trigger(Button::new("trigger").label(text(node, 0, "Details")))
                        .anchor(anchor(node.prop(1), Anchor::TopLeft))
                        .arrow(flag(node, 3, false))
                        .mouse_button(match node.prop(4).as_str() {
                            Some("right") => MouseButton::Right,
                            Some("middle") => MouseButton::Middle,
                            _ => MouseButton::Left,
                        })
                        .overlay_closable(flag(node, 7, true))
                        .default_open(flag(node, 5, false))
                        .appearance(flag(node, 8, true))
                        .children(children);
                    if let Some(offset) = node.prop(2).as_f64() {
                        popover = popover.offset(px(offset as f32));
                    }
                    if let Some(open) = node.prop(6).as_bool() {
                        popover = popover.open(open);
                    }
                    if flag(node, 9, false) {
                        popover = popover.trigger_style(StyleRefinement::default().w_full());
                    }
                    if let Some(change) = node.handler(0) {
                        popover = popover.on_open_change(move |open, window, cx| {
                            change((*open).into(), window, cx);
                        });
                    }
                    popover.into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::HOVER_CARD,
                schema: guiraffe_core::widget_schema!(&widget_specs::HOVER_CARD),
                render: |node, children, _, _| {
                    let mut card = HoverCard::new(node.state_id())
                        .trigger(div().child(text(node, 0, "Details")))
                        .anchor(anchor(node.prop(1), Anchor::TopCenter))
                        .open_delay(delay(node.prop(2), 600))
                        .close_delay(delay(node.prop(3), 300))
                        .appearance(flag(node, 4, true))
                        .children(children);
                    if flag(node, 5, false) {
                        card = card.trigger_style(StyleRefinement::default().w_full());
                    }
                    if let Some(change) = node.handler(0) {
                        card = card.on_open_change(move |open, window, cx| {
                            change((*open).into(), window, cx);
                        });
                    }
                    card.into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::TOOLTIP,
                schema: guiraffe_core::widget_schema!(&widget_specs::TOOLTIP),
                render: |node, children, _, cx| {
                    let text = text(node, 0, "");
                    let key = node.prop(1).as_str().and_then(|text| {
                        Keystroke::parse_with_os(text, cx.platform_info().os).ok()
                    });
                    div()
                        .id(node.state_id())
                        .children(children)
                        .tooltip(move |window, cx| {
                            Tooltip::new(text.clone())
                                .key_binding(key.clone().map(Kbd::new))
                                .build(window, cx)
                        })
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::POPUP_MENU,
                schema: guiraffe_core::widget_schema!(&widget_specs::POPUP_MENU),
                render: |node, _, window, cx| {
                    let menu = MenuContent::new(node);
                    PopupMenu::build(window, cx, |popup, window, cx| {
                        menu.build(popup, window, cx)
                    })
                    .into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::CONTEXT_MENU,
                schema: guiraffe_core::widget_schema!(&widget_specs::CONTEXT_MENU),
                render: |node, children, _, _| {
                    let menu = MenuContent::new(node);
                    div()
                        .id(node.state_id())
                        .children(children)
                        .context_menu(move |popup, window, cx| menu.build(popup, window, cx))
                        .into_any_element()
                },
            },
            WidgetRegistration {
                spec: &widget_specs::DROPDOWN_MENU,
                schema: guiraffe_core::widget_schema!(&widget_specs::DROPDOWN_MENU),
                render: |node, _, _, _| {
                    let menu = MenuContent::new(node);
                    let anchor = anchor(node.prop(8), Anchor::TopLeft);
                    let mut dropdown = Button::new(node.state_id())
                        .label(text(node, 7, "Menu"))
                        .dropdown_menu_with_anchor(anchor, move |popup, window, cx| {
                            menu.build(popup, window, cx)
                        });
                    if let Some(change) = node.handler(1) {
                        dropdown = dropdown.on_open_change(move |open, window, cx| {
                            change((*open).into(), window, cx);
                        });
                    }
                    dropdown.into_any_element()
                },
            },
        ],
    );
}

fn text(node: &dyn WidgetNode, index: usize, default: &str) -> gpui_kit::SharedString {
    node.prop(index)
        .as_str()
        .unwrap_or(default)
        .to_owned()
        .into()
}

fn flag(node: &dyn WidgetNode, index: usize, default: bool) -> bool {
    node.prop(index).as_bool().unwrap_or(default)
}

fn delay(value: &Value, default: u64) -> Duration {
    Duration::from_millis(
        value
            .as_f64()
            .map_or(default, |ms| ms.clamp(0., 60_000.) as u64),
    )
}

fn anchor(value: &Value, default: Anchor) -> Anchor {
    match value.as_str() {
        Some("top-left") => Anchor::TopLeft,
        Some("top-right") => Anchor::TopRight,
        Some("bottom-left") => Anchor::BottomLeft,
        Some("bottom-right") => Anchor::BottomRight,
        Some("top-center") => Anchor::TopCenter,
        Some("bottom-center") => Anchor::BottomCenter,
        Some("left-center") => Anchor::LeftCenter,
        Some("right-center") => Anchor::RightCenter,
        _ => default,
    }
}

struct MenuContent {
    items: Value,
    width: [Option<f64>; 3],
    scrollable: bool,
    check_right: bool,
    external_link_icon: bool,
    select: Option<EventHandler>,
}

impl MenuContent {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            items: node.prop(0).clone(),
            width: std::array::from_fn(|index| node.prop(index + 1).as_f64()),
            scrollable: flag(node, 4, false),
            check_right: node.prop(5).as_str() == Some("right"),
            external_link_icon: flag(node, 6, true),
            select: node.handler(0),
        }
    }

    fn build(
        &self,
        mut menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let dimensions = self.width.map(|value| value.map(|value| px(value as f32)));
        if let Some(width) = dimensions[0] {
            menu = menu.min_w(width);
        }
        if let Some(width) = dimensions[1] {
            menu = menu.max_w(width);
        }
        if let Some(height) = dimensions[2] {
            menu = menu.max_h(height);
        }
        menu = menu
            .scrollable(self.scrollable)
            .external_link_icon(self.external_link_icon)
            .check_side(if self.check_right {
                Side::Right
            } else {
                Side::Left
            });
        build_items(menu, &self.items, self.select.as_ref(), 0, window, cx)
    }
}

fn build_items(
    mut menu: PopupMenu,
    items: &Value,
    event: Option<&EventHandler>,
    depth: usize,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for index in 0..items.list_len().unwrap_or_default() {
        let Some(item) = items.list_get(index) else {
            continue;
        };
        let get = |field: &str| item.field(&[field.into()]);
        let label = get("label").display();
        let icon = get("icon")
            .as_str()
            .map(|path| Icon::default().path(path.to_owned()));
        let entry = match get("kind").as_str() {
            Some("separator") => PopupMenuItem::separator(),
            Some("label") => PopupMenuItem::label(label),
            Some("submenu") if depth < 3 => {
                let contents = get("items").clone();
                let event = event.cloned();
                menu = menu.submenu_with_icon(icon, label, window, cx, move |menu, window, cx| {
                    build_items(menu, &contents, event.as_ref(), depth + 1, window, cx)
                });
                continue;
            }
            Some("submenu") => continue,
            _ => {
                let mut entry = if let Some(link) = get("href").as_str() {
                    PopupMenuItem::link(label, link.to_owned())
                } else {
                    PopupMenuItem::new(label)
                };
                if let Some(icon) = icon {
                    entry = entry.icon(icon);
                }
                if let Some(event) = event.cloned()
                    && let Some(id) = get("id").as_str()
                    && get("href").as_str().is_none()
                {
                    let id = Value::from(id.to_owned());
                    entry = entry.on_click(move |_, window, cx| event(id.clone(), window, cx));
                }
                entry
                    .disabled(get("disabled").as_bool().unwrap_or(false))
                    .checked(get("checked").as_bool().unwrap_or(false))
            }
        };
        menu = menu.item(entry);
    }
    menu
}
