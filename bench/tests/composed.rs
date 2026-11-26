use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

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
    RowLabel(usize, SharedString),
    Toggle(usize),
    Half(bool),
}

impl Event<State> for Change {
    fn handle(self, fx: &mut Fx<'_, State>) {
        if let Self::RowLabel(id, label) = self {
            // Only the selections keyed by the row see this write.
            fx.set_keyed_field(
                id as u64,
                label,
                move |state| &state.rows[id].label,
                move |state| &mut state.rows[id].label,
            );
            return;
        }
        fx.update(|state| match self {
            Self::Label(label) => state.rows[EDITED].label = label,
            Self::RowLabel(..) => {}
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
    store.select_key(id as u64, cx, move |state| {
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
    #[inline(never)]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render.hit();
        row(self.id, self.data.get(cx), &self.store)
    }
}

enum Rows {
    Virtual(Rc<RefCell<HashMap<usize, Entity<RowView>>>>),
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
    fn new(
        cached: bool,
        virtualized: bool,
        probe: Probe,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let rows = (0..probe.rows.len())
            .map(|id| Row {
                label: format!("Row {id:05}").into(),
                checked: id % 2 == 0,
                badge: id % 9 + 1,
            })
            .collect();
        let store = Store::new(State { rows, half: false }, cx);
        let half = store.select(cx, |state| state.half);
        let rows = if virtualized {
            Rows::Virtual(Rc::default())
        } else if cached {
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
            Rows::Virtual(mounted) => {
                let mounted = mounted.clone();
                let probe = self.probe.clone();
                let store = self.store.clone();
                page.child(
                    gpui_kit::uniform_list(
                        "bound-rows",
                        probe.rows.len().div_ceil(stride),
                        move |range, _, cx| {
                            let mut mounted = mounted.borrow_mut();
                            // uniform_list may measure row zero separately.
                            if range != (0..1) {
                                let retained =
                                    range.start.saturating_sub(2)..range.end.saturating_add(2);
                                mounted.retain(|id, _| {
                                    retained.contains(&(id / stride)) && id % stride == 0
                                });
                            }
                            range
                                .map(|position| {
                                    let id = position * stride;
                                    let entity = mounted.entry(id).or_insert_with(|| {
                                        cx.new(|cx| RowView {
                                            id,
                                            data: select_row(&store, id, &probe, cx),
                                            store: store.clone(),
                                            render: probe.rows[id].clone(),
                                        })
                                    });
                                    AnyView::from(entity.clone()).cached(
                                        StyleRefinement::default().w(px(WIDTH)).h(px(ROW_HEIGHT)),
                                    )
                                })
                                .collect()
                        },
                    )
                    .h(px(768.))
                    .w(px(WIDTH)),
                )
            }
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
            cx.new(|cx| Page::new(cached, false, probe, window, cx))
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
    for n in [1_000, 3_000, 10_000] {
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

        // Warm the pending-event queue before measuring steady-state dispatch.
        window.update(|_, cx| store.dispatch(Change::Label(label.clone()), cx));
        let (_, counts) = count_allocations(|| {
            window.update(|_, cx| store.dispatch(Change::Label(label), cx));
        })
        .expect("allocation scope");
        assert!(counts.allocations <= 3, "queued equal write: {counts:?}");
        assert_eq!(
            window.update(|_, cx| store.stats(cx).events_processed),
            processed + 3,
        );
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
        // TODO(ROADMAP.md, reactive sinks):
        // Tighten to O(changed sinks) once Store can route writes to their dependent sinks.
        assert!(
            probe.selectors.get() - visits <= n,
            "selector visits at {n} rows"
        );
        close(window);
    }
}

/// The page's focused input writes its state as it is drawn, state the page
/// read as it rendered. Those writes are the page's own, so a row edit leaves
/// the page as it was drawn: on gpui-fast the row is patched into last frame,
/// allocating as much at 400 rows as at 100.
#[test]
fn fully_visible_row_edits_splice_the_page() {
    let mut app = app();
    let small = fully_visible_edit_allocations(&mut app, 100);
    let large = fully_visible_edit_allocations(&mut app, 400);
    if gpui_kit::GPUI_FAST {
        // The edited row is patched into last frame: an edit allocates for
        // that row alone, whatever the number of rows.
        assert!(large <= 300, "{large} allocations per edit at 400 rows");
        assert_eq!(small, large, "edit allocations grow with rows");
    }
}

/// Median allocations of a row edit in a page showing all `n` rows, checking
/// on the way that the edit runs one selector and, on gpui-fast, splices the
/// page.
fn fully_visible_edit_allocations(app: &mut HeadlessAppContext, n: usize) -> u64 {
    let probe = Probe::new(n);
    let handle = app
        .open_window(
            size(px(WIDTH), px((n + 1) as f32 * ROW_HEIGHT)),
            |window, cx| cx.new(|cx| Page::new(true, false, probe.clone(), window, cx)),
        )
        .expect("mount page");
    app.run_until_parked();
    let mut window = Mounted { app, handle };
    let page = window.update(|window, cx| {
        let page = window.root::<Page>().flatten().expect("page");
        window.focus(&page.read(cx).input.focus_handle(cx), cx);
        page
    });
    let store = window.update(|_, cx| page.read(cx).store.clone());
    let edit = |window: &mut Mounted<'_>, label: &'static str| {
        count_allocations(|| {
            window.update(|_, cx| store.dispatch(Change::RowLabel(EDITED, label.into()), cx));
        })
        .expect("allocation scope")
        .1
        .allocations
    };
    edit(&mut window, "Row 00004");
    edit(&mut window, "Row 00003");
    let (root, row) = (probe.root.get(), probe.rows[EDITED].get());
    let visits = probe.selectors.get();
    let mut allocations: Vec<u64> = [
        "Row 00004",
        "Row 00003",
        "Row 00004",
        "Row 00003",
        "Row 00004",
    ]
    .into_iter()
    .map(|label| edit(&mut window, label))
    .collect();
    assert_eq!(probe.rows[EDITED].get(), row + 5, "the row drew each edit");
    assert_eq!(
        probe.selectors.get() - visits,
        5,
        "a keyed edit runs one selector"
    );
    if gpui_kit::GPUI_FAST {
        assert_eq!(probe.root.get(), root, "the page was not rendered again");
        assert!(
            window
                .update(|window, _| window.frame_patch_stats())
                .patched
                >= 5,
            "the edits were patched"
        );
    }
    close(window);
    percentile(&mut allocations, 50)
}

/// Opens a window showing all `n` rows of a page of cached rows, its input
/// focused.
fn mount_all_visible(
    app: &mut HeadlessAppContext,
    n: usize,
    probe: Probe,
) -> (Entity<Page>, Mounted<'_>) {
    let handle = app
        .open_window(
            size(px(WIDTH), px((n + 1) as f32 * ROW_HEIGHT)),
            |window, cx| cx.new(|cx| Page::new(true, false, probe, window, cx)),
        )
        .expect("mount page");
    app.run_until_parked();
    let mut window = Mounted { app, handle };
    let page = window.update(|window, cx| {
        let page = window.root::<Page>().flatten().expect("page");
        window.focus(&page.read(cx).input.focus_handle(cx), cx);
        page
    });
    (page, window)
}

/// The rendered frame as text, drawn again in full: what a patched frame
/// has to equal.
fn redrawn_in_full(window: &mut Mounted<'_>) -> String {
    window.update(|window, _| window.refresh());
    window.update(|window, _| window.frame_fingerprint())
}

fn row_label(window: &mut Mounted<'_>, store: &Store<State>, row: usize, label: &str) {
    let label = SharedString::from(label.to_string());
    window.update(|_, cx| store.dispatch(Change::RowLabel(row, label), cx));
}

/// A row edited to a label of the same length patches last frame in place
/// on gpui-fast; one that shapes to another number of glyphs is drawn in
/// full. Either way the frame is the one a full redraw draws.
#[test]
fn patched_row_edits_match_a_full_redraw() {
    let mut app = app();
    let probe = Probe::new(40);
    let (page, mut window) = mount_all_visible(&mut app, 40, probe.clone());
    let store = window.update(|_, cx| page.read(cx).store.clone());
    let stats = |window: &mut Mounted<'_>| window.update(|window, _| window.frame_patch_stats());
    row_label(&mut window, &store, EDITED, "Row 00004");
    for (row, label) in [
        (EDITED, "Row 00009"),
        (17, "Row 00071"),
        (EDITED, "Row 00003"),
    ] {
        let before = stats(&mut window);
        let renders = probe.rows[row].get();
        row_label(&mut window, &store, row, label);
        assert_eq!(probe.rows[row].get(), renders + 1, "row {row} drew once");
        let patched = window.update(|window, _| window.frame_fingerprint());
        if gpui_kit::GPUI_FAST {
            assert_eq!(
                stats(&mut window).patched,
                before.patched + 1,
                "{label} patched"
            );
        }
        assert_eq!(
            patched,
            redrawn_in_full(&mut window),
            "{label} as drawn in full"
        );
    }
    // Another number of glyphs: drawn as before. On gpui-fast the row was
    // built for the patch first, so it rendered twice.
    let before = stats(&mut window);
    let renders = probe.rows[EDITED].get();
    row_label(&mut window, &store, EDITED, "Row 000004444");
    assert_eq!(
        probe.rows[EDITED].get(),
        renders + 2 - usize::from(!gpui_kit::GPUI_FAST)
    );
    let drawn = window.update(|window, _| window.frame_fingerprint());
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            stats(&mut window).patched,
            before.patched,
            "a longer label is not patched"
        );
    }
    assert_eq!(drawn, redrawn_in_full(&mut window));
    close(window);
}

/// After rows are patched in, a click lands on the patched row's checkbox
/// and the tab order is the one a full redraw gives.
#[test]
fn patched_rows_hit_test_and_tab_like_a_full_redraw() {
    let mut app = app();
    let probe = Probe::new(20);
    let (page, mut window) = mount_all_visible(&mut app, 20, probe.clone());
    let store = window.update(|_, cx| page.read(cx).store.clone());
    row_label(&mut window, &store, EDITED, "Row 00004");
    row_label(&mut window, &store, EDITED, "Row 00007");

    // The checkbox follows the 130-pixel label and the row's 8-pixel gap.
    let at = point(px(146.), px(ROW_HEIGHT * (1.5 + EDITED as f32)));
    let click = |window: &mut Mounted<'_>| {
        window.update(|window, cx| {
            window.dispatch_event(
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: at,
                    pressed_button: None,
                    modifiers: Modifiers::default(),
                }),
                cx,
            );
        });
        for down in [true, false] {
            window.update(|window, cx| {
                let event = if down {
                    PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                        button: gpui_kit::MouseButton::Left,
                        position: at,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    })
                } else {
                    PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                        button: gpui_kit::MouseButton::Left,
                        position: at,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    })
                };
                window.dispatch_event(event, cx);
            });
        }
    };
    let checked = |window: &mut Mounted<'_>| {
        window.update(|_, cx| {
            store
                .state(cx)
                .rows
                .iter()
                .map(|row| row.checked)
                .collect::<Vec<_>>()
        })
    };
    let unchecked = checked(&mut window);
    click(&mut window);
    let after = checked(&mut window);
    let toggled: Vec<usize> = (0..unchecked.len())
        .filter(|&row| unchecked[row] != after[row])
        .collect();
    assert_eq!(toggled, vec![EDITED], "the click toggled the patched row");

    let tab_order = |window: &mut Mounted<'_>| {
        window.update(|window, cx| {
            window.focus(&page.read(cx).input.focus_handle(cx), cx);
        });
        (0..12)
            .map(|_| {
                window.update(|window, cx| {
                    window.focus_next(cx);
                    format!("{:?}", window.focused(cx).map(|handle| handle.tab_index))
                        + &format!("{:?}", window.focused(cx))
                })
            })
            .collect::<Vec<_>>()
    };
    row_label(&mut window, &store, EDITED, "Row 00008");
    let patched = tab_order(&mut window);
    window.update(|window, _| window.refresh());
    assert_eq!(patched, tab_order(&mut window));
    close(window);
}

/// Allocations of a patched row edit next to those of the same row drawn
/// again unchanged (notified without new data), the frame's baseline, at 1k
/// and 3k rows.
#[test]
#[ignore = "release measurement; run explicitly with --ignored --nocapture"]
fn patched_edit_allocations_against_the_frame_baseline() {
    let mut app = app();
    for n in [1000, 3000] {
        let probe = Probe::new(n);
        let (page, mut window) = mount_all_visible(&mut app, n, probe.clone());
        let store = window.update(|_, cx| page.read(cx).store.clone());
        let row = window.update(|_, cx| match &page.read(cx).rows {
            Rows::Cached(rows) => rows[EDITED].clone(),
            _ => unreachable!("a page of cached rows"),
        });
        for label in ["Row 00004", "Row 00003", "Row 00004"] {
            row_label(&mut window, &store, EDITED, label);
        }
        let mut edits = Vec::new();
        let mut baseline = Vec::new();
        for i in 0..21 {
            let label = if i % 2 == 0 { "Row 00003" } else { "Row 00004" };
            edits.push(
                count_allocations(|| row_label(&mut window, &store, EDITED, label))
                    .expect("allocation scope")
                    .1
                    .allocations,
            );
            baseline.push(
                count_allocations(|| window.update(|_, cx| row.update(cx, |_, cx| cx.notify())))
                    .expect("allocation scope")
                    .1
                    .allocations,
            );
        }
        let stats = window.update(|window, _| window.frame_patch_stats());
        println!(
            "rows={n} edit_allocs={} baseline_allocs={} patched={} refused={}",
            percentile(&mut edits, 50),
            percentile(&mut baseline, 50),
            stats.patched,
            stats.refused
        );
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
    let followup = std::env::var_os("COMPOSED_FOLLOWUP").is_some();
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
                if profile && operation != "label"
                    || equal_only && operation != "equal"
                    || followup && (n != 3_000 || !matches!(operation, "label" | "equal"))
                {
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
                            "equal" if followup => {
                                window.update(|_, cx| store.dispatch(Change::Label(label), cx));
                            }
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

/// Headless CPU, from dispatch through completion of the enclosing window update.
/// The deferred marker runs after the Store's queued flush and before drawing.
#[test]
#[ignore = "run explicitly with --ignored --nocapture"]
fn composed_label_phase_sweep() {
    run_phase_sweep(false);
}

#[test]
#[ignore = "release measurement with a 768-pixel scrolling viewport"]
fn virtualized_label_phase_sweep() {
    run_phase_sweep(true);
}

fn run_phase_sweep(virtualized: bool) {
    let mut app = app();
    for n in [1000, 3000, 10_000] {
        if std::env::var("COMPOSED_PHASE_ROWS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .is_some_and(|rows| rows != n)
        {
            continue;
        }
        let probe = Probe::new(n);
        let all_visible = !virtualized && std::env::var_os("COMPOSED_ALL_VISIBLE").is_some();
        let (page, mut window) = if virtualized || all_visible {
            let height = if all_visible {
                (n + 1) as f32 * ROW_HEIGHT
            } else {
                800.
            };
            let handle = app
                .open_window(size(px(WIDTH), px(height)), |window, cx| {
                    cx.new(|cx| Page::new(true, virtualized, probe.clone(), window, cx))
                })
                .expect("mount virtual list");
            app.run_until_parked();
            let mut window = Mounted {
                app: &mut app,
                handle,
            };
            let page = window.update(|window, cx| {
                let page = window.root::<Page>().flatten().expect("virtual page");
                window.focus(&page.read(cx).input.focus_handle(cx), cx);
                page
            });
            (page, window)
        } else {
            fixture(&mut app, true, probe.clone())
        };
        let store = window.update(|_, cx| page.read(cx).store.clone());
        let mounted_count = window.update(|_, cx| match &page.read(cx).rows {
            Rows::Virtual(mounted) => mounted.borrow().len(),
            Rows::Cached(rows) => rows.len(),
            Rows::Inline(rows) => rows.len(),
        });
        if virtualized {
            assert!(
                (1..=30).contains(&mounted_count),
                "viewport mounted {mounted_count} rows"
            );
        }
        let edited_rows = [EDITED, n - 1];
        for &edited in &edited_rows[..if virtualized { 2 } else { 1 }] {
            let location = if edited == EDITED {
                "visible"
            } else {
                "offscreen"
            };
            if std::env::var("COMPOSED_PHASE_EDIT").is_ok_and(|wanted| wanted != location) {
                continue;
            }
            let mut totals = Vec::new();
            let mut flushes = Vec::new();
            let mut frames = Vec::new();
            let mut allocations = Vec::new();
            let samples = std::env::var("COMPOSED_PHASE_SAMPLES")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(31)
                .max(1);
            for i in 0..samples + 4 {
                let mark = Rc::new(Cell::new(0));
                let marker = mark.clone();
                let before = probe.rows[edited].get();
                let label = if i % 2 == 0 { "Row 00004" } else { "Row 00003" }.into();
                let ((start, end), counts) = count_allocations(|| {
                    if i >= 4 {
                        measured_label_frame(&mut window, &store, edited, label, marker)
                    } else {
                        label_frame(&mut window, &store, edited, label, marker)
                    }
                })
                .expect("count complete frame");
                assert_eq!(
                    probe.rows[edited].get(),
                    before + usize::from(edited == EDITED)
                );
                assert!((start..=end).contains(&mark.get()));
                if i >= 4 {
                    flushes.push(mark.get() - start);
                    frames.push(end - mark.get());
                    totals.push(end - start);
                    allocations.push(counts.allocations);
                }
            }
            let stats = window.update(|window, _| window.frame_patch_stats());
            println!(
                "rows={n} mounted={mounted_count} edited={edited} virtual={virtualized} all_visible={all_visible} median_ns total={} through_flush={} after_flush={} allocs={} patched={} refused={}",
                percentile(&mut totals, 50),
                percentile(&mut flushes, 50),
                percentile(&mut frames, 50),
                percentile(&mut allocations, 50),
                stats.patched,
                stats.refused
            );
        }
        close(window);
    }
}

// Keep a stable symbol for Callgrind to isolate one changed label frame.
#[inline(never)]
fn measured_label_frame(
    window: &mut Mounted<'_>,
    store: &Store<State>,
    edited: usize,
    label: SharedString,
    marker: Rc<Cell<u64>>,
) -> (u64, u64) {
    label_frame(window, store, edited, label, marker)
}

#[inline(always)]
fn label_frame(
    window: &mut Mounted<'_>,
    store: &Store<State>,
    edited: usize,
    label: SharedString,
    marker: Rc<Cell<u64>>,
) -> (u64, u64) {
    let start = cpu_ns();
    window.update(|_, cx| {
        store.dispatch(Change::RowLabel(edited, label), cx);
        cx.defer(move |_| marker.set(cpu_ns()));
    });
    (start, cpu_ns())
}
