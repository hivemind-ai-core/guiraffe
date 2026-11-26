//! Section label styling for popup and dropdown menus (GUIraffe patch).
//!
//! A section label renders as a shaded band with centred monospace text. The
//! style resolves, field by field, from: the menu's own style
//! ([`crate::menu::PopupMenu::section_style`]), the embedder's scoped hook
//! ([`set_menu_section_hook`]), the per-theme global ([`MenuSectionStyles`]),
//! and finally theme-derived defaults.
use crate::{ActiveTheme, Colorize as _};
use gpui::{App, Global, Hsla, Pixels, SharedString, px};
use std::cell::Cell;

/// Horizontal alignment of a section label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MenuSectionAlign {
    Start,
    #[default]
    Center,
    End,
}

/// Overrides for how menu section labels look. `None` fields fall through to
/// the next source (see the module docs).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuSectionStyle {
    pub background: Option<Hsla>,
    pub color: Option<Hsla>,
    pub font_family: Option<SharedString>,
    pub align: Option<MenuSectionAlign>,
    /// Vertical padding inside the band.
    pub padding_y: Option<Pixels>,
}

impl MenuSectionStyle {
    pub fn background(mut self, v: impl Into<Hsla>) -> Self {
        self.background = Some(v.into());
        self
    }
    pub fn color(mut self, v: impl Into<Hsla>) -> Self {
        self.color = Some(v.into());
        self
    }
    pub fn font_family(mut self, v: impl Into<SharedString>) -> Self {
        self.font_family = Some(v.into());
        self
    }
    pub fn align(mut self, v: MenuSectionAlign) -> Self {
        self.align = Some(v);
        self
    }
    pub fn padding_y(mut self, v: Pixels) -> Self {
        self.padding_y = Some(v);
        self
    }

    /// `self` with its unset fields taken from `base`.
    pub fn or(&self, base: &MenuSectionStyle) -> MenuSectionStyle {
        MenuSectionStyle {
            background: self.background.or(base.background),
            color: self.color.or(base.color),
            font_family: self
                .font_family
                .clone()
                .or_else(|| base.font_family.clone()),
            align: self.align.or(base.align),
            padding_y: self.padding_y.or(base.padding_y),
        }
    }
}

/// Per-theme section styles, applied app-wide. Set as a global.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuSectionStyles {
    pub light: MenuSectionStyle,
    pub dark: MenuSectionStyle,
}

impl Global for MenuSectionStyles {}

/// A fully resolved section style.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMenuSection {
    pub background: Hsla,
    pub color: Hsla,
    pub font_family: SharedString,
    pub align: MenuSectionAlign,
    pub padding_y: Pixels,
}

/// Returns the embedder's scoped override for the menu currently rendering.
pub type MenuSectionHook = fn(&App) -> Option<MenuSectionStyle>;

thread_local! {
    static HOOK: Cell<Option<MenuSectionHook>> = const { Cell::new(None) };
}

/// Install the embedder's scoped override (GUIraffe reads its style context).
pub fn set_menu_section_hook(hook: MenuSectionHook) {
    HOOK.with(|h| h.set(Some(hook)));
}

pub(crate) fn scoped(cx: &App) -> Option<MenuSectionStyle> {
    HOOK.with(|h| h.get()).and_then(|hook| hook(cx))
}

/// Resolve the section style for a menu whose own override is `own`.
pub fn resolve_menu_section(own: Option<&MenuSectionStyle>, cx: &App) -> ResolvedMenuSection {
    let theme = cx.theme();
    let dark = theme.is_dark();
    let global = cx.try_global::<MenuSectionStyles>().map(|s| {
        if dark {
            s.dark.clone()
        } else {
            s.light.clone()
        }
    });
    let mut style = own.cloned().unwrap_or_default();
    if let Some(scoped) = scoped(cx) {
        style = style.or(&scoped);
    }
    if let Some(global) = global {
        style = style.or(&global);
    }
    let band = if dark {
        theme.popover.lighten(0.3)
    } else {
        theme.popover.darken(0.05)
    };
    ResolvedMenuSection {
        background: style.background.unwrap_or(band),
        color: style.color.unwrap_or(theme.muted_foreground),
        font_family: style
            .font_family
            .unwrap_or_else(|| theme.mono_font_family.clone()),
        align: style.align.unwrap_or_default(),
        padding_y: style.padding_y.unwrap_or(px(2.)),
    }
}
