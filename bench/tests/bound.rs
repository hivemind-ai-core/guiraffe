//! The composed label-edit benchmark written with `store!` and `ui!` bindings, plus the
//! binding layer's gates:
//!
//! - a steady-state binding update (lens write, flush, compare, notify) allocates nothing;
//! - a label edit costs the same at 1k, 3k and 10k rows (allocations, row renders, flush work);
//! - a structural edit at 10k rows notifies the list and one item;
//! - an unbound `ui!` page allocates exactly what the same builder page does.

use std::cell::{Cell, RefCell};

use gpui_kit::{
    App, AppContext, Context, Entity, HeadlessAppContext, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, WindowHandle,
    component::{Sizable, badge::Badge, checkbox::Checkbox},
    div, px, size,
};
use guiraffe_bench::allocator::{CountingAllocator, count_allocations};
use guiraffe_state::{Event, Fx, KeyedList, Lens as _, Store, store};
use guiraffe_ui::ui;
use nix::time::{ClockId, clock_gettime};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const ROW_HEIGHT: f32 = 32.;
const WIDTH: f32 = 420.;
const EDITED: u64 = 3;

#[derive(Clone, Debug, PartialEq, guiraffe_state::Lens)]
pub struct Row {
    pub label: SharedString,
    pub checked: bool,
    pub badge: usize,
}

store! {
    pub Table {
        rows: KeyedList<u64, Row> = KeyedList::new(),
        title: SharedString = SharedString::new_static("Rows"),
    }
}

#[derive(Debug)]
struct Toggle(u64);

impl Event<Table> for Toggle {
    fn handle(self, fx: &mut Fx<'_, Table>) {
        let lens = Table::rows.by_key(self.0).then(Row::checked);
        if let Some(checked) = lens.get(fx.state()).copied() {
            fx.set_at(lens, !checked);
        }
    }
}

#[derive(Debug)]
struct InsertRow(u64);

impl Event<Table> for InsertRow {
    fn handle(self, fx: &mut Fx<'_, Table>) {
        fx.list_insert(
            Table::rows,
            0,
            self.0,
            Row {
                label: "New".into(),
                checked: false,
                badge: 1,
            },
        );
    }
}

thread_local! {
    static ROW_RENDERS: Cell<usize> = const { Cell::new(0) };
    static PAGE_RENDERS: Cell<usize> = const { Cell::new(0) };
    static DERIVED_RENDERS: Cell<usize> = const { Cell::new(0) };
}

fn row_renders() -> usize {
    ROW_RENDERS.with(Cell::get)
}

struct Page {
    store: Store<Table>,
    mounted: bool,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        PAGE_RENDERS.with(|r| r.set(r.get() + 1));
        let store = self.store.clone();
        if self.mounted {
            return ui! {
                div {
                    flex flex_col w: px(WIDTH)
                    store(store) {
                        div { h: px(ROW_HEIGHT) {<= title} }
                        for (id, row) in <= rows, mounted, row_height: px(ROW_HEIGHT) {
                            div {
                                flex items_center gap_2 w: px(WIDTH) h: px(ROW_HEIGHT)
                                {
                                    ROW_RENDERS.with(|r| r.set(r.get() + 1));
                                    div().w(px(130.)).child(row.label.clone())
                                }
                                Checkbox("checked") {
                                    checked: row.checked
                                    on_click => Toggle(id)
                                }
                                Badge { count: row.badge  small  .child(div().w(px(30.)).h(px(18.))) }
                            }
                        }
                    }
                }
            }
            .into_any_element();
        }
        ui! {
            div {
                flex flex_col w: px(WIDTH) h: px(800.)
                store(store) {
                    div { h: px(ROW_HEIGHT) {<= title} }
                    div {
                        // A value derived from two inputs, one of them the edited row's label:
                        // it recomputes on every label edit and must not allocate.
                        let width <= derived(
                            (title, rows.by_key(EDITED).label),
                            |t, l| t.len() + l.len()
                        );
                        {
                            DERIVED_RENDERS.with(|r| r.set(r.get() + 1));
                            format!("{width}")
                        }
                    }
                    for row in <= rows, row_height: px(ROW_HEIGHT) {
                        div {
                            flex items_center gap_2 w: px(WIDTH) h: px(ROW_HEIGHT)
                            {
                                ROW_RENDERS.with(|r| r.set(r.get() + 1));
                                div().w(px(130.)).child(row.label.clone())
                            }
                            Checkbox("checked") { checked: row.checked }
                            Badge { count: row.badge  small  .child(div().w(px(30.)).h(px(18.))) }
                        }
                    }
                }
            }
        }
        .into_any_element()
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

fn table(n: u64) -> Table {
    Table {
        rows: (0..n)
            .map(|id| {
                (
                    id,
                    Row {
                        label: format!("Row {id:05}").into(),
                        checked: id % 2 == 0,
                        badge: id as usize % 9 + 1,
                    },
                )
            })
            .collect(),
        ..Table::default()
    }
}

fn mount(app: &mut HeadlessAppContext, n: u64, mounted: bool) -> (Store<Table>, Mounted<'_>) {
    let store = app.update(|cx| Store::new(table(n), cx));
    let height = if mounted {
        (n as f32 + 1.) * ROW_HEIGHT
    } else {
        800.
    };
    let handle = app
        .open_window(size(px(WIDTH), px(height)), {
            let store = store.clone();
            move |_, cx| cx.new(|_| Page { store, mounted })
        })
        .expect("mount benchmark window");
    app.run_until_parked();
    (store, Mounted { app, handle })
}

fn close(mut window: Mounted<'_>) {
    window.update(|window, _| window.remove_window());
    window.app.run_until_parked();
}

fn label(i: usize) -> SharedString {
    if i.is_multiple_of(2) {
        "Row 00004"
    } else {
        "Row 00003"
    }
    .into()
}

#[inline(never)]
fn bound_label_edit(window: &mut Mounted<'_>, store: &Store<Table>, row: u64, label: SharedString) {
    window.update(|_, cx| store.set(Table::rows.by_key(row).then(Row::label), label, cx));
}

/// Allocations of a whole visible-row edit frame and of the binding update alone.
fn edit_costs(app: &mut HeadlessAppContext, n: u64) -> (u64, u64, usize, FirstEdit) {
    let (store, mut window) = mount(app, n, false);
    // The first edit after mount, counted on its own: it must cost what later edits cost.
    let page = PAGE_RENDERS.with(Cell::get);
    let first_rows = row_renders();
    let update = window.update(|_, cx| {
        count_allocations(|| {
            store.set(Table::rows.by_key(EDITED).then(Row::label), label(0), cx);
        })
        .expect("allocation scope")
        .1
    });
    window.app.run_until_parked();
    let first = FirstEdit {
        update: update.allocations,
        allocations: 0,
        steady: 0,
        fresh_rows: 0,
        page_renders: PAGE_RENDERS.with(Cell::get) - page,
        row_renders: row_renders() - first_rows,
    };
    for i in 1..4 {
        bound_label_edit(&mut window, &store, EDITED, label(i));
        window.app.run_until_parked();
    }
    let rows = row_renders();
    let derived_renders = DERIVED_RENDERS.with(Cell::get);
    let mut frames = Vec::new();
    let mut updates = Vec::new();
    for i in 0..7 {
        // The binding update: dispatch, keyed flush, compare and notify. No frame.
        let counts = window.update(|_, cx| {
            count_allocations(|| {
                store.set(Table::rows.by_key(EDITED).then(Row::label), label(i), cx);
            })
            .expect("allocation scope")
            .1
        });
        updates.push(counts.allocations);
        // The frame that draws it.
        let (_, counts) = count_allocations(|| window.app.run_until_parked()).expect("scope");
        frames.push(counts.allocations);
    }
    let rows = row_renders() - rows;
    // Same-length labels: the derived width was recomputed on every edit and never changed.
    assert_eq!(DERIVED_RENDERS.with(Cell::get), derived_renders);
    // A steady edit as one scope (update plus frame), and the first edit of a row nobody has
    // edited yet, once the window's frame buffers are warm: the binding layer adds nothing to
    // a site's first edit.
    let (_, steady) = count_allocations(|| {
        bound_label_edit(&mut window, &store, EDITED, label(7));
        window.app.run_until_parked();
    })
    .expect("allocation scope");
    let fresh_rows = row_renders();
    // A label already shaped elsewhere, so text shaping does not count. The window draws the
    // edit as its update returns, so the scope covers update and frame.
    let (_, fresh) = count_allocations(|| {
        bound_label_edit(&mut window, &store, EDITED + 2, label(0));
        window.app.run_until_parked();
    })
    .expect("allocation scope");
    let fresh_rows = row_renders() - fresh_rows;
    close(window);
    frames.sort_unstable();
    updates.sort_unstable();
    let first = FirstEdit {
        steady: steady.allocations,
        allocations: fresh.allocations,
        fresh_rows,
        ..first
    };
    (updates[3], frames[3], rows, first)
}

#[derive(Debug)]
struct FirstEdit {
    /// The first edit's Store-side work (dispatch, flush, compare, notify).
    update: u64,
    /// A never-edited row's first edit, update plus frame.
    allocations: u64,
    fresh_rows: usize,
    steady: u64,
    page_renders: usize,
    row_renders: usize,
}

#[test]
fn steady_state_binding_updates_allocate_nothing_and_stay_flat() {
    let mut app = app();
    let mut frames = Vec::new();
    for n in [1_000, 3_000, 10_000] {
        let (update, frame, rows, first) = edit_costs(&mut app, n);
        if gpui_kit::GPUI_FAST {
            // The first edit after mount renders one row and not the page.
            assert_eq!(first.page_renders, 0, "first edit at {n} rows: {first:?}");
            assert_eq!(first.row_renders, 1, "first edit at {n} rows: {first:?}");
            // A row's first edit costs a steady edit; the binding layer adds nothing.
            assert_eq!(first.fresh_rows, 1, "first edit at {n} rows: {first:?}");
            assert!(
                first.allocations <= first.steady + 8,
                "first edit at {n} rows: {first:?}"
            );
        }
        // The first edit's Store-side work is as small as any edit's.
        assert!(first.update <= 8, "first edit at {n} rows: {first:?}");
        assert_eq!(
            update, 0,
            "binding update at {n} rows allocated {update} times"
        );
        if gpui_kit::GPUI_FAST {
            assert_eq!(rows, 7, "one row render per edit at {n} rows");
        }
        frames.push(frame);
    }
    let (min, max) = (frames.iter().min(), frames.iter().max());
    if let (Some(min), Some(max)) = (min, max) {
        assert!(
            max - min <= 8,
            "edit frame allocations must not grow with rows: {frames:?}"
        );
    }
}

#[test]
fn structural_edits_at_ten_thousand_rows_touch_the_list_and_one_item() {
    let mut app = app();
    let (store, mut window) = mount(&mut app, 10_000, false);
    let rows = row_renders();
    let counts = window.update(|_, cx| {
        count_allocations(|| store.dispatch_now(InsertRow(20_000), cx))
            .expect("scope")
            .1
    });
    // The flush itself: one persistent-vector path copy and the order snapshot, not O(n).
    assert!(counts.allocations <= 64, "insert flush: {counts:?}");
    window.app.run_until_parked();
    window.update(|_, cx| assert_eq!(store.state(cx).rows.key_at(0), Some(&20_000)));
    // The new row mounts and the shifted rows repaint at their new bounds: the cost is the
    // viewport (25 rows here), never the list.
    assert!(
        row_renders() - rows <= 60,
        "{} rows rendered",
        row_renders() - rows
    );
    close(window);
}

// ---- Unbound parity --------------------------------------------------------------------

struct Plain {
    rows: usize,
    macro_built: bool,
}

impl Render for Plain {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows;
        if self.macro_built {
            ui! {
                div {
                    flex flex_col
                    for id in 0..rows key id {
                        div {
                            flex gap_2 h: px(ROW_HEIGHT)
                            "Row"
                            Checkbox(("c", id)) { checked: id % 2 == 0 }
                        }
                    }
                }
            }
        } else {
            div().flex().flex_col().children((0..rows).map(|id| {
                guiraffe_ui::__macro::keyed(
                    id,
                    div()
                        .flex()
                        .gap_2()
                        .h(px(ROW_HEIGHT))
                        .child("Row")
                        .child(Checkbox::new(("c", id)).checked(id % 2 == 0)),
                )
            }))
        }
    }
}

fn plain_frame(app: &mut HeadlessAppContext, macro_built: bool) -> u64 {
    let handle = app
        .open_window(size(px(WIDTH), px(800.)), |_, cx| {
            cx.new(|_| Plain {
                rows: 1_000,
                macro_built,
            })
        })
        .expect("window");
    app.run_until_parked();
    let entity: Entity<Plain> = app
        .update_window(handle.into(), |_, window, _| {
            window.root::<Plain>().flatten()
        })
        .ok()
        .flatten()
        .expect("root");
    let mut counts = Vec::new();
    for _ in 0..5 {
        let (_, c) = count_allocations(|| {
            app.update(|cx| cx.notify(entity.entity_id()));
            app.run_until_parked();
        })
        .expect("scope");
        counts.push(c.allocations);
    }
    let _ = app.update_window(handle.into(), |_, window, _| window.remove_window());
    app.run_until_parked();
    counts.sort_unstable();
    counts[2]
}

#[test]
fn an_unbound_macro_page_costs_what_the_builder_page_costs() {
    let mut app = app();
    let builder = plain_frame(&mut app, false);
    let macro_built = plain_frame(&mut app, true);
    assert_eq!(builder, macro_built, "1k unbound controls, one re-render");
}

// ---- Timings (release; run explicitly) ---------------------------------------------------

fn cpu_ns() -> u64 {
    let time = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).expect("thread CPU clock");
    time.tv_sec() as u64 * 1_000_000_000 + time.tv_nsec() as u64
}

/// `cargo test --release -p guiraffe-bench --test bound bound_label_sweep -- --ignored --nocapture`
#[test]
#[ignore = "release measurement; run explicitly with --ignored --nocapture"]
fn bound_label_sweep() {
    let mut app = app();
    let rows = RefCell::new(Vec::new());
    for mounted in [false, true] {
        for n in [1_000u64, 3_000, 10_000] {
            if mounted && n == 10_000 && !gpui_kit::GPUI_FAST {
                continue;
            }
            let (store, mut window) = mount(&mut app, n, mounted);
            let mut times = Vec::new();
            let mut allocs = Vec::new();
            for i in 0..35 {
                let (elapsed, counts) = count_allocations(|| {
                    let start = cpu_ns();
                    bound_label_edit(&mut window, &store, EDITED, label(i));
                    window.app.run_until_parked();
                    cpu_ns() - start
                })
                .expect("scope");
                if i >= 4 {
                    times.push(elapsed);
                    allocs.push(counts.allocations);
                }
            }
            times.sort_unstable();
            allocs.sort_unstable();
            println!(
                "bound mounted={mounted} rows={n} median_ns={} median_allocs={}",
                times[times.len() / 2],
                allocs[allocs.len() / 2]
            );
            rows.borrow_mut().push((mounted, n, times[times.len() / 2]));
            close(window);
        }
    }
}
