//! Adapter for Markdown document nodes.

use std::sync::Arc;

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, Window, px};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{MarkdownView, widget_specs};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| {
                let key = node.state_id();
                let mut markdown = MarkdownView::new(key.to_string())
                    .source(node.prop(0).display())
                    .selectable(node.prop(1).as_bool().unwrap_or(true))
                    .scrollable(node.prop(2).as_bool().unwrap_or(false))
                    .code_copy(node.prop(5).as_bool().unwrap_or(true));
                if let Some(height) = node.prop(3).as_f64() {
                    markdown = markdown.min_height(px(height as f32));
                }
                if let Some(height) = node.prop(4).as_f64() {
                    markdown = markdown.max_height(px(height as f32));
                }
                MarkdownDocument {
                    markdown,
                    key,
                    handlers: Handlers {
                        link: node.handler(0),
                        task: node.handler(1),
                    },
                }
                .into_any_element()
            },
        }],
    );
}

#[derive(Default)]
struct Handlers {
    link: Option<EventHandler>,
    task: Option<EventHandler>,
}

#[derive(IntoElement)]
struct MarkdownDocument {
    markdown: MarkdownView,
    key: ElementId,
    handlers: Handlers,
}

impl RenderOnce for MarkdownDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |_, _| Handlers::default());
        let link = self.handlers.link.is_some();
        let task = self.handlers.task.is_some();
        state.update(cx, |handlers, _| *handlers = self.handlers);
        // Kit callbacks are Send + Sync. The entity keeps the UI-thread callbacks in App.
        let mut markdown = self.markdown;
        if link {
            let state = state.downgrade();
            markdown = markdown.on_link_click(move |url, window, cx| {
                let handler = state
                    .upgrade()
                    .and_then(|state| state.read(cx).link.clone());
                if let Some(handler) = handler {
                    handler(url.into(), window, cx);
                }
            });
        }
        if task {
            let state = state.downgrade();
            markdown = markdown.on_task_toggle(move |checked, marker, window, cx| {
                let handler = state
                    .upgrade()
                    .and_then(|state| state.read(cx).task.clone());
                if let Some(handler) = handler {
                    let payload = Value::Record(Arc::new(
                        [
                            ("checked".into(), Value::Bool(checked)),
                            ("start".into(), Value::Num(marker.start as f64)),
                            ("end".into(), Value::Num(marker.end as f64)),
                        ]
                        .into(),
                    ));
                    handler(payload, window, cx);
                }
            });
        }
        markdown
    }
}
