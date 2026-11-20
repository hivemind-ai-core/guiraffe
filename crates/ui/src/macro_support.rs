use std::{cell::RefCell, panic::Location};

use gpui_kit::{
    AnyElement, AnyView, App, Bounds, Context, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Render, RenderOnce, StyleRefinement,
    ViewElement, Window,
};

/// A layout-transparent scope for the global IDs of the row's children.
pub struct Keyed {
    key: ElementId,
    child: AnyElement,
    location: Option<&'static Location<'static>>,
}

pub fn keyed<E: IntoElement>(key: impl Into<ElementId>, child: E) -> Keyed {
    let child = child.into_element();
    Keyed {
        key: key.into(),
        location: child.source_location(),
        child: child.into_any(),
    }
}

impl IntoElement for Keyed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Keyed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        self.location
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Move the child into its normal GPUI drawable so it retains its own
        // ID, accessibility node and rendering behavior under this scope.
        let layout = window.with_element_namespace(self.key.clone(), |window| {
            self.child.request_layout(window, cx)
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_namespace(self.key.clone(), |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_element_namespace(self.key.clone(), |window| self.child.paint(window, cx));
    }
}

struct CacheView<F> {
    body: RefCell<Option<F>>,
}

impl<F, E> Render for CacheView<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.body.get_mut().as_ref().map_or_else(
            || gpui_kit::Empty.into_any_element(),
            |body| body().into_any_element(),
        )
    }
}

pub struct Cached<F> {
    id: ElementId,
    style: StyleRefinement,
    body: F,
}

#[track_caller]
pub fn cached<F, E>(style: StyleRefinement, body: F) -> Cached<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    Cached {
        id: ElementId::CodeLocation(*Location::caller()),
        style,
        body,
    }
}

impl<F, E> IntoElement for Cached<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    type Element = ViewElement<Self>;

    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }
}

impl<F, E> RenderOnce for Cached<F>
where
    F: Fn() -> E + 'static,
    E: IntoElement,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let entity = window.use_keyed_state(self.id, cx, |_, _| CacheView {
            body: RefCell::new(None),
        });
        // Supply the latest closure without marking the cache entity dirty.
        *entity.read(cx).body.borrow_mut() = Some(self.body);
        AnyView::from(entity).cached(self.style)
    }
}
