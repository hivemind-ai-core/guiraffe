//! Menu section labels: how the headings between groups of menu items look.
//!
//! By default a section label is a shaded band (slightly darker than the menu in light themes,
//! lighter in dark ones) with centred monospace text. It stays non-interactive and keyboard
//! navigation skips it. Three ways to restyle it, most specific first:
//!
//! - per menu: `PopupMenu::section_style(style)` on a GPUI Kit menu;
//! - per scope: [`MenuSectionStyle::register`] once, then
//!   `provide(StylePatch::new().menu_section(id), child)`; menus built inside the scope use it;
//! - per theme: [`set_menu_section_styles`] for light and dark.
//!
//! Unset fields fall through to the next level and finally to theme-derived defaults.
use gpui_kit::App;
use std::cell::RefCell;

pub use gpui_kit::component::menu::{MenuSectionAlign, MenuSectionStyle, MenuSectionStyles};

/// A registered scoped section style. Copy-sized, so the style context stays one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MenuSectionId(pub(crate) u16);

thread_local! {
    static REGISTRY: RefCell<Vec<MenuSectionStyle>> = const { RefCell::new(Vec::new()) };
}

/// Register a style for scoped use. Register each style once (at startup), not per frame:
/// registrations are kept for the life of the thread. Ids past `u16::MAX` reuse the last one.
pub trait RegisterMenuSection {
    fn register(self) -> MenuSectionId;
}

impl RegisterMenuSection for MenuSectionStyle {
    fn register(self) -> MenuSectionId {
        gpui_kit::component::menu::set_menu_section_hook(scoped_style);
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let max = usize::from(u16::MAX);
            if r.len() < max {
                r.push(self);
            } else if let Some(last) = r.last_mut() {
                *last = self;
            }
            MenuSectionId(u16::try_from(r.len()).unwrap_or(u16::MAX))
        })
    }
}

/// The registered style for `id`, if any.
pub fn registered(id: MenuSectionId) -> Option<MenuSectionStyle> {
    REGISTRY.with(|r| r.borrow().get(usize::from(id.0).wrapping_sub(1)).cloned())
}

fn scoped_style(_: &App) -> Option<MenuSectionStyle> {
    crate::StyleContext::current()
        .menu_section
        .and_then(registered)
}

/// Set the app-wide section style for light and dark themes. Unset fields keep the defaults.
pub fn set_menu_section_styles(light: MenuSectionStyle, dark: MenuSectionStyle, cx: &mut App) {
    cx.set_global(MenuSectionStyles { light, dark });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StyleContext, StylePatch};

    #[test]
    fn a_registered_style_rides_the_style_context() {
        let style = MenuSectionStyle::default().align(MenuSectionAlign::Start);
        let id = style.clone().register();
        assert_eq!(registered(id), Some(style.clone()));
        let ctx = StylePatch::new()
            .menu_section(id)
            .apply(StyleContext::default());
        let got = ctx.scope(|| StyleContext::current().menu_section.and_then(registered));
        assert_eq!(got, Some(style));
    }

    #[test]
    fn the_word_round_trips_the_section_id() {
        let id = MenuSectionStyle::default().register();
        let ctx = StylePatch::new()
            .menu_section(id)
            .apply(StyleContext::default());
        assert_eq!(StyleContext::from_word(ctx.to_word()), ctx);
        assert_eq!(StyleContext::from_word(0), StyleContext::default());
    }
}
