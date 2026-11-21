//! `InputWrapper`: Mantine's `Input.Wrapper` — label, description, the input, error — and
//! the shared input frame colors, so every GUIraffe input lines up and reads alike.

use gpui_kit::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Rgb, Size, StateKind, Variant};

/// How an input's frame is painted (Mantine's input `variant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InputVariant {
    /// Body background with a border.
    #[default]
    Default,
    /// Tinted surface background, the border only shows when open or invalid.
    Filled,
    /// No background or border (inline editing).
    Unstyled,
}

/// The colors of an input frame in one state. The border is always painted (same width in
/// every state, spec/00 §12.2); only its color changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameColors {
    pub background: Rgb,
    pub foreground: Rgb,
    pub placeholder: Rgb,
    pub border: Rgb,
}

/// Resolve the frame colors for `variant` from the theme's semantic state tokens.
///
/// `active` (open popover or focused editor) and `invalid` (an error is shown) only change
/// the border color; `disabled` takes the theme's disabled token.
pub fn frame_colors(
    t: &GuiraffeTheme,
    variant: InputVariant,
    active: bool,
    invalid: bool,
    disabled: bool,
) -> FrameColors {
    let s = t.surfaces();
    let dimmed = t
        .state(StateKind::Dimmed, Variant::Default, None)
        .foreground;
    let (background, rest_border) = match variant {
        InputVariant::Default => (s.body, s.border),
        InputVariant::Filled => (s.surface, s.surface),
        InputVariant::Unstyled => (Rgb::TRANSPARENT, Rgb::TRANSPARENT),
    };
    if disabled {
        let off = t.state(StateKind::Disabled, Variant::Default, None);
        return FrameColors {
            background: if variant == InputVariant::Unstyled {
                Rgb::TRANSPARENT
            } else {
                off.background
            },
            foreground: off.foreground,
            placeholder: off.foreground,
            border: if variant == InputVariant::Unstyled {
                Rgb::TRANSPARENT
            } else {
                off.border
            },
        };
    }
    let border = if invalid {
        error_color(t)
    } else if active && variant != InputVariant::Unstyled {
        t.variant(None, Variant::Filled).background
    } else {
        rest_border
    };
    FrameColors {
        background,
        foreground: s.text,
        placeholder: dimmed,
        border,
    }
}

/// The error text and invalid border color.
pub fn error_color(t: &GuiraffeTheme) -> Rgb {
    t.variant(Some(ColorName::Red), Variant::Subtle).foreground
}

/// Label, description and error around an input (Mantine `Input.Wrapper`).
///
/// ```no_run
/// use guiraffe_inputs::dates::InputWrapper;
/// # fn demo(input: gpui_kit::AnyElement) -> InputWrapper {
/// InputWrapper::new(input)
///     .label("Due date")
///     .description("When the invoice is due")
///     .error(Some("Pick a weekday".into()))
///     .required(true)
/// # }
/// ```
#[derive(IntoElement)]
pub struct InputWrapper {
    input: AnyElement,
    label: Option<SharedString>,
    description: Option<SharedString>,
    error: Option<SharedString>,
    required: bool,
    size: Size,
}

impl InputWrapper {
    pub fn new(input: impl IntoElement) -> Self {
        Self {
            input: input.into_any_element(),
            label: None,
            description: None,
            error: None,
            required: false,
            size: Size::Sm,
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
    /// An error message (also turns the frame red in the pickers). `None` hides it.
    pub fn error(mut self, error: Option<SharedString>) -> Self {
        self.error = error;
        self
    }
    /// Show a required asterisk after the label.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for InputWrapper {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = GuiraffeTheme::global(cx);
        let s = t.surfaces();
        let dimmed = t
            .state(StateKind::Dimmed, Variant::Default, None)
            .foreground;
        let red = error_color(t);
        let fs = self.size.font_size() * t.scale;
        // Mantine: label at the input's font size, helper texts one step smaller.
        let small = (fs * 0.86).round();
        let gap = (self.size.spacing() / 3.0).round();
        let required = self.required;
        div()
            .flex()
            .flex_col()
            .gap(px(gap))
            .when_some(self.label, |el, label| {
                el.child(
                    div()
                        .flex()
                        .gap(px(gap / 2.0))
                        .text_size(px(fs))
                        .line_height(px(fs * self.size.line_height()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(s.text.to_hsla())
                        .child(label)
                        .when(required, |el| {
                            el.child(div().text_color(red.to_hsla()).child("*"))
                        }),
                )
            })
            .when_some(self.description, |el, d| {
                el.child(
                    div()
                        .text_size(px(small))
                        .line_height(px(small * self.size.line_height()))
                        .text_color(dimmed.to_hsla())
                        .child(d),
                )
            })
            .child(self.input)
            .when_some(self.error, |el, e| {
                el.child(
                    div()
                        .text_size(px(small))
                        .line_height(px(small * self.size.line_height()))
                        .text_color(red.to_hsla())
                        .child(e),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guiraffe_core::Scheme;

    #[test]
    fn frame_border_changes_color_only_and_meets_contrast() {
        for scheme in [Scheme::Light, Scheme::Dark] {
            let t = GuiraffeTheme {
                scheme,
                ..Default::default()
            };
            let rest = frame_colors(&t, InputVariant::Default, false, false, false);
            let open = frame_colors(&t, InputVariant::Default, true, false, false);
            let bad = frame_colors(&t, InputVariant::Default, false, true, false);
            let off = frame_colors(&t, InputVariant::Default, false, false, true);
            assert_ne!(rest.border, open.border, "open shows a primary border");
            assert_ne!(rest.border, bad.border, "invalid shows a red border");
            assert_eq!(rest.background, open.background);
            assert_ne!(off.background, rest.background, "disabled is not opacity");
            // Placeholder (dimmed) text keeps 4.5:1 on the frame (spec/00 §12.1).
            assert!(
                rest.placeholder.contrast(rest.background) >= 4.5,
                "{scheme:?}"
            );
            let filled = frame_colors(&t, InputVariant::Filled, false, false, false);
            assert_eq!(filled.border, filled.background, "filled hides its border");
            let none = frame_colors(&t, InputVariant::Unstyled, true, false, false);
            assert_eq!(none.border, Rgb::TRANSPARENT);
        }
    }
}
