//! The shared read-only code surface, with analysis supplied as data.
use super::{column, example};
use crate::catalog::Entry;
use gpui_kit::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px,
};
use guiraffe_code::{
    Anchor, Annotation, AnnotationKind, CodeDocument, CodeHandle, CodeOptions, CodeViewer, Fold,
    Position, SourceRange, Trace,
};
use guiraffe_core::ColorName;
use std::sync::Arc;
pub fn entries() -> Vec<Entry> {
    vec![Entry {
        krate: "code",
        name: "CodeViewer",
        summary: "Read-only highlighted code with shared source selection/copy, analysis highlights, hints, lenses, bubbles, folds and trace lines.",
        covers: &["CodeViewer"],
        demo,
    }]
}
struct Demo(CodeHandle);
impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([example(
            "Select source to copy · click the gutter to fold · hover symbols",
            div().h(px(400.)).child(CodeViewer::new(&self.0)),
        )])
    }
}
fn demo(_: &mut Window, cx: &mut App) -> AnyView {
    let source: Arc<str> = "fn total(items: &[u32]) -> u32 {\n    let sum = items.iter().sum();\n    sum\n}\n\nfn main() {\n    let answer = total(&[10, 20, 12]);\n    println!(\"{answer}\");\n}\n".into();
    let code = CodeHandle::new(71, Arc::new(CodeDocument::new(source)), cx);
    code.set_options(
        CodeOptions {
            margin_chars: 22,
            ..Default::default()
        },
        cx,
    );
    code.highlight("rust".into(), cx);
    let range =
        |line, start, end| SourceRange::new(Position::new(line, start), Position::new(line, end));
    code.set_annotations(
        Arc::from([
            Annotation {
                id: 1,
                range: range(1, 8, 11),
                kind: AnnotationKind::Highlight,
                payload: "".into(),
                color: ColorName::Green,
            },
            Annotation {
                id: 2,
                range: range(1, 11, 11),
                kind: AnnotationKind::InlineHint,
                payload: ": u32".into(),
                color: ColorName::Gray,
            },
            Annotation {
                id: 3,
                range: range(0, 0, 0),
                kind: AnnotationKind::Lens,
                payload: "2 references · inferred return: u32".into(),
                color: ColorName::Blue,
            },
            Annotation {
                id: 4,
                range: range(2, 4, 7),
                kind: AnnotationKind::Bubble,
                payload: "**Data flow**\n`sum` is returned.".into(),
                color: ColorName::Blue,
            },
            Annotation {
                id: 5,
                range: range(1, 8, 11),
                kind: AnnotationKind::Symbol,
                payload: "Local variable **sum**: `u32`".into(),
                color: ColorName::Blue,
            },
        ]),
        cx,
    );
    code.set_folds(
        vec![Fold {
            start: 0,
            end: 4,
            collapsed: false,
        }],
        cx,
    );
    code.set_traces(
        Arc::from([Trace {
            from: Anchor {
                viewer: 71,
                position: Position::new(1, 8),
            },
            to: Anchor {
                viewer: 71,
                position: Position::new(2, 4),
            },
            color: ColorName::Blue,
        }]),
        cx,
    );
    cx.new(|_| Demo(code)).into()
}
