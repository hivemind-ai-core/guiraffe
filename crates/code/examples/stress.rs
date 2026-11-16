//! `cargo run -p guiraffe-code --example stress --features gpui-fast -- --million`
//! Scroll either panel, click fold markers/minimap, select and copy, or hover `value`.
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div,
};
use guiraffe_code::{
    Anchor, CodeDocument, CodeHandle, CodeOptions, CodeViewer, Position, SourceRange, SymbolInfo,
    Trace, TraceHandle,
};
use guiraffe_core::{ColorName, Scheme};
use std::{rc::Rc, sync::Arc};
struct Demo {
    left: CodeHandle,
    right: CodeHandle,
    traces: TraceHandle,
}
impl Render for Demo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .flex()
                    .size_full()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(CodeViewer::new(&self.left)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(CodeViewer::new(&self.right)),
                    ),
            )
            .child(self.traces.element())
    }
}
fn main() {
    let million = std::env::args().any(|arg| arg == "--million");
    let dark = std::env::args().any(|arg| arg == "--dark");
    let source: Arc<str> = if million {
        "let value = compute(42);\n".repeat(999_999).into()
    } else {
        "fn compute(input: i32) -> i32 {\n    let value = input * 2;\n    value + 1\n}\n".into()
    };
    let document = std::thread::spawn(move || Arc::new(CodeDocument::new(source)))
        .join()
        .unwrap_or_else(|_| panic!("document worker failed"));
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            guiraffe_code::init(cx);
            if dark {
                guiraffe_core::set_scheme(Scheme::Dark, cx);
            }
            let opened = cx.open_window(Default::default(), move |_, cx| -> Entity<Demo> {
                let left = CodeHandle::new(1, document, cx);
                left.set_options(
                    CodeOptions {
                        margin_chars: 0,
                        ..Default::default()
                    },
                    cx,
                );
                if !million {
                    left.highlight("rust".into(), cx);
                }
                left.set_symbol_provider(
                    Some(Rc::new(|p| {
                        Some(SymbolInfo {
                            range: SourceRange::new(p, p),
                            occurrences: Arc::from([]),
                            markdown: format!("**Source**\nLine {}, byte {}", p.line + 1, p.column)
                                .into(),
                        })
                    })),
                    cx,
                );
                let right = CodeHandle::new(
                    2,
                    Arc::new(CodeDocument::new(Arc::<str>::from(
                        "fn compute(value: i32) -> i32 {\n    value + 1\n}\n",
                    ))),
                    cx,
                );
                right.highlight("rust".into(), cx);
                let traces = TraceHandle::new(
                    &[left.clone(), right.clone()],
                    Arc::from([Trace {
                        from: Anchor {
                            viewer: 1,
                            position: Position::new(0, 4),
                        },
                        to: Anchor {
                            viewer: 2,
                            position: Position::new(0, 3),
                        },
                        color: ColorName::Teal,
                    }]),
                    cx,
                );
                cx.new(|_| Demo {
                    left,
                    right,
                    traces,
                })
            });
            if let Err(error) = opened {
                eprintln!("opening code example: {error}");
                cx.quit();
            }
        });
}
