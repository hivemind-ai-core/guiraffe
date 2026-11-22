//! DataUi adapter for the Sheet.
//!
//! The document owns the populated cells. When the `cells` prop changes, only the difference
//! against the live model is written, so the user's selection, scroll position and column
//! sizes survive a round trip through the `edit` event and a bound slot.

use std::{collections::BTreeMap, sync::Arc};

use gpui_kit::{
    App, AppContext as _, ElementId, Entity, IntoElement, ParentElement, RenderOnce, Styled,
    Subscription, Window, div, px,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value as Data,
};

use crate::{
    Alignment, Cell, CellFormat, CellPos, NumberFormat, Sheet, SheetEvent, SheetModel, SheetState,
    a1, parse_a1, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| SheetDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Data) -> impl Iterator<Item = &Data> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn field<'a>(value: &'a Data, name: &str) -> &'a Data {
    value.field(&[name.into()])
}

fn count(value: &Data) -> Option<usize> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v <= 1e9 && v.fract() == 0.0)
        .map(|v| v as usize)
}

/// Parse one `sheet-cells` entry. Entries with an invalid address are skipped.
pub(crate) fn parse_cell(value: &Data) -> Option<(CellPos, Cell)> {
    let pos = parse_a1(field(value, "cell").as_str()?)?;
    let text = field(value, "text").as_str()?;
    let number = match field(value, "format").as_str() {
        Some("number") => NumberFormat::Number(
            count(field(value, "decimals")).map_or(2, |decimals| decimals.min(15) as u8),
        ),
        Some("date") => NumberFormat::Date,
        Some("text") => NumberFormat::Text,
        _ => NumberFormat::General,
    };
    let align = match field(value, "align").as_str() {
        Some("center") => Alignment::Center,
        Some("right") => Alignment::Right,
        _ => Alignment::Left,
    };
    Some((
        pos,
        Cell {
            text: Arc::from(text),
            format: CellFormat { number, align },
        },
    ))
}

/// The cells a document declares; the last entry for an address wins.
pub(crate) fn parse_cells(values: &Data) -> BTreeMap<CellPos, Cell> {
    items(values).filter_map(parse_cell).collect()
}

/// Writes that turn `model` into exactly `cells`: changed or new cells, and clears for
/// populated cells the document no longer lists.
pub(crate) fn diff(model: &SheetModel, cells: &BTreeMap<CellPos, Cell>) -> Vec<(CellPos, Cell)> {
    let mut writes: Vec<_> = cells
        .iter()
        .filter(|(pos, cell)| model.contains(**pos) && model.cell(**pos) != Some(*cell))
        .map(|(pos, cell)| (*pos, cell.clone()))
        .collect();
    writes.extend(
        model
            .cells()
            .filter(|(pos, _)| !cells.contains_key(pos))
            .map(|(pos, _)| (pos, Cell::default())),
    );
    writes
}

fn edit_payload(model: &SheetModel, changed: &[CellPos]) -> Data {
    Data::List(
        changed
            .iter()
            .map(|pos| {
                let text = model
                    .cell(*pos)
                    .map_or_else(|| Arc::from(""), |cell| cell.text.clone());
                Data::Record(Arc::new(BTreeMap::from([
                    ("cell".into(), a1(*pos).into()),
                    ("text".into(), text.as_ref().into()),
                ])))
            })
            .collect::<Vec<_>>()
            .into(),
    )
}

#[derive(IntoElement)]
struct SheetDocument {
    key: ElementId,
    dimensions: (usize, usize),
    cells: Data,
    read_only: bool,
    frozen: (usize, usize),
    height: f32,
    handler: Option<EventHandler>,
}

impl SheetDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        let extent = |index, default: usize| count(node.prop(index)).unwrap_or(default).max(1);
        Self {
            key: node.state_id(),
            dimensions: (extent(0, 1000), extent(1, 26)),
            cells: node.prop(2).clone(),
            read_only: node.prop(3).as_bool().unwrap_or_default(),
            frozen: (
                count(node.prop(4)).unwrap_or_default(),
                count(node.prop(5)).unwrap_or_default(),
            ),
            height: node
                .prop(6)
                .as_f64()
                .filter(|v| v.is_finite())
                .map_or(320., |v| v.clamp(48., 4096.) as f32),
            handler: node.handler(0),
        }
    }
}

struct SheetDocumentState {
    dimensions: (usize, usize),
    cells: Data,
    sheet: Entity<SheetState>,
    handler: Option<EventHandler>,
    _subscription: Subscription,
}

fn make_sheet(
    dimensions: (usize, usize),
    cells: &Data,
    window: &mut Window,
    cx: &mut gpui_kit::Context<SheetDocumentState>,
) -> (Entity<SheetState>, Subscription) {
    let model = SheetModel::from_cells(dimensions.0, dimensions.1, parse_cells(cells));
    let sheet = cx.new(|cx| SheetState::new(model, cx));
    let subscription = cx.subscribe_in(
        &sheet,
        window,
        |state: &mut SheetDocumentState, sheet, event: &SheetEvent, window, cx| {
            let SheetEvent::Edited(changed) = event;
            if let Some(handler) = state.handler.clone() {
                let payload = edit_payload(sheet.read(cx).model(), changed);
                handler(payload, window, cx);
            }
        },
    );
    (sheet, subscription)
}

impl RenderOnce for SheetDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let (sheet, subscription) = make_sheet(self.dimensions, &self.cells, window, cx);
            SheetDocumentState {
                dimensions: self.dimensions,
                cells: self.cells.clone(),
                sheet,
                handler: None,
                _subscription: subscription,
            }
        });
        let sheet = state.update(cx, |state, cx| {
            state.handler = self.handler;
            if state.dimensions != self.dimensions {
                state.dimensions = self.dimensions;
                state.cells = self.cells;
                (state.sheet, state._subscription) =
                    make_sheet(state.dimensions, &state.cells, window, cx);
            } else if state.cells != self.cells {
                state.cells = self.cells;
                let cells = parse_cells(&state.cells);
                state.sheet.update(cx, |sheet, cx| {
                    let writes = diff(sheet.model(), &cells);
                    if !writes.is_empty() {
                        sheet.edit(writes, cx);
                    }
                });
            }
            state.sheet.update(cx, |sheet, cx| {
                if sheet.model().read_only != self.read_only {
                    sheet.set_read_only(self.read_only, cx);
                }
                if sheet.frozen() != self.frozen {
                    sheet.freeze(self.frozen.0, self.frozen.1, cx);
                }
            });
            state.sheet.clone()
        });
        div().w_full().h(px(self.height)).child(Sheet::new(&sheet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[(&str, Data)]) -> Data {
        Data::Record(Arc::new(
            fields
                .iter()
                .map(|(name, value)| ((*name).into(), value.clone()))
                .collect(),
        ))
    }

    #[test]
    fn parses_addresses_formats_and_skips_bad_entries() {
        let cells = Data::List(
            vec![
                record(&[("cell", "B2".into()), ("text", "=A1*2".into())]),
                record(&[
                    ("cell", "a1".into()),
                    ("text", "3.5".into()),
                    ("format", "number".into()),
                    ("decimals", Data::Num(1.)),
                    ("align", "right".into()),
                ]),
                record(&[("cell", "not a cell".into()), ("text", "x".into())]),
                record(&[("cell", "C3".into())]),
            ]
            .into(),
        );
        let parsed = parse_cells(&cells);
        assert_eq!(parsed.len(), 2);
        let a1 = &parsed[&CellPos::new(0, 0)];
        assert_eq!(a1.format.number, NumberFormat::Number(1));
        assert_eq!(a1.format.align, Alignment::Right);
        assert_eq!(parsed[&CellPos::new(1, 1)].text.as_ref(), "=A1*2");
    }

    #[test]
    fn diff_writes_only_changes_and_clears_removed_cells() {
        let model = SheetModel::from_cells(
            10,
            10,
            [
                (CellPos::new(0, 0), Cell::new("1")),
                (CellPos::new(0, 1), Cell::new("2")),
            ],
        );
        let wanted = BTreeMap::from([
            (CellPos::new(0, 0), Cell::new("1")),
            (CellPos::new(5, 5), Cell::new("new")),
            (CellPos::new(50, 0), Cell::new("outside")),
        ]);
        let mut writes = diff(&model, &wanted);
        writes.sort_by_key(|(pos, _)| *pos);
        assert_eq!(
            writes,
            vec![
                (CellPos::new(0, 1), Cell::default()),
                (CellPos::new(5, 5), Cell::new("new")),
            ]
        );
    }

    #[test]
    fn edit_payload_reports_addresses_and_current_text() {
        let model = SheetModel::from_cells(4, 4, [(CellPos::new(1, 2), Cell::new("x"))]);
        let payload = edit_payload(&model, &[CellPos::new(1, 2), CellPos::new(0, 0)]);
        assert_eq!(
            field(payload.list_get(0).unwrap_or(&Data::Null), "cell").as_str(),
            Some("C2")
        );
        assert_eq!(
            field(payload.list_get(0).unwrap_or(&Data::Null), "text").as_str(),
            Some("x")
        );
        assert_eq!(
            field(payload.list_get(1).unwrap_or(&Data::Null), "text").as_str(),
            Some("")
        );
    }
}
