use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, Focusable, HeadlessAppContext, InteractiveElement,
    IntoElement, Keystroke, Modifiers, MouseMoveEvent, ParentElement, PlatformInput, Render,
    SharedString, StyleRefinement, Styled, Window, WindowHandle,
    component::{
        Sizable,
        badge::Badge,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    div, point, px, rgb, size,
};
use guiraffe_bench::allocator::{CountingAllocator, count_allocations};
use guiraffe_state::{Event, Fx, Selected, Store};
use guiraffe_test::render::RenderCounter;
use nix::time::{ClockId, clock_gettime};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const ROW_HEIGHT: f32 = 32.;
const WIDTH: f32 = 420.;
const EDITED: usize = 3;

#[derive(Clone, PartialEq)]
struct Row {
    label: SharedString,
    checked: bool,
    badge: usize,
}

#[derive(Clone, PartialEq)]
struct State {
    rows: Vec<Row>,
    half: bool,
}

#[derive(Debug)]
enum Change {
    Label(SharedString),
    Toggle(usize),
    Half(bool),
}

impl Event<State> for Change {
    fn handle(self, fx: &mut Fx<'_, State>) {
        fx.update(|state| match self {
            Self::Label(label) => state.rows[EDITED].label = label,
            Self::Toggle(id) => state.rows[id].checked = !state.rows[id].checked,
            Self::Half(half) => state.half = half,
        });
    }
}

#[derive(Clone)]
struct Probe {
    root: RenderCounter,
    rows: Rc<[RenderCounter]>,
    selectors: Rc<Cell<usize>>,
}

impl Probe {
    fn new(n: usize) -> Self {
        Self {
            root: RenderCounter::new(),
            rows: (0..n).map(|_| RenderCounter::new()).collect(),
            selectors: Rc::default(),
        }
    }
}

fn select_row<T: 'static>(
    store: &Store<State>,
    id: usize,
    probe: &Probe,
    cx: &mut Context<T>,
) -> Selected<Row> {
    let visits = probe.selectors.clone();
    store.select(cx, move |state| {
        visits.set(visits.get() + 1);
        state.rows[id].clone()
    })
}

fn row(id: usize, data: &Row, store: &Store<State>) -> impl IntoElement + use<> {
    let checkbox = store.downgrade();
    let button = store.downgrade();
    div()
        .id(("row", id))
        .flex()
        .items_center()
        .gap_2()
        .w(px(WIDTH))
        .h(px(ROW_HEIGHT))
        .flex_shrink_0()
        .hover(|style| style.bg(rgb(0x6688bb)))
        .child(div().w(px(130.)).child(data.label.clone()))
        .child(
            Checkbox::new("checked")
                .checked(data.checked)
                .on_click(move |_, _, cx| {
                    if let Some(store) = checkbox.upgrade() {
                        store.dispatch(Change::Toggle(id), cx);
                    }
                }),
        )
        .child(
            Badge::new()
                .count(data.badge)
                .small()
                .child(div().w(px(30.)).h(px(18.))),
        )
        .child(
            Button::new("toggle")
                .label("Toggle")
                .small()
                .on_click(move |_, _, cx| {
                    if let Some(store) = button.upgrade() {
                        store.dispatch(Change::Toggle(id), cx);
                    }
                }),
        )
}

struct RowView {
    id: usize,
    data: Selected<Row>,
    store: Store<State>,
    render: RenderCounter,
}

impl Render for RowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render.hit();
        row(self.id, self.data.get(cx), &self.store)
    }
}

enum Rows {
    Inline(Vec<Selected<Row>>),
    Cached(Vec<Entity<RowView>>),
}

struct Page {
    store: Store<State>,
    half: Selected<bool>,
    rows: Rows,
    input: Entity<InputState>,
    probe: Probe,
}

impl Page {
    fn new(cached: bool, probe: Probe, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows = (0..probe.rows.len())
            .map(|id| Row {
                label: format!("Row {id:05}").into(),
                checked: id % 2 == 0,
                badge: id % 9 + 1,
            })
            .collect();
        let store = Store::new(State { rows, half: false }, cx);
        let half = store.select(cx, |state| state.half);
        let rows = if cached {
            Rows::Cached(
                probe
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(id, render)| {
                        cx.new(|cx| RowView {
                            id,
                            data: select_row(&store, id, &probe, cx),
                            store: store.clone(),
                            render: render.clone(),
                        })
                    })
                    .collect(),
            )
        } else {
            Rows::Inline(
                (0..probe.rows.len())
                    .map(|id| select_row(&store, id, &probe, cx))
                    .collect(),
            )
        };
        Self {
            rows,
            store,
            half,
            input: cx.new(|cx| InputState::new(window, cx)),
            probe,
        }
    }
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.probe.root.hit();
        let stride = if *self.half.get(cx) { 2 } else { 1 };
        let page = div().flex().flex_col().w(px(WIDTH)).child(
            div()
                .h(px(ROW_HEIGHT))
                .flex_shrink_0()
                .child(Input::new(&self.input)),
        );
        match &self.rows {
            Rows::Inline(rows) => {
                page.children(rows.iter().enumerate().step_by(stride).map(|(id, data)| {
                    self.probe.rows[id].hit();
                    row(id, data.get(cx), &self.store)
                }))
            }
            Rows::Cached(rows) => page.children(rows.iter().step_by(stride).map(|view| {
                AnyView::from(view.clone()).cached(
                    StyleRefinement::default()
                        .w(px(WIDTH))
                        .h(px(ROW_HEIGHT))
                        .flex_shrink_0(),
                )
            })),
        }
    }
}

struct Mounted<'a> {
    app: &'a mut HeadlessAppContext,
    handle: WindowHandle<Page>,
}

impl Mounted<'_> {
    fn update<R>(&mut self, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        self.app
            .update_window(self.handle.into(), |_, window, cx| f(window, cx))
            .expect("live benchmark window")
    }
}

fn app() -> HeadlessAppContext {
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app =
        HeadlessAppContext::with_asset_source(text, std::sync::Arc::new(gpui_kit::assets::Assets));
    app.update(guiraffe_components::init);
    app
}

fn mount(app: &mut HeadlessAppContext, cached: bool, probe: Probe) -> Mounted<'_> {
    let handle = app
        .open_window(size(px(WIDTH), px(800.)), |window, cx| {
            cx.new(|cx| Page::new(cached, probe, window, cx))
        })
        .expect("mount benchmark window");
    app.run_until_parked();
    Mounted { app, handle }
}

fn fixture(
    app: &mut HeadlessAppContext,
    cached: bool,
    probe: Probe,
) -> (Entity<Page>, Mounted<'_>) {
    let mut mounted = mount(app, cached, probe);
    let page = mounted.update(|window, cx| {
        let page = window.root::<Page>().flatten().expect("benchmark page");
        window.focus(&page.read(cx).input.focus_handle(cx), cx);
        page
    });
    (page, mounted)
}

#[inline(never)]
fn label_edit(window: &mut Mounted<'_>, store: &Store<State>, label: SharedString) {
    window.update(|_, cx| store.dispatch(Change::Label(label), cx));
}

#[derive(Debug)]
struct WriteLabel(SharedString);

impl Event<State> for WriteLabel {
    fn handle(self, fx: &mut Fx<'_, State>) {
        fx.set_field(
            self.0,
            |state| &state.rows[EDITED].label,
            |state, label| state.rows[EDITED].label = label,
        );
    }
}

#[test]
fn cached_label_updates_and_selector_scaling() {
    let mut app = app();
    for n in [1_000, 10_000] {
        let probe = Probe::new(n);
        let (page, mut window) = fixture(&mut app, true, probe.clone());
        let store = window.update(|_, cx| page.read(cx).store.clone());
        let label = window.update(|_, cx| store.state(cx).rows[EDITED].label.clone());
        let notifications = Rc::new(Cell::new(0));
        let _observations = window.update(|_, cx| {
            let Rows::Cached(rows) = &page.read(cx).rows else {
                unreachable!()
            };
            let row = rows[EDITED].clone();
            [
                cx.observe(&page, {
                    let notifications = notifications.clone();
                    move |_, _| notifications.set(notifications.get() + 1)
                }),
                cx.observe(&row, {
                    let notifications = notifications.clone();
                    move |_, _| notifications.set(notifications.get() + 1)
                }),
            ]
        });
        let before: Vec<_> = probe.rows.iter().map(RenderCounter::get).collect();
        assert!(
            before.iter().all(|&renders| renders > 0),
            "every row mounts"
        );
        let root_before = probe.root.get();
        let visits = probe.selectors.get();
        let processed = window.update(|_, cx| store.stats(cx).events_processed);
        let (_, counts) = count_allocations(|| {
            window.update(|_, cx| store.dispatch_now(WriteLabel(label.clone()), cx));
        })
        .expect("allocation scope");
        assert_eq!(counts.allocations, 0);
        assert_eq!(
            window.update(|_, cx| store.stats(cx).events_processed),
            processed + 1,
            "the equal write went through the Store"
        );
        assert_eq!(probe.root.get(), root_before);
        assert_eq!(probe.selectors.get(), visits);
        assert!(probe.rows.iter().zip(&before).all(|(r, &b)| r.get() == b));

        // The unguarded event also leaves every render untouched.
        window.update(|_, cx| store.dispatch(Change::Label(label), cx));
        assert_eq!(probe.root.get(), root_before);
        assert_eq!(probe.selectors.get(), visits);
        assert!(probe.rows.iter().zip(&before).all(|(r, &b)| r.get() == b));

        assert_eq!(notifications.get(), 0);
        label_edit(&mut window, &store, "Edited    ".into());
        assert_eq!(notifications.get(), 1);
        for (id, (counter, previous)) in probe.rows.iter().zip(before).enumerate() {
            assert_eq!(
                counter.get() - previous,
                usize::from(id == EDITED),
                "row {id}"
            );
        }
        // TODO(plans/ROADMAP-0.3.md#e-fine-grained-reactivity-below-the-view-response-to-a-critique-of-gpui):
        // Tighten to O(changed sinks) once Store can route writes to their dependent sinks.
        assert_eq!(probe.selectors.get() - visits, n);
        close(window);
    }
}

fn cpu_ns() -> u64 {
    let time = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).expect("thread CPU clock");
    time.tv_sec() as u64 * 1_000_000_000 + time.tv_nsec() as u64
}

fn percentile(values: &mut [u64], percent: usize) -> u64 {
    values.sort_unstable();
    values[(values.len() * percent).div_ceil(100) - 1]
}

#[test]
#[ignore = "release measurements; run explicitly with --ignored --nocapture"]
fn composed_measurements() {
    let mut app = app();
    let profile = std::env::var_os("COMPOSED_PROFILE").is_some();
    let equal_only = !profile && std::env::var_os("COMPOSED_EQUAL_ONLY").is_some();
    let samples = if profile { 10 } else { 31 };
    println!(
        "variant,n,operation,median_cpu_ns,p99_cpu_ns,median_allocs,root_renders,row_renders,selector_calls,fast_views_reused"
    );
    for n in [1_000, 3_000, 10_000] {
        for cached in [false, true] {
            if profile && !(n == 3_000 && cached) {
                continue;
            }
            let probe = Probe::new(n);
            let (page, mut window) = fixture(&mut app, cached, probe.clone());
            let store = window.update(|_, cx| page.read(cx).store.clone());
            for operation in ["label", "key", "hover", "equal", "filter"] {
                if profile && operation != "label" || equal_only && operation != "equal" {
                    continue;
                }
                let mut time = Vec::with_capacity(samples);
                let mut allocations = Vec::with_capacity(samples);
                let mut activity = Vec::with_capacity(samples);
                for i in 0..samples + 4 {
                    if operation == "filter" {
                        window.update(|_, cx| store.dispatch(Change::Half(false), cx));
                    }
                    let label = window.update(|_, cx| {
                        if operation == "equal" {
                            store.state(cx).rows[EDITED].label.clone()
                        } else if i % 2 == 0 {
                            "Row 00004".into()
                        } else {
                            "Row 00003".into()
                        }
                    });
                    let start = counters(&mut window, &probe);
                    let (elapsed, counts) = count_allocations(|| {
                        let start = cpu_ns();
                        match operation {
                            "label" => label_edit(&mut window, &store, label),
                            "equal" => {
                                window.update(|_, cx| store.dispatch_now(WriteLabel(label), cx))
                            }
                            "filter" => {
                                window.update(|_, cx| store.dispatch(Change::Half(true), cx));
                            }
                            "key" => {
                                window.update(|window, cx| {
                                    window.dispatch_keystroke(
                                        Keystroke {
                                            modifiers: Modifiers::default(),
                                            key: "a".into(),
                                            key_char: Some("a".into()),
                                        },
                                        cx,
                                    );
                                });
                            }
                            "hover" => {
                                window.update(|window, cx| {
                                    window.dispatch_event(
                                        PlatformInput::MouseMove(MouseMoveEvent {
                                            position: point(
                                                px(10.),
                                                px(ROW_HEIGHT * (1.5 + (i % 2) as f32)),
                                            ),
                                            pressed_button: None,
                                            modifiers: Modifiers::default(),
                                        }),
                                        cx,
                                    );
                                });
                            }
                            _ => unreachable!(),
                        }
                        cpu_ns() - start
                    })
                    .expect("allocation scope");
                    let end = counters(&mut window, &probe);
                    if i >= 4 {
                        time.push(elapsed);
                        allocations.push(counts.allocations);
                        activity.push(std::array::from_fn(|j| end[j] - start[j]));
                    }
                }
                if operation == "key" {
                    window.update(|_, cx| {
                        let input = page.read(cx).input.read(cx);
                        assert_eq!(
                            input.value().len(),
                            samples + 4,
                            "keystrokes reach the input"
                        );
                    });
                }
                print_sample(cached, n, operation, time, allocations, activity);
            }
            close(window);

            if !profile && !equal_only {
                let mut time = Vec::new();
                let mut allocations = Vec::new();
                let mut activity = Vec::new();
                for i in 0..samples + 4 {
                    let probe = Probe::new(n);
                    let ((elapsed, mut window), counts) = count_allocations(|| {
                        let start = cpu_ns();
                        let window = mount(&mut app, cached, probe.clone());
                        (cpu_ns() - start, window)
                    })
                    .expect("allocation scope");
                    if i >= 4 {
                        time.push(elapsed);
                        allocations.push(counts.allocations);
                        activity.push(counters(&mut window, &probe));
                    }
                    close(window);
                }
                print_sample(cached, n, "mount", time, allocations, activity);
            }
        }
    }
}

fn close(mut window: Mounted<'_>) {
    window.update(|window, _| window.remove_window());
    window.app.run_until_parked();
}

fn counters(window: &mut Mounted<'_>, probe: &Probe) -> [u64; 4] {
    let reused = window.update(|_window, _| {
        #[cfg(feature = "gpui-fast")]
        {
            _window.layout_stats().views_reused
        }
        #[cfg(not(feature = "gpui-fast"))]
        {
            0
        }
    });
    [
        probe.root.get() as u64,
        probe.rows.iter().map(|r| r.get() as u64).sum(),
        probe.selectors.get() as u64,
        reused,
    ]
}

fn print_sample(
    cached: bool,
    n: usize,
    operation: &str,
    mut time: Vec<u64>,
    mut allocations: Vec<u64>,
    activity: Vec<[u64; 4]>,
) {
    let variant = if cached { "cached" } else { "inline" };
    let [root, rows, selectors, reused] = std::array::from_fn(|i| {
        percentile(&mut activity.iter().map(|a| a[i]).collect::<Vec<_>>(), 50)
    });
    let reused = cfg!(feature = "gpui-fast")
        .then_some(reused)
        .map_or_else(|| "-".to_owned(), |count| count.to_string());
    println!(
        "{variant},{n},{operation},{},{},{},{root},{rows},{selectors},{reused}",
        percentile(&mut time, 50),
        percentile(&mut time, 99),
        percentile(&mut allocations, 50)
    );
}
