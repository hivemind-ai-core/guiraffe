//! DataUi adapter for the copy action and its native feedback state.

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use guiraffe_core::{
    ColorName, Size,
    widget_catalog::{EventHandler, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::CopyButton;
use crate::widget_specs;

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[4],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[4]),
            render: |node, _, _, _| {
                CopyDocument {
                    key: node.state_id(),
                    id: node.element_id().to_string().into(),
                    value: node.prop(0).display(),
                    label: match node.prop(1) {
                        Value::Str(label) => Some(label.clone()),
                        _ => None,
                    },
                    size: node.size(2),
                    color: node.color(3),
                    disabled: node.prop(4).as_bool().unwrap_or_default(),
                    focusable: node.prop(5).as_bool(),
                    handler: node.handler(0),
                }
                .into_any_element()
            },
        }],
    );
}

#[derive(IntoElement)]
struct CopyDocument {
    key: ElementId,
    id: SharedString,
    value: SharedString,
    label: Option<SharedString>,
    size: Option<Size>,
    color: Option<ColorName>,
    disabled: bool,
    focusable: Option<bool>,
    handler: Option<EventHandler>,
}

impl RenderOnce for CopyDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let button = window
            .use_keyed_state(self.key, cx, |_, cx| CopyButton::new(cx))
            .read(cx)
            .clone();
        button.update(cx, |button, cx| {
            button.set_value(self.value, cx);
            button.id = self.id;
            button.set_label(self.label, cx);
            button.set_size(self.size, cx);
            button.set_color(self.color, cx);
            button.set_disabled(self.disabled, cx);
            button.set_focusable(self.focusable, cx);
            if let Some(handler) = self.handler {
                button.on_copy(move |value, window, cx| handler(value.clone().into(), window, cx));
            } else {
                button.on_copy = None;
            }
        });
        button
    }
}
