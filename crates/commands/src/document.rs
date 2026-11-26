//! Document adapters for native command routing and menus.

use gpui_kit::{
    App, ElementId, IntoElement, ParentElement, RenderOnce, Styled, WeakEntity, Window, div,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{
    Command, CommandId, CommandScope, Commands, Keybinding, Menu, When, registry,
    ribbon::{
        CommandRibbon, RibbonButton, RibbonLayout, RibbonMenu, RibbonModel, RibbonProps,
        RibbonState,
    },
    widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
                render: |_, children, _, cx| match registry(cx) {
                    Ok(commands) => CommandScope::new(&commands)
                        .children(children)
                        .into_any_element(),
                    Err(_) => div().children(children).into_any_element(),
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[1],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
                render: |node, _, _, _| {
                    DocumentMenu {
                        key: node.state_id(),
                        id: node.element_id(),
                        entries: node.prop(0).clone(),
                        categories: node.prop(1).as_bool().unwrap_or(true),
                        keybinds: node.prop(2).as_bool().unwrap_or(true),
                        invoke: node.handler(0),
                        error: node.handler(1),
                    }
                    .into_any_element()
                },
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[2],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[2]),
                render: |node, _, _, _| {
                    let count = |index| {
                        node.prop(index)
                            .as_f64()
                            .filter(|v| v.is_finite() && *v >= 1.)
                            .map(|v| v as usize)
                    };
                    let defaults = RibbonProps::default();
                    DocumentRibbon {
                        key: node.state_id(),
                        props: RibbonProps {
                            // A document places its ribbon itself.
                            placement: crate::ribbon::RibbonPlacement::Slotted,
                            rows_per_column: count(0).unwrap_or(defaults.rows_per_column),
                            show_icons: node.prop(1).as_bool().unwrap_or(true),
                            show_shortcuts: node.prop(2).as_bool().unwrap_or(true),
                            max_columns: count(3).unwrap_or(defaults.max_columns),
                            row_height: node.prop(4).as_f64().map(|v| v as f32),
                        },
                        open: node.prop(5).as_bool().unwrap_or(false),
                        label: node
                            .prop(7)
                            .as_str()
                            .unwrap_or("Commands")
                            .to_owned()
                            .into(),
                    }
                    .into_any_element()
                },
            },
        ],
    );
}

/// DataUi mount of the ribbon: the application's default registry with an empty layout
/// (documents cannot describe ribbon layouts yet; see KNOWN_ISSUES.md), the status-bar
/// button and the bar while open.
#[derive(IntoElement)]
struct DocumentRibbon {
    key: ElementId,
    props: RibbonProps,
    open: bool,
    label: gpui_kit::SharedString,
}

impl RenderOnce for DocumentRibbon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Ok(commands) = registry(cx) else {
            return div().into_any_element();
        };
        let model = {
            let empty = RibbonLayout::new("root").menu(RibbonMenu::new("root", "Commands"));
            let catalog = commands.entity().read(cx).catalog();
            RibbonModel::build(&empty, catalog, &Default::default())
        };
        let Ok(model) = model else {
            return div().into_any_element();
        };
        let props = self.props;
        let ribbon = window.use_keyed_state(self.key, cx, |window, cx| {
            RibbonState::new(&commands, std::rc::Rc::new(model), window, cx)
        });
        let open = self.open;
        ribbon.update(cx, |state, cx| {
            state.set_props(props, cx);
            if open && !state.is_open() {
                state.open(window, cx);
            } else if !open && state.is_open() {
                state.close(window, cx);
            }
        });
        crate::ribbon::ribbon_scope(&ribbon)
            .flex()
            .flex_col()
            .children(CommandRibbon::when_open(&ribbon, cx))
            .child(RibbonButton::new(&ribbon).label(self.label))
            .into_any_element()
    }
}

#[derive(IntoElement)]
struct DocumentMenu {
    key: ElementId,
    id: ElementId,
    entries: Value,
    categories: bool,
    keybinds: bool,
    invoke: Option<EventHandler>,
    error: Option<EventHandler>,
}

struct MenuState {
    entries: Value,
    commands: Commands,
    invoke: Option<EventHandler>,
}

fn register_entries(
    entries: &Value,
    state: WeakEntity<MenuState>,
    cx: &mut App,
) -> (Commands, Vec<String>) {
    let commands = Commands::new(cx);
    let mut errors = Vec::new();
    let id = ["id".into()];
    let title = ["title".into()];
    let keywords = ["keywords".into()];
    let category = ["category".into()];
    let shortcut = ["keybinding".into()];
    let when = ["when".into()];
    let enabled = ["enabled".into()];
    for index in 0..entries.list_len().unwrap_or_default() {
        let Some(entry) = entries.list_get(index) else {
            continue;
        };
        let name = entry.field(&id).as_str().unwrap_or_default();
        let result = (|| {
            let id = CommandId::parse(name)?;
            let event_id: gpui_kit::SharedString = id.as_str().to_owned().into();
            let state = state.clone();
            let handler = move |_: &crate::Payload, window: &mut Window, cx: &mut App| {
                if let Some(state) = state.upgrade()
                    && let Some(handler) = state.read(cx).invoke.clone()
                {
                    handler(event_id.clone().into(), window, cx);
                }
            };
            let mut command = Command::new(
                id,
                entry.field(&title).as_str().unwrap_or_default(),
                handler,
            )
            .keywords(entry.field(&keywords).as_str().unwrap_or_default())
            .category(entry.field(&category).as_str().unwrap_or_default())
            .enabled(entry.field(&enabled).as_bool().unwrap_or(true));
            if let Some(shortcut) = entry.field(&shortcut).as_str() {
                command = command.keybinding(Keybinding::parse(shortcut)?);
            }
            if let Some(when) = entry.field(&when).as_str() {
                command = command.when(When::parse(when)?);
            }
            commands.register(command, cx)
        })();
        if let Err(error) = result {
            errors.push(format!("Command {name} failed to register ({error})"));
        }
    }
    (commands, errors)
}

impl RenderOnce for DocumentMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let commands = if self.entries.list_len().is_some() {
            let state = window.use_keyed_state(self.key, cx, |_, cx| MenuState {
                entries: Value::Null,
                commands: Commands::new(cx),
                invoke: None,
            });
            state.update(cx, |state, cx| {
                state.invoke = self.invoke;
                if state.entries != self.entries {
                    state.entries = self.entries;
                    let (commands, errors) = register_entries(&state.entries, cx.weak_entity(), cx);
                    state.commands = commands;
                    if let Some(error) = self.error
                        && !errors.is_empty()
                    {
                        window.defer(cx, move |window, cx| {
                            for message in errors {
                                error(message.into(), window, cx);
                            }
                        });
                    }
                }
                Some(state.commands.clone())
            })
        } else {
            registry(cx).ok()
        };
        commands.map_or_else(
            || div().into_any_element(),
            |commands| {
                let menu = Menu::new(self.id, &commands)
                    .show_categories(self.categories)
                    .show_keybindings(self.keybinds);
                CommandScope::new(&commands).child(menu).into_any_element()
            },
        )
    }
}
