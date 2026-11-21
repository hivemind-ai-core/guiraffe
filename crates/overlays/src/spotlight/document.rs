//! DataUi adapter for an embedded command picker.

use gpui_kit::{App, AppContext, ElementId, Entity, IntoElement, RenderOnce, Window};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use super::{Command, Spotlight};
use crate::widget_specs;

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[3],
            schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[3]),
            render: |node, _, _, _| {
                SpotlightDocument {
                    key: node.state_id(),
                    id: node.element_id(),
                    commands: node.prop(0).clone(),
                    on_select: node.handler(0),
                }
                .into_any_element()
            },
        }],
    );
}

fn parse_commands(value: &Value) -> Vec<Command> {
    let id = ["id".into()];
    let label = ["label".into()];
    let keywords = ["keywords".into()];
    let disabled = ["disabled".into()];
    (0..value.list_len().unwrap_or_default())
        .filter_map(|index| {
            let item = value.list_get(index)?;
            let mut command = Command::new(item.field(&id).as_str()?, item.field(&label).as_str()?);
            command.keywords = item
                .field(&keywords)
                .as_str()
                .unwrap_or_default()
                .to_owned();
            command.disabled = item.field(&disabled).as_bool().unwrap_or_default();
            Some(command)
        })
        .collect()
}

#[derive(IntoElement)]
struct SpotlightDocument {
    key: ElementId,
    id: ElementId,
    commands: Value,
    on_select: Option<EventHandler>,
}

struct SpotlightState {
    commands: Value,
    on_select: Option<EventHandler>,
    view: Entity<Spotlight>,
}

impl RenderOnce for SpotlightDocument {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let state: gpui_kit::WeakEntity<SpotlightState> = cx.weak_entity();
            let view = cx.new(|cx| {
                let selected = move |id: &str, window: &mut Window, cx: &mut App| {
                    if let Some(state) = state.upgrade()
                        && let Some(handler) = state.read(cx).on_select.clone()
                    {
                        handler(id.into(), window, cx);
                    }
                };
                let mut view = Spotlight::new(parse_commands(&self.commands), window, cx, selected);
                view.inline_id = Some(self.id.clone());
                view
            });
            SpotlightState {
                commands: self.commands.clone(),
                on_select: None,
                view,
            }
        });
        state.update(cx, |state, cx| {
            state.on_select = self.on_select;
            let commands = (state.commands != self.commands).then(|| {
                state.commands = self.commands;
                parse_commands(&state.commands)
            });
            state.view.update(cx, |view, cx| {
                if let Some(commands) = commands {
                    view.set_commands(commands, cx);
                }
            });
            state.view.clone()
        })
    }
}
