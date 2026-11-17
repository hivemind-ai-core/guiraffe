//! Shared interaction plumbing for GUIraffe's own controls (spec/00 §12, spec/18).
//!
//! GUIraffe widgets that cannot be a Kit `Button` (custom shapes, segments, chips, list rows)
//! replicate its behavior with [`PressableExt::press`]:
//! - hover / pressed / disabled colors from [`GuiraffeTheme::state`] (never raw opacity),
//! - pointer cursor, or `not-allowed` while disabled (events are the caller's to suppress,
//!   see [`PressableExt::on_press`]),
//! - a pointer press never takes keyboard focus (`prevent_default` on mouse down, like Kit),
//! - opt-in tab stop via [`crate::tab_stop`] and a focus-visible ring,
//! - Enter / Space activation for free: GPUI fires `on_click` with a keyboard
//!   `ClickEvent` when the focused element sees a clean Enter/Space,
//! - an AccessKit role, label and selected / toggled / disabled state.

use gpui_kit::{
    A11ySubtreeBuilder, AnyElement, App, Bounds, BoxShadow, ClickEvent, Element, ElementId,
    FocusHandle, GlobalElementId, InspectorElementId, InteractiveElement, Interactivity,
    IntoElement, LayoutId, MouseButton, ParentElement, Pixels, Role, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Toggled, Window, accesskit, point, px,
};

use crate::{
    Rgb,
    palette::ColorName,
    state::{StateColors, StateKind},
    theme::GuiraffeTheme,
    variant::Variant,
};

/// Every state color a pressable control needs, resolved once per render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressColors {
    pub rest: StateColors,
    pub hover: StateColors,
    pub pressed: StateColors,
    pub selected: StateColors,
    pub disabled: StateColors,
    pub ring: Rgb,
}

impl GuiraffeTheme {
    /// [`PressColors`] for a control of `variant` in `color` (primary when `None`).
    /// Rest and hover keep the variant's own (possibly translucent) colors so the control
    /// still reads correctly on raised surfaces; the other states come from
    /// [`GuiraffeTheme::state`].
    pub fn press_colors(&self, variant: Variant, color: Option<ColorName>) -> PressColors {
        let st = |k| self.state(k, variant, color);
        let v = self.variant(color, variant);
        PressColors {
            rest: StateColors {
                foreground: v.foreground,
                background: v.background,
                border: v.border,
            },
            hover: StateColors {
                foreground: v.foreground,
                background: v.hover,
                border: v.border,
            },
            pressed: st(StateKind::Pressed),
            selected: st(StateKind::Selected),
            disabled: st(StateKind::Disabled),
            ring: st(StateKind::FocusRing).border,
        }
    }
}

/// Focus-visible ring drawn inside the border box (no layout shift), 2px like Kit.
pub fn focus_ring(s: StyleRefinement, ring: Rgb) -> StyleRefinement {
    s.shadow(vec![BoxShadow {
        color: ring.to_hsla(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
        inset: true,
    }])
}

/// How a pressable control behaves and what it tells assistive technology.
#[derive(Debug, Clone)]
pub struct Press {
    pub colors: PressColors,
    pub disabled: bool,
    /// `None`: a tab stop only inside a [`crate::focus_scope`].
    pub focusable: Option<bool>,
    pub role: Role,
    pub label: Option<SharedString>,
    pub selected: Option<bool>,
    pub toggled: Option<bool>,
    /// Paint colors (rest/hover/pressed or selected/disabled). Off for controls that paint
    /// their own surface and only want behavior + semantics.
    pub paint: bool,
}

impl Press {
    pub fn new(colors: PressColors) -> Self {
        Self {
            colors,
            disabled: false,
            focusable: None,
            role: Role::Button,
            label: None,
            selected: None,
            toggled: None,
            paint: true,
        }
    }
    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn focusable(mut self, v: Option<bool>) -> Self {
        self.focusable = v;
        self
    }
    pub fn role(mut self, r: Role) -> Self {
        self.role = r;
        self
    }
    pub fn label(mut self, l: impl Into<SharedString>) -> Self {
        self.label = Some(l.into());
        self
    }
    pub fn selected(mut self, v: bool) -> Self {
        self.selected = Some(v);
        self
    }
    pub fn toggled(mut self, v: bool) -> Self {
        self.toggled = Some(v);
        self
    }
    pub fn paint(mut self, v: bool) -> Self {
        self.paint = v;
        self
    }
    /// Whether the control is in the Tab order right now.
    pub fn is_tab_stop(&self) -> bool {
        !self.disabled && crate::tab_stop(self.focusable)
    }
}

/// A focus handle kept per element id across renders (for `RenderOnce` controls).
pub fn keyed_focus(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(id.into(), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone()
}

pub trait PressableExt: StatefulInteractiveElement + Styled {
    /// Apply Kit-equivalent state feedback, opt-in focus and semantics.
    fn press(mut self, p: &Press, focus: &FocusHandle) -> Self {
        let c = p.colors;
        let h = Rgb::to_hsla;
        let tab = p.is_tab_stop();
        self = self.role(p.role);
        if let Some(l) = &p.label {
            self = self.aria_label(l.clone());
        }
        if let Some(sel) = p.selected {
            self = self.aria_selected(sel);
        }
        if let Some(t) = p.toggled {
            self = self.aria_toggled(if t { Toggled::True } else { Toggled::False });
        }
        if p.disabled {
            if p.paint {
                self = self
                    .bg(h(c.disabled.background))
                    .border_color(h(c.disabled.border))
                    .text_color(h(c.disabled.foreground));
            }
            // Not focusable, and a press does not move focus elsewhere either.
            return self
                .cursor_not_allowed()
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default());
        }
        if p.paint {
            let rest = if p.selected == Some(true) {
                c.selected
            } else {
                c.rest
            };
            self = self
                .bg(h(rest.background))
                .border_color(h(rest.border))
                .text_color(h(rest.foreground));
            if p.selected != Some(true) {
                let (hover, pressed) = (c.hover, c.pressed);
                self = self
                    .hover(move |s| s.bg(h(hover.background)))
                    .active(move |s| s.bg(h(pressed.background)));
            }
        }
        let ring = c.ring;
        self.track_focus(&focus.clone().tab_stop(tab))
            .focus_visible(move |s| focus_ring(s, ring))
            .cursor_pointer()
            // Like Kit's Button: a pointer press never takes keyboard focus.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
    }

    /// `on_click` that does nothing while `disabled` (pointer and Enter/Space alike).
    fn on_press(
        self,
        disabled: bool,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if disabled { self } else { self.on_click(f) }
    }
}

impl<E: StatefulInteractiveElement + Styled> PressableExt for E {}

/// Marks the wrapped element's accessibility node disabled. GPUI 0.3 has no
/// `aria_disabled` setter (Kit's own wrapper is crate-private), so GUIraffe carries this one.
/// Call it right after `.id(..)` and before `.test_support()` so test snapshots see the flag.
pub struct A11yDisabled<E> {
    inner: E,
    disabled: bool,
}

pub trait A11yDisabledExt: Sized {
    fn a11y_disabled(self, disabled: bool) -> A11yDisabled<Self> {
        A11yDisabled {
            inner: self,
            disabled,
        }
    }
}
impl<E: Element + InteractiveElement> A11yDisabledExt for E {}

impl<E: Element> IntoElement for A11yDisabled<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl<E: InteractiveElement> InteractiveElement for A11yDisabled<E> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.inner.interactivity()
    }
}
impl<E: StatefulInteractiveElement> StatefulInteractiveElement for A11yDisabled<E> {}
impl<E: Styled> Styled for A11yDisabled<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}
impl<E: ParentElement> ParentElement for A11yDisabled<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

impl<E: Element> Element for A11yDisabled<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx)
    }
    fn a11y_role(&self) -> Option<Role> {
        self.inner.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        if self.disabled {
            node.set_disabled();
        }
    }
    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_colors_use_state_tokens() {
        let t = GuiraffeTheme::default();
        let c = t.press_colors(Variant::Light, None);
        assert_eq!(
            c.disabled,
            t.state(StateKind::Disabled, Variant::Light, None)
        );
        assert_ne!(c.hover.background, c.pressed.background);
    }

    #[test]
    fn tab_stop_is_opt_in_and_off_when_disabled() {
        let p = Press::new(GuiraffeTheme::default().press_colors(Variant::Light, None));
        assert!(!p.is_tab_stop());
        assert!(p.clone().focusable(Some(true)).is_tab_stop());
        assert!(!p.focusable(Some(true)).disabled(true).is_tab_stop());
    }
}
