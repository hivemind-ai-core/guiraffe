//! Where a re-rendered view's allocations go on gpui-fast: a measurement for
//! `docs/performance.md` ("Allocations of a re-rendered view").
//!
//! A page of cached views is mounted, then one of them is notified and drawn
//! again `RERENDER_ITERATIONS` times (400 by default). The test prints the
//! median allocations per re-render. Under `valgrind --tool=dhat` the sites
//! hit once per re-render stand out against mounting by their counts: see
//! `scripts/rerender-sites.py`, which runs this and attributes them.
//!
//! `RERENDER_VIEW` picks the view: `empty` (a view rendering one `div`),
//! `plain` (an id, a click listener and a text label) or `component` (the
//! composed benchmark's row: a label, a checkbox, a badge and a button), each
//! notified without new data; or `store`, the plain view showing a keyed
//! Store selection, its label edited through the Store each time.

use gpui_kit::{
    AnyView, AppContext, Context, Entity, HeadlessAppContext, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, StyleRefinement, Styled,
    Window,
    component::{Sizable, badge::Badge, button::Button, checkbox::Checkbox},
    div, px, size,
};
use guiraffe_bench::allocator::{CountingAllocator, count_allocations};
use guiraffe_state::{Event, Fx, Selected, Store};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const ROWS: usize = 40;
const ROW_HEIGHT: f32 = 32.;
const WIDTH: f32 = 420.;
const EDITED: usize = 3;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Empty,
    Plain,
    Component,
    Store,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Empty => "empty",
            Kind::Plain => "plain",
            Kind::Component => "component",
            Kind::Store => "store",
        }
    }
}

#[derive(Clone, PartialEq)]
struct Labels(Vec<SharedString>);

#[derive(Debug)]
struct SetLabel(usize, &'static str);

impl Event<Labels> for SetLabel {
    fn handle(self, fx: &mut Fx<'_, Labels>) {
        let SetLabel(row, label) = self;
        fx.set_keyed_field(
            row as u64,
            SharedString::from(label),
            move |labels| &labels.0[row],
            move |labels| &mut labels.0[row],
        );
    }
}

struct RowView {
    id: usize,
    kind: Kind,
    label: SharedString,
    selected: Option<Selected<SharedString>>,
}

impl Render for RowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id;
        if let Some(selected) = &self.selected {
            self.label = selected.get(cx).clone();
        }
        match self.kind {
            Kind::Empty => div().w(px(WIDTH)).h(px(ROW_HEIGHT)).into_any_element(),
            Kind::Plain | Kind::Store => div()
                .id(("plain", id))
                .w(px(WIDTH))
                .h(px(ROW_HEIGHT))
                .on_click(move |_, _, _| {
                    std::hint::black_box(id);
                })
                .child(self.label.clone())
                .into_any_element(),
            Kind::Component => div()
                .id(("row", id))
                .flex()
                .items_center()
                .gap_2()
                .w(px(WIDTH))
                .h(px(ROW_HEIGHT))
                .child(div().w(px(130.)).child(self.label.clone()))
                .child(Checkbox::new("checked").checked(id % 2 == 0))
                .child(
                    Badge::new()
                        .count(id)
                        .small()
                        .child(div().w(px(30.)).h(px(18.))),
                )
                .child(Button::new("toggle").label("Toggle").small())
                .into_any_element(),
        }
    }
}

struct Page {
    rows: Vec<Entity<RowView>>,
    store: Store<Labels>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(WIDTH))
            .children(self.rows.iter().map(|row| {
                AnyView::from(row.clone()).cached(
                    StyleRefinement::default()
                        .w(px(WIDTH))
                        .h(px(ROW_HEIGHT))
                        .flex_shrink_0(),
                )
            }))
    }
}

fn kinds() -> Vec<Kind> {
    match std::env::var("RERENDER_VIEW").as_deref() {
        Ok("empty") => vec![Kind::Empty],
        Ok("plain") => vec![Kind::Plain],
        Ok("component") => vec![Kind::Component],
        Ok("store") => vec![Kind::Store],
        _ => vec![Kind::Empty, Kind::Plain, Kind::Component, Kind::Store],
    }
}

fn app() -> HeadlessAppContext {
    let text = gpui_kit::platform::current_platform(true).text_system();
    let mut app =
        HeadlessAppContext::with_asset_source(text, std::sync::Arc::new(gpui_kit::assets::Assets));
    app.update(guiraffe_components::init);
    app
}

/// Mounts a page of `kind` rows, re-renders one of them `iterations` times,
/// and returns the median, least and most allocations per re-render, with the
/// window's patch stats.
fn measure(
    app: &mut HeadlessAppContext,
    kind: Kind,
    iterations: usize,
) -> (u64, u64, u64, gpui_kit::PatchStats) {
    let handle = app
        .open_window(
            size(px(WIDTH), px((ROWS + 1) as f32 * ROW_HEIGHT)),
            move |_, cx| {
                cx.new(|cx| {
                    let store = Store::new(
                        Labels((0..ROWS).map(|id| format!("Row {id:05}").into()).collect()),
                        cx,
                    );
                    Page {
                        rows: (0..ROWS)
                            .map(|id| {
                                let store = store.clone();
                                // The selection is the row's, so that its
                                // refresh notifies the row.
                                cx.new(move |cx| RowView {
                                    id,
                                    kind,
                                    label: format!("Row {id:05}").into(),
                                    selected: (kind == Kind::Store).then(|| {
                                        store.select_key(id as u64, cx, move |labels: &Labels| {
                                            labels.0[id].clone()
                                        })
                                    }),
                                })
                            })
                            .collect(),
                        store,
                    }
                })
            },
        )
        .expect("open window");
    app.run_until_parked();
    let (row, store) = app
        .update_window(handle.into(), |_, window, cx| {
            let page = window.root::<Page>().flatten().expect("page");
            let page = page.read(cx);
            (page.rows[EDITED].clone(), page.store.clone())
        })
        .expect("live window");
    let edits = std::cell::Cell::new(0usize);
    let rerender = |app: &mut HeadlessAppContext| {
        app.update_window(handle.into(), |_, _, cx| {
            if kind == Kind::Store {
                edits.set(edits.get() + 1);
                let label = if edits.get() % 2 == 0 {
                    "Row 00003"
                } else {
                    "Row 00004"
                };
                store.dispatch(SetLabel(EDITED, label), cx);
            } else {
                row.update(cx, |_, cx| cx.notify());
            }
        })
        .expect("live window");
    };
    for _ in 0..8 {
        rerender(app);
    }
    let mut counts: Vec<u64> = (0..iterations)
        .map(|_| {
            count_allocations(|| rerender(app))
                .expect("allocation scope")
                .1
                .allocations
        })
        .collect();
    counts.sort_unstable();
    let stats = app
        .update_window(handle.into(), |_, window, _| window.frame_patch_stats())
        .expect("live window");
    app.update_window(handle.into(), |_, window, _| window.remove_window())
        .expect("live window");
    app.run_until_parked();
    (
        counts[counts.len() / 2],
        counts[0],
        counts[counts.len() - 1],
        stats,
    )
}

#[test]
#[ignore = "measurement; run explicitly with --ignored --nocapture"]
fn rerendered_view_allocations() {
    let iterations = std::env::var("RERENDER_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(400usize);
    let mut app = app();
    for kind in kinds() {
        let (median, least, most, stats) = measure(&mut app, kind, iterations);
        println!(
            "view={} rerenders={iterations} median_allocations={median} min={least} max={most} patched={} refused={}",
            kind.name(),
            stats.patched,
            stats.refused
        );
    }
}

/// What a label edited through a keyed Store selection allocates over the
/// same view drawn again unchanged: on gpui-fast, the Store's event and its
/// deferred flush, and measuring and laying out the changed label. Upstream
/// draws frames in full and does not patch them, and its edit costs 27 more in
/// the test profile. See docs/performance.md.
const STORE_EDIT_EXTRA: u64 = if gpui_kit::GPUI_FAST { 6 } else { 27 };

/// A patched row edit allocates what an unchanged redraw of the row does,
/// plus the edit's own [`STORE_EDIT_EXTRA`], on both backends; on gpui-fast
/// both are patched.
#[test]
fn patched_edit_allocates_an_unchanged_redraw_and_the_edit() {
    let mut app = app();
    let (redraw, ..) = measure(&mut app, Kind::Plain, 40);
    let (edit, _, _, stats) = measure(&mut app, Kind::Store, 40);
    assert!(
        edit <= redraw + STORE_EDIT_EXTRA,
        "an edit allocates {edit}, an unchanged redraw {redraw}"
    );
    if gpui_kit::GPUI_FAST {
        assert_eq!(stats.refused, 0, "{stats:?}");
        assert!(stats.patched >= 40, "{stats:?}");
    }
}
