//! Inherited style context (spec/14): containers provide, widgets read.
//!
//! Stateless components and views render during their parent's `request_layout`, so a
//! [`Provide`] element pushes its context on a stack for exactly the extent of laying out its
//! children. Widgets call [`StyleContext::current`] in `render`. Single-threaded (the UI
//! thread), so the stack is a thread-local with no locking.
//!
//! gpui-fast draws a notified view again on its own, without running the views and elements
//! around it, so no provider is on the stack then. The context is therefore also handed to the
//! backend as one word ([`gpui_kit::InheritedState`]): it captures the word where each view is
//! laid out, pushes it back while the view is built again there, and lays a retained view out
//! afresh when the context around it changed. Upstream GPUI renders a view inside its
//! ancestors, so it never needs the word.

use std::cell::{Cell, RefCell};

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

use crate::tokens::Size;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

impl Density {
    /// Scale a spacing token for this density.
    pub fn space(self, size: Size) -> f32 {
        let base = size.spacing();
        match self {
            Density::Compact => (base * 0.75).round(),
            Density::Comfortable => base,
            Density::Spacious => (base * 1.25).round(),
        }
    }
}

/// What a widget is drawn on (drives text, border and hover colours for contrast).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceKind {
    #[default]
    Body,
    Raised,
    Sunken,
}

/// Default emphasis for interactive widgets in this region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Emphasis {
    #[default]
    Default,
    /// Toolbars, table rows: widgets default to quiet (subtle) variants.
    Quiet,
}

/// The inherited style context. `Copy` and tiny, so reading it is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleContext {
    pub density: Density,
    pub control_size: Size,
    pub surface: SurfaceKind,
    pub emphasis: Emphasis,
    /// Inside a focus scope (form): non-text controls join Tab navigation (spec/00 §12.3).
    /// Outside one they never take keyboard focus.
    pub keyboard_nav: bool,
}

impl Default for StyleContext {
    fn default() -> Self {
        Self {
            density: Density::Comfortable,
            control_size: Size::Sm,
            surface: SurfaceKind::Body,
            emphasis: Emphasis::Default,
            keyboard_nav: false,
        }
    }
}

thread_local! {
    static STACK: RefCell<Vec<StyleContext>> = const { RefCell::new(Vec::new()) };
    static CARRIED: Cell<bool> = const { Cell::new(false) };
}

/// Hands the context to the backend, so that a view built again on its own reads the context
/// it was laid out in (see the module docs). Called by the first provider laid out, which is
/// before any view it encloses is laid out.
fn carry_context() {
    if !CARRIED.replace(true) {
        gpui_kit::set_inherited_state(&Carried);
    }
}

struct Carried;

impl gpui_kit::InheritedState for Carried {
    fn capture(&self) -> u64 {
        StyleContext::current().to_word()
    }
    fn enter(&self, state: u64) {
        STACK.with(|s| s.borrow_mut().push(StyleContext::from_word(state)));
    }
    fn leave(&self) {
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

impl StyleContext {
    /// The context in effect for the widget currently rendering.
    pub fn current() -> StyleContext {
        STACK
            .with(|s| s.borrow().last().copied())
            .unwrap_or_default()
    }

    /// Run `f` with `self` as the current context (for code that builds elements eagerly).
    pub fn scope<R>(self, f: impl FnOnce() -> R) -> R {
        let _guard = Push::new(self);
        f()
    }

    pub fn with_density(self, density: Density) -> Self {
        Self { density, ..self }
    }
    pub fn with_control_size(self, control_size: Size) -> Self {
        Self {
            control_size,
            ..self
        }
    }
    pub fn with_surface(self, surface: SurfaceKind) -> Self {
        Self { surface, ..self }
    }
    pub fn with_emphasis(self, emphasis: Emphasis) -> Self {
        Self { emphasis, ..self }
    }

    /// The context as one word, `0` for the default (what the backend captures when nothing
    /// is inherited).
    fn to_word(self) -> u64 {
        let density = match self.density {
            Density::Comfortable => 0,
            Density::Compact => 1,
            Density::Spacious => 2,
        };
        let size = match self.control_size {
            Size::Sm => 0,
            Size::Xs => 1,
            Size::Md => 2,
            Size::Lg => 3,
            Size::Xl => 4,
        };
        let surface = match self.surface {
            SurfaceKind::Body => 0,
            SurfaceKind::Raised => 1,
            SurfaceKind::Sunken => 2,
        };
        let emphasis = match self.emphasis {
            Emphasis::Default => 0,
            Emphasis::Quiet => 1,
        };
        density | size << 2 | surface << 5 | emphasis << 7 | u64::from(self.keyboard_nav) << 8
    }

    /// The context [`StyleContext::to_word`] gave `word`; unknown bits read as the default.
    fn from_word(word: u64) -> Self {
        let d = StyleContext::default();
        StyleContext {
            density: match word & 0b11 {
                1 => Density::Compact,
                2 => Density::Spacious,
                _ => d.density,
            },
            control_size: match word >> 2 & 0b111 {
                1 => Size::Xs,
                2 => Size::Md,
                3 => Size::Lg,
                4 => Size::Xl,
                _ => d.control_size,
            },
            surface: match word >> 5 & 0b11 {
                1 => SurfaceKind::Raised,
                2 => SurfaceKind::Sunken,
                _ => d.surface,
            },
            emphasis: match word >> 7 & 1 {
                1 => Emphasis::Quiet,
                _ => d.emphasis,
            },
            keyboard_nav: word >> 8 & 1 == 1,
        }
    }
}

/// Pops on drop, so a panic while laying out children never leaves a stale context.
struct Push;

impl Push {
    fn new(ctx: StyleContext) -> Self {
        STACK.with(|s| s.borrow_mut().push(ctx));
        Push
    }
}

impl Drop for Push {
    fn drop(&mut self) {
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

/// A partial override of the inherited context. Applied at *layout time* to whatever context
/// encloses the provider, so nested providers compose correctly. `Copy`, no allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StylePatch {
    pub density: Option<Density>,
    pub control_size: Option<Size>,
    pub surface: Option<SurfaceKind>,
    pub emphasis: Option<Emphasis>,
    pub keyboard_nav: Option<bool>,
}

impl StylePatch {
    pub const fn new() -> Self {
        Self {
            density: None,
            control_size: None,
            surface: None,
            emphasis: None,
            keyboard_nav: None,
        }
    }
    pub const fn density(mut self, v: Density) -> Self {
        self.density = Some(v);
        self
    }
    pub const fn control_size(mut self, v: Size) -> Self {
        self.control_size = Some(v);
        self
    }
    pub const fn surface(mut self, v: SurfaceKind) -> Self {
        self.surface = Some(v);
        self
    }
    pub const fn emphasis(mut self, v: Emphasis) -> Self {
        self.emphasis = Some(v);
        self
    }
    pub const fn keyboard_nav(mut self, v: bool) -> Self {
        self.keyboard_nav = Some(v);
        self
    }
    /// Apply on top of `base`.
    pub fn apply(self, base: StyleContext) -> StyleContext {
        StyleContext {
            density: self.density.unwrap_or(base.density),
            control_size: self.control_size.unwrap_or(base.control_size),
            surface: self.surface.unwrap_or(base.surface),
            emphasis: self.emphasis.unwrap_or(base.emphasis),
            keyboard_nav: self.keyboard_nav.unwrap_or(base.keyboard_nav),
        }
    }
}

/// Provide `patch` (over the enclosing context) to everything rendered inside `child`.
pub fn provide(patch: StylePatch, child: impl IntoElement) -> Provide {
    Provide {
        patch,
        resolved: None,
        child: Some(child.into_any_element()),
    }
}

pub struct Provide {
    patch: StylePatch,
    resolved: Option<StyleContext>,
    child: Option<AnyElement>,
}

impl IntoElement for Provide {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Provide {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        carry_context();
        let ctx = self.patch.apply(StyleContext::current());
        self.resolved = Some(ctx);
        let _guard = Push::new(ctx);
        let mut child = self
            .child
            .take()
            .unwrap_or_else(|| gpui_kit::Empty.into_any_element());
        let id = child.request_layout(window, cx);
        (id, child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        let _guard = Push::new(
            self.resolved
                .unwrap_or_else(|| self.patch.apply(StyleContext::current())),
        );
        child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let _guard = Push::new(
            self.resolved
                .unwrap_or_else(|| self.patch.apply(StyleContext::current())),
        );
        child.paint(window, cx);
    }
}

/// Opt a subtree into Tab navigation (a form, dialog or settings page): non-text controls
/// inside become tab stops and show focus rings for keyboard focus. Outside a scope,
/// clicking a button, segment or chip never takes keyboard focus (spec/00 §12.3).
pub fn focus_scope(child: impl gpui_kit::IntoElement) -> Provide {
    provide(StylePatch::new().keyboard_nav(true), child)
}

/// Whether a non-text control rendered now should be a tab stop: true inside a
/// [`focus_scope`], or when the control opts in itself (`explicit`).
pub fn tab_stop(explicit: Option<bool>) -> bool {
    explicit.unwrap_or(StyleContext::current().keyboard_nav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_scoped_nesting_restores() {
        assert_eq!(StyleContext::current(), StyleContext::default());
        let toolbar = StyleContext::default()
            .with_control_size(Size::Xs)
            .with_emphasis(Emphasis::Quiet);
        toolbar.scope(|| {
            assert_eq!(StyleContext::current().control_size, Size::Xs);
            StyleContext::current()
                .with_density(Density::Compact)
                .scope(|| {
                    let c = StyleContext::current();
                    assert_eq!(
                        (c.control_size, c.density, c.emphasis),
                        (Size::Xs, Density::Compact, Emphasis::Quiet),
                        "inherits"
                    );
                });
            assert_eq!(
                StyleContext::current().density,
                Density::Comfortable,
                "restored after inner scope"
            );
        });
        assert_eq!(StyleContext::current(), StyleContext::default());
    }

    #[test]
    fn panic_inside_scope_restores_stack() {
        let r = std::panic::catch_unwind(|| {
            StyleContext::default()
                .with_control_size(Size::Xl)
                .scope(|| panic!("boom"))
        });
        assert!(r.is_err());
        assert_eq!(StyleContext::current(), StyleContext::default());
    }

    #[test]
    fn patches_compose_over_the_enclosing_context() {
        let outer = StylePatch::new()
            .control_size(Size::Xs)
            .emphasis(Emphasis::Quiet)
            .apply(StyleContext::default());
        let inner = StylePatch::new().density(Density::Compact).apply(outer);
        assert_eq!(
            (inner.control_size, inner.emphasis, inner.density),
            (Size::Xs, Emphasis::Quiet, Density::Compact)
        );
        assert_eq!(
            StylePatch::new().apply(outer),
            outer,
            "empty patch is identity"
        );
    }

    #[test]
    fn words_round_trip_and_default_is_zero() {
        assert_eq!(StyleContext::default().to_word(), 0);
        assert_eq!(StyleContext::from_word(0), StyleContext::default());
        let mut seen = std::collections::HashSet::new();
        for density in [Density::Compact, Density::Comfortable, Density::Spacious] {
            for control_size in Size::ALL {
                for surface in [SurfaceKind::Body, SurfaceKind::Raised, SurfaceKind::Sunken] {
                    for emphasis in [Emphasis::Default, Emphasis::Quiet] {
                        for keyboard_nav in [false, true] {
                            let c = StyleContext {
                                density,
                                control_size,
                                surface,
                                emphasis,
                                keyboard_nav,
                            };
                            let w = c.to_word();
                            assert!(seen.insert(w), "{c:?} has its own word");
                            assert_eq!(StyleContext::from_word(w), c);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn carried_context_enters_and_leaves_over_the_stack() {
        use gpui_kit::InheritedState;
        let carried = StyleContext::default()
            .with_control_size(Size::Lg)
            .with_density(Density::Compact);
        StyleContext::default()
            .with_surface(SurfaceKind::Raised)
            .scope(|| {
                let outer = Carried.capture();
                Carried.enter(carried.to_word());
                assert_eq!(StyleContext::current(), carried, "replaces, not patches");
                assert_eq!(Carried.capture(), carried.to_word());
                Carried.leave();
                assert_eq!(Carried.capture(), outer);
            });
        assert_eq!(Carried.capture(), 0);
    }

    #[test]
    fn density_scales_spacing() {
        assert!(Density::Compact.space(Size::Md) < Density::Comfortable.space(Size::Md));
        assert!(Density::Spacious.space(Size::Md) > Density::Comfortable.space(Size::Md));
    }
}
