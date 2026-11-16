//! Gallery pages. Each page is a small view entity.

use std::time::Duration;

use gpui_kit::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Window, assets::IconName, div, prelude::*, px,
};
use guiraffe_components::*;
use guiraffe_core::{ColorName, GuiraffeTheme, Size, Variant};
use guiraffe_motion::{
    Animated, Presence, Spring, Timing, TransitionKind, framed, sample_presence, set_presence,
};
use guiraffe_state::{RedoEvent, Selected, Store, UndoEvent};

use crate::{
    Page,
    todos::{Add, ClearDone, Remove, Todo, TodoState, Toggle},
};

pub fn build(page: Page, window: &mut Window, cx: &mut App) -> AnyView {
    match page {
        Page::Overview => cx.new(|_| Overview).into(),
        Page::Components => cx.new(ComponentsPage::new).into(),
        Page::State => cx.new(StatePage::new).into(),
        Page::Motion => cx.new(|_| MotionPage::new()).into(),
        Page::Flow => FlowPage::build(window, cx),
        Page::Overlays => cx.new(|_| OverlaysPage { toasts: 0 }).into(),
        Page::Windows => cx.new(|_| WindowsPage).into(),
    }
}

fn page(title: &'static str, blurb: &'static str) -> gpui_kit::Div {
    div()
        .p(px(Size::Xl.spacing()))
        .flex()
        .flex_col()
        .gap(px(Size::Md.spacing()))
        .child(Title::new(title).order(2))
        .child(Text::new(blurb).dimmed(true))
}

use gpui_kit::component::Disableable as _;

fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
) -> gpui_kit::component::button::Button {
    use gpui_kit::component::button::Button;
    Button::new(gpui_kit::ElementId::Name(id.into())).label(label)
}

// ---------------------------------------------------------------- Overview

pub struct Overview;

impl Render for Overview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stat = |icon: IconName,
                    color: ColorName,
                    label: &'static str,
                    value: &'static str,
                    pct: f32| {
            Card::new().child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(Text::new(label).size(Size::Xs).dimmed(true))
                            .child(Title::new(value).order(3)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                RingProgress::new()
                                    .diameter(48.)
                                    .thickness(6.)
                                    .section(pct, color),
                            )
                            .child(ThemeIcon::new(icon).color(color).variant(Variant::Light)),
                    ),
            )
        };
        let _ = cx;
        page("Welcome to GUIraffe", "A Mantine-inspired toolkit for fast, fluid desktop apps on GPUI.")
            .child(
                div()
                    .flex()
                    .gap(px(Size::Md.spacing()))
                    .child(div().flex_1().child(stat(IconName::Boxes, ColorName::Blue, "Crates", "14", 70.0)))
                    .child(div().flex_1().child(stat(IconName::FlaskConical, ColorName::Teal, "Tests", "300+", 85.0)))
                    .child(div().flex_1().child(stat(IconName::Gauge, ColorName::Grape, "Re-renders saved", "12 → 0", 100.0))),
            )
            .child(
                Blockquote::new()
                    .color(ColorName::Violet)
                    .cite("DESIGN.md")
                    .child("Building a GUIraffe app should feel as convenient as building a React app — with native speed and a fraction of the memory."),
            )
    }
}

// ---------------------------------------------------------------- Components

pub struct ComponentsPage {
    segmented: Entity<SegmentedControl>,
}

impl ComponentsPage {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            segmented: SegmentedControl::new(["Preview", "Code", "Export"], cx),
        }
    }
}

impl Render for ComponentsPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let variants = [
            Variant::Filled,
            Variant::Light,
            Variant::Outline,
            Variant::Subtle,
            Variant::Default,
        ];
        page(
            "Components",
            "Theme-driven components; every color, size and radius comes from tokens.",
        )
        .child(Title::new("SegmentedControl").order(4))
        .child(div().w(px(360.)).child(self.segmented.clone()))
        .child(Title::new("ThemeIcon & ActionIcon variants").order(4))
        .child(
            div()
                .flex()
                .gap(px(8.))
                .children(variants.iter().enumerate().map(|(i, v)| {
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(
                            ThemeIcon::new(IconName::Sparkles)
                                .variant(*v)
                                .color(ColorName::Indigo),
                        )
                        .child(
                            ActionIcon::new(("act", i), IconName::Heart)
                                .variant(*v)
                                .color(ColorName::Pink),
                        )
                })),
        )
        .child(Title::new("Indicator, Pill, Highlight").order(4))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(16.))
                .child(
                    Indicator::new(ThemeIcon::new(IconName::Mail).variant(Variant::Light))
                        .label("12")
                        .color(ColorName::Red),
                )
                .child(Pill::new("p1", "Rust").on_remove(|_, _, _| {}))
                .child(Pill::new("p2", "GPUI"))
                .child(Highlight::new("Fast, fluid and beautiful").term("fluid")),
        )
        .child(Title::new("RingProgress").order(4))
        .child(
            div().flex().gap(px(16.)).child(
                RingProgress::new()
                    .diameter(90.)
                    .section(30., ColorName::Cyan)
                    .section(25., ColorName::Pink)
                    .section(15., ColorName::Orange)
                    .label("70%"),
            ),
        )
    }
}

// ---------------------------------------------------------------- State

pub struct StatePage {
    store: Store<TodoState>,
    todos: Selected<std::sync::Arc<Vec<Todo>>>,
    remaining: Selected<usize>,
    counter: u32,
}

impl StatePage {
    fn new(cx: &mut Context<Self>) -> Self {
        let store = Store::new(TodoState::default(), cx);
        store.enable_undo_lens(100, |s: &TodoState| s.todos.clone(), |s, t| s.todos = t, cx);
        for t in ["Design the API", "Write the tests", "Ship the gallery"] {
            store.dispatch(Add(t.into()), cx);
        }
        store.flush(cx);
        let todos = store.select(cx, |s| s.todos.clone());
        let remaining = store.select(cx, TodoState::remaining);
        Self {
            store,
            todos,
            remaining,
            counter: 0,
        }
    }
}

impl Render for StatePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.clone();
        let s = GuiraffeTheme::global(cx).surfaces();
        let todos = self.todos.get(cx).clone();
        let remaining = *self.remaining.get(cx);
        let (can_undo, can_redo) = (store.can_undo(cx), store.can_redo(cx));
        page("State", "Event-driven state (re-frame style): pure handlers, batched dispatch, lens-scoped undo, change-only selections.")
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("add", "Add task").on_click({
                        let store = store.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.counter += 1;
                            store.dispatch(Add(format!("New task #{}", this.counter)), cx);
                        })
                    }))
                    .child(button("clear", "Clear done").on_click({
                        let store = store.clone();
                        move |_, _, cx| store.dispatch(ClearDone, cx)
                    }))
                    .child(button("undo", "Undo").disabled(!can_undo).on_click({
                        let store = store.clone();
                        move |_, _, cx| store.dispatch(UndoEvent, cx)
                    }))
                    .child(button("redo", "Redo").disabled(!can_redo).on_click({
                        let store = store.clone();
                        move |_, _, cx| store.dispatch(RedoEvent, cx)
                    })),
            )
            .child(Text::new(format!("{remaining} remaining")).size(Size::Sm).dimmed(true))
            .child(Paper::new().with_border(true).p(Size::Xs).child(div().flex().flex_col().children(todos.iter().map(|t| {
                let (id, done) = (t.id, t.done);
                let toggle = store.clone();
                let remove = store.clone();
                div()
                    .id(("todo", id))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .h(px(36.))
                    .rounded(px(4.))
                    .hover(|st| st.bg(s.hover.to_hsla()))
                    .child(
                        div()
                            .id(("check", id))
                            .cursor_pointer()
                            .child(ThemeIcon::new(if done { IconName::CircleCheck } else { IconName::Circle }).size(Size::Xs).variant(if done { Variant::Filled } else { Variant::Subtle }).color(ColorName::Teal))
                            .on_click(move |_, _, cx| toggle.dispatch(Toggle(id), cx)),
                    )
                    .child(div().flex_1().when(done, |el| el.line_through().text_color(s.dimmed.to_hsla())).child(t.title.clone()))
                    .child(close_button(("rm", id)).on_click(move |_, _, cx| remove.dispatch(Remove(id), cx)))
            }))))
    }
}

// ---------------------------------------------------------------- Motion

pub struct MotionPage {
    shown: bool,
    presence: Presence,
    ball: Animated<f32>,
    right: bool,
}

impl MotionPage {
    fn new() -> Self {
        Self {
            shown: true,
            presence: Presence::new(
                true,
                Timing::Spring(Spring::SNAPPY),
                Timing::tween(Duration::from_millis(160), guiraffe_motion::Easing::EaseIn),
            ),
            ball: Animated::new(0.0, Timing::Spring(Spring::BOUNCY)),
            right: false,
        }
    }
}

impl Render for MotionPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let vis = sample_presence(&mut self.presence, window, cx);
        let x = self.ball.sample(window, cx);
        let primary = GuiraffeTheme::global(cx).primary_color().to_hsla();
        page("Motion", "Springs and transitions; frames are requested only while something moves, and only this page re-renders.")
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(button("toggle", if self.shown { "Hide card" } else { "Show card" }).on_click(cx.listener(|this, _, _, cx| {
                        this.shown = !this.shown;
                        if set_presence(&mut this.presence, this.shown, cx) {
                            cx.notify();
                        }
                    })))
                    .child(button("ball", "Bounce").on_click(cx.listener(|this, _, _, cx| {
                        this.right = !this.right;
                        if this.ball.set(if this.right { 320.0 } else { 0.0 }, cx) {
                            cx.notify();
                        }
                    }))),
            )
            .child(div().h(px(120.)).when_some(vis, |el, v| {
                el.child(framed(Card::new().child(Title::new("Hello, presence").order(4)).child(Text::new("Fades up on enter, eases out on exit.").dimmed(true)), TransitionKind::FadeUp.frame(v, 0.0)))
            }))
            .child(div().relative().h(px(48.)).w(px(368.)).child(div().absolute().left(px(x)).top(px(4.)).size(px(40.)).rounded_full().bg(primary)))
    }
}

// ---------------------------------------------------------------- Flow

pub struct FlowPage;

impl FlowPage {
    fn build(_: &mut Window, cx: &mut App) -> AnyView {
        use guiraffe_flow::core::{Edge, Graph, Node, Point};
        let graph = Graph::from_parts(
            [
                Node::new(1, "Prompt", Point::new(60., 120.)),
                Node::new(2, "Retrieve docs", Point::new(320., 40.)),
                Node::new(3, "Model", Point::new(320., 220.)),
                Node::new(4, "Answer", Point::new(600., 130.)),
            ],
            [
                Edge {
                    label: "query".into(),
                    animated: true,
                    ..Edge::new(1, 1, 2)
                },
                Edge::new(2, 1, 3),
                Edge::new(3, 2, 3),
                Edge {
                    label: "tokens".into(),
                    animated: true,
                    ..Edge::new(4, 3, 4)
                },
            ],
        );
        let graph = match graph {
            Ok(g) => g,
            Err(e) => {
                return cx
                    .new(|_| FlowHost {
                        flow: None,
                        error: Some(e.to_string().into()),
                    })
                    .into();
            }
        };
        let error = std::rc::Rc::new(std::cell::RefCell::new(None::<SharedString>));
        let err = error.clone();
        let flow = cx.new(move |cx| match guiraffe_flow::Flow::new(graph, cx) {
            Ok(flow) => flow,
            Err(e) => {
                *err.borrow_mut() = Some(e.to_string().into());
                // An empty graph is always valid; the error is shown on the page instead.
                match guiraffe_flow::Flow::new(Graph::default(), cx) {
                    Ok(flow) => flow,
                    Err(_) => unreachable!("empty graph is valid"),
                }
            }
        });
        let error = error.borrow_mut().take();
        cx.new(|_| FlowHost {
            flow: Some(flow),
            error,
        })
        .into()
    }
}

pub struct FlowHost {
    flow: Option<Entity<guiraffe_flow::Flow>>,
    error: Option<SharedString>,
}

impl Render for FlowHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        page("Flow", "A react-flow style graph editor: drag nodes, connect handles, box-select, zoom and pan.")
            .when_some(self.error.clone(), |el, e| el.child(Text::new(e).color(ColorName::Red)))
            .when_some(self.flow.clone(), |el, f| el.child(Paper::new().with_border(true).p(Size::Xs).h(px(420.)).child(f)))
    }
}

// ---------------------------------------------------------------- Overlays

pub struct OverlaysPage {
    toasts: u64,
}

impl Render for OverlaysPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        page(
            "Overlays",
            "Toasts, stacked confirmations and more, managed per window.",
        )
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(button("toast", "Show toast").on_click(cx.listener(
                    |this, _, window, cx| {
                        this.toasts += 1;
                        let toast = guiraffe_overlays::toast::Toast::new(
                            guiraffe_overlays::toast::ToastId(this.toasts),
                            format!("Saved draft #{}", this.toasts),
                        );
                        let _ = guiraffe_overlays::OverlayHost::notify(window, cx, toast);
                    },
                )))
                .child(button("confirm", "Delete project…").on_click(cx.listener(
                    |_, _, window, cx| {
                        if let Ok((_, result)) = guiraffe_overlays::OverlayHost::confirm(
                            window,
                            cx,
                            "Delete project?",
                            "This cannot be undone.",
                        ) {
                            cx.spawn_in(window, async move |_, cx| {
                                let ok = result.await;
                                cx.update(|window, cx| {
                                    let msg = if ok {
                                        "Project deleted"
                                    } else {
                                        "Kept your project"
                                    };
                                    let _ = guiraffe_overlays::OverlayHost::notify(
                                        window,
                                        cx,
                                        guiraffe_overlays::toast::Toast::new(
                                            guiraffe_overlays::toast::ToastId(10_000),
                                            msg,
                                        ),
                                    );
                                })
                                .ok();
                            })
                            .detach();
                        }
                    },
                ))),
        )
    }
}

// ---------------------------------------------------------------- Windows

pub struct WindowsPage;

impl Render for WindowsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = guiraffe_windows::WindowManager::windows(cx).len();
        page("Windows", "Multi-window with keyed reuse, close guards, a typed cross-window bus and layout persistence.")
            .child(Text::new(format!("{count} gallery window(s) open")))
            .child(button("open", "Open another window").on_click(|_, _, cx| {
                let _ = crate::shell::open(Page::Components, cx);
            }))
    }
}
