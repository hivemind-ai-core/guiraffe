//! Built-in document widgets, mapped onto GUIraffe components and GPUI Controls.
//! Spacing comes from the inherited style context (spec/14), not from the document.

use gpui_kit::{
    AnyElement, IntoElement, ParentElement, Styled, component::button::Button, div, px,
};
use guiraffe_components::{Card, Highlight, Pill, RingProgress, Text, Title};
use guiraffe_core::{Size, StyleContext};

use crate::widget::{NodeView, Registry, widget};

fn gap(n: &NodeView<'_>) -> f32 {
    StyleContext::current()
        .density
        .space(n.size(n.prop_index("gap")).unwrap_or(Size::Sm))
}

/// Split children into rows of `cols` (the last row may be shorter).
fn chunk_rows(kids: Vec<AnyElement>, cols: usize) -> Vec<Vec<AnyElement>> {
    let mut rows = Vec::new();
    let mut row = Vec::with_capacity(cols);
    for k in kids {
        row.push(k);
        if row.len() == cols {
            rows.push(std::mem::replace(&mut row, Vec::with_capacity(cols)));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

use crate::catalog::*;

pub fn register(r: &mut Registry) {
    crate::controls::register(r);
    r.register(
        STACK.0.id,
        widget(&STACK.1, |n, kids, _, _| {
            div()
                .flex()
                .flex_col()
                .gap(px(gap(n)))
                .children(kids)
                .into_any_element()
        }),
    );
    r.register(
        GROUP.0.id,
        widget(&GROUP.1, |n, kids, _, _| {
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(gap(n)))
                .children(kids)
                .into_any_element()
        }),
    );
    r.register(
        GRID.0.id,
        widget(&GRID.1, |n, kids, _, _| {
            let cols = n
                .num(n.prop_index("columns"))
                .unwrap_or(2.0)
                .clamp(1.0, 12.0) as usize;
            let g = gap(n);
            div()
                .flex()
                .flex_col()
                .gap(px(g))
                .children(chunk_rows(kids, cols).into_iter().map(|row| {
                    div()
                        .flex()
                        .gap(px(g))
                        .children(row.into_iter().map(|c| div().flex_1().min_w_0().child(c)))
                }))
                .into_any_element()
        }),
    );
    r.register(
        TITLE.0.id,
        widget(&TITLE.1, |n, _, _, _| {
            Title::new(n.text(n.prop_index("text")))
                .order(
                    n.num(n.prop_index("order"))
                        .map(|o| o.clamp(1.0, 6.0) as u8)
                        .unwrap_or(3),
                )
                .into_any_element()
        }),
    );
    r.register(
        TEXT.0.id,
        widget(&TEXT.1, |n, _, _, _| {
            let mut t =
                Text::new(n.text(n.prop_index("text"))).dimmed(n.bool(n.prop_index("dimmed")));
            if let Some(s) = n.size(n.prop_index("size")) {
                t = t.size(s);
            }
            if let Some(c) = n.color(n.prop_index("color")) {
                t = t.color(c);
            }
            if n.bool(n.prop_index("single-line")) {
                t = t
                    .truncate(true)
                    .h(px(Size::Xl.control_height()))
                    .line_height(px(Size::Xl.control_height()));
            }
            t.into_any_element()
        })
        .uniform_rows_if(|props| {
            matches!(
                TEXT.1
                    .prop_index("single-line")
                    .and_then(|index| props.get(index)),
                Some(crate::Prop::Lit(crate::Value::Bool(true)))
            )
        }),
    );
    r.register(
        HIGHLIGHT.0.id,
        widget(&HIGHLIGHT.1, |n, _, _, _| {
            Highlight::new(n.text(n.prop_index("text")))
                .term(n.text(n.prop_index("term")))
                .into_any_element()
        }),
    );
    r.register(
        PILL.0.id,
        widget(&PILL.1, |n, _, _, _| {
            Pill::new(n.element_id(), n.text(n.prop_index("label"))).into_any_element()
        }),
    );
    r.register(
        CARD.0.id,
        widget(&CARD.1, |_, kids, _, _| {
            Card::new().children(kids).into_any_element()
        }),
    );
    r.register(
        RING.0.id,
        widget(&RING.1, |n, _, _, _| {
            RingProgress::new()
                .diameter(n.num(n.prop_index("diameter")).unwrap_or(64.0) as f32)
                .section(
                    n.num(n.prop_index("value")).unwrap_or(0.0) as f32,
                    n.color(n.prop_index("color")).unwrap_or_default(),
                )
                .label(n.text(n.prop_index("label")))
                .into_any_element()
        }),
    );
    r.register(
        BUTTON.0.id,
        widget(&BUTTON.1, |n, _, _, _| {
            let b = Button::new(n.element_id()).label(n.text(n.prop_index("label")));
            let b = match n.handler(n.event_index("click")) {
                Some(h) => b
                    .on_click(move |_, w, cx| h(crate::Value::Null, w, cx))
                    .into_any_element(),
                None => b.into_any_element(),
            };
            // Document ids double as test/automation selectors in `test-support` builds.
            #[cfg(feature = "test-support")]
            if let Some(id) = n.selector() {
                use gpui_kit::InteractiveElement as _;
                return div()
                    .debug_selector(move || id.to_string())
                    .child(b)
                    .into_any_element();
            }
            b
        }),
    );
    // Content-sized child (Mocket `width: "hug"`): opts out of the parent's cross-axis stretch.
    r.register(
        HUG.0.id,
        widget(&HUG.1, |_, kids, _, _| {
            div()
                .flex()
                .flex_col()
                .self_start()
                .children(kids)
                .into_any_element()
        }),
    );
    r.register(
        SPACER.0.id,
        widget(&SPACER.1, |_, _, _, _| div().flex_1().into_any_element()),
    );
}

pub use crate::catalog::KINDS;
