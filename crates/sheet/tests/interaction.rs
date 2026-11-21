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
