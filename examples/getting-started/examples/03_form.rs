//! Step 3: a sign-up form with validation and error states.
//!
//! `cargo run -p getting-started-examples --example 03_form --features gpui-fast`
//! `cargo test -p getting-started-examples --example 03_form` runs the validation tests.

use gpui_kit::{
    App, AppContext, Context, Entity, Hsla, IntoElement, Render, SharedString, Subscription,
    Window, WindowOptions,
    component::{
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
    },
    div,
    prelude::*,
    px,
};
use guiraffe_components::{Card, Text, Title};
use guiraffe_core::{ColorName, GuiraffeTheme, Size, StateKind, Variant, focus_scope};
use guiraffe_inputs::{PasswordStrength, logic::password_strength};
use guiraffe_ui::{component, ui};

// ---------------------------------------------------------------- pure logic

/// What the user typed. Plain data, so validation is testable without a window.
#[derive(Debug, Clone, Default, PartialEq)]
struct Values {
    name: String,
    email: String,
    password: String,
    terms: bool,
}

/// One optional message per field. `None` means the field is valid.
#[derive(Debug, Clone, Default, PartialEq)]
struct Errors {
    name: Option<&'static str>,
    email: Option<&'static str>,
    password: Option<&'static str>,
    terms: Option<&'static str>,
}

impl Errors {
    fn is_empty(&self) -> bool {
        *self == Errors::default()
    }
}

fn validate(v: &Values) -> Errors {
    let email = v.email.trim();
    Errors {
        name: v.name.trim().is_empty().then_some("Enter your name."),
        email: if email.is_empty() {
            Some("Enter your email address.")
        } else if !email
            .split_once('@')
            .is_some_and(|(user, host)| !user.is_empty() && host.contains('.'))
        {
            Some("That doesn't look like an email address.")
        } else {
            None
        },
        password: (password_strength(&v.password) < 3)
            .then_some("Use at least 10 characters with mixed case, digits or symbols."),
        terms: (!v.terms).then_some("Accept the terms to continue."),
    }
}

// ---------------------------------------------------------------- the view

/// The message under an invalid field (nothing when the field is valid).
#[component]
fn FieldError(message: Option<&'static str>) -> impl IntoElement {
    ui! {
        div {
            if let Some(message) = message { Text(message) { color: ColorName::Red size: Size::Xs } }
        }
    }
}

struct SignupForm {
    name: Entity<InputState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    terms: bool,
    /// Errors are shown only after the first submit attempt, then live.
    show_errors: bool,
    submitted: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl SignupForm {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Ada Lovelace"));
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("ada@example.com"));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        // Inputs are Controls: they own their text and emit events. Re-render the form
        // when any of them changes so the error states and strength meter stay live.
        let _subscriptions = [&name, &email, &password]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            name,
            email,
            password,
            terms: false,
            show_errors: false,
            submitted: None,
            _subscriptions,
        }
    }

    fn values(&self, cx: &App) -> Values {
        Values {
            name: self.name.read(cx).value().to_string(),
            email: self.email.read(cx).value().to_string(),
            password: self.password.read(cx).value().to_string(),
            terms: self.terms,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let values = self.values(cx);
        self.show_errors = true;
        self.submitted = validate(&values)
            .is_empty()
            .then(|| format!("Welcome aboard, {}!", values.name.trim()).into());
        cx.notify();
    }
}

impl Render for SignupForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let values = self.values(cx);
        let errors = if self.show_errors {
            validate(&values)
        } else {
            Errors::default()
        };
        // The theme's red ring colour for invalid fields (the same token the DataUi
        // renderer uses), so it has the right contrast in light and dark schemes.
        let invalid: Hsla = GuiraffeTheme::global(cx)
            .state(StateKind::FocusRing, Variant::Outline, Some(ColorName::Red))
            .border
            .to_hsla();
        let border = |error: Option<&str>| error.map(|_| invalid);
        let (name, email, password) = (&self.name, &self.email, &self.password);
        let terms = self.terms;
        let submitted = self.submitted.clone();

        let form = ui! {
            div {
                w: px(380.)
                flex flex_col gap_3
                Title("Create an account") { order: 2 }

                Text("Name") { size: Size::Sm }
                Input(name) { .when_some(border(errors.name), |i, c| i.border_color(c)) }
                FieldError(errors.name)

                Text("Email") { size: Size::Sm }
                Input(email) { .when_some(border(errors.email), |i, c| i.border_color(c)) }
                FieldError(errors.email)

                Text("Password") { size: Size::Sm }
                Input(password) { .when_some(border(errors.password), |i, c| i.border_color(c)) }
                PasswordStrength(&values.password)
                FieldError(errors.password)

                Checkbox("terms") {
                    label: "I accept the terms"
                    checked: terms
                    on_click: cx.listener(|this, checked: &bool, _, cx| {
                        this.terms = *checked;
                        cx.notify();
                    })
                }
                FieldError(errors.terms)

                Button("submit") {
                    label: "Sign up";
                    .primary()
                    on_click: cx.listener(|this, _, _, cx| this.submit(cx))
                }
                if let Some(message) = submitted { Text(message) { color: ColorName::Teal } }
            }
        };

        ui! {
            div {
                size_full flex items_center justify_center
                // A focus scope opts the checkbox and button into Tab navigation.
                Card { {focus_scope(form)} }
            }
        }
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            guiraffe_inputs::init(cx);
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let form = cx.new(|cx| SignupForm::new(window, cx));
                cx.new(|cx| gpui_kit::component::Root::new(form, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the form window: {error}");
                cx.quit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Values {
        Values {
            name: "Ada".into(),
            email: "ada@example.com".into(),
            password: "Analytical-Engine-1843".into(),
            terms: true,
        }
    }

    #[test]
    fn a_complete_form_has_no_errors() {
        assert!(validate(&valid()).is_empty());
    }

    #[test]
    fn each_field_reports_its_own_error() {
        let empty = validate(&Values::default());
        assert!(empty.name.is_some() && empty.email.is_some());
        assert!(empty.password.is_some() && empty.terms.is_some());

        for email in ["ada", "@example.com", "ada@localhost"] {
            let errors = validate(&Values {
                email: email.into(),
                ..valid()
            });
            assert_eq!(
                errors,
                Errors {
                    email: Some("That doesn't look like an email address."),
                    ..Errors::default()
                },
                "{email}"
            );
        }

        let weak = validate(&Values {
            password: "password".into(),
            ..valid()
        });
        assert!(weak.password.is_some() && weak.name.is_none());
    }
}
