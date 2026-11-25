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
use guiraffe_state::{Event, Fx, KeyedList, Lens as _, ListLens, Store, store};
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
    if i % 2 == 0 { "Row 00004" } else { "Row 00003" }.into()
}

#[inline(never)]
fn bound_label_edit(window: &mut Mounted<'_>, store: &Store<Table>, row: u64, label: SharedString) {
    window.update(|_, cx| store.set(Table::rows.by_key(row).then(Row::label), label, cx));
}

/// Allocations of a whole visible-row edit frame and of the binding update alone.
fn edit_costs(app: &mut HeadlessAppContext, n: u64) -> (u64, u64, usize) {
    let (store, mut window) = mount(app, n, false);
    for i in 0..4 {
        bound_label_edit(&mut window, &store, EDITED, label(i));
        window.app.run_until_parked();
    }
    let rows = row_renders();
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
    close(window);
    frames.sort_unstable();
    updates.sort_unstable();
    (updates[3], frames[3], rows)
}

#[test]
fn steady_state_binding_updates_allocate_nothing_and_stay_flat() {
    let mut app = app();
    let mut frames = Vec::new();
    for n in [1_000, 3_000, 10_000] {
        let (update, frame, rows) = edit_costs(&mut app, n);
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
    if gpui_kit::GPUI_FAST {
        // The new row renders; the others keep their views (they shift by one row).
        assert!(
            row_renders() - rows <= 2,
            "{} rows rendered",
            row_renders() - rows
        );
    }
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
