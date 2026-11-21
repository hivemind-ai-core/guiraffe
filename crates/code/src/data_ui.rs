//! Adapter for document-backed read-only code views.

use std::{rc::Rc, sync::Arc};

use gpui_kit::{App, ElementId, IntoElement, RenderOnce, SharedString, Window};
use guiraffe_core::{
    ColorName,
    widget_catalog::{EventHandler, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{
    AnchorBias, Annotation, AnnotationKind, BlameCommit, CodeDocument, CodeHandle, CodeOptions,
    CodeViewer, DecorationAction, DecorationHandler, DecorationInput, DecorationLayerOptions,
    DecorationStyle, Decorations, Fold, GutterMark, HighlightShape, InlineWidget, Position,
    Severity, SourceRange, UnderlinePattern, widget_specs,
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
                    layers: node.prop(8).clone(),
                    on_decoration: node.handler(0),
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

fn parse_layers(document: Arc<CodeDocument>, value: &Value) -> Option<Arc<Decorations>> {
    let len = value.list_len().filter(|len| *len > 0)?;
    let mut layers = Decorations::new(document.clone()).ok()?;
    for layer_index in 0..len {
        let Some(layer) = value.list_get(layer_index) else {
            continue;
        };
        let field = |name: &str| layer.field(&[name.into()]);
        let Some(id) = exact_integer(field("id")).and_then(|id| u32::try_from(id).ok()) else {
            continue;
        };
        let options = DecorationLayerOptions {
            z_index: field("zIndex").as_f64().unwrap_or_default() as i32,
            merge_identical: field("mergeIdentical").as_bool().unwrap_or(false),
        };
        let marks = field("decorations");
        let inputs: Vec<_> = (0..marks.list_len().unwrap_or_default())
            .filter_map(|index| {
                let mark = marks.list_get(index)?;
                let field = |name: &str| mark.field(&[name.into()]);
                let line = exact_integer(field("line"))? as usize;
                let end_line = exact_integer(field("endLine")).map_or(line, |line| line as usize);
                let start = exact_integer(field("start"))? as usize;
                let end = exact_integer(field("end"))? as usize;
                let range =
                    SourceRange::new(Position::new(line, start), Position::new(end_line, end));
                document.range(range)?;
                let bias = |name| {
                    if field(name).as_str() == Some("left") {
                        AnchorBias::Left
                    } else {
                        AnchorBias::Right
                    }
                };
                Some(DecorationInput {
                    id: exact_integer(field("id"))?,
                    range,
                    style: layers.intern_style(parse_style(field("style"))?).ok()?,
                    start_bias: bias("startBias"),
                    end_bias: bias("endBias"),
                })
            })
            .collect();
        let _ = layers.update_layer(id, options, &[], inputs);
    }
    Some(Arc::new(layers))
}

fn exact_integer(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0)
        .map(|value| value as u64)
}

fn parse_style(value: &Value) -> Option<DecorationStyle> {
    let field = |name: &str| value.field(&[name.into()]);
    let text = |name| Arc::from(field(name).as_str().unwrap_or_default());
    let severity = match field("severity").as_str() {
        Some("warning") => Severity::Warning,
        Some("info") => Severity::Info,
        Some("hint") => Severity::Hint,
        _ => Severity::Error,
    };
    Some(match field("kind").as_str()? {
        "highlight" => DecorationStyle::Highlight {
            shape: match field("shape").as_str() {
                Some("outline") => HighlightShape::Outline,
                Some("rounded") => HighlightShape::Rounded,
                Some("bracket-pair") => HighlightShape::BracketPair,
                _ => HighlightShape::Background,
            },
            color: field("color")
                .as_str()
                .and_then(ColorName::from_name)
                .unwrap_or_default(),
        },
        "diagnostic" => DecorationStyle::Diagnostic {
            severity,
            pattern: match field("pattern").as_str() {
                Some("straight") => UnderlinePattern::Straight,
                Some("dotted") => UnderlinePattern::Dotted,
                Some("dashed") => UnderlinePattern::Dashed,
                Some("double") => UnderlinePattern::Double,
                _ => UnderlinePattern::Wavy,
            },
            message: text("message"),
        },
        "gutter" => DecorationStyle::Gutter {
            lane: exact_integer(field("lane")).unwrap_or(0).min(3) as u8,
            mark: match field("mark").as_str() {
                Some("breakpoint") => GutterMark::Breakpoint,
                Some("bookmark") => GutterMark::Bookmark,
                Some("added") => GutterMark::Added,
                Some("modified") => GutterMark::Modified,
                Some("deleted") => GutterMark::Deleted,
                Some("covered") => GutterMark::Covered,
                Some("uncovered") => GutterMark::Uncovered,
                Some("partially-covered") => GutterMark::PartiallyCovered,
                _ => GutterMark::Error,
            },
        },
        "blame" => DecorationStyle::Blame(Arc::new(BlameCommit {
            author: text("author"),
            short_hash: text("shortHash"),
            message: text("message"),
            date: text("date"),
            age_days: exact_integer(field("ageDays"))
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as u32,
            url: field("url").as_str().map(Arc::from),
        })),
        "inline" => DecorationStyle::Inline {
            widget: match field("widget").as_str() {
                Some("ghost") => InlineWidget::Ghost,
                Some("end-of-line-diagnostic") if field("severity").as_str().is_some() => {
                    InlineWidget::EndOfLineDiagnosticSeverity(severity)
                }
                Some("end-of-line-diagnostic") => InlineWidget::EndOfLineDiagnostic,
                Some("code-lens") => InlineWidget::CodeLens,
                _ => InlineWidget::Inlay,
            },
            label: text("label"),
        },
        "review" => DecorationStyle::Review {
            markdown: text("markdown"),
        },
        "block" => DecorationStyle::Block {
            estimated_rows: exact_integer(field("estimatedRows"))
                .unwrap_or(1)
                .clamp(1, u16::MAX.into()) as u16,
            payload: text("payload"),
        },
        "unnecessary" => DecorationStyle::Unnecessary,
        "deprecated" => DecorationStyle::Deprecated,
        _ => return None,
    })
}

struct ViewerState {
    layers: Value,
    handle: CodeHandle,
    source: SharedString,
    language: SharedString,
    annotations: Value,
    folds: Value,
}

#[derive(IntoElement)]
struct CodeDocumentView {
    layers: Value,
    on_decoration: Option<EventHandler>,
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
                layers: Value::Null,
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
            if source_changed || self.layers != state.layers {
                state.layers = self.layers;
                let layers = state.layers.clone();
                state
                    .handle
                    .set_document_decorations(move |document| parse_layers(document, &layers), cx);
            }
            let handler = self.on_decoration.map(|handler| {
                let window = window.window_handle();
                Rc::new(move |event: crate::DecorationEvent, cx: &mut App| {
                    let action = match event.action {
                        DecorationAction::Hover => "hover",
                        DecorationAction::Leave => "leave",
                        DecorationAction::Click => "click",
                    };
                    let payload = Value::Record(Arc::new(std::collections::BTreeMap::from([
                        ("action".into(), action.into()),
                        ("layer".into(), Value::Num(f64::from(event.layer))),
                        ("id".into(), Value::Num(event.decoration as f64)),
                    ])));
                    let handler = handler.clone();
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, window, cx| handler(payload, window, cx));
                    });
                }) as DecorationHandler
            });
            state.handle.on_decoration(handler);
            state.handle.set_options(self.options, cx);
            state.handle.clone()
        });
        CodeViewer::new(&handle)
    }
}
