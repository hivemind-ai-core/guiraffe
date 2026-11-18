use gpui_kit::{Context, IntoElement, Render, TestAppContext, VisualTestContext, Window};
use guiraffe_code::{
    Anchor, Annotation, AnnotationKind, CodeDocument, CodeHandle, CodeOptions, CodeViewer, Fold,
    Position, SourceRange, SymbolInfo, Trace,
};
use guiraffe_core::ColorName;
use std::{cell::Cell, rc::Rc, sync::Arc};
struct Host {
    code: CodeHandle,
    renders: Rc<Cell<u64>>,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        CodeViewer::new(&self.code)
    }
}
fn mount(
    cx: &mut TestAppContext,
    text: Arc<str>,
) -> (CodeHandle, Rc<Cell<u64>>, &mut VisualTestContext) {
    let renders = Rc::new(Cell::new(0));
    let root_renders = renders.clone();
    let mut handle = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let code = CodeHandle::new(1, Arc::new(CodeDocument::new(text)), cx);
        code.set_options(
            CodeOptions {
                margin_chars: 0,
                ..Default::default()
            },
            cx,
        );
        handle = Some(code.clone());
        Host {
            code,
            renders: root_renders,
        }
    });
    vcx.run_until_parked();
    (
        handle.unwrap_or_else(|| panic!("missing handle")),
        renders,
        vcx,
    )
}
#[gpui_kit::test]
fn hover_and_trace_changes_do_not_rerender_text(cx: &mut TestAppContext) {
    let (code, root, vcx) = mount(cx, Arc::from("fn main() {\n    value();\n}\n"));
    let range = SourceRange::new(Position::new(1, 4), Position::new(1, 9));
    vcx.update(|_, cx| {
        code.set_symbol_provider(
            Some(Rc::new(move |_| {
                Some(SymbolInfo {
                    range,
                    occurrences: Arc::from([range]),
                    markdown: "**value**\nA symbol detail card.".into(),
                })
            })),
            cx,
        )
    });
    vcx.run_until_parked();
    let before = code.stats();
    let before_root = root.get();
    vcx.update(|_, cx| {
        code.set_hover(Some(Position::new(1, 5)), cx);
        code.set_traces(
            Arc::from([Trace {
                from: Anchor {
                    viewer: 1,
                    position: Position::new(0, 3),
                },
                to: Anchor {
                    viewer: 1,
                    position: Position::new(1, 4),
                },
                color: ColorName::Blue,
            }]),
            cx,
        );
    });
    vcx.run_until_parked();
    let after = code.stats();
    assert!(after.overlay_renders > before.overlay_renders);
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            after.text_renders, before.text_renders,
            "overlay-only update reshaped code"
        );
        assert_eq!(root.get(), before_root, "host was invalidated");
    }
    let before = code.stats();
    vcx.update(|_, cx| code.set_hover(Some(Position::new(1, 5)), cx));
    vcx.run_until_parked();
    assert_eq!(
        code.stats().overlay_renders,
        before.overlay_renders,
        "same hover is a no-op"
    );
}
#[gpui_kit::test]
fn real_keyboard_selection_copy_and_folding(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("fn main() {\n    call();\n}\n"));
    vcx.update(|window, cx| code.focus(window, cx));
    vcx.simulate_keystrokes("ctrl-a ctrl-c");
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()),
            Some(code.document().source().to_string())
        );
    });
    let before = code.stats();
    vcx.update(|_, cx| {
        code.set_folds(
            vec![Fold {
                start: 0,
                end: 3,
                collapsed: true,
            }],
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(code.stats().last_visible_rows < before.last_visible_rows);
    vcx.update(|_, cx| code.toggle_fold(0, cx));
    vcx.run_until_parked();
    assert_eq!(code.stats().last_visible_rows, before.last_visible_rows);
}
#[gpui_kit::test]
fn ten_million_line_document_shapes_only_a_viewport_and_reuses_cache(cx: &mut TestAppContext) {
    use std::fmt::Write;
    let mut source = String::with_capacity(20_000_000);
    for line in 0..9_999_999 {
        let _ = writeln!(source, "{}", line % 10);
    }
    let (code, _, vcx) = mount(cx, Arc::from(source));
    assert_eq!(code.document().line_count(), 10_000_000);
    vcx.update(|window, cx| code.focus(window, cx));
    let start_y = code.viewport().y;
    for _ in 0..32 {
        vcx.simulate_keystrokes("pagedown");
    }
    vcx.run_until_parked();
    assert!(code.viewport().y > start_y);
    for _ in 0..32 {
        vcx.simulate_keystrokes("pageup");
    }
    vcx.run_until_parked();
    assert!((code.viewport().y - start_y).abs() < 0.01);

    assert!(code.stats().last_visible_rows < 100);
    let before = code.stats();
    vcx.update(|_, cx| code.scroll_to_line(9_900_000, cx));
    vcx.run_until_parked();
    let after = code.stats();
    let anchor_before = code
        .anchor_in_window(Position::new(9_900_000, 0))
        .expect("anchor");
    let y = code.viewport().y;
    vcx.update(|_, cx| {
        code.scroll_to(
            guiraffe_code::Viewport {
                y: y + 0.25,
                ..code.viewport()
            },
            cx,
        )
    });
    vcx.run_until_parked();
    assert_eq!(code.viewport().y, y + 0.25);
    let anchor_after = code
        .anchor_in_window(Position::new(9_900_000, 0))
        .expect("anchor after subpixel scroll");
    assert!((f32::from(anchor_before.y - anchor_after.y) - 0.25).abs() < 0.01);
    assert!(code.viewport().y > 1_000_000.);
    assert!(after.last_visible_rows < 100);
    assert!(
        after.shaped_lines - before.shaped_lines <= (after.last_visible_rows * 2 + 1) as u64,
        "only visible source and gutter lines may be shaped"
    );
    vcx.update(|_, cx| code.scroll_to_line(0, cx));
    vcx.run_until_parked();
    assert_eq!(
        code.stats().shaped_lines,
        after.shaped_lines,
        "returning to cached content must not shape again"
    );
    assert!(code.stats().cache_hits > after.cache_hits);
}
#[gpui_kit::test]
fn annotation_updates_preserve_source_and_plain_hover_does_not_shape(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("let result = 42;\n"));
    let doc = code.document();
    let before = code.stats();
    vcx.update(|_, cx| {
        code.set_annotations(
            Arc::from([Annotation {
                id: 7,
                range: SourceRange::new(Position::new(0, 4), Position::new(0, 10)),
                kind: AnnotationKind::Bubble,
                payload: "Analysis **result**".into(),
                color: ColorName::Blue,
            }]),
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(Arc::ptr_eq(&doc, &code.document()));
    if gpui_kit::GPUI_FAST {
        assert_eq!(before.text_renders, code.stats().text_renders);
    }
}

#[gpui_kit::test]
fn latest_source_wins_and_queued_analysis_is_coalesced(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("initial"));
    assert!(!code.is_analyzing());
    vcx.update(|_, cx| {
        for i in 0..100 {
            code.set_source(
                format!("source {i}").into(),
                "unregistered-language".into(),
                cx,
            );
        }
        assert!(code.is_analyzing());
    });
    vcx.run_until_parked();
    assert!(!code.is_analyzing());
    assert_eq!(code.document().source().as_ref(), "source 99");
    assert_eq!(
        code.stats().analyses_started,
        1,
        "pending requests replaced before the worker starts"
    );
    vcx.update(|_, cx| {
        code.set_source("obsolete".into(), "rust".into(), cx);
        code.set_highlighted(
            guiraffe_code::Highlighted::plain(Arc::new(CodeDocument::new(Arc::<str>::from(
                "authoritative",
            )))),
            cx,
        );
    });
    vcx.run_until_parked();
    assert_eq!(code.document().source().as_ref(), "authoritative");
}

#[gpui_kit::test]
fn mouse_hover_drag_and_minimap_click_use_source_coordinates(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("let value = 42;\n".repeat(1000)));
    let start = code
        .anchor_in_window(Position::new(0, 4))
        .unwrap_or_else(|| panic!("source anchor"));
    let end = code
        .anchor_in_window(Position::new(0, 9))
        .unwrap_or_else(|| panic!("source anchor"));
    vcx.simulate_mouse_down(
        start,
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    vcx.simulate_mouse_move(
        end,
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        end,
        gpui_kit::MouseButton::Left,
        gpui_kit::Modifiers::default(),
    );
    vcx.run_until_parked();
    assert_eq!(
        code.selection(),
        Some(SourceRange::new(Position::new(0, 4), Position::new(0, 9)))
    );
    let bounds = code.bounds();
    vcx.simulate_click(
        gpui_kit::point(
            bounds.right() - gpui_kit::px(5.),
            bounds.bottom() - gpui_kit::px(5.),
        ),
        gpui_kit::Modifiers::default(),
    );
    vcx.run_until_parked();
    assert!(code.viewport().y > 1000.);
}

#[gpui_kit::test]
fn cross_view_traces_follow_scroll_and_keep_both_text_layers_retained(cx: &mut TestAppContext) {
    use gpui_kit::{ParentElement, Styled, div};
    use guiraffe_code::{TraceHandle, Viewport};
    struct Pair {
        left: CodeHandle,
        right: CodeHandle,
        traces: TraceHandle,
    }
    impl Render for Pair {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .flex()
                        .size_full()
                        .child(div().flex_1().h_full().child(CodeViewer::new(&self.left)))
                        .child(div().flex_1().h_full().child(CodeViewer::new(&self.right))),
                )
                .child(self.traces.element())
        }
    }
    let mut controls = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let doc = Arc::new(CodeDocument::new(Arc::<str>::from(
            "let value = 1;\n".repeat(1000),
        )));
        let left = CodeHandle::new(1, doc.clone(), cx);
        let right = CodeHandle::new(2, doc, cx);
        left.set_options(
            CodeOptions {
                margin_chars: 0,
                ..Default::default()
            },
            cx,
        );
        right.set_options(
            CodeOptions {
                margin_chars: 0,
                ..Default::default()
            },
            cx,
        );
        let traces = TraceHandle::new(&[left.clone(), right.clone()], Arc::from([]), cx);
        controls = Some((left.clone(), right.clone(), traces.clone()));
        Pair {
            left,
            right,
            traces,
        }
    });
    vcx.run_until_parked();
    let (left, right, traces) = controls.unwrap_or_else(|| panic!("controls missing"));
    let before = (left.stats(), right.stats(), traces.render_count());
    vcx.update(|_, cx| {
        traces.set_traces(
            Arc::from([Trace {
                from: Anchor {
                    viewer: 1,
                    position: Position::new(5, 4),
                },
                to: Anchor {
                    viewer: 2,
                    position: Position::new(7, 4),
                },
                color: ColorName::Blue,
            }]),
            cx,
        )
    });
    vcx.run_until_parked();
    assert!(traces.render_count() > before.2);
    if gpui_kit::GPUI_FAST {
        assert_eq!(left.stats().text_renders, before.0.text_renders);
        assert_eq!(right.stats().text_renders, before.1.text_renders);
    }
    let start = left
        .anchor_in_window(Position::new(5, 4))
        .unwrap_or_else(|| panic!("visible anchor"));
    let before = traces.render_count();
    vcx.update(|_, cx| left.scroll_to(Viewport { x: 0., y: 10. }, cx));
    vcx.run_until_parked();
    let end = left
        .anchor_in_window(Position::new(5, 4))
        .unwrap_or_else(|| panic!("visible anchor after scroll"));
    assert_eq!(start.y - end.y, gpui_kit::px(10.));
    assert!(traces.render_count() > before);
}

#[gpui_kit::test]
fn stale_prepared_analysis_is_rejected_without_writes(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("old source\n"));
    let old = code.document();
    let prepared = std::thread::spawn(move || {
        guiraffe_code::PreparedAnalysis::new(
            old,
            Arc::from([Annotation {
                id: 1,
                range: SourceRange::new(Position::new(0, 0), Position::new(0, 3)),
                kind: AnnotationKind::Highlight,
                payload: "".into(),
                color: ColorName::Red,
            }]),
            vec![],
        )
    })
    .join()
    .unwrap_or_else(|_| panic!("analysis worker failed"));
    vcx.update(|_, cx| {
        code.set_highlighted(
            guiraffe_code::Highlighted::plain(Arc::new(CodeDocument::new(Arc::<str>::from(
                "new source\n",
            )))),
            cx,
        )
    });
    vcx.run_until_parked();
    let before = code.stats();
    vcx.update(|_, cx| assert!(!code.set_analysis(prepared, cx)));
    vcx.run_until_parked();
    assert_eq!(code.stats(), before);
}
#[gpui_kit::test]
fn construction_preserves_theme_and_inline_hints_keep_selection_in_source(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guiraffe_code::init(cx);
        guiraffe_core::set_scheme(guiraffe_core::Scheme::Dark, cx);
    });
    let (code, _, vcx) = mount(cx, Arc::from("let value = 42;\n"));
    vcx.update(|_, cx| {
        assert_eq!(
            guiraffe_core::GuiraffeTheme::global(cx).scheme,
            guiraffe_core::Scheme::Dark
        )
    });
    vcx.update(|_, cx| {
        code.set_annotations(
            Arc::from([Annotation {
                id: 1,
                range: SourceRange::new(Position::new(0, 4), Position::new(0, 9)),
                kind: AnnotationKind::InlineHint,
                payload: ": i32".into(),
                color: ColorName::Gray,
            }]),
            cx,
        )
    });
    vcx.run_until_parked();
    let start = code
        .anchor_in_window(Position::new(0, 10))
        .unwrap_or_else(|| panic!("source after hint"));
    vcx.simulate_click(start, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(
        code.selection(),
        Some(SourceRange::new(Position::new(0, 10), Position::new(0, 10)))
    );
}

#[gpui_kit::test]
fn a_new_request_drops_an_inflight_completion(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("initial"));
    vcx.update(|_, cx| code.set_source("stale".into(), "rust".into(), cx));
    for _ in 0..100 {
        if code.stats().analyses_started == 1 {
            break;
        }
        vcx.background_executor.tick();
    }
    assert_eq!(code.stats().analyses_started, 1);
    assert_eq!(
        code.document().source().as_ref(),
        "initial",
        "first result has not been published"
    );
    vcx.update(|_, cx| code.set_source("latest".into(), "text".into(), cx));
    vcx.run_until_parked();
    assert_eq!(code.document().source().as_ref(), "latest");
    assert_eq!(code.stats().stale_analyses, 1);
    assert_eq!(code.stats().analyses_started, 2);
}

#[gpui_kit::test]
fn real_symbol_hover_and_definition_callback_allow_reentry(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("let value = 42;\n"));
    let range = SourceRange::new(Position::new(0, 4), Position::new(0, 9));
    let activated = Rc::new(Cell::new(false));
    let flag = activated.clone();
    let copy = code.clone();
    vcx.update(|_, cx| {
        code.set_symbol_provider(
            Some(Rc::new(move |_| {
                Some(SymbolInfo {
                    range,
                    occurrences: Arc::from([range]),
                    markdown: "Symbol **value**".into(),
                })
            })),
            cx,
        );
        code.on_definition(Some(Rc::new(move |target, _, cx| {
            assert_eq!(target, range);
            flag.set(true);
            copy.on_definition(None);
            copy.set_hover(None, cx);
        })))
    });
    let anchor = code
        .anchor_in_window(Position::new(0, 5))
        .unwrap_or_else(|| panic!("visible symbol"));
    let before = code.stats();
    vcx.simulate_mouse_move(anchor, None, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert!(code.stats().overlay_renders > before.overlay_renders);
    vcx.simulate_click(
        anchor,
        gpui_kit::Modifiers {
            control: true,
            ..Default::default()
        },
    );
    vcx.run_until_parked();
    assert!(activated.get());
}

#[gpui_kit::test]
fn trace_anchors_outside_the_horizontal_viewport_are_hidden(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("let value = 42;".repeat(200)));
    assert!(code.anchor_in_window(Position::new(0, 4)).is_some());
    vcx.update(|_, cx| code.scroll_to(guiraffe_code::Viewport { x: 1000., y: 0. }, cx));
    vcx.run_until_parked();
    assert!(code.anchor_in_window(Position::new(0, 4)).is_none());
}

#[gpui_kit::test]
fn releasing_outside_stops_mouse_selection(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("let value = 42;\n"));
    let start = code
        .anchor_in_window(Position::new(0, 4))
        .unwrap_or_else(|| panic!("visible start"));
    let end = code
        .anchor_in_window(Position::new(0, 9))
        .unwrap_or_else(|| panic!("visible end"));
    let modifiers = gpui_kit::Modifiers::default();
    vcx.simulate_mouse_down(start, gpui_kit::MouseButton::Left, modifiers);
    vcx.simulate_mouse_move(end, gpui_kit::MouseButton::Left, modifiers);
    let bounds = code.bounds();
    vcx.simulate_mouse_up(
        gpui_kit::point(
            bounds.right() + gpui_kit::px(20.),
            bounds.bottom() + gpui_kit::px(20.),
        ),
        gpui_kit::MouseButton::Left,
        modifiers,
    );
    let selected = code.selection();
    vcx.simulate_mouse_move(start, None, modifiers);
    vcx.run_until_parked();
    assert_eq!(
        code.selection(),
        selected,
        "hover after an outside release must not extend the selection"
    );
}

#[gpui_kit::test]
fn copy_without_a_nonempty_selection_keeps_clipboard(cx: &mut TestAppContext) {
    let (code, _, vcx) = mount(cx, Arc::from("source text"));
    vcx.update(|_, cx| code.scroll_to(guiraffe_code::Viewport { x: 1e12, y: 0. }, cx));
    assert_eq!(
        code.viewport().x,
        0.,
        "a short line cannot scroll past its content"
    );
    vcx.update(|window, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("keep me".into()));
        code.focus(window, cx);
    });
    vcx.simulate_keystrokes("ctrl-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("keep me")
        );
        code.select(
            Some(SourceRange::new(Position::default(), Position::default())),
            cx,
        );
    });
    vcx.simulate_keystrokes("ctrl-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some("keep me")
        )
    });
}
#[gpui_kit::test]
fn horizontal_scroll_is_bounded_and_long_line_shaping_is_capped(cx: &mut TestAppContext) {
    let source: Arc<str> = "א界🙂".repeat(112_000).into();
    let (code, _, vcx) = mount(cx, source.clone());
    assert!(code.stats().max_shaped_bytes <= guiraffe_code::MAX_SHAPED_LINE_BYTES);
    assert_eq!(code.stats().last_truncated_rows, 1);
    assert!(code.anchor_in_window(Position::new(0, 100_000)).is_none());
    vcx.update(|_, cx| code.scroll_to(guiraffe_code::Viewport { x: 1e15, y: 0. }, cx));
    vcx.run_until_parked();
    assert!(code.viewport().x > 0. && code.viewport().x < 1e6);
    vcx.update(|window, cx| code.focus(window, cx));
    vcx.simulate_keystrokes("ctrl-a ctrl-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some(source.as_ref())
        )
    });
}

#[gpui_kit::test]
fn scroll_burst_and_theme_change_preserve_selection_and_keyboard_focus(cx: &mut TestAppContext) {
    let line = "א界🙂";
    let source: Arc<str> = format!("{line}\n").repeat(128).into();
    let (code, _, vcx) = mount(cx, source);
    let selection = SourceRange::new(Position::new(120, 0), Position::new(120, line.len()));
    vcx.update(|window, cx| {
        code.focus(window, cx);
        code.select(Some(selection), cx);
        for target in 0..127 {
            code.scroll_to_line(target, cx);
            if target == 64 {
                guiraffe_core::set_scheme(guiraffe_core::Scheme::Dark, cx);
            }
        }
    });
    vcx.run_until_parked();
    assert_eq!(code.selection(), Some(selection));
    assert!(code.anchor_in_window(selection.start).is_some());
    vcx.simulate_keystrokes("ctrl-c");
    vcx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
            Some(line)
        );
    });
}

#[gpui_kit::test]
fn dragging_on_the_minimap_scrolls_continuously_and_does_not_select(cx: &mut TestAppContext) {
    use gpui_kit::{Modifiers, MouseButton, point, px};
    let (code, _, vcx) = mount(cx, Arc::from("let value = 42;\n".repeat(1000)));
    let b = code.bounds();
    let x = b.right() - px(5.);
    let at = |frac: f32| point(x, b.top() + (b.bottom() - b.top()) * frac);
    vcx.simulate_mouse_down(at(0.1), MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let first = code.viewport().y;
    vcx.simulate_mouse_move(at(0.5), MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    let middle = code.viewport().y;
    assert!(
        middle > first,
        "drag down scrolls down: {first} -> {middle}"
    );
    // Leaving the minimap horizontally keeps dragging (clamped to its vertical extent).
    vcx.simulate_mouse_move(
        point(b.left() + px(20.), b.top() + (b.bottom() - b.top()) * 0.9),
        MouseButton::Left,
        Modifiers::default(),
    );
    vcx.run_until_parked();
    let far = code.viewport().y;
    assert!(
        far > middle,
        "drag continues off the minimap: {middle} -> {far}"
    );
    assert!(
        code.selection().is_none_or(|s| s.start == s.end),
        "a minimap drag never selects text: {:?}",
        code.selection()
    );
    vcx.simulate_mouse_up(at(0.9), MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    // After release, moving the pointer no longer scrolls.
    vcx.simulate_mouse_move(at(0.2), None, Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(code.viewport().y, far, "released: no more scrolling");
}
