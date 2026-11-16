//! Inherited style context (spec/14): containers provide, widgets read.
//!
//! Stateless components and views render during their parent's `request_layout`, so a
//! [`Provide`] element pushes its context on a stack for exactly the extent of laying out its
//! children. Widgets call [`StyleContext::current`] in `render`. Single-threaded (the UI
//! thread), so the stack is a thread-local with no locking.

use std::cell::RefCell;

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Window,
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
}

impl Default for StyleContext {
    fn default() -> Self {
        Self { density: Density::Comfortable, control_size: Size::Sm, surface: SurfaceKind::Body, emphasis: Emphasis::Default }
    }
}

thread_local! {
    static STACK: RefCell<Vec<StyleContext>> = const { RefCell::new(Vec::new()) };
}

impl StyleContext {
    /// The context in effect for the widget currently rendering.
    pub fn current() -> StyleContext {
        STACK.with(|s| s.borrow().last().copied()).unwrap_or_default()
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
        Self { control_size, ..self }
    }
    pub fn with_surface(self, surface: SurfaceKind) -> Self {
        Self { surface, ..self }
    }
    pub fn with_emphasis(self, emphasis: Emphasis) -> Self {
        Self { emphasis, ..self }
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

/// Provide `context` (computed from the parent's) to everything rendered inside `child`.
pub fn provide(f: impl FnOnce(StyleContext) -> StyleContext, child: impl IntoElement) -> Provide {
    Provide { context: f(StyleContext::current()), resolved: None, child: Some(child.into_any_element()), make: None }
}

/// A provider that derives its context lazily at layout time from the enclosing one.
pub fn provide_with(f: impl Fn(StyleContext) -> StyleContext + 'static, child: impl IntoElement) -> Provide {
    Provide { context: StyleContext::default(), resolved: None, child: Some(child.into_any_element()), make: Some(Box::new(f)) }
}

pub struct Provide {
    context: StyleContext,
    resolved: Option<StyleContext>,
    child: Option<AnyElement>,
    make: Option<Box<dyn Fn(StyleContext) -> StyleContext>>,
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

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, AnyElement) {
        let ctx = match &self.make {
            Some(f) => f(StyleContext::current()),
            None => self.context,
        };
        self.resolved = Some(ctx);
        let _guard = Push::new(ctx);
        let mut child = self.child.take().unwrap_or_else(|| gpui_kit::Empty.into_any_element());
        let id = child.request_layout(window, cx);
        (id, child)
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, child: &mut AnyElement, window: &mut Window, cx: &mut App) {
        let _guard = Push::new(self.resolved.unwrap_or(self.context));
        child.prepaint(window, cx);
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, child: &mut AnyElement, _: &mut (), window: &mut Window, cx: &mut App) {
        let _guard = Push::new(self.resolved.unwrap_or(self.context));
        child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_scoped_nesting_restores() {
        assert_eq!(StyleContext::current(), StyleContext::default());
        let toolbar = StyleContext::default().with_control_size(Size::Xs).with_emphasis(Emphasis::Quiet);
        toolbar.scope(|| {
            assert_eq!(StyleContext::current().control_size, Size::Xs);
            StyleContext::current().with_density(Density::Compact).scope(|| {
                let c = StyleContext::current();
                assert_eq!((c.control_size, c.density, c.emphasis), (Size::Xs, Density::Compact, Emphasis::Quiet), "inherits");
            });
            assert_eq!(StyleContext::current().density, Density::Comfortable, "restored after inner scope");
        });
        assert_eq!(StyleContext::current(), StyleContext::default());
    }

    #[test]
    fn panic_inside_scope_restores_stack() {
        let r = std::panic::catch_unwind(|| StyleContext::default().with_control_size(Size::Xl).scope(|| panic!("boom")));
        assert!(r.is_err());
        assert_eq!(StyleContext::current(), StyleContext::default());
    }

    #[test]
    fn density_scales_spacing() {
        assert!(Density::Compact.space(Size::Md) < Density::Comfortable.space(Size::Md));
        assert!(Density::Spacious.space(Size::Md) > Density::Comfortable.space(Size::Md));
    }
}
