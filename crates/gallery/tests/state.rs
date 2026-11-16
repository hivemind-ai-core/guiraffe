use guiraffe_gallery::todos::{Add, Remove, TodoState, Toggle};
use guiraffe_state::{Store, UndoEvent};

#[gpui_kit::test]
fn todo_flow_through_the_store(cx: &mut gpui_kit::TestAppContext) {
    let store = cx.update(|cx| {
        let s = Store::new(TodoState::default(), cx);
        s.enable_undo_lens(10, |s: &TodoState| s.todos.clone(), |s, t| s.todos = t, cx);
        s
    });
    cx.update(|cx| {
        store.dispatch(Add("one".into()), cx);
        store.dispatch(Add("two".into()), cx);
        store.dispatch(Toggle(1), cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| store.state(cx).remaining()), 1);
    cx.update(|cx| store.dispatch(Remove(2), cx));
    cx.run_until_parked();
    cx.update(|cx| store.dispatch(UndoEvent, cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| store.state(cx).todos.len()),
        2,
        "undo restores the removed task"
    );
}
