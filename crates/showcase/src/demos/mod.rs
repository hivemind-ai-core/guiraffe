//! Demo modules, one per crate. Each exposes `entries()`.

use gpui_kit::{AnyElement, IntoElement, ParentElement, SharedString, Styled, div, px};
use guiraffe_components::{Paper, Text};
use guiraffe_core::Size;

pub mod chat;
pub mod code;
pub mod components;
pub mod flow;
pub mod inputs;
pub mod layout;
pub mod markdown;
pub mod mermaid;
pub mod overlays;
pub mod timeline;
pub mod ui;

/// A labelled example block inside a demo.
pub(crate) fn example(label: impl Into<SharedString>, body: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(Size::Xs.spacing()))
        .child(Text::new(label).size(Size::Sm).dimmed(true))
        .child(Paper::new().with_border(true).p(Size::Md).child(body))
        .into_any_element()
}

/// A tinted placeholder box for layout demos.
pub(crate) fn tile(label: impl Into<SharedString>, cx: &gpui_kit::App) -> gpui_kit::Div {
    let v = guiraffe_core::GuiraffeTheme::global(cx).variant(None, guiraffe_core::Variant::Light);
    div()
        .min_h(px(36.))
        .px(px(Size::Sm.spacing()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(v.background.to_hsla())
        .text_color(v.foreground.to_hsla())
        .text_size(px(Size::Sm.font_size()))
        .child(label.into())
}

/// A vertical list of examples.
pub(crate) fn column(items: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(Size::Lg.spacing()))
        .children(items)
        .into_any_element()
}
