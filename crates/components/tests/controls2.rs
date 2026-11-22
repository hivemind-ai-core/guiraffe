//! API conventions §12 interaction tests for the components-2 controls: Burger, CopyButton, Spoiler,
//! Stepper, ColorSwatch and Loader. Pointer presses never take focus, Tab reaches them inside a
//! focus scope, Enter/Space activate, AccessKit sees role/label/state, and selection never moves
//! layout.
use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, Styled, TestAppContext, VisualTestContext, Window,
    component::input::{Input, InputState},
    div, px,
    test::{TestQueryExt, TestWindowExt},
};
use guiraffe_components::{
    Burger, ColorSwatch, CopyButton, Loader, Spoiler, Step, Stepper, SwatchSelected,
};
use guiraffe_core::{ColorName, focus_scope};
use guiraffe_test::layout::{assert_same_bounds, capture};

struct Host {
    input: Entity<InputState>,
    burger: Entity<Burger>,
    copy: Entity<CopyButton>,
    spoiler: Entity<Spoiler>,
    loader: Entity<Loader>,
    steps: Rc<Cell<u32>>,
    swatches: Rc<Cell<u32>>,
    ring: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (steps, swatches) = (self.steps.clone(), self.swatches.clone());
        let ring = self.ring;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().w(px(200.)).child(Input::new(&self.input)))
            .child(focus_scope(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(self.burger.clone())
                    .child(self.copy.clone())
                    .child(div().w(px(300.)).child(self.spoiler.clone()))
                    .child(
                        div().w(px(400.)).child(
                            Stepper::new([Step::new("One"), Step::new("Two"), Step::new("Three")])
                                .active(1)
                                .on_step_click(move |_, _, _, _| steps.set(steps.get() + 1)),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .child(
                                div().debug_selector(|| "swatch-a".into()).child(
                                    ColorSwatch::new("swatch-a", ColorName::Teal.shade(6))
                                        .label("Teal")
                                        .selected(if ring {
                                            SwatchSelected::Ring
                                        } else {
                                            SwatchSelected::None
                                        })
                                        .on_click(move |_, _, _| swatches.set(swatches.get() + 1)),
                                ),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "swatch-b".into())
                                    .child(ColorSwatch::new("swatch-b", ColorName::Red.shade(6))),
                            ),
                    )
                    .child(self.loader.clone()),
            ))
    }
}

fn setup(cx: &mut TestAppContext) -> (&mut VisualTestContext, Entity<Host>, FocusHandle) {
    cx.update(guiraffe_components::init);
    let (host, vcx) = cx.add_window_view(|window, cx| {
        let copy = CopyButton::new(cx);
        copy.update(cx, |b, cx| b.set_value("cargo add guiraffe", cx));
        let spoiler = Spoiler::new(cx);
        spoiler.update(cx, |s, cx| {
            s.set_content(|_, _| div().child("Body").into_any_element(), cx)
        });
        Host {
            input: cx.new(|cx| InputState::new(window, cx)),
            burger: Burger::new(cx),
            copy,
            spoiler,
            loader: Loader::new(cx),
            steps: Rc::default(),
            swatches: Rc::default(),
            ring: false,
        }
    });
    vcx.run_until_parked();
    let handle = vcx.update(|window, cx| {
        let h = host.read(cx).input.focus_handle(cx);
        window.focus(&h, cx);
        h
    });
    vcx.run_until_parked();
    (vcx, host, handle)
}

const BURGER: &str = "gr-burger:gr-burger";
const COPY: &str = "gr-copy-button:cargo add guiraffe";
const SPOILER: &str = "gr-spoiler-toggle:gr-spoiler";

#[gpui_kit::test]
fn pointer_presses_never_take_focus(cx: &mut TestAppContext) {
    let (vcx, host, input) = setup(cx);
    vcx.update(|window, cx| {
        for id in [BURGER, COPY, SPOILER, "gr-step:0", "swatch-a"] {
            window.click(id, cx);
            assert!(input.is_focused(window), "{id} click stole focus");
        }
    });
    let (burger, copy, spoiler, steps, swatches) = host.read_with(vcx, |h, cx| {
        (
            h.burger.read(cx).is_open(),
            h.copy.read(cx).copied(),
            h.spoiler.read(cx).is_expanded(),
            h.steps.get(),
            h.swatches.get(),
        )
    });
    assert!(burger && copy && spoiler);
    assert_eq!((steps, swatches), (1, 1));
}

#[gpui_kit::test]
fn tab_reaches_each_control_and_enter_activates_it(cx: &mut TestAppContext) {
    let (vcx, host, _) = setup(cx);
    vcx.update(|window, cx| {
        for id in [BURGER, COPY, SPOILER, "gr-step:0"] {
            window.focus_next(cx);
            window.render_frame(cx);
            assert_eq!(window.find(id).focused(), Some(true), "Tab reaches {id}");
            window.press("enter", cx);
        }
    });
    let (burger, copy, spoiler, steps) = host.read_with(vcx, |h, cx| {
        (
            h.burger.read(cx).is_open(),
            h.copy.read(cx).copied(),
            h.spoiler.read(cx).is_expanded(),
            h.steps.get(),
        )
    });
    assert!(burger, "Enter toggles the burger");
    assert!(copy, "Enter copies");
    assert!(spoiler, "Enter expands the spoiler");
    assert_eq!(steps, 1, "Enter clicks a step");
}

#[gpui_kit::test]
fn controls_outside_a_focus_scope_are_not_tab_stops(cx: &mut TestAppContext) {
    struct V(Entity<Burger>);
    impl Render for V {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(self.0.clone())
        }
    }
    cx.update(guiraffe_components::init);
    let (_, vcx) = cx.add_window_view(|_, cx| V(Burger::new(cx)));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.focus_next(cx);
        window.render_frame(cx);
        assert_ne!(window.find(BURGER).focused(), Some(true));
    });
}

#[gpui_kit::test]
fn roles_names_and_states_are_exposed(cx: &mut TestAppContext) {
    let (vcx, host, _) = setup(cx);
    vcx.update(|window, cx| {
        window.render_frame(cx);
        let burger = window.find_by_label("Open navigation");
        assert_eq!(burger.role(), Some(Role::Button));
        assert_eq!(burger.expanded(), Some(false));
        assert_eq!(window.find_by_label("Copy").role(), Some(Role::Button));
        let spoiler = window.find(SPOILER);
        assert_eq!(spoiler.role(), Some(Role::Button));
        assert_eq!(spoiler.label(), Some("Show more"));
        assert_eq!(spoiler.expanded(), Some(false));
        assert_eq!(window.find("gr-step:1").selected(), Some(true));
        assert_eq!(window.find("gr-step:0").selected(), Some(false));
        assert_eq!(window.find("swatch-a").label(), Some("Teal"));
        assert_eq!(
            window.find_by_label("Loading").role(),
            Some(Role::ProgressIndicator)
        );
        window.click(BURGER, cx);
        window.render_frame(cx);
        assert_eq!(
            window.find_by_label("Close navigation").expanded(),
            Some(true)
        );
    });
    vcx.update(|_, cx| {
        host.update(cx, |h, cx| {
            h.copy.update(cx, |b, cx| b.set_disabled(true, cx))
        })
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(COPY).disabled(), Some(true));
        window.click(COPY, cx);
    });
    assert!(
        !host.read_with(vcx, |h, cx| h.copy.read(cx).copied()),
        "a disabled CopyButton copies nothing"
    );
}

#[gpui_kit::test]
fn swatch_selection_never_moves_layout(cx: &mut TestAppContext) {
    let (vcx, host, _) = setup(cx);
    let sels = ["swatch-a", "swatch-b"];
    let base = capture(vcx, &sels);
    vcx.update(|_, cx| {
        host.update(cx, |h, cx| {
            h.ring = true;
            cx.notify();
        })
    });
    assert_same_bounds(vcx, &sels, &base, "selected with a ring");
}

#[gpui_kit::test]
fn focus_rings_never_move_layout(cx: &mut TestAppContext) {
    let (vcx, _, _) = setup(cx);
    let sels = [BURGER, COPY];
    let base = capture(vcx, &sels);
    vcx.update(|window, cx| {
        window.focus_next(cx);
        window.press("f13", cx);
    });
    assert!(
        vcx.debug_bounds("focus-ring").is_some(),
        "focus-visible ring drawn"
    );
    assert_same_bounds(vcx, &sels, &base, "focused via Tab");
}

#[gpui_kit::test]
fn avatar_group_more_replaces_the_count_and_names_everyone(cx: &mut TestAppContext) {
    use guiraffe_components::{AvatarGroup, Person};
    struct V;
    impl Render for V {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let people = || ["Ada Lovelace", "Grace Hopper", "Alan Turing"].map(Person::new);
            div()
                .child(AvatarGroup::new(people()).limit(2).id("plain"))
                .child(
                    AvatarGroup::new(people())
                        .limit(2)
                        .id("custom")
                        .more(div().debug_selector(|| "custom-more".into()).child("…")),
                )
        }
    }
    cx.update(guiraffe_components::init);
    let (_, vcx) = cx.add_window_view(|_, _| V);
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("custom-more").is_some());
    assert_eq!(
        vcx.debug_bounds("gr-avatar-more:custom"),
        None,
        "a custom chip replaces the +N text"
    );
    assert!(vcx.debug_bounds("gr-avatar-more:plain").is_some());
    vcx.update(|window, cx| {
        window.render_frame(cx);
        let g = window.find("plain");
        assert_eq!(g.label(), Some("Ada Lovelace, Grace Hopper and 1 more"));
    });
}
