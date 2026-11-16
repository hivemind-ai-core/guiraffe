//! The State page's model: a todo list on guiraffe-state, with undo scoped to the list.

use std::sync::Arc;

use guiraffe_state::{Event, Fx};

#[derive(Debug, Clone, PartialEq)]
pub struct Todo {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TodoState {
    /// `Arc` so undo snapshots and comparisons are cheap (structural sharing).
    pub todos: Arc<Vec<Todo>>,
    pub next_id: u64,
    /// Not undoable: a running count of everything ever added.
    pub added_total: u64,
}

impl TodoState {
    pub fn remaining(&self) -> usize {
        self.todos.iter().filter(|t| !t.done).count()
    }
}

#[derive(Debug)]
pub struct Add(pub String);
impl Event<TodoState> for Add {
    fn handle(self, fx: &mut Fx<'_, TodoState>) {
        let title = self.0.trim().to_owned();
        if title.is_empty() {
            return;
        }
        fx.update(|s| {
            s.next_id += 1;
            s.added_total += 1;
            let id = s.next_id;
            Arc::make_mut(&mut s.todos).push(Todo {
                id,
                title,
                done: false,
            });
        });
    }
}

#[derive(Debug)]
pub struct Toggle(pub u64);
impl Event<TodoState> for Toggle {
    fn handle(self, fx: &mut Fx<'_, TodoState>) {
        fx.update(|s| {
            if let Some(t) = Arc::make_mut(&mut s.todos)
                .iter_mut()
                .find(|t| t.id == self.0)
            {
                t.done = !t.done;
            }
        });
    }
}

#[derive(Debug)]
pub struct Remove(pub u64);
impl Event<TodoState> for Remove {
    fn handle(self, fx: &mut Fx<'_, TodoState>) {
        fx.update(|s| Arc::make_mut(&mut s.todos).retain(|t| t.id != self.0));
    }
}

#[derive(Debug)]
pub struct ClearDone;
impl Event<TodoState> for ClearDone {
    fn handle(self, fx: &mut Fx<'_, TodoState>) {
        fx.update(|s| Arc::make_mut(&mut s.todos).retain(|t| !t.done));
    }
}

#[cfg(test)]
mod tests {
    use guiraffe_state::{Coeffects, Engine, UndoEvent};

    use super::*;

    fn engine() -> Engine<TodoState> {
        let mut e = Engine::new(TodoState::default());
        e.enable_undo_lens(50, |s: &TodoState| s.todos.clone(), |s, t| s.todos = t);
        e
    }

    #[test]
    fn add_toggle_clear_and_blank_titles() {
        let mut e = engine();
        e.dispatch(Add("Write docs".into()));
        e.dispatch(Add("   ".into()));
        e.dispatch(Add("Ship".into()));
        e.dispatch(Toggle(1));
        e.process(&Coeffects::default());
        assert_eq!((e.state().todos.len(), e.state().remaining()), (2, 1));
        e.dispatch(ClearDone);
        e.process(&Coeffects::default());
        assert_eq!(
            e.state()
                .todos
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Ship"]
        );
    }

    #[test]
    fn undo_restores_the_list_but_not_the_counter() {
        let mut e = engine();
        e.dispatch(Add("a".into()));
        e.dispatch(Add("b".into()));
        e.dispatch(UndoEvent);
        e.process(&Coeffects::default());
        assert_eq!(e.state().todos.len(), 1);
        assert_eq!(e.state().added_total, 2, "outside the undo lens");
        assert_eq!(e.state().next_id, 2, "ids never reused after undo");
    }
}
