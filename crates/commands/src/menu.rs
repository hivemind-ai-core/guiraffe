use crate::{
    Commands,
    service::{InvocationSource, InvokeBinding, MenuRow, ShowCommands},
};
use gpui_kit::{
    App, ElementId, IntoElement, ParentElement, RenderOnce, StyleRefinement, Styled, Window,
    component::{
        Disableable, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
    },
    div,
    prelude::*,
    px, uniform_list,
};
use guiraffe_core::{GuiraffeTheme, Size, StateKind, StyleContext, Variant};

/// Stateless command-routing boundary. All focused descendants reuse their actual
/// GPUI key contexts; only keyboard routing lives here, not the registry's data.
#[derive(IntoElement)]
pub struct CommandScope {
    commands: Commands,
    children: Vec<gpui_kit::AnyElement>,
    style: StyleRefinement,
}
impl CommandScope {
    pub fn new(commands: &Commands) -> Self {
        Self {
            commands: commands.clone(),
            children: Vec::new(),
            style: Default::default(),
        }
    }
}
impl ParentElement for CommandScope {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui_kit::AnyElement>) {
        self.children.extend(elements);
    }
}
impl Styled for CommandScope {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for CommandScope {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let commands = self.commands;
        let palette = commands.clone();
        let mut element = div()
            .key_context("GrCommand")
            .children(self.children)
            .on_action(move |action: &InvokeBinding, window, cx| {
                if !commands.dispatch_binding(&action.binding, window, cx) {
                    cx.propagate();
                }
            })
            .on_action(move |_: &ShowCommands, window, cx| {
                if palette.open_spotlight(window, cx).is_err() {
                    cx.propagate();
                }
            });
        element.style().refine(&self.style);
        element
    }
}
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(default)]
pub struct MenuProps {
    pub show_categories: bool,
    pub show_keybindings: bool,
}
impl Default for MenuProps {
    fn default() -> Self {
        Self {
            show_categories: true,
            show_keybindings: true,
        }
    }
}
/// A virtualized in-app command menu. Context visibility follows the service's
/// explicit host ContextStack, preserving the opening context during menu focus.
#[derive(IntoElement)]
pub struct Menu {
    id: ElementId,
    commands: Commands,
    props: MenuProps,
    style: StyleRefinement,
}
impl Menu {
    pub fn new(id: impl Into<ElementId>, commands: &Commands) -> Self {
        Self {
            id: id.into(),
            commands: commands.clone(),
            props: Default::default(),
            style: Default::default(),
        }
    }
    pub fn props(mut self, props: MenuProps) -> Self {
        self.props = props;
        self
    }
    pub fn show_categories(mut self, value: bool) -> Self {
        self.props.show_categories = value;
        self
    }
    pub fn show_keybindings(mut self, value: bool) -> Self {
        self.props.show_keybindings = value;
        self
    }
}
impl Styled for Menu {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for Menu {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.commands.entity().read(cx);
        let rows = state.rows(self.props.show_categories);
        let count = rows.len();
        let service = self.commands;
        let hints = self.props.show_keybindings;
        let context = StyleContext::current();
        let height = context.control_size.control_height();
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let mut root = div()
            .id(self.id.clone())
            .w_full()
            .rounded(px(theme.default_radius.radius()))
            .bg(surfaces.surface.to_hsla())
            .border_1()
            .border_color(surfaces.border.to_hsla());
        root = if count == 0 {
            root.child(
                div()
                    .p(px(context.density.space(Size::Xs)))
                    .text_color(surfaces.dimmed.to_hsla())
                    .child("No commands"),
            )
        } else {
            root.child(
                uniform_list((self.id, "items"), count, move |range, _, cx| {
                    let theme = GuiraffeTheme::global(cx);
                    let surfaces = theme.surfaces();
                    // Semantic state tokens (API conventions §12.1), resolved per scheme.
                    let state = |kind| theme.state(kind, Variant::Subtle, None);
                    let dimmed = state(StateKind::Dimmed).foreground.to_hsla();
                    let disabled = state(StateKind::Disabled).foreground.to_hsla();
                    let hover = state(StateKind::Hover).background.to_hsla();
                    let pressed = state(StateKind::Pressed).background.to_hsla();
                    // Opt-in tab stops (§12.3): only inside a focus scope.
                    let tab_stop = guiraffe_core::tab_stop(None);
                    range
                        .filter_map(|index| rows.get(index))
                        .map(|row| match row {
                            MenuRow::Heading(title) => div()
                                .h(px(height))
                                .px(px(context.density.space(Size::Xs)))
                                .flex()
                                .items_center()
                                .text_color(dimmed)
                                .text_size(px(Size::Xs.font_size()))
                                .child(title.clone())
                                .into_any_element(),
                            MenuRow::Item {
                                id,
                                element_id,
                                title,
                                hint,
                                enabled,
                            } => {
                                // Both clones are reference-count bumps: the row owns a shared
                                // id, and its element id was formatted when the row was built.
                                let id = id.clone();
                                let commands = service.clone();
                                let quiet = theme.variant(None, Variant::Subtle);
                                Button::new(element_id.clone())
                                    .w_full()
                                    .with_size(px(height))
                                    .rounded(px(theme.default_radius.radius()))
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(surfaces.surface.to_hsla())
                                            .hover(hover)
                                            .active(pressed)
                                            .foreground(if *enabled {
                                                surfaces.text.to_hsla()
                                            } else {
                                                disabled
                                            }),
                                    )
                                    .border_color(quiet.border.to_hsla())
                                    .disabled(!*enabled)
                                    .tab_stop(tab_stop && *enabled)
                                    .accessibility_label(title.clone())
                                    .child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(context.density.space(Size::Sm)))
                                            .child(title.clone())
                                            .when(hints, |el| {
                                                el.child(
                                                    div()
                                                        .text_color(dimmed)
                                                        .text_size(px(Size::Xs.font_size()))
                                                        .child(hint.clone()),
                                                )
                                            }),
                                    )
                                    .on_click(move |_, window, cx| {
                                        let contexts =
                                            commands.entity().read(cx).contexts().clone();
                                        let _ = commands.execute(
                                            &id,
                                            &contexts,
                                            InvocationSource::Menu,
                                            window,
                                            cx,
                                        );
                                    })
                                    .into_any_element()
                            }
                        })
                        .collect()
                })
                .h(px(height * count.clamp(1, 12) as f32))
                .w_full(),
            )
        };
        root.style().refine(&self.style);
        root
    }
}
