use crate::{Children, Navigation, NodeId, TreeModel, TreeSource};
use gpui_kit::base::Selectable;
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Render, Role,
    ScrollEvent, StatefulInteractiveElement, Styled, Window,
    component::{
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
    },
    div, px,
};
use guiraffe_core::{
    GuiraffeTheme, Variant,
    control::{focus_ring, ring_visible},
};
use std::{rc::Rc, time::Instant};

const ROW: f64 = 36.;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
    Enum(String),
    Colour(u32),
}

impl Value {
    fn text(&self) -> String {
        match self {
            Self::Text(s) | Self::Enum(s) => s.clone(),
            Self::Number(n) => n.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Colour(rgb) => format!("#{rgb:06x}"),
        }
    }

    fn parse(&self, text: &str) -> Option<Self> {
        match self {
            Self::Text(_) => Some(Self::Text(text.to_owned())),
            Self::Number(_) => text
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(Self::Number),
            Self::Colour(_) => {
                let digits = text.strip_prefix('#')?;
                (digits.len() == 6)
                    .then(|| u32::from_str_radix(digits, 16).ok())
                    .flatten()
                    .map(Self::Colour)
            }
            _ => None,
        }
    }
}

pub struct Property<'a> {
    pub value: &'a Value,
    pub default: &'a Value,
    pub choices: &'a [String],
    pub read_only: bool,
}

pub trait TreeData: TreeSource {
    fn icon(&self, _: NodeId) -> Option<&str> {
        None
    }
    fn badge(&self, _: NodeId) -> Option<&str> {
        None
    }
    fn checked(&self, _: NodeId) -> Option<bool> {
        None
    }
    fn property(&self, _: NodeId) -> Option<Property<'_>> {
        None
    }
    fn validate(&self, _: NodeId, _: &Value) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TreeEvent {
    LoadChildren(NodeId),
    Selected(NodeId),
    Checked(NodeId, bool),
    Reorder { node: NodeId, before: NodeId },
    Edited(NodeId, Value),
}

struct Edit {
    id: NodeId,
    input: Entity<InputState>,
    _subscription: gpui_kit::Subscription,
}

pub struct TreeView<const PROPERTIES: bool> {
    source: Rc<dyn TreeData>,
    model: TreeModel,
    focus: FocusHandle,
    tab_stop: bool,
    scroll: f64,
    height: f32,
    drag: Option<(NodeId, gpui_kit::Point<gpui_kit::Pixels>)>,
    edit: Option<Edit>,
    error: Option<String>,
}

pub type TreeList = TreeView<false>;
pub type PropertyGrid = TreeView<true>;

impl<const P: bool> EventEmitter<TreeEvent> for TreeView<P> {}

impl<const P: bool> TreeView<P> {
    pub fn new(source: Rc<dyn TreeData>, cx: &mut Context<Self>) -> Self {
        Self {
            model: TreeModel::new(source.as_ref()),
            source,
            focus: cx.focus_handle(),
            tab_stop: false,
            height: 288.,
            scroll: 0.,
            drag: None,
            edit: None,
            error: None,
        }
    }

    pub fn focusable(mut self, enabled: bool) -> Self {
        self.tab_stop = enabled;
        self
    }

    pub fn replace_source(&mut self, source: Rc<dyn TreeData>, cx: &mut Context<Self>) {
        self.source = source;
        self.model.refresh(self.source.as_ref());
        self.clamp_scroll();
        cx.notify();
    }

    pub fn set_expanded(&mut self, id: NodeId, open: bool, cx: &mut Context<Self>) {
        if let Some(id) = self.model.set_expanded(id, open, self.source.as_ref()) {
            cx.emit(TreeEvent::LoadChildren(id));
        }
        self.clamp_scroll();
        cx.notify();
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.clamp(
            0.,
            (self.model.rows().len() as f64 * ROW - f64::from(self.height)).max(0.),
        );
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self.edit.is_some() {
            if key == "escape" {
                self.edit = None;
                self.error = None;
                if self.tab_stop {
                    window.focus(&self.focus, cx);
                }
                cx.notify();
                cx.stop_propagation();
            }
            return;
        }
        let direction = match key {
            "up" => Some(Navigation::Up),
            "down" => Some(Navigation::Down),
            "left" => Some(Navigation::Left),
            "right" => Some(Navigation::Right),
            "home" => Some(Navigation::Home),
            "end" => Some(Navigation::End),
            _ => None,
        };
        if let Some(direction) = direction {
            if let Some(id) = self.model.navigate(direction, self.source.as_ref()) {
                cx.emit(TreeEvent::LoadChildren(id));
            }
        } else if key == "enter"
            && P
            && let Some(id) = self.model.cursor()
            && let Some(property) = self.source.property(id).filter(|p| !p.read_only)
        {
            match property.value {
                Value::Bool(value) => self.edit_value(id, Value::Bool(!value), cx),
                Value::Enum(_) => return, // The individual choices are tab stops.
                _ => self.start_edit(id, window, cx),
            }
        } else if matches!(key, "space" | "enter") {
            if let Some(id) = self.model.cursor() {
                self.model.select(id, event.keystroke.modifiers.shift);
                cx.emit(TreeEvent::Selected(id));
            }
        } else if !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.platform
            && let Some(text) = &event.keystroke.key_char
            && let Some(ch) = text.chars().next()
        {
            self.model
                .type_ahead(ch, Instant::now(), self.source.as_ref());
        } else {
            return;
        }

        if let Some(index) = self.model.cursor().and_then(|id| self.model.row_index(id)) {
            let top = index as f64 * ROW;
            self.scroll = self
                .scroll
                .clamp((top + ROW - f64::from(self.height)).max(0.), top);
        }
        self.clamp_scroll();
        cx.notify();
        cx.stop_propagation();
    }

    fn edit_value(&mut self, id: NodeId, value: Value, cx: &mut Context<Self>) {
        if self
            .source
            .property(id)
            .is_none_or(|property| property.read_only)
        {
            return;
        }
        match self.source.validate(id, &value) {
            Ok(()) => {
                self.error = None;
                self.edit = None;
                cx.emit(TreeEvent::Edited(id, value));
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn start_edit(&mut self, id: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(property) = self
            .source
            .property(id)
            .filter(|property| !property.read_only)
        else {
            return;
        };
        let text = property.value.text();
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |input, cx| input.set_value(text, window, cx));
        let subscription = cx.subscribe_in(
            &input,
            window,
            move |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let text = input.read(cx).value();
                    if let Some(value) = this
                        .source
                        .property(id)
                        .and_then(|property| property.value.parse(&text))
                    {
                        this.edit_value(id, value, cx);
                        if this.edit.is_none() && this.tab_stop {
                            window.focus(&this.focus, cx);
                        }
                    } else {
                        this.error = Some("Invalid value".to_owned());
                        cx.notify();
                    }
                }
            },
        );
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.error = None;
        self.edit = Some(Edit {
            id,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn property_cell(&self, id: NodeId, cx: &Context<Self>) -> Option<AnyElement> {
        let property = self.source.property(id)?;
        let value = match property.value {
            _ if property.read_only => div().child(property.value.text()).into_any_element(),
            Value::Bool(value) => Checkbox::new(("property-bool", id as usize))
                .tab_stop(self.tab_stop)
                .checked(*value)
                .on_change(cx.listener(move |this, value: &bool, _, cx| {
                    this.edit_value(id, Value::Bool(*value), cx)
                }))
                .into_any_element(),
            Value::Enum(value) => div()
                .flex()
                .gap(px(2.))
                .children(property.choices.iter().map(|choice| {
                    let selected = choice == value;
                    let choice = choice.clone();
                    Button::new(choice.clone())
                        .tab_stop(self.tab_stop)
                        .label(choice.clone())
                        .selected(selected)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_value(id, Value::Enum(choice.clone()), cx)
                        }))
                }))
                .into_any_element(),
            _ => match self.edit.as_ref().filter(|edit| edit.id == id) {
                Some(edit) => Input::new(&edit.input).into_any_element(),
                None => Button::new(("property-value", id as usize))
                    .tab_stop(self.tab_stop)
                    .label(property.value.text())
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.start_edit(id, window, cx)),
                    )
                    .into_any_element(),
            },
        };
        let default = property.default.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .w(px(240.))
                .child(value)
                .children(
                    (!property.read_only && property.value != property.default).then(|| {
                        Button::new(("property-reset", id as usize))
                            .tab_stop(self.tab_stop)
                            .label("Reset")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_value(id, default.clone(), cx);
                            }))
                    }),
                )
                .into_any_element(),
        )
    }
}

impl<const P: bool> Render for TreeView<P> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let colours = theme.press_colors(Variant::Light, None);
        let range = self.model.window(self.scroll, f64::from(self.height), ROW);
        let offset = range.start as f64 * ROW - self.scroll;
        let rows = self.model.rows()[range].iter().copied().map(|row| {
            let id = row.id;
            let open = self.model.expanded(id);
            let branch = self.source.children(id) != Children::Ready(&[]);
            let selected = self.model.selected(id);
            let active = self.model.cursor() == Some(id);
            let label = self.source.label(id).to_owned();
            let select = cx.listener(move |this, event: &gpui_kit::ClickEvent, _, cx| {
                this.model
                    .select(id, event.modifiers().control || event.modifiers().platform);
                cx.emit(TreeEvent::Selected(id));
                cx.notify();
            });
            let mut element = div()
                .id(("tree-row", id as usize))
                .role(Role::TreeItem)
                .aria_label(label.clone())
                .aria_selected(selected)
                .h(px(ROW as f32))
                .w_full()
                .flex()
                .items_center()
                .gap(px(4.))
                .pl(px((row.depth.min(45) * 16) as f32))
                .hover(|style| style.bg(colours.hover.background.to_hsla()));
            if branch {
                element = element.aria_expanded(open);
            }
            if selected || active {
                element = element
                    .bg(colours.selected.background.to_hsla())
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD);
            }
            if !P {
                element = element
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, _, _| {
                            this.drag = Some((id, event.position));
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui_kit::MouseUpEvent, _, cx| {
                            if let Some((node, start)) = this.drag.take()
                                && node != id
                                && f32::from((event.position.y - start.y).abs()) >= 4.
                                && this
                                    .model
                                    .row_index(node)
                                    .zip(this.model.row_index(id))
                                    .is_some_and(|(a, b)| {
                                        this.model.rows()[a].parent == this.model.rows()[b].parent
                                    })
                            {
                                cx.emit(TreeEvent::Reorder { node, before: id });
                            }
                        }),
                    );
            }
            element
                .child(if branch {
                    Button::new(("tree-expand", id as usize))
                        .tab_stop(false)
                        .label(if open { "▾" } else { "▸" })
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.set_expanded(id, !open, cx)),
                        )
                        .into_any_element()
                } else {
                    div().w(px(28.)).into_any_element()
                })
                .children(
                    (!P).then(|| self.source.icon(id))
                        .flatten()
                        .map(str::to_owned),
                )
                .child(
                    div().flex_1().overflow_hidden().child(
                        Button::new(("tree-select", id as usize))
                            .tab_stop(false)
                            .ghost()
                            .justify_start()
                            .accessibility_label(label.clone())
                            .child(div().w_full().text_left().child(label))
                            .on_click(select),
                    ),
                )
                .children(
                    (!P).then(|| self.source.badge(id))
                        .flatten()
                        .map(str::to_owned),
                )
                .children(
                    (!P).then(|| self.source.checked(id))
                        .flatten()
                        .map(|checked| {
                            Checkbox::new(("tree-check", id as usize))
                                .tab_stop(false)
                                .checked(checked)
                                .on_change(cx.listener(move |_, value: &bool, _, cx| {
                                    cx.emit(TreeEvent::Checked(id, *value))
                                }))
                        }),
                )
                .children(if P { self.property_cell(id, cx) } else { None })
        });
        let mut tree = div()
            .id("virtual-tree")
            .role(Role::Tree)
            .aria_label(if P { "Properties" } else { "Tree" })
            .relative()
            .overflow_hidden()
            .h(px(self.height))
            .w_full()
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                this.scroll -= f64::from(f32::from(event.delta.pixel_delta(px(24.)).y));
                this.clamp_scroll();
                cx.notify();
                cx.stop_propagation();
            }))
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)),
            )
            .child(
                div()
                    .absolute()
                    .w_full()
                    .top(px(offset as f32))
                    .children(rows),
            );
        if self.tab_stop {
            tree = tree.track_focus(&self.focus).tab_stop(true);
        }
        div()
            .w_full()
            .child(focus_ring(
                tree,
                self.tab_stop && ring_visible(&self.focus, window),
                px(0.),
                colours.ring,
            ))
            .children(self.error.clone().map(|error| div().child(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        Modifiers, ScrollDelta, ScrollSource, StyleRefinement, TestAppContext, TouchPhase, point,
    };
    use std::cell::{Cell, RefCell};

    struct Data(Vec<NodeId>, Value, Value);
    impl TreeSource for Data {
        fn roots(&self) -> &[NodeId] {
            &[0]
        }
        fn children(&self, id: NodeId) -> Children<'_> {
            Children::Ready(if id == 0 { &self.0 } else { &[] })
        }
        fn label(&self, id: NodeId) -> &str {
            if id == 0 { "Settings" } else { "Property" }
        }
    }
    impl TreeData for Data {
        fn property(&self, id: NodeId) -> Option<Property<'_>> {
            (id > 0).then_some(Property {
                value: &self.1,
                default: &self.2,
                choices: &[],
                read_only: id == 2,
            })
        }
        fn validate(&self, _: NodeId, value: &Value) -> Result<(), String> {
            if matches!(value, Value::Number(n) if *n < 0.) {
                Err("Out of range".into())
            } else {
                Ok(())
            }
        }
    }

    struct Host(Entity<PropertyGrid>, Rc<Cell<usize>>);
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.1.set(self.1.get() + 1);
            div().size_full().child(
                self.0
                    .clone()
                    .cached(StyleRefinement::default().w_full().h(px(320.))),
            )
        }
    }

    struct Container(Entity<Host>);
    impl Render for Container {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                self.0
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
        }
    }

    #[gpui_kit::test]
    fn keyboard_expand_scroll_and_validation(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::base::init(cx);
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
        });
        let renders = Rc::new(Cell::new(0));
        let events = Rc::new(RefCell::new(Vec::new()));
        let view = cx.new(|cx| {
            PropertyGrid::new(
                Rc::new(Data(
                    (1..50).collect(),
                    Value::Number(10.),
                    Value::Number(5.),
                )),
                cx,
            )
            .focusable(true)
        });
        let (_, window) = cx.add_window_view(|window, cx| {
            let events = events.clone();
            cx.subscribe(&view, move |_, _, event: &TreeEvent, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach();
            let host = cx.new(|_| Host(view.clone(), renders.clone()));
            let container = cx.new(|_| Container(host));
            gpui_kit::base::Root::new(container, window, cx)
        });
        window.run_until_parked();
        window.update(|window, cx| {
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            window.draw(cx).clear(cx);
        });
        window.run_until_parked();
        window.simulate_keystrokes("home");
        window.run_until_parked();
        let initial = renders.get();
        window.simulate_keystrokes("right down space end");
        window.run_until_parked();
        assert_eq!(
            view.read_with(window, |view, _| view.model.rows().len()),
            50
        );
        assert_eq!(
            view.read_with(window, |view, _| view.model.cursor()),
            Some(49)
        );
        assert!(events.borrow().contains(&TreeEvent::Selected(1)));
        if gpui_kit::GPUI_FAST {
            assert_eq!(
                renders.get(),
                initial,
                "expand and navigation invalidated host"
            );
        }
        let before = view.read_with(window, |view, _| view.scroll);
        let position = point(px(15.), px(120.));
        window.simulate_mouse_move(position, None, Modifiers::default());
        window.run_until_parked();
        let initial = renders.get();
        window.simulate_event(ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(100.))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.scroll) < before);
        if gpui_kit::GPUI_FAST {
            assert_eq!(renders.get(), initial, "scroll invalidated host");
        }
        window.simulate_keystrokes("home down enter");
        window.run_until_parked();
        let input = view
            .read_with(window, |view, _| {
                view.edit.as_ref().map(|edit| edit.input.clone())
            })
            .expect("Enter opens the number editor");
        window.update(|window, cx| input.update(cx, |input, cx| input.set_value("-1", window, cx)));
        window.simulate_keystrokes("enter");
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.error.is_some()));
        window.update(|window, cx| input.update(cx, |input, cx| input.set_value("5", window, cx)));
        window.simulate_keystrokes("enter");
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.error.is_none()));
        window.simulate_keystrokes("down enter");
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.edit.is_none()));
        assert!(
            events
                .borrow()
                .contains(&TreeEvent::Edited(1, Value::Number(5.)))
        );
        assert!(!events.borrow().iter().any(|e| matches!(
            e,
            TreeEvent::Edited(2, _) | TreeEvent::Edited(1, Value::Number(-1.))
        )));
    }
}
