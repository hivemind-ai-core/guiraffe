//! DataUi adapter for a sortable board.

use std::{collections::HashSet, sync::Arc};

use gpui_kit::{
    App, AppContext, ElementId, Entity, IntoElement, RenderOnce, SharedString, Subscription, Window,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_spec::Schema,
    widget_value::Value,
};

use super::{Card, SortableBoard};
use crate::{sortable::SortEvent, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: &Schema {
                props: &["columns", "cards-per-row"],
                events: &["reorder"],
            },
            render: |node, _, _, _| BoardDocument::new(node).into_any_element(),
        }],
    );
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    (0..value.list_len().unwrap_or_default()).filter_map(|i| value.list_get(i))
}

fn columns(value: &Value) -> Vec<(SharedString, Vec<Card>)> {
    let title = ["title".into()];
    let cards = ["cards".into()];
    let id = ["id".into()];
    let label = ["label".into()];
    let mut ids = HashSet::new();
    items(value)
        .map(|column| {
            (
                column.field(&title).display(),
                items(column.field(&cards))
                    .filter_map(|card| {
                        let index = card.field(&id).as_f64().filter(|v| {
                            *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0
                        })? as usize;
                        ids.insert(index).then(|| Card {
                            id: index,
                            label: card.field(&label).display(),
                        })
                    })
                    .collect(),
            )
        })
        .collect()
}

#[derive(IntoElement)]
struct BoardDocument {
    key: ElementId,
    columns: Value,
    cards_per_row: u16,
    reorder: Option<EventHandler>,
}

impl BoardDocument {
    fn new(node: &dyn WidgetNode) -> Self {
        Self {
            key: node.state_id(),
            columns: node.prop(0).clone(),
            cards_per_row: node
                .prop(1)
                .as_f64()
                .unwrap_or(1.0)
                .clamp(1.0, u16::MAX.into()) as u16,
            reorder: node.handler(0),
        }
    }
}

struct BoardState {
    board: Entity<SortableBoard>,
    columns: Value,
    reorder: Option<EventHandler>,
    subscription: Subscription,
}

fn build_board(
    value: &Value,
    window: &mut Window,
    cx: &mut gpui_kit::Context<BoardState>,
) -> (Entity<SortableBoard>, Subscription) {
    let source = columns(value);
    let board = cx.new(|cx| SortableBoard::new(source, cx));
    let subscription = cx.subscribe_in(
        &board,
        window,
        |state: &mut BoardState, board, event: &SortEvent, window, cx| {
            if let Some(reorder) = &state.reorder {
                let lists = board
                    .read(cx)
                    .lists()
                    .iter()
                    .map(|list| {
                        Value::List(list.iter().map(|card| Value::Num(card.id as f64)).collect())
                    })
                    .collect();
                let payload = Value::Record(Arc::new(
                    [
                        ("source".into(), Value::Num(event.source as f64)),
                        ("destination".into(), Value::Num(event.destination as f64)),
                        (
                            "insertionIndex".into(),
                            Value::Num(event.reorder.insertion_index as f64),
                        ),
                        (
                            "originalIndices".into(),
                            Value::List(
                                event
                                    .reorder
                                    .original_indices
                                    .iter()
                                    .map(|i| Value::Num(*i as f64))
                                    .collect(),
                            ),
                        ),
                        ("lists".into(), Value::List(lists)),
                    ]
                    .into(),
                ));
                reorder(payload, window, cx);
            }
        },
    );
    (board, subscription)
}

impl RenderOnce for BoardDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let (board, subscription) = build_board(&self.columns, window, cx);
            BoardState {
                board,
                columns: self.columns.clone(),
                reorder: None,
                subscription,
            }
        });
        state.update(cx, |state, cx| {
            if state.columns != self.columns {
                state.columns = self.columns;
                (state.board, state.subscription) = build_board(&state.columns, window, cx);
            }
            state.reorder = self.reorder;
            state.board.update(cx, |board, cx| {
                if board.cards_per_row != self.cards_per_row {
                    board.cards_per_row = self.cards_per_row;
                    cx.notify();
                }
            });
            state.board.clone()
        })
    }
}
