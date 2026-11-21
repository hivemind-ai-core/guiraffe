use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, px,
};
use guiraffe_components::{SegmentedControl, SegmentedEvent};
use guiraffe_sheet::{
    Alignment, Cell, CellFormat, CellPos, NumberFormat, Sheet, SheetModel, SheetState,
};

use crate::catalog::Entry;

pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "sheet",
        name: "Spreadsheet",
        summary: "A sparse million-row sheet with frozen panes, range selection, fill and formulas.",
        covers: &["Sheet"],
        demo,
    }]
}

struct Demo {
    sheet: Entity<SheetState>,
    access: Entity<SegmentedControl>,
    panes: Entity<SegmentedControl>,
    _subscriptions: [Subscription; 2],
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        super::column([
            super::example(
                "Options",
                div()
                    .flex()
                    .gap_3()
                    .child(self.access.clone())
                    .child(self.panes.clone()),
            ),
            super::example(
                "Quarterly budget · Double-click a cell to edit, or drag the selection corner to fill.",
                div().h(px(380.)).child(Sheet::new(&self.sheet)),
            ),
        ])
    }
}

fn demo(_: &mut Window, cx: &mut App) -> AnyView {
    let mut entries = Vec::new();
    for (col, title) in ["Category", "January", "February", "March", "Quarter"]
        .into_iter()
        .enumerate()
    {
        entries.push((CellPos::new(0, col), Cell::new(title)));
    }
    for (row, (category, values)) in [
        ("Rent", [1240, 1240, 1240]),
        ("Supplies", [230, 415, 310]),
        ("Shipping", [532, 492, 611]),
        ("Software", [285, 305, 305]),
    ]
    .into_iter()
    .enumerate()
    {
        let row = row + 1;
        entries.push((CellPos::new(row, 0), Cell::new(category)));
        for (index, value) in values.into_iter().chain([values.iter().sum()]).enumerate() {
            let mut cell = Cell::new(value.to_string());
            cell.format = CellFormat {
                number: NumberFormat::Number(2),
                align: Alignment::Right,
            };
            entries.push((CellPos::new(row, index + 1), cell));
        }
    }
    let mut model = SheetModel::from_cells(1_000_000, 1000, entries);
    model.select(CellPos::new(2, 1), false);
    model.select(CellPos::new(2, 3), true);
    let sheet = cx.new(|cx| SheetState::new(model, cx));
    let access = SegmentedControl::new(["Editable", "Read only"], cx);
    let panes = SegmentedControl::new(["Frozen headers", "Unfrozen"], cx);
    cx.new(|cx| {
        let access_sub = cx.subscribe(&access, |demo: &mut Demo, _, event: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(index) = *event;
            demo.sheet
                .update(cx, |sheet, cx| sheet.set_read_only(index == 1, cx));
        });
        let panes_sub = cx.subscribe(&panes, |demo: &mut Demo, _, event: &SegmentedEvent, cx| {
            let SegmentedEvent::Changed(index) = *event;
            demo.sheet.update(cx, |sheet, cx| {
                let frozen = usize::from(index == 0);
                sheet.freeze(frozen, frozen, cx);
            });
        });
        sheet.update(cx, |sheet, cx| sheet.freeze(1, 1, cx));
        Demo {
            sheet,
            access,
            panes,
            _subscriptions: [access_sub, panes_sub],
        }
    })
    .into()
}
