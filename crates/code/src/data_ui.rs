//! Adapter for document-backed read-only code views.

use std::sync::Arc;

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use guiraffe_core::{
    ColorName,
    widget_catalog::{WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{
    Annotation, AnnotationKind, CodeDocument, CodeHandle, CodeOptions, CodeViewer, Fold, Position,
    SourceRange, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
            render: |node, _, _, _| {
                let annotations = node.prop(6);
                CodeDocumentView {
                    key: node.state_id(),
                    source: node.prop(0).display(),
                    language: node.prop(1).display(),
                    options: CodeOptions {
                        size: node.size(2),
                        line_numbers: node.prop(3).as_bool().unwrap_or(true),
                        minimap: node.prop(4).as_bool().unwrap_or(true),
                        margin_chars: node.prop(5).as_f64().unwrap_or(24.0).clamp(0.0, 1024.0)
                            as usize,
                    },
                    annotations: annotations.clone(),
                    folds: node.prop(7).clone(),
                }
                .into_any_element()
            },
        }],
    );
}

fn parse_annotations(values: &Value) -> Arc<[Annotation]> {
    let id = ["id".into()];
    let line = ["line".into()];
    let end_line = ["endLine".into()];
    let start = ["start".into()];
    let end = ["end".into()];
    let kind = ["kind".into()];
    let payload = ["payload".into()];
    let color = ["color".into()];
    let integer = |value: &Value| {
        value
            .as_f64()
            .filter(|v| *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0)
    };
    (0..values.list_len().unwrap_or_default())
        .filter_map(|i| {
            let value = values.list_get(i)?;
            let line = integer(value.field(&line))? as usize;
            let start = integer(value.field(&start))? as usize;
            let end = integer(value.field(&end))? as usize;
            let end_line = integer(value.field(&end_line)).map_or(line, |line| line as usize);
            if (line, start) > (end_line, end) {
                return None;
            }
            let kind = match value.field(&kind).as_str()? {
                "highlight" => AnnotationKind::Highlight,
                "underline" => AnnotationKind::Underline,
                "inline-hint" => AnnotationKind::InlineHint,
                "lens" => AnnotationKind::Lens,
                "bubble" => AnnotationKind::Bubble,
                "symbol" => AnnotationKind::Symbol,
                _ => return None,
            };
            Some(Annotation {
                id: integer(value.field(&id))? as u64,
                range: SourceRange::new(Position::new(line, start), Position::new(end_line, end)),
                kind,
                payload: value.field(&payload).as_str().unwrap_or_default().into(),
                color: value
                    .field(&color)
                    .as_str()
                    .and_then(ColorName::from_name)
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn parse_folds(values: &Value) -> Vec<Fold> {
    let start = ["start".into()];
    let end = ["end".into()];
    let collapsed = ["collapsed".into()];
    let line = |line: &Value| {
        line.as_f64()
            .filter(|line| (0.0..=f64::from(u32::MAX)).contains(line) && line.fract() == 0.0)
            .map(|line| line as usize)
    };
    (0..values.list_len().unwrap_or_default())
        .filter_map(|i| {
            let value = values.list_get(i)?;
            let start = line(value.field(&start))?;
            let end = line(value.field(&end))?;
            (end.saturating_sub(start) > 1).then(|| Fold {
                start,
                end,
                collapsed: value.field(&collapsed).as_bool().unwrap_or(true),
            })
        })
        .collect()
}

struct ViewerState {
    handle: CodeHandle,
    source: SharedString,
    language: SharedString,
    annotations: Value,
    folds: Value,
}

#[derive(IntoElement)]
struct CodeDocumentView {
    key: ElementId,
    source: SharedString,
    language: SharedString,
    options: CodeOptions,
    annotations: Value,
    folds: Value,
}

impl RenderOnce for CodeDocumentView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |_, cx| {
            let handle =
                CodeHandle::new(cx.entity_id().as_u64(), Arc::new(CodeDocument::new("")), cx);
            ViewerState {
                handle,
                source: SharedString::default(),
                language: SharedString::default(),
                annotations: Value::Null,
                folds: Value::Null,
            }
        });
        let handle = state.update(cx, |state, cx| {
            let source_changed = self.source != state.source || self.language != state.language;
            if source_changed {
                state.source = self.source;
                state.language = self.language;
                state.handle.set_source(
                    state.source.as_ref().into(),
                    state.language.as_ref().into(),
                    cx,
                );
            }
            if source_changed || self.annotations != state.annotations {
                state.annotations = self.annotations;
                state
                    .handle
                    .set_annotations(parse_annotations(&state.annotations), cx);
            }
            if source_changed || self.folds != state.folds {
                state.folds = self.folds;
                state.handle.set_folds(parse_folds(&state.folds), cx);
            }
            state.handle.set_options(self.options, cx);
            state.handle.clone()
        });
        CodeViewer::new(&handle)
    }
}
