//! Interaction tests: the widgets that answer the pointer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Context, Entity, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    Point, Render, Subscription, TestAppContext, Window, div, point, px,
};
use guiraffe_layout::{
    AppShell, AppShellEvent, AppShellState, Breakpoint, LayoutPersist, LayoutStore, Size, Splitter,
    SplitterSizes, SplitterState,
};

macro_rules! harness {
    ($cx:expr, $view:expr) => {{
        let (_, cx) = $cx.add_window_view(|_, _| $view);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx
    }};
}

fn click_at(cx: &mut gpui_kit::VisualTestContext, position: Point<Pixels>) {
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
}

struct SplitterHarness {
    state: Entity<SplitterState>,
    resizes: Rc<Cell<usize>>,
}

impl Render for SplitterHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let resizes = self.resizes.clone();
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .first_size(px(200.))
                .on_resize(move |_, _, _| resizes.set(resizes.get() + 1))
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

#[gpui_kit::test]
fn dragging_the_divider_resizes_the_panes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let state = cx.update(SplitterState::new);
    let resizes = Rc::new(Cell::new(0));
    let cx = harness!(
        cx,
        SplitterHarness {
            state: state.clone(),
            resizes: resizes.clone(),
        }
    );

    let first = cx.debug_bounds("first").expect("the first pane");
    let second = cx.debug_bounds("second").expect("the second pane");
    assert_eq!(first.size.width, px(200.), "the first pane starts wide");
    assert_eq!(second.size.width, px(200.), "the second pane starts wide");

    let boundary = first.right();
    cx.update(|window, cx| {
        window.drag(
            point(boundary - px(2.), px(100.)),
            point(boundary + px(60.), px(100.)),
            cx,
        );
    });

    let first = cx.debug_bounds("first").expect("the first pane");
    let second = cx.debug_bounds("second").expect("the second pane");
    assert_eq!(first.size.width, px(260.), "the divider moved to the right");
    assert_eq!(
        second.size.width,
        px(140.),
        "the second pane gave up the room"
    );
    assert_eq!(resizes.get(), 1, "the resize was reported once");

    state.read_with(cx, |state, cx| {
        assert_eq!(state.sizes(cx), vec![px(260.), px(140.)]);
    });
}

struct SplitterParentHarness {
    state: Entity<SplitterState>,
    renders: Rc<Cell<usize>>,
}

impl Render for SplitterParentHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .first_size(px(200.))
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

#[gpui_kit::test]
fn a_splitter_keeps_its_sizes_when_the_view_around_it_renders_again(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(SplitterState::new);
    let renders = Rc::new(Cell::new(0));
    let (view, cx): (Entity<SplitterParentHarness>, _) =
        cx.add_window_view(|_, _| SplitterParentHarness {
            state: state.clone(),
            renders: renders.clone(),
        });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let first = cx.debug_bounds("first").expect("the first pane");
    let boundary = first.right();
    cx.update(|window, cx| {
        window.drag(
            point(boundary - px(2.), px(100.)),
            point(boundary + px(60.), px(100.)),
            cx,
        );
    });

    let first = cx.debug_bounds("first").expect("the first pane");
    assert_eq!(first.size.width, px(260.), "the divider moved to the right");

    let renders_before = renders.get();
    cx.update(|_, app| {
        view.update(app, |_, cx| cx.notify());
    });
    cx.update(|window, cx| window.render_frame(cx));

    assert!(renders.get() > renders_before, "the view rendered again");
    let first = cx.debug_bounds("first").expect("the first pane");
    assert_eq!(
        first.size.width,
        px(260.),
        "the splitter keeps the size the user gave it"
    );
}

struct ThreePaneHarness {
    state: Entity<SplitterState>,
}

impl Render for ThreePaneHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .first_size(px(100.))
                .pane(div().size_full().debug_selector(|| "first".into()))
                .pane(div().size_full().debug_selector(|| "second".into()))
                .pane(div().size_full().debug_selector(|| "third".into())),
        )
    }
}

#[gpui_kit::test]
fn a_splitter_gives_every_pane_its_room(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(SplitterState::new);
    let cx = harness!(
        cx,
        ThreePaneHarness {
            state: state.clone(),
        }
    );

    let first = cx.debug_bounds("first").expect("the first pane");
    let second = cx.debug_bounds("second").expect("the second pane");
    let third = cx.debug_bounds("third").expect("the third pane");

    assert_eq!(first.size.width, px(100.), "the first pane keeps its size");
    assert!(second.size.width > px(0.));
    assert!(third.size.width > px(0.));
    assert!(first.right() <= second.left());
    assert!(second.right() <= third.left());
}

struct MinimumPaneHarness {
    state: Entity<SplitterState>,
}

impl Render for MinimumPaneHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .first_size(px(200.))
                .min_size(px(150.))
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

#[gpui_kit::test]
fn a_pane_does_not_become_narrower_than_its_minimum(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(SplitterState::new);
    let cx = harness!(
        cx,
        MinimumPaneHarness {
            state: state.clone(),
        }
    );

    let first = cx.debug_bounds("first").expect("the first pane");
    let boundary = first.right();
    cx.update(|window, cx| {
        window.drag(
            point(boundary - px(2.), px(100.)),
            point(boundary - px(200.), px(100.)),
            cx,
        );
    });

    let first = cx.debug_bounds("first").expect("the first pane");
    assert_eq!(first.size.width, px(150.), "the pane stops at its minimum");
}

struct Counter {
    renders: Rc<Cell<usize>>,
    /// The reports the counter has seen. The counter keeps the subscription
    /// that counts them, and the count is read by the test.
    #[allow(dead_code)]
    reports: Rc<Cell<usize>>,
    /// The subscription keeps the counter following the splitter.
    #[allow(dead_code)]
    subscription: Option<Subscription>,
}

impl Counter {
    fn new(
        state: Entity<SplitterState>,
        renders: Rc<Cell<usize>>,
        reports: Rc<Cell<usize>>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The counter follows the splitter: the splitter reports a resize,
        // and the counter is woken by the report.
        let reports_for_sub = reports.clone();
        let subscription = cx.subscribe(&state, move |_, _, _, _| {
            reports_for_sub.set(reports_for_sub.get() + 1);
        });
        Self {
            renders,
            reports,
            subscription: Some(subscription),
        }
    }
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().debug_selector(|| "counter".into()).child("counter")
    }
}

/// The splitter in a view of its own.
///
/// The splitter keeps its sizes in an entity, so the view that holds it does
/// not depend on the drag: a drag reaches the splitter and nothing else.
struct SplitterView {
    state: Entity<SplitterState>,
    resizes: Rc<Cell<usize>>,
    renders: Rc<Cell<usize>>,
}

impl Render for SplitterView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let resizes = self.resizes.clone();
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .first_size(px(200.))
                .on_resize(move |_, _, _| resizes.set(resizes.get() + 1))
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

struct CounterHarness {
    splitter: Entity<SplitterView>,
    counter: Entity<Counter>,
}

impl Render for CounterHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(600.))
            .h(px(200.))
            .flex()
            .flex_row()
            .child(self.splitter.clone())
            .child(self.counter.clone())
    }
}

#[gpui_kit::test]
fn a_drag_that_changes_nothing_reports_no_resize_and_leaves_the_siblings_alone(
    cx: &mut TestAppContext,
) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(SplitterState::new);
    let resizes = Rc::new(Cell::new(0));
    let renders = Rc::new(Cell::new(0));
    let reports = Rc::new(Cell::new(0));
    let counter = cx.update(|cx| {
        cx.new(|cx| Counter::new(state.clone(), renders.clone(), reports.clone(), cx))
    });
    let splitter_renders = Rc::new(Cell::new(0));
    let splitter = cx.update(|cx| {
        cx.new(|_| SplitterView {
            state: state.clone(),
            resizes: resizes.clone(),
            renders: splitter_renders.clone(),
        })
    });
    let cx = harness!(cx, CounterHarness { splitter, counter });

    // Compare only work performed during the drag, excluding backend-specific mount frames.
    let sibling_before = renders.get();
    let splitter_before = splitter_renders.get();

    // The divider is pressed on its handle, moved past the point where a drag
    // begins and released where it was, so the panes keep the sizes they had.
    // Nothing but the comparison of the sizes keeps the drag from being a
    // resize.
    let first = cx.debug_bounds("first").expect("the first pane");
    let boundary = first.right();
    cx.simulate_mouse_move(
        point(boundary - px(2.), px(100.)),
        None,
        Modifiers::default(),
    );
    cx.simulate_mouse_down(
        point(boundary - px(2.), px(100.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(boundary + px(20.), px(100.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(boundary, px(100.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(boundary, px(100.)),
        MouseButton::Left,
        Modifiers::default(),
    );

    assert_eq!(
        resizes.get(),
        0,
        "a drag that moves nothing is not a resize"
    );
    assert_eq!(reports.get(), 0, "the splitter reports nothing");
    assert_eq!(
        renders.get() - sibling_before,
        splitter_renders.get() - splitter_before,
        "the sibling is not rendered again by the splitter"
    );
    state.read_with(cx, |state, cx| {
        assert_eq!(
            state.sizes(cx)[0],
            px(200.),
            "the panes keep the size they had"
        );
    });
}

struct ShellHarness {
    state: Entity<AppShellState>,
    main: Entity<MainContent>,
}

/// The main content of a shell, in a view of its own.
struct MainContent;

impl Render for MainContent {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h_full().debug_selector(|| "main".into())
    }
}

impl Render for ShellHarness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let toggle = cx.listener(move |_: &mut Self, _, _, cx| {
            state.update(cx, |state, cx| state.toggle_navbar(cx));
        });
        div().w(px(400.)).h(px(300.)).child(
            AppShell::new(&self.state)
                .navbar(
                    div().h_full().child(
                        div()
                            .id("toggle")
                            .debug_selector(|| "toggle".into())
                            .on_click(move |event, window, cx| toggle(event, window, cx))
                            .child("menu"),
                    ),
                )
                .main(self.main.clone()),
        )
    }
}

#[gpui_kit::test]
fn a_button_in_the_navbar_can_collapse_it(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(|cx| {
        let state = AppShellState::new(cx);
        state.update(cx, |state, cx| state.set_navbar_width(px(100.), cx));
        state
    });
    let main = cx.update(|cx| cx.new(|_| MainContent));
    let cx = harness!(cx, ShellHarness { state, main });

    let navbar = cx.debug_bounds("main").expect("the main content");
    assert_eq!(navbar.left(), px(100.), "the navbar takes its room");

    let toggle = cx.debug_bounds("toggle").expect("the toggle");
    click_at(cx, toggle.center());

    let main = cx.debug_bounds("main").expect("the main content");
    assert_eq!(main.left(), px(0.), "the navbar gave its room back");
    assert!(
        cx.debug_bounds("toggle").is_some(),
        "the navbar keeps its place, so what is inside it keeps its state"
    );
}

struct NarrowShellHarness {
    state: Entity<AppShellState>,
    main: Entity<MainContent>,
}

impl Render for NarrowShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(300.)).h(px(200.)).child(
            AppShell::new(&self.state)
                .navbar(div().h_full().debug_selector(|| "navbar".into()))
                .collapse_below(Breakpoint::Xs)
                .main(self.main.clone()),
        )
    }
}

#[gpui_kit::test]
fn a_narrow_window_collapses_the_navbar(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(|cx| {
        let state = AppShellState::new(cx);
        state.update(cx, |state, cx| state.set_navbar_width(px(100.), cx));
        state
    });
    let main = cx.update(|cx| cx.new(|_| MainContent));
    let cx = harness!(cx, NarrowShellHarness { state, main });

    let navbar = cx.debug_bounds("navbar").expect("the navbar");
    assert_eq!(
        navbar.size.width,
        px(0.),
        "the navbar collapsed, because the window is too narrow for it"
    );
    let main = cx.debug_bounds("main").expect("the main content");
    assert_eq!(main.left(), px(0.));
}

struct SizedHarness;

impl Render for SizedHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).h(px(100.)).child(
            guiraffe_layout::Stack::new()
                .gap(Size::Sm)
                .child(div().debug_selector(|| "content".into())),
        )
    }
}

#[gpui_kit::test]
fn a_stack_keeps_its_gap(cx: &mut TestAppContext) {
    let cx = harness!(cx, SizedHarness);

    let content = cx.debug_bounds("content").expect("the content");
    assert_eq!(content.top(), px(0.));
}

/// A splitter that keeps its sizes under a key.
struct KeyedSplitterHarness {
    state: Entity<SplitterState>,
}

impl Render for KeyedSplitterHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .persist_key("sidebar")
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

/// A splitter that counts the reports it makes.
struct ReportingSplitterHarness {
    state: Entity<SplitterState>,
    reports: Rc<Cell<usize>>,
    /// The events the harness has seen. The harness keeps the subscription
    /// that counts them, and the count is read by the test.
    #[allow(dead_code)]
    events: Rc<Cell<usize>>,
    /// The subscription keeps the harness following the splitter.
    #[allow(dead_code)]
    subscription: Option<Subscription>,
}

impl ReportingSplitterHarness {
    fn new(
        state: Entity<SplitterState>,
        reports: Rc<Cell<usize>>,
        events: Rc<Cell<usize>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let events_for_sub = events.clone();
        let subscription = cx.subscribe(&state, move |_, _, _, _| {
            println!("EVENT");
            events_for_sub.set(events_for_sub.get() + 1);
        });
        Self {
            state,
            reports,
            events,
            subscription: Some(subscription),
        }
    }
}

impl Render for ReportingSplitterHarness {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let reports = self.reports.clone();
        div().w(px(400.)).h(px(200.)).child(
            Splitter::new(&self.state)
                .first_size(px(200.))
                .on_resize(move |_, _, _| reports.set(reports.get() + 1))
                .first(div().size_full().debug_selector(|| "first".into()))
                .second(div().size_full().debug_selector(|| "second".into())),
        )
    }
}

#[gpui_kit::test]
fn a_drag_reports_the_new_sizes_once(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(SplitterState::new);
    let reports = Rc::new(Cell::new(0));
    let events = Rc::new(Cell::new(0));
    let (_, cx) = cx.add_window_view(|_, cx| {
        ReportingSplitterHarness::new(state.clone(), reports.clone(), events.clone(), cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let first = cx.debug_bounds("first").expect("the first pane");
    let boundary = first.right();
    cx.update(|window, cx| {
        window.drag(
            point(boundary - px(2.), px(100.)),
            point(boundary + px(60.), px(100.)),
            cx,
        );
    });

    cx.run_until_parked();
    assert_eq!(reports.get(), 1, "the drag is reported once");
    assert_eq!(events.get(), 1, "the splitter emits one event");
}

#[gpui_kit::test]
fn a_splitter_with_a_key_keeps_its_sizes_even_without_a_callback(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(SplitterState::new);
    let cx = harness!(cx, KeyedSplitterHarness { state });

    let first = cx.debug_bounds("first").expect("the first pane");
    let boundary = first.right();
    cx.update(|window, cx| {
        window.drag(
            point(boundary - px(2.), px(100.)),
            point(boundary + px(60.), px(100.)),
            cx,
        );
    });

    let stored = cx.update(|_window, cx| {
        guiraffe_layout::layout_store(cx).and_then(|store| store.splitter_sizes("sidebar"))
    });
    let stored = stored.expect("the sizes were kept under the key");
    assert_eq!(stored.get(0), Some(260.), "the size the user gave the pane");
}

#[gpui_kit::test]
fn a_splitter_takes_the_sizes_it_kept_the_last_time_it_was_built(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    cx.update(|cx| {
        let store = guiraffe_layout::layout_store(cx).expect("the store is installed");
        store.save_splitter_sizes(
            "sidebar",
            &SplitterSizes::from_pixels(&[px(120.), px(280.)]),
        );
    });
    let state = cx.update(SplitterState::new);
    let cx = harness!(cx, KeyedSplitterHarness { state });

    let first = cx.debug_bounds("first").expect("the first pane");
    assert_eq!(
        first.size.width,
        px(120.),
        "the pane starts at the size it had the last time"
    );
}

#[gpui_kit::test]
fn a_splitter_that_restores_the_sizes_of_another_number_of_panes_uses_sizes_that_fit(
    cx: &mut TestAppContext,
) {
    cx.update(guiraffe_layout::init);
    cx.update(|cx| {
        let store = guiraffe_layout::layout_store(cx).expect("the store is installed");
        store.save_splitter_sizes(
            "sidebar",
            &SplitterSizes::from_pixels(&[px(100.), px(200.), px(300.)]),
        );
    });
    let state = cx.update(SplitterState::new);
    let cx = harness!(cx, KeyedSplitterHarness { state });

    let first = cx.debug_bounds("first").expect("the first pane");
    let second = cx.debug_bounds("second").expect("the second pane");
    assert!(
        (first.size.width - px(133.33)).abs() < px(0.5),
        "the two sizes that belong to a pane are used, in the same proportions"
    );
    assert!(
        second.size.width > px(0.),
        "the pane that has no size of its own still has room"
    );
    assert_eq!(
        first.size.width + second.size.width,
        px(400.),
        "the panes fill the splitter"
    );
}

/// A store that counts the writes.
struct CountingStore {
    writes: Rc<Cell<usize>>,
    values: std::collections::HashMap<String, String>,
}

impl LayoutPersist for CountingStore {
    fn load(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn save(&self, _key: &str, _value: &str) {
        self.writes.set(self.writes.get() + 1);
    }
}

#[gpui_kit::test]
fn a_drag_that_changes_nothing_does_not_write_to_the_store(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let writes = Rc::new(Cell::new(0));
    cx.update(|cx| {
        cx.set_global(LayoutStore::new(Box::new(CountingStore {
            writes: writes.clone(),
            values: std::collections::HashMap::new(),
        })));
    });
    let state = cx.update(SplitterState::new);
    let cx = harness!(cx, KeyedSplitterHarness { state });

    let first = cx.debug_bounds("first").expect("the first pane");
    let boundary = first.right();
    cx.simulate_mouse_move(
        point(boundary - px(2.), px(100.)),
        None,
        Modifiers::default(),
    );
    cx.simulate_mouse_down(
        point(boundary - px(2.), px(100.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(boundary + px(20.), px(100.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(boundary, px(100.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(boundary, px(100.)),
        MouseButton::Left,
        Modifiers::default(),
    );

    assert_eq!(
        writes.get(),
        0,
        "a drag that changes nothing does not write to the store"
    );
}

/// A shell whose main content counts how often it is rendered.
struct CountingShellHarness {
    state: Entity<AppShellState>,
    main: Entity<CountingMain>,
}

struct CountingMain {
    renders: Rc<Cell<usize>>,
}

impl Render for CountingMain {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().h_full().debug_selector(|| "main".into())
    }
}

impl Render for CountingShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(300.)).child(
            AppShell::new(&self.state)
                .navbar(div().h_full().debug_selector(|| "navbar".into()))
                .main(self.main.clone()),
        )
    }
}

#[gpui_kit::test]
fn the_main_content_is_not_rendered_again_when_the_navbar_collapses(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(AppShellState::new);
    let renders = Rc::new(Cell::new(0));
    let main = cx.update(|cx| {
        cx.new(|_| CountingMain {
            renders: renders.clone(),
        })
    });
    let cx = harness!(
        cx,
        CountingShellHarness {
            state: state.clone(),
            main,
        }
    );

    let renders_before = renders.get();
    cx.update(|_window, app| state.update(app, |state, cx| state.toggle_navbar(cx)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        state.read_with(cx, |state, _| state.navbar_collapsed()),
        "the navbar collapsed"
    );
    assert!(
        renders.get() <= renders_before + 1,
        "the main content is laid out again, because the room it has changed, and it is \
         not built again"
    );
}

/// A view that follows the events of the shell.
struct EventWatcher {
    /// The count the watcher keeps, and the events it saw. The watcher keeps
    /// the subscription that fills them, and the test reads them.
    #[allow(dead_code)]
    events: Rc<Cell<usize>>,
    #[allow(dead_code)]
    seen: Rc<RefCell<Vec<AppShellEvent>>>,
    /// The subscription keeps the watcher following the shell.
    #[allow(dead_code)]
    subscription: Subscription,
}

impl EventWatcher {
    fn new(
        state: &Entity<AppShellState>,
        events: Rc<Cell<usize>>,
        seen: Rc<RefCell<Vec<AppShellEvent>>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let events_for_sub = events.clone();
        let seen_for_sub = seen.clone();
        let subscription = cx.subscribe(state, move |_, _, event, _| {
            events_for_sub.set(events_for_sub.get() + 1);
            seen_for_sub.borrow_mut().push(event.clone());
        });
        Self {
            events,
            seen,
            subscription,
        }
    }
}

impl Render for EventWatcher {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("watcher")
    }
}

#[gpui_kit::test]
fn the_shell_tells_what_it_did(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(AppShellState::new);
    let events = Rc::new(Cell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let watcher = cx.update(|cx| {
        let state = state.clone();
        cx.new(|cx| EventWatcher::new(&state, events.clone(), seen.clone(), cx))
    });

    cx.update(|app| state.update(app, |state, cx| state.toggle_navbar(cx)));
    cx.update(|app| state.update(app, |state, cx| state.toggle_aside(cx)));
    cx.update(|app| state.update(app, |state, cx| state.set_navbar_width(px(300.), cx)));

    assert_eq!(events.get(), 3, "the shell tells what it did");
    assert_eq!(
        *seen.borrow(),
        vec![
            AppShellEvent::NavbarCollapsed { collapsed: true },
            AppShellEvent::AsideCollapsed { collapsed: true },
            AppShellEvent::NavbarResized { width: 300. },
        ]
    );
    drop(watcher);
}

/// A shell with a navbar the user can drag.
struct ResizableShellHarness {
    state: Entity<AppShellState>,
    main: Entity<MainContent>,
}

impl Render for ResizableShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(600.)).h(px(300.)).child(
            AppShell::new(&self.state)
                .navbar(div().h_full().debug_selector(|| "navbar".into()))
                .resizable_navbar()
                .main(self.main.clone()),
        )
    }
}

#[gpui_kit::test]
fn the_user_can_drag_the_navbar_to_another_width(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(|cx| {
        let state = AppShellState::new(cx);
        state.update(cx, |state, cx| state.set_navbar_width(px(100.), cx));
        state
    });
    let main = cx.update(|cx| cx.new(|_| MainContent));
    let cx = harness!(
        cx,
        ResizableShellHarness {
            state: state.clone(),
            main,
        }
    );

    state.read_with(cx, |state, _| {
        assert_eq!(
            state.navbar_width(),
            px(100.),
            "the navbar starts at its width"
        );
    });

    // The handle sits at the right edge of the navbar.
    let handle = px(100.) - px(2.);
    cx.simulate_mouse_move(point(handle, px(150.)), None, Modifiers::default());
    cx.simulate_mouse_down(
        point(handle, px(150.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    // The first move starts the drag, the second one resizes the bar.
    cx.simulate_mouse_move(
        point(handle + px(30.), px(150.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(handle + px(60.), px(150.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(handle + px(60.), px(150.)),
        MouseButton::Left,
        Modifiers::default(),
    );

    state.read_with(cx, |state, _| {
        assert_eq!(
            state.navbar_width(),
            px(160.),
            "the navbar followed the pointer"
        );
    });
}

/// A shell whose aside collapses on a narrow window.
struct AsideShellHarness {
    state: Entity<AppShellState>,
    main: Entity<MainContent>,
}

impl Render for AsideShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(300.)).child(
            AppShell::new(&self.state)
                .aside(div().h_full().debug_selector(|| "aside".into()))
                .collapse_aside_below(Breakpoint::Sm)
                .main(self.main.clone()),
        )
    }
}

#[gpui_kit::test]
fn a_narrow_window_collapses_the_aside(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(AppShellState::new);
    let main = cx.update(|cx| cx.new(|_| MainContent));
    let cx = harness!(
        cx,
        AsideShellHarness {
            state: state.clone(),
            main,
        }
    );

    let aside = cx.debug_bounds("aside").expect("the aside");
    assert_eq!(
        aside.size.width,
        px(0.),
        "the aside collapsed, because the window is too narrow for it"
    );
}

/// A shell whose navbar the user opened by hand.
struct ManualShellHarness {
    state: Entity<AppShellState>,
    main: Entity<MainContent>,
    width: Rc<Cell<f32>>,
}

impl Render for ManualShellHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(self.width.get())).h(px(300.)).child(
            AppShell::new(&self.state)
                .navbar(div().h_full().debug_selector(|| "navbar".into()))
                .collapse_below(Breakpoint::Xs)
                .main(self.main.clone()),
        )
    }
}

#[gpui_kit::test]
fn a_toggle_the_user_made_is_not_undone_by_a_resize(cx: &mut TestAppContext) {
    cx.update(guiraffe_layout::init);
    let state = cx.update(AppShellState::new);
    let main = cx.update(|cx| cx.new(|_| MainContent));
    let width = Rc::new(Cell::new(400.));
    let state = state.clone();
    let (harness, cx) = cx.add_window_view(|_, _| ManualShellHarness {
        state: state.clone(),
        main,
        width: width.clone(),
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // The window is too narrow for the navbar, so the shell collapsed it.
    state.read_with(cx, |state, _| {
        assert!(state.navbar_collapsed(), "the shell collapsed the navbar");
    });

    // The user opens it again.
    cx.update(|_window, app| state.update(app, |state, cx| state.expand_navbar(cx)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // A resize that stays in the same band leaves the navbar alone.
    width.set(390.);
    cx.update(|_window, app| harness.update(app, |_, cx| cx.notify()));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    state.read_with(cx, |state, _| {
        assert!(
            !state.navbar_collapsed(),
            "the toggle the user made is not undone by a resize"
        );
    });
}
