//! The shared selection context menu on the code viewer: right-click, keyboard, Copy,
//! Select all, Copy with line numbers, Go to definition, Escape/outside dismissal.
use gpui_kit::{
    Context, IntoElement, Modifiers, MouseButton, Render, TestAppContext, VisualTestContext,
    Window, point, px,
};
use guiraffe_code::{CodeDocument, CodeHandle, CodeOptions, CodeViewer, Position, SourceRange};
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct Host(CodeHandle);
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CodeViewer::new(&self.0)
    }
}
const SOURCE: &str =
    "fn total(items: &[u32]) -> u32 {\n    let sum = items.iter().sum();\n    sum\n}\n";
fn mount(cx: &mut TestAppContext) -> (CodeHandle, &mut VisualTestContext) {
    let mut handle = None;
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let code = CodeHandle::new(1, Arc::new(CodeDocument::new(Arc::<str>::from(SOURCE))), cx);
        code.set_options(
            CodeOptions {
                margin_chars: 0,
                ..Default::default()
            },
            cx,
        );
        handle = Some(code.clone());
        Host(code)
    });
    vcx.run_until_parked();
    (handle.unwrap_or_else(|| panic!("handle")), vcx)
}
fn at(code: &CodeHandle, line: usize, column: usize) -> gpui_kit::Point<gpui_kit::Pixels> {
    code.anchor_in_window(Position::new(line, column))
        .unwrap_or_else(|| panic!("anchor {line}:{column}"))
}
fn right_click(vcx: &mut VisualTestContext, p: gpui_kit::Point<gpui_kit::Pixels>) {
    vcx.simulate_mouse_down(p, MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(p, MouseButton::Right, Modifiers::default());
    vcx.run_until_parked();
}
fn clipboard(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
}
fn enabled(code: &CodeHandle, label: &str) -> Option<bool> {
    code.context_menu()
        .entries()
        .into_iter()
        .find(|(l, _)| l.as_ref() == label)
        .map(|(_, e)| e)
}

#[gpui_kit::test]
fn right_click_opens_at_pointer_and_copy_copies_exactly_the_selection(cx: &mut TestAppContext) {
    let (code, vcx) = mount(cx);
    let sum = SourceRange::new(Position::new(1, 8), Position::new(1, 11));
    vcx.update(|_, cx| code.select(Some(sum), cx));
    let p = at(&code, 1, 9);
    right_click(vcx, p);
    let menu = code.context_menu().clone();
    assert!(menu.is_open());
    assert_eq!(menu.position(), Some(p));
    assert_eq!(
        code.selection(),
        Some(sum),
        "right-click inside keeps the selection"
    );
    assert_eq!(enabled(&code, "Copy"), Some(true));
    assert_eq!(enabled(&code, "Select all"), Some(true));
    assert_eq!(enabled(&code, "Copy with line numbers"), Some(true));
    assert_eq!(
        enabled(&code, "Go to definition"),
        None,
        "no handler, no item"
    );
    vcx.update(|window, cx| assert!(menu.activate("Copy", window, cx)));
    vcx.run_until_parked();
    assert!(!menu.is_open());
    assert_eq!(clipboard(vcx).as_deref(), Some("sum"));
}

#[gpui_kit::test]
fn copy_is_disabled_without_a_selection(cx: &mut TestAppContext) {
    let (code, vcx) = mount(cx);
    vcx.update(|_, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".into())));
    right_click(vcx, at(&code, 2, 4));
    let menu = code.context_menu().clone();
    assert!(menu.is_open());
    assert_eq!(enabled(&code, "Copy"), Some(false));
    assert_eq!(enabled(&code, "Copy with line numbers"), Some(false));
    vcx.update(|window, cx| assert!(!menu.activate("Copy", window, cx)));
    assert_eq!(clipboard(vcx).as_deref(), Some("before"));
}

#[gpui_kit::test]
fn select_all_then_copy_with_line_numbers(cx: &mut TestAppContext) {
    let (code, vcx) = mount(cx);
    right_click(vcx, at(&code, 0, 3));
    let menu = code.context_menu().clone();
    vcx.update(|window, cx| assert!(menu.activate("Select all", window, cx)));
    vcx.run_until_parked();
    let doc = code.document();
    let end = doc
        .position(doc.source().len())
        .unwrap_or_else(|| panic!("end"));
    assert_eq!(
        code.selection(),
        Some(SourceRange::new(Position::default(), end))
    );
    vcx.update(|_, cx| {
        code.select(
            Some(SourceRange::new(Position::new(1, 4), Position::new(2, 7))),
            cx,
        )
    });
    right_click(vcx, at(&code, 1, 6));
    vcx.update(|window, cx| assert!(menu.activate("Copy with line numbers", window, cx)));
    assert_eq!(
        clipboard(vcx).as_deref(),
        Some("2  let sum = items.iter().sum();\n3      sum")
    );
}

#[gpui_kit::test]
fn escape_and_outside_click_close_and_focus_returns(cx: &mut TestAppContext) {
    let (code, vcx) = mount(cx);
    vcx.update(|window, cx| code.focus(window, cx));
    right_click(vcx, at(&code, 0, 3));
    assert!(code.context_menu().is_open());
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(!code.context_menu().is_open());
    // Focus is back on the viewer: the keyboard shortcut reopens the menu.
    vcx.simulate_keystrokes("shift-f10");
    vcx.run_until_parked();
    assert!(
        code.context_menu().is_open(),
        "Shift+F10 opens when focused"
    );
    let b = code.bounds();
    vcx.simulate_click(
        point(b.origin.x + px(4.), b.bottom() - px(4.)),
        Modifiers::default(),
    );
    vcx.run_until_parked();
    assert!(!code.context_menu().is_open());
}

#[gpui_kit::test]
fn clicking_the_rendered_copy_item_copies(cx: &mut TestAppContext) {
    let (code, vcx) = mount(cx);
    vcx.update(|_, cx| {
        code.select(
            Some(SourceRange::new(Position::new(0, 3), Position::new(0, 8))),
            cx,
        )
    });
    let p = at(&code, 0, 4);
    right_click(vcx, p);
    // The first item sits just below the anchor.
    vcx.simulate_click(p + point(px(30.), px(14.)), Modifiers::default());
    vcx.run_until_parked();
    assert!(!code.context_menu().is_open());
    assert_eq!(clipboard(vcx).as_deref(), Some("total"));
}

#[gpui_kit::test]
fn go_to_definition_appears_with_a_handler_and_symbol(cx: &mut TestAppContext) {
    let (code, vcx) = mount(cx);
    let hit = Rc::new(RefCell::new(None));
    let seen = hit.clone();
    let sum = SourceRange::new(Position::new(2, 4), Position::new(2, 7));
    vcx.update(|_, cx| {
        code.set_symbol_provider(
            Some(Rc::new(move |_| {
                Some(guiraffe_code::SymbolInfo {
                    range: sum,
                    occurrences: Arc::from([sum]),
                    markdown: "`sum`".into(),
                })
            })),
            cx,
        );
    });
    code.on_definition(Some(Rc::new(move |range, _, _| {
        *seen.borrow_mut() = Some(range);
    })));
    right_click(vcx, at(&code, 2, 5));
    assert_eq!(enabled(&code, "Go to definition"), Some(true));
    let menu = code.context_menu().clone();
    vcx.update(|window, cx| assert!(menu.activate("Go to definition", window, cx)));
    assert_eq!(*hit.borrow(), Some(sum));
}
