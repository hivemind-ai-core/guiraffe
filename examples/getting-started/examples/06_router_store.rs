//! Step 6: a multi-page app with a typed router and an event-driven store.
//!
//! `cargo run -p getting-started-examples --example 06_router_store --features gpui-fast`
//! `cargo test -p getting-started-examples --example 06_router_store` runs the handler tests.

use std::{sync::Arc, time::Duration};

use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, WindowOptions,
    component::{
        Disableable, Selectable,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
    },
    div, px,
};
use guiraffe_components::{Card, Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Size};
use guiraffe_router::{Route, Router, routes};
use guiraffe_state::{Event, Fx, Selected, Store, UndoEvent};

// ---------------------------------------------------------------- routes

routes! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Page {
        Todos = "/",
        Todo { id: u64 } = "/todos/:id",
        Settings = "/settings",
    }
}

// ---------------------------------------------------------------- state and events

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    id: u64,
    title: SharedString,
    done: bool,
}

/// The whole app state. It must be `Clone + PartialEq`; the `Arc` keeps cloning for undo
/// cheap and lets unchanged lists compare by pointer first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppState {
    todos: Arc<Vec<Todo>>,
    next_id: u64,
    status: Option<SharedString>,
}

#[derive(Debug)]
struct AddTodo(SharedString);
#[derive(Debug)]
struct ToggleTodo(u64);
#[derive(Debug)]
struct RemoveTodo(u64);
#[derive(Debug)]
struct ClearStatus;

/// An effect is plain data; the store runs the handler registered for its type.
#[derive(Debug)]
struct Log(String);

impl Event<AppState> for AddTodo {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        let title = self.0.trim().to_string();
        if title.is_empty() {
            return; // no state change: no re-render, no undo entry
        }
        fx.update(|s| {
            s.next_id += 1;
            let id = s.next_id;
            Arc::make_mut(&mut s.todos).push(Todo {
                id,
                title: title.into(),
                done: false,
            });
        });
    }
}

impl Event<AppState> for ToggleTodo {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        fx.update(|s| {
            if let Some(todo) = Arc::make_mut(&mut s.todos)
                .iter_mut()
                .find(|t| t.id == self.0)
            {
                todo.done = !todo.done;
            }
        });
    }
}

impl Event<AppState> for RemoveTodo {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        let Some(title) = fx
            .state()
            .todos
            .iter()
            .find(|t| t.id == self.0)
            .map(|t| t.title.clone())
        else {
            return;
        };
        fx.update(|s| {
            Arc::make_mut(&mut s.todos).retain(|t| t.id != self.0);
            s.status = Some(format!("Removed “{title}”. Undo brings it back.").into());
        });
        fx.effect(Log(format!("removed todo {}", self.0)));
        // A timer, described as data: the store dispatches ClearStatus in 4 s.
        fx.dispatch_debounced("status", Duration::from_secs(4), ClearStatus);
    }
}

impl Event<AppState> for ClearStatus {
    fn handle(self, fx: &mut Fx<'_, AppState>) {
        fx.update(|s| s.status = None);
    }
    fn undoable(&self) -> bool {
        false
    }
}

fn seed() -> AppState {
    let titles = [
        "Read the getting started guide",
        "Try the showcase",
        "Ship it",
    ];
    AppState {
        todos: Arc::new(
            titles
                .iter()
                .zip(1..)
                .map(|(title, id)| Todo {
                    id,
                    title: (*title).into(),
                    done: false,
                })
                .collect(),
        ),
        next_id: titles.len() as u64,
        status: None,
    }
}

// ---------------------------------------------------------------- views

/// The shell: navigation plus the current page. It only re-renders when the route or a
/// value it selected from the store changes.
struct Shell {
    router: Router<Page>,
    page: Selected<Page>,
    store: Store<AppState>,
    todos: Selected<Arc<Vec<Todo>>>,
    status: Selected<Option<SharedString>>,
    open_count: Selected<usize>,
    new_todo: Entity<InputState>,
    #[cfg(feature = "devtools")]
    devtools: guiraffe_devtools::Devtools,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    fn new(store: Store<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let router = Router::new(Page::Todos, cx);
        let page = router.select(cx, |p| p.clone());
        let todos = store.select(cx, |s| s.todos.clone());
        let status = store.select(cx, |s| s.status.clone());
        // A memoised selection: the count is recomputed only when the list changes.
        let open_count = store.select_memo(
            cx,
            |s| s.todos.clone(),
            |todos| todos.iter().filter(|t| !t.done).count(),
        );
        let new_todo = cx.new(|cx| InputState::new(window, cx).placeholder("What needs doing?"));
        let _subscriptions = vec![cx.subscribe_in(
            &new_todo,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let title = input.read(cx).value();
                    this.store.dispatch(AddTodo(title), cx);
                    input.update(cx, |input, cx| input.set_value("", window, cx));
                }
            },
        )];
        Self {
            #[cfg(feature = "devtools")]
            devtools: devtools(&store, cx),
            router,
            page,
            store,
            todos,
            status,
            open_count,
            new_todo,
            _subscriptions,
        }
    }

    fn nav(&self, target: Page, label: &'static str, current: &Page) -> Button {
        let router = self.router.clone();
        let selected = match (&target, current) {
            // The detail page belongs to the Todos section.
            (Page::Todos, Page::Todo { .. }) => true,
            _ => &target == current,
        };
        Button::new(label)
            .label(label)
            .ghost()
            .selected(selected)
            .on_click(move |_, _, cx| {
                router.navigate(target.clone(), cx);
            })
    }

    fn todos_page(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let rows = self.todos.get(cx).iter().map(|todo| {
            let (id, store, router) = (todo.id, self.store.clone(), self.router.clone());
            let toggle = self.store.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    Checkbox::new(("done", id))
                        .label(todo.title.clone())
                        .checked(todo.done)
                        .on_click(move |_, _, cx| toggle.dispatch(ToggleTodo(id), cx)),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(Button::new(("open", id)).label("Open").ghost().on_click(
                            move |_, _, cx| {
                                router.navigate(Page::Todo { id }, cx);
                            },
                        ))
                        .child(
                            Button::new(("remove", id))
                                .label("Remove")
                                .danger()
                                .on_click(move |_, _, cx| store.dispatch(RemoveTodo(id), cx)),
                        ),
                )
        });
        let undo = self.store.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(Title::new("Todos").order(2))
            .child(Text::new(format!("{} open", self.open_count.get(cx))).dimmed(true))
            .child(Input::new(&self.new_todo))
            .child(Card::new().child(div().flex().flex_col().gap(px(8.)).children(rows)))
            .child(
                div().flex().child(
                    Button::new("undo")
                        .label("Undo")
                        .disabled(!self.store.can_undo(cx))
                        .on_click(move |_, _, cx| undo.dispatch(UndoEvent, cx)),
                ),
            )
            .into_any_element()
    }

    fn todo_page(&self, id: u64, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let back = self.router.clone();
        let body = match self.todos.get(cx).iter().find(|t| t.id == id) {
            Some(todo) => div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(Title::new(todo.title.clone()).order(2))
                .child(
                    Text::new(if todo.done { "Done" } else { "Open" }).color(if todo.done {
                        ColorName::Teal
                    } else {
                        ColorName::Blue
                    }),
                ),
            None => div().child(Text::new(format!("No todo with id {id}.")).color(ColorName::Red)),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(body)
            .child(Text::new(format!("Route path: {}", Page::Todo { id }.to_path())).dimmed(true))
            .child(
                div()
                    .flex()
                    .child(Button::new("back").label("Back").on_click(move |_, _, cx| {
                        back.back(cx);
                    })),
            )
            .into_any_element()
    }

    fn settings_page(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let history = self.router.history(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(Title::new("Settings").order(2))
            .child(Text::new(format!("{} history entries", history.entries().len())).dimmed(true))
            .into_any_element()
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        let page = self.page.get(cx).clone();
        let content = match page {
            Page::Todos => self.todos_page(cx),
            Page::Todo { id } => self.todo_page(id, cx),
            Page::Settings => self.settings_page(cx),
        };
        let status = self.status.get(cx).clone();
        let shell = div()
            .size_full()
            .flex()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .child(
                div()
                    .w(px(180.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .p(px(Size::Sm.spacing()))
                    .bg(s.surface.to_hsla())
                    .child(self.nav(Page::Todos, "Todos", &page))
                    .child(self.nav(Page::Settings, "Settings", &page)),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .p(px(Size::Xl.spacing()))
                    .child(content)
                    .children(status.map(|text| Text::new(text).color(ColorName::Orange))),
            );
        // With `--features devtools`, Ctrl-Shift-I opens the inspector over the app.
        #[cfg(feature = "devtools")]
        return self.devtools.layer(self.devtools.track("shell", shell));
        #[cfg(not(feature = "devtools"))]
        shell.into_any_element()
    }
}

/// Developer tools for this window: render counters, frame times and the store's event
/// history with time travel. Compiled only with `--features devtools`.
#[cfg(feature = "devtools")]
fn devtools(store: &Store<AppState>, cx: &mut App) -> guiraffe_devtools::Devtools {
    use guiraffe_devtools::{Devtools, DiagnosticValue, inspect_store};
    let tools = Devtools::new(cx);
    let (adapter, _recorder) = inspect_store(
        "Todos",
        store,
        128,
        |s: &AppState| {
            DiagnosticValue::from(
                s.todos
                    .iter()
                    .map(|t| {
                        DiagnosticValue::from(format!(
                            "{} [{}]",
                            t.title,
                            if t.done { "x" } else { " " }
                        ))
                    })
                    .collect::<Vec<_>>(),
            )
        },
        cx,
    );
    tools.add_store(adapter, cx);
    tools
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            guiraffe_components::init(cx);
            #[cfg(feature = "devtools")]
            guiraffe_devtools::init(cx);
            let store = Store::new(seed(), cx);
            store.enable_undo(50, cx);
            store.on_effect(cx, |Log(line): Log, _, _| eprintln!("[todos] {line}"));
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let shell = cx.new(|cx| Shell::new(store, window, cx));
                #[cfg(feature = "devtools")]
                shell.read(cx).devtools.clone().focus(window, cx);
                cx.new(|cx| gpui_kit::component::Root::new(shell, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the todos window: {error}");
                cx.quit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_state::{Coeffects, Engine};

    /// Handlers are pure: test them with an `Engine`, no window or GPUI needed.
    #[test]
    fn add_toggle_remove_and_undo() {
        let mut engine = Engine::new(AppState::default());
        engine.enable_undo(10);
        engine.dispatch(AddTodo("  write docs ".into()));
        engine.dispatch(AddTodo("   ".into()));
        engine.process(&Coeffects::default());
        assert_eq!(engine.state().todos.len(), 1);
        assert_eq!(engine.state().todos[0].title, "write docs");

        engine.dispatch(ToggleTodo(1));
        engine.process(&Coeffects::default());
        assert!(engine.state().todos[0].done);

        engine.dispatch(RemoveTodo(1));
        let outcome = engine.process(&Coeffects::default());
        assert!(engine.state().todos.is_empty());
        assert!(engine.state().status.is_some());
        // The log line and the delayed ClearStatus come back as effects (data).
        assert_eq!(outcome.effects.len(), 2);

        assert!(engine.undo());
        assert_eq!(engine.state().todos.len(), 1);
    }

    #[test]
    fn routes_round_trip() {
        assert!(Page::check().is_empty());
        assert_eq!(Page::parse("/todos/7"), Some(Page::Todo { id: 7 }));
        assert_eq!(Page::Todo { id: 7 }.to_path(), "/todos/7");
        assert_eq!(Page::parse("/settings"), Some(Page::Settings));
    }
}
