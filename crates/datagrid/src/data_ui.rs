//! Document-backed sources for the virtualized data grid.

use std::sync::Arc;

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, Subscription, Window};
use guiraffe_core::{
    Density,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{
    Align, CellRef, ColumnDef, DataGrid, EditError, EditOp, GridEvent, GridHandle, GridOptions,
    GridSource, Pin, SelectionMode, SortDir, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| GridDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

fn count(value: &Value) -> Option<usize> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v < usize::MAX as f64 && v.fract() == 0.0)
        .map(|v| v as usize)
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value.field(&[name.into()])
}

fn columns(values: &Value) -> Vec<ColumnDef> {
    items(values)
        .enumerate()
        .filter_map(|(index, value)| {
            let id = field(value, "id").as_str()?;
            let title = field(value, "title").as_str()?;
            let source = count(field(value, "source")).unwrap_or(index);
            let width = |name, default| {
                field(value, name)
                    .as_f64()
                    .unwrap_or(default)
                    .clamp(1.0, 4096.0) as f32
            };
            let flag = |name, default| field(value, name).as_bool().unwrap_or(default);
            let min = width("minWidth", 48.0);
            let max = width("maxWidth", 1200.0).max(min);
            Some(
                ColumnDef::new(id.to_owned(), title.to_owned(), source)
                    .min_width(min)
                    .max_width(max)
                    .width(width("width", 140.0).clamp(min, max))
                    .align(match field(value, "align").as_str() {
                        Some("start") => Align::Start,
                        Some("center") => Align::Center,
                        Some("end") => Align::End,
                        _ => Align::Auto,
                    })
                    .pin(match field(value, "pin").as_str() {
                        Some("left") => Pin::Left,
                        Some("right") => Pin::Right,
                        _ => Pin::None,
                    })
                    .sortable(flag("sortable", true))
                    .resizable(flag("resizable", true))
                    .editable(flag("editable", false))
                    .visible(flag("visible", true)),
            )
        })
        .collect()
}

struct DocumentSource {
    rows: Vec<Vec<Value>>,
    revision: u64,
}

impl DocumentSource {
    fn new(rows: &Value) -> Self {
        Self {
            rows: items(rows)
                .map(|row| items(row).cloned().collect())
                .collect(),
            revision: 0,
        }
    }
}

impl GridSource for DocumentSource {
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn cell(&self, row: usize, col: usize) -> CellRef<'_> {
        match self.rows.get(row).and_then(|row| row.get(col)) {
            Some(Value::Str(value)) => CellRef::Text(value.as_ref()),
            Some(Value::Num(value)) => CellRef::Number(*value),
            Some(Value::Bool(value)) => CellRef::Bool(*value),
            _ => CellRef::Empty,
        }
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn set_cell(&mut self, row: usize, col: usize, text: &str) -> Result<(), EditError> {
        let Some(value) = self.rows.get_mut(row).and_then(|row| row.get_mut(col)) else {
            return Err(EditError::ReadOnly);
        };
        *value = match value {
            Value::Num(_) => Value::Num(
                text.parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| EditError::Invalid("Expected a number".into()))?,
            ),
            Value::Bool(_) => Value::Bool(
                text.parse()
                    .map_err(|_| EditError::Invalid("Expected true or false".into()))?,
            ),
            _ => text.into(),
        };
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Record(Arc::new(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned().into(), value))
            .collect(),
    ))
}

fn number(value: usize) -> Value {
    Value::Num(value as f64)
}

fn operation(op: EditOp) -> Value {
    match op {
        EditOp::Edit => "edit",
        EditOp::Paste => "paste",
        EditOp::Cut => "cut",
        EditOp::Clear => "clear",
        EditOp::Undo => "undo",
        EditOp::Redo => "redo",
    }
    .into()
}

fn event_payload(event: &GridEvent) -> (usize, Value) {
    match event {
        GridEvent::SelectionChanged {
            cursor,
            selected_rows,
        } => (
            0,
            record([
                (
                    "cursor",
                    cursor.map_or(Value::Null, |p| {
                        record([("row", number(p.row)), ("column", number(p.col))])
                    }),
                ),
                ("selectedRows", number(*selected_rows)),
            ]),
        ),
        GridEvent::SortChanged(keys) => (
            1,
            Value::List(
                keys.iter()
                    .map(|key| {
                        record([
                            ("column", number(key.column)),
                            (
                                "direction",
                                match key.dir {
                                    SortDir::Asc => "asc",
                                    SortDir::Desc => "desc",
                                }
                                .into(),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        GridEvent::CellEdited {
            row,
            key,
            column,
            value,
        } => (
            2,
            record([
                ("row", number(*row)),
                ("key", Value::Num(key.0 as f64)),
                ("column", number(*column)),
                ("value", value.clone().into()),
            ]),
        ),
        GridEvent::EditCancelled { row, column } => (
            3,
            record([("row", number(*row)), ("column", number(*column))]),
        ),
        GridEvent::Copied { cells, truncated } => (
            4,
            record([
                ("cells", number(*cells)),
                ("truncated", (*truncated).into()),
            ]),
        ),
        GridEvent::EditRejected {
            row,
            column,
            reason,
        } => (
            5,
            record([
                ("row", number(*row)),
                ("column", number(*column)),
                ("reason", reason.clone().into()),
            ]),
        ),
        GridEvent::RowActivated { row, key } => (
            6,
            record([("row", number(*row)), ("key", Value::Num(key.0 as f64))]),
        ),
        GridEvent::ColumnResized { column, width } => (
            7,
            record([
                ("column", number(*column)),
                ("width", Value::Num((*width).into())),
            ]),
        ),
        GridEvent::ColumnReordered { column, order } => (
            8,
            record([
                ("column", number(*column)),
                (
                    "order",
                    Value::List(order.iter().copied().map(number).collect()),
                ),
            ]),
        ),
        GridEvent::CellsChanged {
            op,
            cells,
            truncated,
        } => (
            9,
            record([
                ("op", operation(*op)),
                ("cells", number(*cells)),
                ("truncated", (*truncated).into()),
            ]),
        ),
        GridEvent::CellsRejected {
            op,
            rejected,
            applied,
            first,
        } => (
            10,
            record([
                ("op", operation(*op)),
                ("rejected", number(*rejected)),
                ("applied", number(*applied)),
                (
                    "first",
                    Value::List(
                        first
                            .iter()
                            .map(|item| {
                                record([
                                    ("row", item.row.map_or(Value::Null, number)),
                                    ("column", item.column.map_or(Value::Null, number)),
                                    ("reason", item.reason.clone().into()),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
    }
}

#[derive(IntoElement)]
struct GridDocument {
    key: ElementId,
    rows: Value,
    columns: Value,
    options: GridOptions,
    handlers: [Option<EventHandler>; 11],
}

impl GridDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            rows: node.prop(0).clone(),
            columns: node.prop(1).clone(),
            options: GridOptions {
                selection: match node.prop(2).as_str() {
                    Some("none") => SelectionMode::None,
                    Some("multi-row") => SelectionMode::MultiRow,
                    Some("cell") => SelectionMode::Cell,
                    Some("range") => SelectionMode::Range,
                    _ => SelectionMode::SingleRow,
                },
                zebra: node.prop(3).as_bool().unwrap_or(true),
                size: node.size(4),
                density: match node.prop(5).as_str() {
                    Some("compact") => Some(Density::Compact),
                    Some("comfortable") => Some(Density::Comfortable),
                    Some("spacious") => Some(Density::Spacious),
                    _ => None,
                },
                focusable: node.prop(6).as_bool(),
                overscan: count(node.prop(7)).unwrap_or_default().min(1024),
                sync_below: count(node.prop(8)).unwrap_or(50_000),
                label: node
                    .prop(9)
                    .as_str()
                    .unwrap_or("Data grid")
                    .to_owned()
                    .into(),
            },
            handlers: std::array::from_fn(|index| node.handler(index)),
        }
    }
}

struct GridDocumentState {
    grid: GridHandle,
    rows: Value,
    columns: Value,
    handlers: [Option<EventHandler>; 11],
    _subscription: Subscription,
}

impl RenderOnce for GridDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let grid = GridHandle::new(DocumentSource::new(&self.rows), columns(&self.columns), cx);
            let subscription = cx.subscribe_in(
                grid.events(),
                window,
                |state: &mut GridDocumentState, _, event, window, cx| {
                    let (index, payload) = event_payload(event);
                    if let Some(handler) = &state.handlers[index] {
                        handler(payload, window, cx);
                    }
                },
            );
            GridDocumentState {
                grid,
                rows: self.rows.clone(),
                columns: self.columns.clone(),
                handlers: std::array::from_fn(|_| None),
                _subscription: subscription,
            }
        });
        let grid = state.update(cx, |state, cx| {
            state.handlers = self.handlers;
            if state.rows != self.rows {
                state.rows = self.rows;
                state
                    .grid
                    .set_source(Arc::new(DocumentSource::new(&state.rows)), cx);
            }
            if state.columns != self.columns {
                state.columns = self.columns;
                state.grid.set_columns(columns(&state.columns), cx);
            }
            state.grid.set_options(self.options, cx);
            state.grid.clone()
        });
        DataGrid::new(&grid)
    }
}
