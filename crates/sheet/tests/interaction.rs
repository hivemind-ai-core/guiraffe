use gpui_kit::{Modifiers, TestAppContext, point, px, size};
use guiraffe_sheet::{Cell, CellPos, SheetModel, SheetState};

#[gpui_kit::test]
fn select_edit_and_undo(cx: &mut TestAppContext) {
    cx.update(guiraffe_core::init);
    let (sheet, window) = cx.add_window_view(|_, cx| {
        let model =
            SheetModel::from_cells(1_000_000, 1_000, [(CellPos::new(0, 0), Cell::new("100"))]);
        SheetState::new(model, cx)
    });
    window.simulate_resize(size(px(800.), px(600.)));
    window.run_until_parked();
    window.simulate_click(point(px(85.), px(40.)), Modifiers::default());
    window.simulate_keystrokes("right shift-down");
    window.update(|_, cx| {
        assert_eq!(sheet.read(cx).model().selection.rect(), Some((0..2, 1..2)));
    });
    window.simulate_keystrokes("f2 escape");
    window.update(|_, cx| {
        assert!(sheet.read(cx).model().cell(CellPos::new(1, 1)).is_none());
    });
    window.simulate_keystrokes("5 enter");
    window.update(|_, cx| {
        assert_eq!(
            sheet
                .read(cx)
                .model()
                .cell(CellPos::new(1, 1))
                .map(|c| c.text.as_ref()),
            Some("5")
        );
        sheet.update(cx, |s, cx| s.set_read_only(true, cx));
    });
    window.simulate_keystrokes("ctrl-z");
    window.update(|_, cx| {
        assert!(sheet.read(cx).model().cell(CellPos::new(1, 1)).is_some());
        sheet.update(cx, |s, cx| s.set_read_only(false, cx));
    });
    window.simulate_keystrokes("ctrl-z");
    window.update(|_, cx| {
        assert!(sheet.read(cx).model().cell(CellPos::new(1, 1)).is_none());
    });
}

#[gpui_kit::test]
fn fill_handle_extends_series_as_one_transaction(cx: &mut TestAppContext) {
    cx.update(guiraffe_core::init);
    let (sheet, window) = cx.add_window_view(|_, cx| {
        let mut model = SheetModel::from_cells(
            1_000_000,
            1_000,
            [
                (CellPos::new(0, 0), Cell::new("2")),
                (CellPos::new(1, 0), Cell::new("4")),
            ],
        );
        model.select(CellPos::new(0, 0), false);
        model.select(CellPos::new(1, 0), true);
        SheetState::new(model, cx)
    });
    window.simulate_resize(size(px(800.), px(600.)));
    window.run_until_parked();
    let from = point(px(170.), px(75.));
    let to = point(px(170.), px(140.));
    let button = gpui_kit::MouseButton::Left;
    window.simulate_mouse_down(from, button, Modifiers::default());
    window.simulate_mouse_move(to, Some(button), Modifiers::default());
    window.simulate_mouse_up(to, button, Modifiers::default());
    window.update(|_, cx| {
        assert_eq!(
            sheet
                .read(cx)
                .model()
                .cell(CellPos::new(4, 0))
                .map(|c| c.text.as_ref()),
            Some("10")
        );
    });
    window.simulate_keystrokes("ctrl-z");
    window.update(|_, cx| {
        assert_eq!(sheet.read(cx).model().cells().count(), 2);
    });
}

#[gpui_kit::test]
fn whole_range_clipboard_paste_is_one_edit(cx: &mut TestAppContext) {
    cx.update(guiraffe_core::init);
    let (sheet, window) =
        cx.add_window_view(|_, cx| SheetState::new(SheetModel::new(1_000_000, 1_000), cx));
    window.simulate_resize(size(px(800.), px(600.)));
    window.run_until_parked();
    window.simulate_click(point(px(85.), px(40.)), Modifiers::default());
    window.update(|_, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("10\t11\n12\t13".into()));
    });
    window.simulate_keystrokes("ctrl-v shift-down shift-right ctrl-c");
    window.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("10\t11\n12\t13"),
        );
        assert_eq!(sheet.read(cx).model().cells().count(), 4);
    });
    window.simulate_keystrokes("ctrl-z");
    window.update(|_, cx| assert_eq!(sheet.read(cx).model().cells().count(), 0));
}

#[gpui_kit::test]
fn user_edits_emit_events_and_builders_configure_state(cx: &mut TestAppContext) {
    use guiraffe_sheet::SheetEvent;
    use std::{cell::RefCell, rc::Rc};

    cx.update(guiraffe_core::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    let (sheet, window) = cx.add_window_view(|_, cx| {
        SheetState::new(SheetModel::new(100, 10), cx)
            .frozen_rows(2)
            .frozen_cols(500)
    });
    window.update(|_, cx| {
        assert_eq!(
            sheet.read(cx).frozen(),
            (2, 10),
            "columns clamp to the sheet"
        );
        cx.subscribe(&sheet, move |_, event: &SheetEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
        sheet.update(cx, |s, cx| {
            s.edit([(CellPos::new(0, 0), Cell::new("1"))], cx)
        });
    });
    assert!(
        events.borrow().is_empty(),
        "programmatic edits are not echoed"
    );
    window.simulate_resize(size(px(800.), px(600.)));
    window.run_until_parked();
    window.simulate_click(point(px(85.), px(40.)), Modifiers::default());
    window.simulate_keystrokes("7 enter");
    window.run_until_parked();
    assert_eq!(
        events.borrow().as_slice(),
        &[SheetEvent::Edited(vec![CellPos::new(0, 0)])]
    );

    let (read_only, window) =
        cx.add_window_view(|_, cx| SheetState::new(SheetModel::new(10, 10), cx).read_only(true));
    window.simulate_resize(size(px(800.), px(600.)));
    window.run_until_parked();
    window.simulate_click(point(px(85.), px(40.)), Modifiers::default());
    window.simulate_keystrokes("7 enter");
    window.update(|_, cx| {
        assert!(read_only.read(cx).model().read_only);
        assert!(
            read_only
                .read(cx)
                .model()
                .cell(CellPos::new(0, 0))
                .is_none()
        );
    });
}
