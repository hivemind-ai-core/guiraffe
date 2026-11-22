//! ThemeIcon, ActionIcon and CloseButton.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, ElementId, IntoElement, ParentElement,
    RenderOnce, SharedString, StyleRefinement, Styled, TestSupportExt, Window, div, prelude::*,
    px, svg,
};
use guiraffe_core::{
    A11yDisabledExt, ColorName, GuiraffeTheme, Press, PressableExt, Size, Variant, keyed_focus,
};

use crate::style::{hsla, radius, variant};

enum ComponentIcon {
    Named(IconName),
    Asset(SharedString),
}

impl ComponentIcon {
    fn into_element(self) -> AnyElement {
        match self {
            Self::Named(icon) => icon.into_any_element(),
            Self::Asset(path) => AssetIcon(path).into_any_element(),
        }
    }

    fn label(&self) -> SharedString {
        match self {
            Self::Named(icon) => format!("{icon:?}").into(),
            Self::Asset(path) => path
                .rsplit('/')
                .next()
                .map(|name| name.strip_suffix(".svg").unwrap_or(name))
                .filter(|name| !name.is_empty())
                .unwrap_or("Action")
                .to_owned()
                .into(),
        }
    }
}

#[derive(IntoElement)]
struct AssetIcon(SharedString);

impl RenderOnce for AssetIcon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let style = window.text_style();
        svg()
            .path(self.0)
            .flex_shrink_0()
            .size(style.font_size.to_pixels(window.rem_size()))
            .text_color(style.color)
    }
}

/// An icon inside a colored, variant-styled square.
#[derive(IntoElement)]
pub struct ThemeIcon {
    icon: ComponentIcon,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    variant: Variant,
    radius: Option<Size>,
    style: StyleRefinement,
}

impl ThemeIcon {
    pub fn new(icon: IconName) -> Self {
        Self::with_icon(ComponentIcon::Named(icon))
    }

    /// Build the themed icon from a path in the application's SVG assets.
    pub fn from_asset(path: impl Into<SharedString>) -> Self {
        Self::with_icon(ComponentIcon::Asset(path.into()))
    }

    fn with_icon(icon: ComponentIcon) -> Self {
        Self {
            icon,
            size: None,
            color: None,
            variant: Variant::Filled,
            radius: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
}

crate::props!(ThemeIcon { size: Size => size, variant: Variant => variant });
crate::styled!(ThemeIcon, ActionIcon);

impl ThemeIcon {
    pub fn radius(mut self, r: Size) -> Self {
        self.radius = Some(r);
        self
    }
}

/// Box side for icon containers (Mantine's ThemeIcon/ActionIcon sizes are ~control height × 0.67).
pub fn icon_box(size: Size) -> f32 {
    (size.control_height() * 0.67).round()
}

impl RenderOnce for ThemeIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self
            .size
            .unwrap_or(guiraffe_core::StyleContext::current().control_size);
        let v = variant(cx, self.color, self.variant);
        let side = px(icon_box(size));
        let el = div()
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius(self.radius, cx))
            .bg(hsla(v.background))
            .border_1()
            .border_color(hsla(v.border))
            .text_color(hsla(v.foreground))
            .text_size(px(size.icon_size()))
            .child(self.icon.into_element());
        crate::style::refine(el, &self.style)
    }
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// An icon-only button with Mantine variants. Behaves like a Kit `Button` (spec/00 §12):
/// hover / pressed / disabled state tokens, pointer cursor, a press never takes focus,
/// opt-in tab stop ([`guiraffe_core::focus_scope`] or [`ActionIcon::focusable`]) with a
/// focus-visible ring and Enter/Space activation, and an AccessKit `Button` named by
/// [`ActionIcon::label`] (defaults to the tooltip, then the icon name).
///
/// [`ActionIcon::toggled`] makes it a toggle button (panel switches, bold/italic): it is
/// exposed as pressed (`aria-pressed`) and selected, paints the theme's Selected state and
/// adds a bar along its bottom edge, so the state is never conveyed by colour alone.
/// [`ActionIcon::tooltip`] shows a Kit tooltip on hover and is also the accessible
/// description (or the name, when no label is set).
///
/// ```ignore
/// ActionIcon::new("toggle-left", IconName::PanelLeft)
///     .tooltip("Left panel")
///     .toggled(left_open)
///     .on_click(|_, window, cx| toggle_left(window, cx))
/// ```
#[derive(IntoElement)]
pub struct ActionIcon {
    id: ElementId,
    icon: ComponentIcon,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    /// `None`: quiet regions (toolbars, tables) get Subtle, others Light.
    variant: Option<Variant>,
    disabled: bool,
    /// Shows a spinner instead of the icon and ignores presses, keeping its colours (Mantine).
    loading: bool,
    focusable: Option<bool>,
    label: Option<SharedString>,
    /// `Some`: a toggle button, pressed when `true`.
    toggled: Option<bool>,
    tooltip: Option<SharedString>,
    on_click: Option<OnClick>,
    style: StyleRefinement,
}

impl ActionIcon {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self::with_icon(id.into(), ComponentIcon::Named(icon))
    }

    /// Build the action icon from a path in the application's SVG assets.
    pub fn from_asset(id: impl Into<ElementId>, path: impl Into<SharedString>) -> Self {
        Self::with_icon(id.into(), ComponentIcon::Asset(path.into()))
    }

    fn with_icon(id: ElementId, icon: ComponentIcon) -> Self {
        Self {
            id,
            icon,
            size: None,
            color: None,
            variant: None,
            disabled: false,
            loading: false,
            focusable: None,
            label: None,
            toggled: None,
            tooltip: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }
    pub fn color(mut self, c: ColorName) -> Self {
        self.color = Some(c);
        self
    }
    /// Accessible name (icon-only buttons need one).
    pub fn label(mut self, l: impl Into<SharedString>) -> Self {
        self.label = Some(l.into());
        self
    }
    /// Make this a toggle button in the given state (exposed as pressed/selected, with a
    /// non-colour cue when on). Callers flip the state in `on_click`.
    pub fn toggled(mut self, on: bool) -> Self {
        self.toggled = Some(on);
        self
    }
    /// Tooltip text shown on hover; also the accessible description (or name, without a
    /// [`label`](Self::label)).
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    /// Opt into Tab navigation outside a focus scope (or out of it inside one).
    pub fn focusable(mut self, v: bool) -> Self {
        self.focusable = Some(v);
        self
    }
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

crate::props!(ActionIcon { size: Size => size, variant: Variant => variant, disabled: bool => disabled, loading: bool => loading });

impl RenderOnce for ActionIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let v_kind = self.variant.unwrap_or(match ctx.emphasis {
            guiraffe_core::Emphasis::Quiet => Variant::Subtle,
            guiraffe_core::Emphasis::Default => Variant::Light,
        });
        let colors = GuiraffeTheme::global(cx).press_colors(v_kind, self.color);
        let side = px(icon_box(size));
        let focus = keyed_focus(self.id.clone(), window, cx);
        let tooltip = self.tooltip;
        // A tooltip that differs from the name is read after it as the description.
        let description = tooltip
            .clone()
            .filter(|t| self.label.as_ref().is_some_and(|l| l != t));
        let label = self
            .label
            .or_else(|| tooltip.clone())
            .unwrap_or_else(|| self.icon.label());
        let loading = self.loading;
        // Screen readers hear that the action is in progress.
        let label = if loading {
            SharedString::from(format!("{label} (loading)"))
        } else {
            label
        };
        let r = radius(None, cx);
        let mut press = Press::new(colors)
            .radius(r)
            .disabled(self.disabled)
            .focusable(self.focusable)
            .label(label);
        let toggled = self.toggled;
        if let Some(on) = toggled {
            press = press.toggled(on).selected(on);
        }
        let on = toggled == Some(true);
        // Shape cue for "on" (spec/00 §12.4): an overlay bar along the bottom edge, so the
        // box keeps its size (§12.2) and the state reads without colour.
        let indicator = on.then(|| {
            let bar = if self.disabled {
                colors.disabled.foreground
            } else {
                colors.selected.foreground
            };
            div()
                .debug_selector(|| "action-icon-toggled".into())
                .absolute()
                .bottom(px(2.))
                .left(px(4.))
                .right(px(4.))
                .h(px(2.))
                .rounded_full()
                .bg(bar.to_hsla())
        });
        let disabled = self.disabled;
        // Loading ignores presses without looking disabled.
        let inert = disabled || loading;
        let on_click = self.on_click;
        let el = div()
            .id(self.id)
            .a11y_disabled(disabled)
            .test_support()
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .rounded(r)
            .border_1()
            .text_size(px(size.icon_size()))
            .when(loading, |el| {
                el.child(gpui_kit::component::Sizable::with_size(
                    gpui_kit::component::spinner::Spinner::new(),
                    gpui_kit::component::Size::Small,
                ))
            })
            .when(!loading, |el| el.child(self.icon.into_element()))
            .when_some(description, |el, d| el.aria_description(d))
            .children(indicator)
            .press(&press, &focus, window)
            .when_some(tooltip, |el, text| {
                el.tooltip(move |window, cx| {
                    let inner = Tooltip::new(text.clone()).build(window, cx);
                    cx.new(|_| TooltipHost(inner)).into()
                })
            })
            .when_some(on_click, |el, f| el.on_press(inert, f));
        crate::style::refine(el, &self.style)
    }
}

/// Hosts the Kit tooltip under a stable debug selector, so tests can see it appear.
struct TooltipHost(gpui_kit::AnyView);

impl gpui_kit::Render for TooltipHost {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "action-icon-tooltip".into())
            .child(self.0.clone())
    }
}

/// The standard × button (dialogs, pills, notifications), named "Close".
pub fn close_button(id: impl Into<ElementId>) -> ActionIcon {
    ActionIcon::new(id, IconName::X)
        .variant(Variant::Subtle)
        .color(ColorName::Gray)
        .size(Size::Sm)
        .label("Close")
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_box_grows_with_size() {
        use guiraffe_core::Size;
        let v: Vec<f32> = Size::ALL.iter().map(|s| super::icon_box(*s)).collect();
        assert!(v.windows(2).all(|w| w[0] < w[1]), "{v:?}");
    }
}
