//! Built-in document widgets, mapped onto GUIraffe components and GPUI Controls.
//! Spacing comes from the inherited style context (spec/14), not from the document.

use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, Styled, Window, component::button::Button, div, px,
};
use guiraffe_components::{Card, Highlight, Pill, RingProgress, Text, Title};
use guiraffe_core::{Size, StyleContext};

use crate::widget::{NodeView, Registry};

fn gap(node: &NodeView<'_>) -> f32 {
    StyleContext::current()
        .density
        .space(node.size("gap").unwrap_or(Size::Sm))
}

pub fn register(r: &mut Registry) {
    r.register(
        "stack",
        |n: &NodeView<'_>, kids: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            div()
                .flex()
                .flex_col()
                .gap(px(gap(n)))
                .children(kids)
                .into_any_element()
        },
    );
    r.register(
        "group",
        |n: &NodeView<'_>, kids: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(gap(n)))
                .children(kids)
                .into_any_element()
        },
    );
    r.register(
        "title",
        |n: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            let order = n.num("order").map(|o| o.clamp(1.0, 6.0) as u8).unwrap_or(3);
            Title::new(n.text("text")).order(order).into_any_element()
        },
    );
    r.register(
        "text",
        |n: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            let mut t = Text::new(n.text("text")).dimmed(n.bool("dimmed"));
            if let Some(s) = n.size("size") {
                t = t.size(s);
            }
            if let Some(c) = n.color("color") {
                t = t.color(c);
            }
            t.into_any_element()
        },
    );
    r.register(
        "highlight",
        |n: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            Highlight::new(n.text("text"))
                .term(n.text("term"))
                .into_any_element()
        },
    );
    r.register(
        "pill",
        |n: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            Pill::new(n.element_id(), n.text("label")).into_any_element()
        },
    );
    r.register(
        "card",
        |_: &NodeView<'_>, kids: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            Card::new().children(kids).into_any_element()
        },
    );
    r.register(
        "ring",
        |n: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            let color = n.color("color").unwrap_or_default();
            let value = n.num("value").unwrap_or(0.0) as f32;
            RingProgress::new()
                .diameter(n.num("diameter").unwrap_or(64.0) as f32)
                .section(value, color)
                .label(n.text("label"))
                .into_any_element()
        },
    );
    r.register(
        "button",
        |n: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            let b = Button::new(n.element_id()).label(n.text("label"));
            let b = match n.handler("click") {
                Some(h) => b.on_click(move |_, w, cx| h(w, cx)).into_any_element(),
                None => b.into_any_element(),
            };
            // Document ids are test/automation selectors (e2e probe, interaction tests); the
            // wrapper only exists in `test-support` builds, so apps pay nothing for it.
            #[cfg(feature = "test-support")]
            if let Some(id) = n.spec.id.clone() {
                use gpui_kit::InteractiveElement as _;
                return div()
                    .debug_selector(move || id.to_string())
                    .child(b)
                    .into_any_element();
            }
            b
        },
    );
    r.register(
        "spacer",
        |_: &NodeView<'_>, _: Vec<AnyElement>, _: &mut Window, _: &mut App| {
            div().flex_1().into_any_element()
        },
    );
}
