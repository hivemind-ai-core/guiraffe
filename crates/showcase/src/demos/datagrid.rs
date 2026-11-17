//! guiraffe-datagrid: a million computed rows with live knobs, plus a small editable grid.

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, px,
};
use guiraffe_components::{SegmentedControl, SegmentedEvent, Text};
use guiraffe_core::{Density, Size};
use guiraffe_datagrid::{
    Align, CellRef, ColumnDef, DataGrid, EditError, Filter, FnSource, GridEvent, GridHandle,
    GridOptions, Pin, SelectionMode, SortDir, SortKey, VecSource,
};
use std::sync::Arc;

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{SIZE_NAMES, SIZES};

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "datagrid",
        name: "Data grid",
        summary: "Virtualized rows and columns over 1,000,000 computed rows: pinned, resizable, reorderable columns; multi-sort; row/cell/range selection; keyboard navigation; inline editing.",
        covers: &["DataGrid"],
        demo,
    }]
}

const CITIES: [&str; 8] = [
    "Amsterdam",
    "Berlin",
    "Cairo",
    "Dakar",
    "Lima",
    "Oslo",
    "Seoul",
    "Tokyo",
];
const STATUS: [&str; 4] = ["active", "paused", "pending", "closed"];
const ROWS: usize = 1_000_000;
const COLS: usize = 50;

/// Every cell is computed from `(row, col)` on demand; nothing is stored.
fn cell(row: usize, col: usize) -> CellRef<'static> {
    let h = row
        .wrapping_mul(2_654_435_761)
        .wrapping_add(col.wrapping_mul(40_503));
    match col {
        0 => CellRef::Int(row as i64 + 1),
        1 => CellRef::Text(CITIES[h % CITIES.len()]),
        2 => CellRef::Text(STATUS[(h >> 3) % STATUS.len()]),
        3 => CellRef::Number(((h >> 5) % 100_000) as f64 / 100.),
        4 => CellRef::Bool(h.is_multiple_of(3)),
        _ => CellRef::Int(((h >> 7) % 10_000) as i64),
    }
}

fn big_columns(pinned: usize) -> Vec<ColumnDef> {
    (0..COLS)
        .map(|c| {
            let def = match c {
                0 => ColumnDef::new("id", "ID", c).width(90.),
                1 => ColumnDef::new("city", "City", c).width(130.),
                2 => ColumnDef::new("status", "Status", c).width(110.),
                3 => ColumnDef::new("amount", "Amount", c)
                    .width(120.)
                    .formatter(|v, out| {
                        if let CellRef::Number(n) = v {
                            let _ = std::fmt::Write::write_fmt(out, format_args!("${n:.2}"));
                        }
                    })
                    .align(Align::End),
                4 => ColumnDef::new("flag", "Flag", c)
                    .width(80.)
                    .formatter(|v, out| {
                        out.push_str(if matches!(v, CellRef::Bool(true)) {
                            "yes"
                        } else {
                            "no"
                        })
                    }),
                _ => ColumnDef::new(format!("m{c}"), format!("Metric {}", c - 4), c).width(110.),
            };
            let pin = match (pinned, c) {
                (1 | 2, 0) => Pin::Left,
                (2, 2) => Pin::Right,
                _ => Pin::None,
            };
            def.pin(pin)
        })
        .collect()
}

struct Person {
    name: String,
    role: String,
    age: i64,
}

fn people() -> VecSource<Person> {
    let rows = [
        ("Ada Lovelace", "Analyst", 36),
        ("Grace Hopper", "Admiral", 85),
        ("Alan Turing", "Mathematician", 41),
        ("Katherine Johnson", "Engineer", 101),
        ("Edsger Dijkstra", "Professor", 72),
        ("Barbara Liskov", "Professor", 86),
    ]
    .into_iter()
    .map(|(n, r, a)| Person {
        name: n.into(),
        role: r.into(),
        age: a,
    })
    .collect();
    VecSource::new(rows)
        .editable_column(
            |p: &Person| CellRef::Text(&p.name),
            |p, t| {
                p.name = t.to_string();
                Ok(())
            },
        )
        .editable_column(
            |p: &Person| CellRef::Text(&p.role),
            |p, t| {
                p.role = t.to_string();
                Ok(())
            },
        )
        .editable_column(
            |p: &Person| CellRef::Int(p.age),
            |p, t| {
                p.age = t
                    .trim()
                    .parse()
                    .map_err(|_| EditError::Invalid(format!("{t:?} is not a whole number")))?;
                Ok(())
            },
        )
}

struct Demo {
    big: GridHandle,
    small: GridHandle,
    knobs: Vec<(&'static str, Entity<SegmentedControl>)>,
    log: Entity<EventLog>,
    _subs: Vec<Subscription>,
}

const KNOBS: &[(&str, &[&str], usize)] = &[
    ("Density", &["compact", "comfortable", "spacious"], 1),
    ("Zebra", &["off", "on"], 1),
    ("Pinned", &["none", "left", "left + right"], 1),
    ("Selection", &["single", "multi", "cell", "range"], 1),
    ("Size", SIZE_NAMES, 2),
    (
        "Sort",
        &["none", "amount ↑", "amount ↓", "city ↑ + amount ↓"],
        0,
    ),
    ("Filter flag", &["all", "yes", "no"], 0),
    ("Hide metrics", &["off", "on"], 0),
];

impl Demo {
    fn apply(big: &GridHandle, knob: usize, index: usize, cx: &mut App) {
        let mut o = big.options();
        match knob {
            0 => {
                o.density = Some(match index {
                    0 => Density::Compact,
                    2 => Density::Spacious,
                    _ => Density::Comfortable,
                })
            }
            1 => o.zebra = index == 1,
            4 => o.size = SIZES.get(index).copied(),
            5 => {
                let key = |column, dir| SortKey { column, dir };
                let keys = match index {
                    1 => vec![key(3, SortDir::Asc)],
                    2 => vec![key(3, SortDir::Desc)],
                    3 => vec![key(1, SortDir::Asc), key(3, SortDir::Desc)],
                    _ => vec![],
                };
                big.set_sort(keys, cx);
                return;
            }
            6 => {
                let filter: Option<Filter> = match index {
                    1 => Some(Arc::new(|v| matches!(v, CellRef::Bool(true)))),
                    2 => Some(Arc::new(|v| matches!(v, CellRef::Bool(false)))),
                    _ => None,
                };
                big.set_filter(4, filter, cx);
                return;
            }
            7 => {
                for def in 5..COLS {
                    big.set_column_visible(def, index == 0, cx);
                }
                return;
            }
            2 => {
                for (def, col) in big_columns(index).into_iter().enumerate() {
                    big.set_column_pin(def, col.pin, cx);
                }
                return;
            }
            3 => {
                o.selection = match index {
                    0 => SelectionMode::SingleRow,
                    2 => SelectionMode::Cell,
                    3 => SelectionMode::Range,
                    _ => SelectionMode::MultiRow,
                }
            }
            _ => return,
        }
        big.set_options(o, cx);
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Options",
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(20.))
                    .gap_y(px(10.))
                    .children(self.knobs.iter().map(|(label, c)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(Text::new(*label).size(Size::Xs).dimmed(true))
                            .child(c.clone())
                    })),
            ),
            example(
                "1,000,000 rows × 50 columns, computed on demand · click a header to sort (Shift adds a key) · drag edges to resize, headers to reorder",
                div().h(px(300.)).child(DataGrid::new(&self.big)),
            ),
            example(
                "Editable: double-click or F2 to edit, Enter commits, Esc cancels (Age must be a number)",
                div().h(px(250.)).child(DataGrid::new(&self.small)),
            ),
            example("Events from both grids", self.log.clone()),
        ])
    }
}

fn demo(_: &mut Window, cx: &mut App) -> AnyView {
    let big = GridHandle::new(FnSource::new(ROWS, cell), big_columns(1), cx);
    big.set_options(
        GridOptions {
            selection: SelectionMode::MultiRow,
            focusable: Some(true),
            label: "Orders".into(),
            ..Default::default()
        },
        cx,
    );
    let small = GridHandle::new(
        people(),
        vec![
            ColumnDef::new("name", "Name", 0).width(200.).editable(true),
            ColumnDef::new("role", "Role", 1).width(160.).editable(true),
            ColumnDef::new("age", "Age", 2).width(90.).editable(true),
        ],
        cx,
    );
    small.set_options(
        GridOptions {
            selection: SelectionMode::Cell,
            focusable: Some(true),
            label: "People".into(),
            ..Default::default()
        },
        cx,
    );
    let knobs: Vec<_> = KNOBS
        .iter()
        .map(|(label, options, default)| {
            let c = SegmentedControl::new(options.iter().copied(), cx);
            c.update(cx, |c, cx| {
                c.set_size(Some(Size::Xs), cx);
                c.select(*default, cx);
            });
            (*label, c)
        })
        .collect();
    let log = EventLog::new(cx);
    for (name, grid) in [("orders", &big), ("people", &small)] {
        EventLog::watch_with(
            &log,
            grid.events(),
            move |e: &GridEvent| match e {
                GridEvent::SortChanged(keys) => {
                    let k: Vec<_> = keys
                        .iter()
                        .map(|k| format!("{}:{:?}", k.column, k.dir))
                        .collect();
                    format!("{name}: SortChanged [{}]", k.join(", ")).into()
                }
                other => format!("{name}: {other:?}").into(),
            },
            cx,
        );
    }
    cx.new(|cx| {
        let subs = knobs
            .iter()
            .enumerate()
            .map(|(i, (_, c))| {
                let big = big.clone();
                cx.subscribe(c, move |_, _, e: &SegmentedEvent, cx| {
                    let SegmentedEvent::Changed(index) = e;
                    Demo::apply(&big, i, *index, cx);
                })
            })
            .collect();
        Demo {
            big,
            small,
            knobs,
            log,
            _subs: subs,
        }
    })
    .into()
}
