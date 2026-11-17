//! §12.3: a pointer click on a button keeps focus in a focused text input, and Tab inside a
//! focus scope reaches the button while one outside the scope is skipped.
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    Modifiers, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    component::button::Button,
    component::input::{Input, InputState},
    div, px,
};
use guiraffe_components::ScopedButton;
use guiraffe_core::focus_scope;

struct Form {
    input: Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().w(px(200.)).child(Input::new(&self.input)))
            .child(
                div()
                    .debug_selector(|| "outside".into())
                    .child(ScopedButton::new(Button::new("outside").label("Outside"))),
            )
            .child(focus_scope(div().debug_selector(|| "inside".into()).child(
                ScopedButton::new(Button::new("inside").label("Inside")),
            )))
    }
}

fn setup(cx: &mut TestAppContext) -> (&mut VisualTestContext, FocusHandle) {
    cx.update(guiraffe_core::init);
    let (form, vcx) = cx.add_window_view(|window, cx| Form {
        input: cx.new(|cx| InputState::new(window, cx)),
    });
    vcx.run_until_parked();
    let handle = vcx.update(|window, cx| {
        let h = form.read(cx).input.focus_handle(cx);
        window.focus(&h, cx);
        h
    });
    vcx.run_until_parked();
    (vcx, handle)
}

#[gpui_kit::test]
fn click_on_button_keeps_input_focus(cx: &mut TestAppContext) {
    let (vcx, input) = setup(cx);
    for sel in ["outside", "inside"] {
        let b = vcx.debug_bounds(sel).expect("button laid out");
        vcx.simulate_click(b.center(), Modifiers::none());
        vcx.run_until_parked();
        vcx.update(|window, _| assert!(input.is_focused(window), "click on {sel} stole focus"));
    }
}

#[gpui_kit::test]
fn tab_reaches_only_the_scoped_button(cx: &mut TestAppContext) {
    let (vcx, input) = setup(cx);
    vcx.update(|window, cx| window.focus_next(cx));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        assert!(!input.is_focused(window), "Tab moved focus off the input");
        assert!(window.focused(cx).is_some(), "the scoped button is focused");
    });
    vcx.update(|window, cx| window.focus_next(cx));
    vcx.run_until_parked();
    vcx.update(|window, _| {
        assert!(
            input.is_focused(window),
            "only two tab stops (input, inside button): the outside button is skipped"
        )
    });
}
