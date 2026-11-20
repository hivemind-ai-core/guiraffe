use std::{cell::RefCell, rc::Rc};

use gpui_kit::{App, EntityId, StyleRefinement, TestAppContext, Window, div, prelude::*, px};
use guiraffe_test::render::RenderCounter;
use guiraffe_ui::{component, ui};

#[component]
fn Label(text: &'static str, #[prop(default)] bold: bool) -> impl IntoElement {
    ui! { div { .when(bold, |node| node.font_weight(gpui_kit::FontWeight::BOLD)) {text} } }
}

struct Layout;

impl Render for Layout {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let fragment = ui! { div { w: px(70.) } div { w: px(30.) } };
        ui! {
            div {
                flex
                div { flex flex_col .debug_selector(|| "macro".to_owned())
                    Label("One") { bold: true }
                    if true { Label("Two") }
                    div { flex .children(fragment) }
                }
                {div().flex().flex_col().debug_selector(|| "builder".to_owned())
                    .child(Label::new("One").bold(true))
                    .children(Some(Label::new("Two")))
                    .child(div().flex().child(div().w(px(70.))).child(div().w(px(30.))))}
            }
        }
    }
}

#[gpui_kit::test]
fn layout_matches_builder(cx: &mut TestAppContext) {
    let (_, window) = cx.add_window_view(|_, _| Layout);
    window.run_until_parked();
    let macro_bounds = window.debug_bounds("macro").expect("macro bounds");
    let builder_bounds = window.debug_bounds("builder").expect("builder bounds");
    assert_eq!(macro_bounds.size, builder_bounds.size);
    assert_eq!(macro_bounds.right(), builder_bounds.left());
}

type Seen = Rc<RefCell<Vec<(usize, EntityId)>>>;

struct RowProbe(usize, Seen);

impl IntoElement for RowProbe {
    type Element = gpui_kit::ViewElement<Self>;

    fn into_element(self) -> Self::Element {
        gpui_kit::ViewElement::new(self)
    }
}

impl RenderOnce for RowProbe {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state("same-row-local-id", cx, |_, _| ());
        self.1.borrow_mut().push((self.0, state.entity_id()));
        div().h(px(24.))
    }
}

impl RowProbe {
    fn new(row: usize, seen: Seen) -> Self {
        Self(row, seen)
    }
}

struct Rows([usize; 2], Seen);

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui! {
            div {
                flex flex_col id: "rows"
                for row in self.0 key row { RowProbe(row, self.1.clone()) }
            }
        }
    }
}

#[gpui_kit::test]
fn keyed_rows_retain_state_after_reorder(cx: &mut TestAppContext) {
    let seen = Seen::default();
    let (rows, window) = cx.add_window_view(|_, _| Rows([1, 2], seen.clone()));
    let draw = |window: &mut gpui_kit::VisualTestContext| {
        window.update(|window, cx| window.draw(cx).clear(cx));
        std::mem::take(&mut *seen.borrow_mut())
    };
    seen.borrow_mut().clear();
    let initial = draw(window);
    assert_eq!(initial.len(), 2);
    assert_ne!(initial[0].1, initial[1].1);
    window.update(|_, cx| {
        rows.update(cx, |rows, cx| {
            rows.0.reverse();
            cx.notify();
        })
    });
    seen.borrow_mut().clear();
    let reordered = draw(window);
    assert_eq!(reordered, [initial[1], initial[0]]);
}

struct Leaf(RenderCounter);
impl Render for Leaf {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.hit();
        ui! { div { h: px(30.) "Leaf" } }
    }
}

struct Host {
    leaf: gpui_kit::Entity<Leaf>,
    host: RenderCounter,
    cache: RenderCounter,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.host.hit();
        let counter = self.cache.clone();
        ui! {
            div {
                size_full flex flex_col
                view(self.leaf.clone())
                cached(StyleRefinement::default().w(px(200.)).h(px(40.))) {
                    { counter.hit(); div().size_full().child("Cached") }
                }
            }
        }
    }
}

#[gpui_kit::test]
fn cached_boundary_and_view_notifications(cx: &mut TestAppContext) {
    let host_count = RenderCounter::new();
    let leaf_count = RenderCounter::new();
    let cache_count = RenderCounter::new();
    let leaf = cx.new(|_| Leaf(leaf_count.clone()));
    let (host, window) = cx.add_window_view(|_, _| Host {
        leaf: leaf.clone(),
        host: host_count.clone(),
        cache: cache_count.clone(),
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    let initial = (host_count.get(), cache_count.get());
    assert!(initial.0 > 0 && initial.1 > 0);

    window.update(|_, cx| cx.notify(host.entity_id()));
    window.run_until_parked();
    assert!(host_count.get() > initial.0);
    assert_eq!(cache_count.get(), initial.1);

    let before = (host_count.get(), leaf_count.get());
    window.update(|_, cx| cx.notify(leaf.entity_id()));
    window.run_until_parked();
    assert!(leaf_count.get() > before.1);
    if gpui_kit::GPUI_FAST {
        assert_eq!(host_count.get(), before.0);
    }
    assert_eq!(cache_count.get(), initial.1);
}
