//! Step 4: a virtualised data grid with sorting and filtering.
//!
//! `cargo run -p getting-started-examples --example 04_datagrid --features gpui-fast`

use std::{fmt::Write as _, sync::Arc};

use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, WindowOptions,
    component::input::{Input, InputEvent, InputState},
    div, px,
};
use guiraffe_components::{SegmentedControl, SegmentedEvent, Text, Title};
use guiraffe_core::{GuiraffeTheme, Size};
use guiraffe_datagrid::{
    Align, CellRef, ColumnDef, DataGrid, Filter, GridEvent, GridHandle, GridOptions, SortDir,
    VecSource,
};

const ROWS: usize = 5_000;
const CUSTOMERS: [&str; 8] = [
    "Ada",
    "Grace",
    "Alan",
    "Katherine",
    "Edsger",
    "Barbara",
    "Linus",
    "Margaret",
];
const CITIES: [&str; 6] = ["Amsterdam", "Berlin", "Lima", "Oslo", "Seoul", "Tokyo"];
const STATUSES: [&str; 3] = ["open", "shipped", "returned"];

/// Your own row type. The grid borrows cells from it; nothing is copied.
struct Order {
    id: i64,
    customer: String,
    city: &'static str,
    status: &'static str,
    amount: f64,
    paid: bool,
}

/// Deterministic sample data (no RNG dependency).
fn orders() -> Vec<Order> {
    (0..ROWS)
        .map(|i| {
            let h = i.wrapping_mul(2_654_435_761);
            Order {
                id: i as i64 + 1,
                customer: format!("{} #{}", CUSTOMERS[h % CUSTOMERS.len()], (h >> 4) % 100),
                city: CITIES[(h >> 8) % CITIES.len()],
                status: STATUSES[(h >> 12) % STATUSES.len()],
                amount: ((h >> 3) % 100_000) as f64 / 100.,
                paid: h % 3 != 0,
            }
        })
        .collect()
}

/// Column definitions point at source columns by index (the order of `.column` calls).
fn grid(cx: &mut App) -> GridHandle {
    let source = VecSource::new(orders())
        .column(|o: &Order| CellRef::Int(o.id))
        .column(|o: &Order| CellRef::Text(&o.customer))
        .column(|o: &Order| CellRef::Text(o.city))
        .column(|o: &Order| CellRef::Text(o.status))
        .column(|o: &Order| CellRef::Number(o.amount))
        .column(|o: &Order| CellRef::Bool(o.paid))
        .key(|o: &Order| o.id as u64);
    let columns = vec![
        ColumnDef::new("id", "ID", 0).width(80.),
        ColumnDef::new("customer", "Customer", 1).width(180.),
        ColumnDef::new("city", "City", 2),
        ColumnDef::new("status", "Status", 3).width(110.),
        ColumnDef::new("amount", "Amount", 4)
            .align(Align::End)
            .formatter(|cell, out| {
                if let CellRef::Number(n) = cell {
                    // Formatters append to a buffer the grid passes in.
                    let _ = write!(out, "${n:.2}");
                }
            }),
        ColumnDef::new("paid", "Paid", 5)
            .width(80.)
            .formatter(|cell, out| {
                out.push_str(if matches!(cell, CellRef::Bool(true)) {
                    "yes"
                } else {
                    "no"
                })
            }),
    ];
    let handle = GridHandle::new(source, columns, cx);
    handle.set_options(
        GridOptions {
            focusable: Some(true),
            label: "Orders".into(),
            ..Default::default()
        },
        cx,
    );
    handle
}

struct Orders {
    grid: GridHandle,
    search: Entity<InputState>,
    status: Entity<SegmentedControl>,
    sort: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl Orders {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let grid = grid(cx);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by customer"));
        let status = SegmentedControl::new(["all"].into_iter().chain(STATUSES), cx);
        let _subscriptions = vec![
            // Typing in the search box filters the Customer column (definition 1).
            cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let needle = input.read(cx).value().trim().to_lowercase();
                    let filter: Option<Filter> = (!needle.is_empty()).then(|| {
                        Arc::new(move |cell: CellRef<'_>| {
                            matches!(cell, CellRef::Text(s) if s.to_lowercase().contains(&needle))
                        }) as Filter
                    });
                    this.grid.set_filter(1, filter, cx);
                    cx.notify();
                }
            }),
            // The segmented control filters the Status column (definition 3).
            cx.subscribe(&status, |this, _, event: &SegmentedEvent, cx| {
                let SegmentedEvent::Changed(index) = *event;
                let filter: Option<Filter> = index
                    .checked_sub(1)
                    .and_then(|i| STATUSES.get(i).copied())
                    .map(|wanted| {
                        Arc::new(move |cell: CellRef<'_>| cell == CellRef::Text(wanted)) as Filter
                    });
                this.grid.set_filter(3, filter, cx);
                cx.notify();
            }),
            // Clicking a header sorts (Shift+click adds a key); the grid tells us.
            cx.subscribe(grid.events(), |this, _, event: &GridEvent, cx| {
                if let GridEvent::SortChanged(keys) = event {
                    let columns = this.grid.columns();
                    this.sort = keys
                        .iter()
                        .filter_map(|k| {
                            let title = &columns.get(k.column)?.title;
                            let arrow = if k.dir == SortDir::Asc { "↑" } else { "↓" };
                            Some(format!("{title} {arrow}"))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                        .into();
                    cx.notify();
                }
            }),
        ];
        Self {
            grid,
            search,
            status,
            sort: SharedString::default(),
            _subscriptions,
        }
    }
}

impl Render for Orders {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let shown = self.grid.view_row_count();
        let sort = if self.sort.is_empty() {
            "click a header to sort".into()
        } else {
            format!("sorted by {}", self.sort)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(Size::Lg.spacing()))
            .bg(s.body.to_hsla())
            .child(Title::new("Orders").order(2))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(div().w(px(240.)).child(Input::new(&self.search)))
                    .child(self.status.clone()),
            )
            .child(Text::new(format!("{shown} of {ROWS} orders · {sort}")).dimmed(true))
            // The grid fills the remaining space and builds only the visible rows.
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(DataGrid::new(&self.grid)),
            )
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            guiraffe_components::init(cx);
            guiraffe_datagrid::init(cx);
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let orders = cx.new(|cx| Orders::new(window, cx));
                cx.new(|cx| gpui_kit::component::Root::new(orders, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the orders window: {error}");
                cx.quit();
            }
        });
}
