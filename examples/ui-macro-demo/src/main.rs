use std::collections::HashMap;

use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, SharedString, StyleRefinement, Window,
    WindowOptions,
    component::{
        Disableable, Selectable,
        button::Button,
        input::{Input, InputState},
    },
    div,
    prelude::*,
    px,
};
use guiraffe_components::{Card, Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, apply};
use guiraffe_ui::{component, ui};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Tasks,
    Detail,
    Stats,
}

#[derive(Clone, Copy)]
struct Task {
    id: usize,
    title: &'static str,
    owner: &'static str,
    done: bool,
}

const TASKS: [Task; 4] = [
    Task {
        id: 1,
        title: "Review keyboard navigation",
        owner: "Ada",
        done: false,
    },
    Task {
        id: 2,
        title: "Publish the build notes",
        owner: "Lin",
        done: true,
    },
    Task {
        id: 3,
        title: "Check the dark theme",
        owner: "Sam",
        done: false,
    },
    Task {
        id: 4,
        title: "Update the release checklist",
        owner: "Ada",
        done: true,
    },
];

#[component]
fn TaskLabel(
    title: SharedString,
    owner: SharedString,
    #[prop(default)] done: bool,
) -> impl IntoElement {
    ui! {
        div {
            flex flex_col gap_1
            Text(title)
            Text(owner) { dimmed: true }
            if done { Text("Complete") { color: ColorName::Teal } }
        }
    }
}

struct TeamTasks {
    page: Page,
    open_only: bool,
    selected: Task,
    title: Entity<InputState>,
    assignee: Entity<InputState>,
    scheme: Scheme,
    saved: Option<SharedString>,
    edits: HashMap<usize, (SharedString, SharedString)>,
}

impl TeamTasks {
    fn new(page: Page, scheme: Scheme, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let [selected, ..] = TASKS;
        Self {
            page,
            open_only: false,
            selected,
            title: cx.new(|cx| InputState::new(window, cx).default_value(selected.title)),
            assignee: cx.new(|cx| InputState::new(window, cx).default_value(selected.owner)),
            scheme,
            saved: None,
            edits: HashMap::new(),
        }
    }
}

impl Render for TeamTasks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let dark = self.scheme == Scheme::Dark;
        let open_only = self.open_only;
        let title = self.title.read(cx).value();
        let empty = title.trim().is_empty();
        let selected = self.selected;
        let saved = self.saved.clone();
        let edits = &self.edits;
        let tasks = TASKS
            .into_iter()
            .filter(|task| !open_only || !task.done)
            .map(|task| {
                let (title, owner) = edits
                    .get(&task.id)
                    .cloned()
                    .unwrap_or_else(|| (task.title.into(), task.owner.into()));
                (task, title, owner)
            });
        let title_input = self.title.clone();
        let assignee_input = self.assignee.clone();
        ui! {
            div {
                size_full flex gap_6 p_6
                div {
                    w: px(185.)
                    flex_none flex flex_col gap_3
                    Title("Team tasks") { order: 2 }
                    Button("tasks") {
                        label: "All tasks"
                        selected: page == Page::Tasks
                        on_click: cx.listener(|this, _, _, cx| { this.page = Page::Tasks; cx.notify(); })
                    }
                    Button("detail") {
                        label: "Task details"
                        selected: page == Page::Detail
                        on_click: cx.listener(|this, _, _, cx| { this.page = Page::Detail; cx.notify(); })
                    }
                    Button("stats") {
                        label: "Team stats"
                        selected: page == Page::Stats
                        on_click: cx.listener(|this, _, _, cx| { this.page = Page::Stats; cx.notify(); })
                    }
                    Button("theme") {
                        label: if dark { "Light theme" } else { "Dark theme" }
                        on_click: cx.listener(|this, _, _, cx| {
                            this.scheme = if this.scheme == Scheme::Dark { Scheme::Light } else { Scheme::Dark };
                            apply(GuiraffeTheme { scheme: this.scheme, ..Default::default() }, cx);
                            cx.notify();
                        })
                    }
                }
                div {
                    flex_1 flex flex_col gap_4
                    match page {
                        Page::Tasks => div {
                            flex flex_col gap_3
                            Title("Work queue") { order: 1 }
                            Button("filter") {
                                label: if open_only { "Show all tasks" } else { "Show open tasks" }
                                selected: open_only
                                on_click: cx.listener(|this, _, _, cx| { this.open_only = !this.open_only; cx.notify(); })
                            }
                            for (task, title, owner) in tasks key task.id {
                                Card {
                                    div {
                                        flex justify_between items_center gap_4
                                        TaskLabel(title.clone(), owner.clone()) { done: task.done }
                                        Button("open") {
                                            label: "Open"
                                            on_click: cx.listener(move |this, _, window, cx| {
                                                this.selected = task;
                                                this.title.update(cx, |input, cx| input.set_value(title.clone(), window, cx));
                                                this.assignee.update(cx, |input, cx| input.set_value(owner.clone(), window, cx));
                                                this.saved = None;
                                                this.page = Page::Detail;
                                                cx.notify();
                                            })
                                        }
                                    }
                                }
                            }
                        },
                        Page::Detail => div {
                            flex flex_col gap_3
                            Title("Task details") { order: 1 }
                            Text(format!("Task {}", selected.id)) { dimmed: true }
                            Text("Title")
                            Input(&title_input)
                            if empty { Text("Enter a task title.") { color: ColorName::Red } }
                            Text("Assignee")
                            Input(&assignee_input)
                            Button("save") {
                                label: "Save"
                                disabled: empty
                                on_click: cx.listener(|this, _, _, cx| {
                                    let title = this.title.read(cx).value();
                                    if !title.trim().is_empty() {
                                        let owner = this.assignee.read(cx).value();
                                        this.edits.insert(this.selected.id, (title.clone(), owner));
                                        this.saved = Some(title);
                                        cx.notify();
                                    }
                                })
                            }
                            if let Some(saved) = saved { Text(format!("Saved {saved}")) { color: ColorName::Teal } }
                        },
                        Page::Stats => div {
                            flex flex_col gap_3
                            Title("Team stats") { order: 1 }
                            Card {
                                Text("Completion by status")
                                cached(StyleRefinement::default().w(px(360.)).h(px(115.))) {
                                    div {
                                        flex flex_col gap_3 p_3
                                        for (label, width, color) in [
                                            ("Open · 2", 128., gpui_kit::rgb(0x397dc5)),
                                            ("Complete · 2", 128., gpui_kit::rgb(0x23876d)),
                                        ] {
                                            div {
                                                flex items_center gap_3
                                                div { w: px(110.) {label} }
                                                div { w: px(width) h: px(12.) rounded_full bg: color }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

fn init(scheme: Scheme, cx: &mut App) {
    guiraffe_components::init(cx);
    apply(
        GuiraffeTheme {
            scheme,
            ..Default::default()
        },
        cx,
    );
}

fn main() {
    gpui_kit::application().run(|cx| {
        init(Scheme::Light, cx);
        let result = cx.open_window(WindowOptions::default(), |window, cx| {
            let tasks = cx.new(|cx| {
                let tasks = TeamTasks::new(Page::Tasks, Scheme::Light, window, cx);
                cx.observe(&tasks.title, |_, _, cx| cx.notify()).detach();
                tasks
            });
            cx.new(|cx| gpui_kit::component::Root::new(tasks, window, cx))
        });
        if let Err(error) = result {
            eprintln!("Could not open team tasks: {error}");
            cx.quit();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::size;
    use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

    #[test]
    fn pages_in_both_themes() -> anyhow::Result<()> {
        let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
            .update_from_env();
        for (page, name) in [
            (Page::Tasks, "tasks"),
            (Page::Detail, "detail"),
            (Page::Stats, "stats"),
        ] {
            for (scheme, theme) in [(Scheme::Light, "light"), (Scheme::Dark, "dark")] {
                let image =
                    Snapshotter::new()?.render(size(px(940.), px(600.)), move |window, cx| {
                        init(scheme, cx);
                        let tasks = cx.new(|cx| TeamTasks::new(page, scheme, window, cx));
                        cx.new(|cx| gpui_kit::component::Root::new(tasks, window, cx))
                    })?;
                assert_snapshot(&format!("{name}/{theme}"), &image, &config)?;
            }
        }
        Ok(())
    }
}
