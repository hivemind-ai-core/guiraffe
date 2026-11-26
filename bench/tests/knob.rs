//! Knob gates: an unbound knob costs what a plain element of the same shape costs to
//! re-render, and a store-bound knob's binding update allocates nothing.

use std::cell::Cell;

use gpui_kit::{
    AppContext, Context, Entity, HeadlessAppContext, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, WindowHandle, div, px, size,
};
use guiraffe_bench::allocator::{CountingAllocator, count_allocations};
use guiraffe_components::controls::{KnobControl, knob_dial};
use guiraffe_components::mapping::{Curve, Unit, ValueMap, ValueMapping};
use guiraffe_state::{Store, store};
use guiraffe_ui::ui;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn cutoff() -> ValueMap {
    ValueMap::new(20.0, 20_000.0)
        .curve(Curve::Log)
        .unit(Unit::Hertz)
        .default_to(1_000.0)
}

fn app() -> HeadlessAppContext {
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app =
        HeadlessAppContext::with_asset_source(text, std::sync::Arc::new(gpui_kit::assets::Assets));
    app.update(guiraffe_components::init);
    app
}

/// The same element a knob renders, built by hand: a focusable, accessible slider column
/// with the dial and the formatted value.
struct Plain {
    value: f64,
    focus: gpui_kit::FocusHandle,
}

impl Render for Plain {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let map = cutoff();
        let text = gpui_kit::SharedString::from(map.format(self.value));
        div()
            .id("plain-knob")
            .track_focus(&self.focus)
            .role(gpui_kit::Role::Slider)
            .aria_label(text.clone())
            .aria_value(text.clone())
            .aria_numeric_value(self.value)
            .aria_min_numeric_value(map.min())
            .aria_max_numeric_value(map.max())
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .text_xs()
            .on_mouse_down(MouseButton::Left, |_, _, _| {})
            .on_scroll(|_, _, _| {})
            .on_key_down(|_, _, _| {})
            .child(div().relative().child(knob_dial(
                map.to_normal(self.value),
                None,
                None,
                40.,
                cx,
            )))
            .child(text)
    }
}

/// Median allocations of re-rendering `entity` (a notify and a frame).
fn rerender<T: 'static>(
    app: &mut HeadlessAppContext,
    window: WindowHandle<Root>,
    entity: &Entity<T>,
) -> u64 {
    let mut counts = Vec::new();
    for _ in 0..9 {
        let (_, c) = count_allocations(|| {
            app.update_window(window.into(), |_, _, cx| cx.notify(entity.entity_id()))
                .ok();
            app.run_until_parked();
        })
        .expect("scope");
        counts.push(c.allocations);
    }
    counts.sort_unstable();
    counts[4]
}

struct Root {
    knob: Entity<KnobControl>,
    plain: Entity<Plain>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_4()
            .child(self.knob.clone())
            .child(self.plain.clone())
    }
}

#[test]
fn an_unbound_knob_costs_what_the_same_plain_element_costs() {
    let mut app = app();
    let handle = app
        .open_window(size(px(200.), px(100.)), |window, cx| {
            let knob = KnobControl::new().mapping(cutoff()).build(window, cx);
            let plain = cx.new(|cx| Plain {
                value: 1_000.0,
                focus: cx.focus_handle(),
            });
            cx.new(|_| Root { knob, plain })
        })
        .expect("window");
    app.run_until_parked();
    let (knob, plain) = app
        .update_window(handle.into(), |_, window, cx| {
            let root = window.root::<Root>().flatten().expect("root");
            let root = root.read(cx);
            (root.knob.clone(), root.plain.clone())
        })
        .expect("live");
    let plain_cost = rerender(&mut app, handle, &plain);
    let knob_cost = rerender(&mut app, handle, &knob);
    // The knob's own handlers and view: a few closures, nothing that grows.
    assert!(
        knob_cost <= plain_cost + 8,
        "knob {knob_cost} vs plain {plain_cost} allocations per re-render"
    );
}

store! {
    pub Synth {
        cutoff: f64 = 1_000.0,
    }
}

thread_local! {
    static PAGE: Cell<usize> = const { Cell::new(0) };
}

struct Page {
    store: Store<Synth>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        PAGE.with(|p| p.set(p.get() + 1));
        let store = self.store.clone();
        let map = cutoff();
        ui! {
            div {
                store(store) {
                    KnobControl {
                        value <=> cutoff
                        .mapping(map)
                        .path("synth/cutoff")
                    }
                }
            }
        }
    }
}

#[test]
fn a_bound_knob_updates_without_allocating() {
    let mut app = app();
    let store = app.update(Synth::store);
    let handle = app
        .open_window(size(px(200.), px(100.)), {
            let store = store.clone();
            move |_, cx| cx.new(|_| Page { store })
        })
        .expect("window");
    app.run_until_parked();
    let write = |app: &mut HeadlessAppContext, value: f64| {
        app.update_window(handle.into(), |_, _, cx| {
            count_allocations(|| store.set(Synth::cutoff, value, cx))
                .expect("scope")
                .1
                .allocations
        })
        .expect("live")
    };
    for i in 0..4 {
        write(&mut app, 500.0 + f64::from(i));
        app.run_until_parked();
    }
    let mut counts: Vec<u64> = (0..7)
        .map(|i| {
            let n = write(&mut app, 2_000.0 + f64::from(i));
            app.run_until_parked();
            n
        })
        .collect();
    counts.sort_unstable();
    assert_eq!(counts[3], 0, "binding update allocations: {counts:?}");
    let shown = app
        .update_window(handle.into(), |_, window, cx| {
            let id =
                guiraffe_core::protocol::ControlId::lookup("synth/cutoff", cx).expect("registered");
            guiraffe_core::protocol::ActionBus::query(id, window, cx)
                .ok()
                .and_then(|s| s.value)
        })
        .expect("live");
    assert_eq!(
        shown,
        Some(guiraffe_core::widget_value::Value::Num(2_006.0))
    );
}
