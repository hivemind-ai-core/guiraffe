use gpui_kit::{AppContext, Context, IntoElement, Render, Window, px, size};
use gpui_kit::{ParentElement, Styled, div};
use guiraffe_code::{
    Anchor, Annotation, AnnotationKind, CodeDocument, CodeHandle, CodeViewer, Fold, GutterColumn,
    Highlighted, Position, SourceRange, Trace,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};
use std::{cell::RefCell, rc::Rc};
use std::{collections::BTreeMap, sync::Arc};
struct Scene(CodeHandle);
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CodeViewer::new(&self.0)
    }
}
#[test]
fn code_surface_in_light_and_dark() -> Result<(), Box<dyn std::error::Error>> {
    let source: Arc<str> = concat!(
        "// A shared code surface\n",
        "fn evaluate(input: i32) -> i32 {\n",
        "    let value = input * 2;\n",
        "    value + 1\n",
        "}\n",
        "\n",
        "fn main() {\n",
        "    println!(\"{}\", evaluate(21));\n",
        "}\n"
    )
    .into();
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let source = source.clone();
        let highlighted = std::thread::spawn(move || {
            Highlighted::compute(
                Arc::new(CodeDocument::new(source)),
                "rust",
                scheme == Scheme::Dark,
            )
        })
        .join()
        .map_err(|_| "highlight worker panicked")?;
        let mut snapshot = Snapshotter::new()?.with_setup(move |cx| {
            guiraffe_code::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let handle = Rc::new(RefCell::new(None::<CodeHandle>));
        let captured = handle.clone();
        let image = snapshot.render(size(px(900.), px(360.)), move |_, cx| {
            let code = CodeHandle::new(1, highlighted.document().clone(), cx);
            code.set_highlighted(highlighted, cx);
            let range = |line, start, end| {
                SourceRange::new(Position::new(line, start), Position::new(line, end))
            };
            code.set_annotations(
                Arc::from([
                    Annotation {
                        id: 1,
                        range: range(2, 4, 25),
                        kind: AnnotationKind::Highlight,
                        payload: "".into(),
                        color: ColorName::Blue,
                    },
                    Annotation {
                        id: 2,
                        range: range(3, 4, 9),
                        kind: AnnotationKind::Underline,
                        payload: "Potential overflow".into(),
                        color: ColorName::Red,
                    },
                    Annotation {
                        id: 3,
                        range: range(1, 0, 0),
                        kind: AnnotationKind::Lens,
                        payload: "2 references · supplied by analysis".into(),
                        color: ColorName::Teal,
                    },
                    Annotation {
                        id: 4,
                        range: range(2, 8, 13),
                        kind: AnnotationKind::Bubble,
                        payload: "**value**\nComputed from `input`.".into(),
                        color: ColorName::Blue,
                    },
                    Annotation {
                        id: 5,
                        range: range(3, 4, 9),
                        kind: AnnotationKind::InlineHint,
                        payload: ": i32".into(),
                        color: ColorName::Gray,
                    },
                ]),
                cx,
            );
            code.set_gutters(
                vec![GutterColumn {
                    title: "Blame".into(),
                    cells: BTreeMap::from([(1, "Ada".into()), (6, "Lin".into())]),
                    width_chars: 4,
                }],
                cx,
            );
            code.set_folds(
                vec![Fold {
                    start: 1,
                    end: 5,
                    collapsed: false,
                }],
                cx,
            );
            code.set_traces(
                Arc::from([Trace {
                    from: Anchor {
                        viewer: 1,
                        position: Position::new(2, 8),
                    },
                    to: Anchor {
                        viewer: 1,
                        position: Position::new(3, 4),
                    },
                    color: ColorName::Teal,
                }]),
                cx,
            );
            *captured.borrow_mut() = Some(code.clone());
            cx.new(|_| Scene(code))
        })?;
        let background = image.get_pixel(image.width() / 2, image.height() - 2);
        let luminance =
            u16::from(background.0[0]) + u16::from(background.0[1]) + u16::from(background.0[2]);
        assert!(
            if scheme == Scheme::Dark {
                luminance < 300
            } else {
                luminance > 600
            },
            "scheme actually painted its surface"
        );
        assert!(
            handle
                .borrow()
                .as_ref()
                .is_some_and(|h| h.stats().visible_bubbles == 1),
            "bubble was laid out: {:?}",
            handle.borrow().as_ref().map(|h| h.stats())
        );
        assert!(
            image.pixels().any(|p| p.0[0].abs_diff(p.0[2]) > 40),
            "colored syntax/overlays painted"
        );
        assert_snapshot(
            if scheme == Scheme::Light {
                "code-light"
            } else {
                "code-dark"
            },
            &image,
            &config,
        )?;
    }
    Ok(())
}

#[test]
fn shared_code_viewer_inside_markdown() -> Result<(), Box<dyn std::error::Error>> {
    use gpui_kit::{
        Entity,
        component::text::{MarkdownExtensions, TextView, TextViewState},
    };
    struct Embedded {
        state: Entity<TextViewState>,
        extensions: MarkdownExtensions,
    }
    impl Render for Embedded {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let s = guiraffe_core::GuiraffeTheme::global(cx).surfaces();
            div()
                .size_full()
                .bg(s.body.to_hsla())
                .text_color(s.text.to_hsla())
                .child(TextView::new(&self.state).markdown_extensions(self.extensions.clone()))
        }
    }
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut snapshot = Snapshotter::new()?.with_setup(move |cx| {
            guiraffe_code::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let image=snapshot.render(size(px(800.),px(320.)),|_,cx|{
    let state=cx.new(|cx|TextViewState::markdown("# Code in Markdown\n\nThe fence below reuses the standalone code surface.\n\n```rust\nfn main() {\n    let value = 42;\n    println!(\"{value}\");\n}\n```\n",cx));
    let extensions=guiraffe_code::code_fences(MarkdownExtensions::default(),12);cx.new(|_|Embedded{state,extensions})
  })?;
        assert!(
            image.pixels().any(|p| p.0[2] > p.0[0].saturating_add(40)),
            "shared viewer painted highlighted code"
        );
        assert_snapshot(
            if scheme == Scheme::Light {
                "markdown-light"
            } else {
                "markdown-dark"
            },
            &image,
            &config,
        )?;
    }
    Ok(())
}

struct ScaleScene {
    code: CodeHandle,
    initial_line: Option<usize>,
}

impl Render for ScaleScene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let initial_scroll = self.initial_line.take().map(|line| {
            let code = self.code.clone();
            gpui_kit::canvas(
                move |_, _, cx| cx.defer(move |cx| code.scroll_to_line(line, cx)),
                |_, _, _, _| {},
            )
            .absolute()
            .w(px(0.))
            .h(px(0.))
        });
        div()
            .relative()
            .size_full()
            .child(CodeViewer::new(&self.code))
            .children(initial_scroll)
    }
}

#[test]
fn code_at_production_scale() -> Result<(), Box<dyn std::error::Error>> {
    const LINES: usize = 10_000_000;
    let document = Arc::new(CodeDocument::new(Arc::<str>::from(
        "x\n".repeat(LINES - 1) + "x",
    )));
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, theme) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
        let mut snapshot = Snapshotter::new()?.with_setup(move |cx| {
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        for (line, position) in [(0, "top"), (LINES / 2, "middle"), (LINES - 1, "end")] {
            let document = document.clone();
            let captured = Rc::new(RefCell::new(None));
            let control = captured.clone();
            let image = snapshot.render(size(px(384.), px(204.)), move |_, cx| {
                let code = CodeHandle::new(1, document, cx);
                *control.borrow_mut() = Some(code.clone());
                cx.new(|_| ScaleScene {
                    code,
                    initial_line: Some(line),
                })
            })?;
            assert!(
                captured
                    .borrow()
                    .as_ref()
                    .and_then(|code| code.anchor_in_window(Position::new(line, 0)))
                    .is_some()
            );
            assert_snapshot(&format!("scale/{position}-{theme}"), &image, &config)?;
        }
    }
    Ok(())
}

#[test]
fn layered_annotations_in_light_and_dark() -> Result<(), Box<dyn std::error::Error>> {
    use guiraffe_code::{
        AnchorBias, BlameCommit, DecorationInput, DecorationStyle, Decorations, GutterMark,
        HighlightShape, Severity, UnderlinePattern,
    };

    let document = Arc::new(CodeDocument::new(
        "fn total() {\n    let base = 4;\n    let tax = 2;\n    base + taz\n}\n",
    ));
    let mut decorations = Decorations::new(document.clone())?;
    let ranges = [
        (
            0,
            1,
            8,
            12,
            DecorationStyle::Highlight {
                shape: HighlightShape::Rounded,
                color: ColorName::Blue,
            },
        ),
        (
            1,
            3,
            11,
            14,
            DecorationStyle::Diagnostic {
                severity: Severity::Error,
                pattern: UnderlinePattern::Wavy,
                message: "Unknown variable".into(),
            },
        ),
        (
            2,
            2,
            0,
            0,
            DecorationStyle::Gutter {
                lane: 0,
                mark: GutterMark::Covered,
            },
        ),
        (
            2,
            3,
            0,
            0,
            DecorationStyle::Gutter {
                lane: 0,
                mark: GutterMark::Uncovered,
            },
        ),
        (
            3,
            1,
            8,
            12,
            DecorationStyle::Review {
                markdown: "**Review**\nAdd a case without tax.".into(),
            },
        ),
        (
            3,
            2,
            8,
            11,
            DecorationStyle::Block {
                estimated_rows: 2,
                payload: "Coverage review · Verify the zero-tax case".into(),
            },
        ),
    ];
    for (id, (layer, line, start, end, style)) in ranges.into_iter().enumerate() {
        let style = decorations.intern_style(style)?;
        decorations.update_layer(
            layer,
            Default::default(),
            &[],
            [DecorationInput {
                id: id as u64,
                range: SourceRange::new(Position::new(line, start), Position::new(line, end)),
                style,
                start_bias: AnchorBias::Left,
                end_bias: AnchorBias::Right,
            }],
        )?;
    }
    let blame = decorations.intern_style(DecorationStyle::Blame(Arc::new(BlameCommit {
        author: "Ada Rivera".into(),
        short_hash: "af86a44".into(),
        message: "Include tax".into(),
        date: "2026-11-07".into(),
        age_days: 14,
        url: None,
    })))?;
    decorations.update_layer(
        4,
        Default::default(),
        &[],
        [DecorationInput {
            id: 0,
            range: SourceRange::new(Position::new(0, 0), Position::new(4, 1)),
            style: blame,
            start_bias: AnchorBias::Left,
            end_bias: AnchorBias::Right,
        }],
    )?;
    let decorations = Arc::new(decorations);
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "annotations-light"),
        (Scheme::Dark, "annotations-dark"),
    ] {
        let mut snapshot = Snapshotter::new()?.with_setup(move |cx| {
            guiraffe_code::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let data = decorations.clone();
        let analysis = data.prepare_analysis(Vec::new(), Vec::new());
        let image = snapshot.render(size(px(1000.), px(250.)), move |_, cx| {
            let code = CodeHandle::new(1, data.document().clone(), cx);
            code.set_analysis(analysis, cx);
            code.set_decorations(Some(data), cx);
            code.set_hover(Some(Position::new(1, 9)), cx);
            code.on_block_widget(
                Some(Rc::new(|widget, _, cx| {
                    let surface = GuiraffeTheme::global(cx).surfaces();
                    div()
                        .h(px(30.))
                        .pl(px(7.))
                        .bg(surface.surface.to_hsla())
                        .text_color(surface.dimmed.to_hsla())
                        .child(widget.payload.to_string())
                        .into_any_element()
                })),
                cx,
            );
            cx.new(|_| Scene(code))
        })?;
        assert_snapshot(name, &image, &config)?;
    }
    Ok(())
}
