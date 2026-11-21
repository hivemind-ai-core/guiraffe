//! CopyButton: copies a value to the clipboard and shows a "copied" state for two seconds.
//!
//! The copied state is a Control concern: it outlives a frame and resets on a timer. Copying is
//! the only side effect; the value is data (`.set_value`), never read from the environment.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::{
    App, AppContext, ClickEvent, ClipboardItem, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, TestSupportExt,
    Window, div, prelude::*, px,
};
use guiraffe_core::{
    A11yDisabledExt, ColorName, GuiraffeTheme, Press, PressableExt, Size, Variant,
};

use crate::style::{metrics, radius, theme};

/// How long "copied" is shown, in milliseconds (Mantine: 2000).
pub const COPIED_MS: u64 = 2000;

/// The element id of a copy button: the value it copies, shortened.
fn element_id(value: &SharedString) -> SharedString {
    let mut id = String::with_capacity(48);
    id.push_str("gr-copy-button:");
    id.extend(value.chars().take(24));
    id.into()
}

type OnCopy = Box<dyn Fn(&SharedString, &mut Window, &mut App) + 'static>;

/// A button that copies its value and confirms for two seconds.
///
/// ```
/// use guiraffe_components::CopyButton;
///
/// # fn build(cx: &mut gpui_kit::App) {
/// let copy = CopyButton::new(cx);
/// copy.update(cx, |b, cx| b.set_value("npm install guiraffe", cx));
/// # }
/// ```
pub struct CopyButton {
    value: SharedString,
    /// The element id, built from the value: it changes with the value, not
    /// with every render.
    id: SharedString,
    label: Option<SharedString>,
    copied: bool,
    /// Incremented per copy so a stale timer cannot clear a newer copy.
    generation: u64,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    disabled: bool,
    on_copy: Option<OnCopy>,
    focus: FocusHandle,
    /// `None`: a tab stop only inside a focus scope (spec/00 §12.3).
    focusable: Option<bool>,
}

impl CopyButton {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus: cx.focus_handle(),
            focusable: None,
            value: SharedString::default(),
            id: element_id(&SharedString::default()),
            label: None,
            copied: false,
            generation: 0,
            size: None,
            color: None,
            disabled: false,
            on_copy: None,
        })
    }

    pub fn copied(&self) -> bool {
        self.copied
    }

    pub fn value(&self) -> &SharedString {
        &self.value
    }

    /// The visible label; without one the button is icon-only.
    pub fn set_label(&mut self, label: Option<SharedString>, cx: &mut Context<Self>) {
        if self.label != label {
            self.label = label;
            cx.notify();
        }
    }

    pub fn set_value(&mut self, value: impl Into<SharedString>, cx: &mut Context<Self>) {
        let value = value.into();
        if self.value != value {
            self.id = element_id(&value);
            self.value = value;
            cx.notify();
        }
    }

    pub fn set_size(&mut self, size: Option<Size>, cx: &mut Context<Self>) {
        if self.size != size {
            self.size = size;
            cx.notify();
        }
    }

    pub fn set_color(&mut self, color: Option<ColorName>, cx: &mut Context<Self>) {
        if self.color != color {
            self.color = color;
            cx.notify();
        }
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }

    /// Opt in or out of the Tab order (`None`: only inside a focus scope).
    pub fn set_focusable(&mut self, focusable: Option<bool>, cx: &mut Context<Self>) {
        if self.focusable != focusable {
            self.focusable = focusable;
            cx.notify();
        }
    }

    pub fn on_copy(&mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) {
        self.on_copy = Some(Box::new(f));
    }

    /// Copy the current value. Returns whether anything was copied.
    ///
    /// A disabled button copies nothing and reports nothing, matching a real button.
    pub fn copy(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.disabled {
            return false;
        }
        let value = self.value.clone();
        cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
        self.copied = true;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        cx.notify();
        // Clear the confirmation after two seconds, unless another copy restarted it.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(COPIED_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.copied {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
        if let Some(f) = self.on_copy.take() {
            f(&value, window, cx);
            self.on_copy = Some(f);
        }
        true
    }
}

impl Render for CopyButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let copied = self.copied;
        let kind = if copied {
            Variant::Filled
        } else {
            Variant::Light
        };
        let colors = GuiraffeTheme::global(cx).press_colors(kind, self.color);
        let id = self.id.clone();
        let label = self.label.clone();
        // Icon-only buttons still need a name; the state is part of it.
        let name: SharedString = match (&label, copied) {
            (Some(l), false) => l.clone(),
            (Some(l), true) => format!("{l} (copied)").into(),
            (None, false) => "Copy".into(),
            (None, true) => "Copied".into(),
        };
        let disabled = self.disabled;
        let r = radius(None, cx);
        let press = Press::new(colors)
            .radius(r)
            .disabled(disabled)
            .focusable(self.focusable)
            .label(name);
        let side = px(size.control_height());
        div()
            .id(id.clone())
            .a11y_disabled(disabled)
            .test_support()
            .debug_selector(move || id.to_string())
            .flex()
            .items_center()
            .justify_center()
            .gap(px(metrics::gap(size)))
            .h(side)
            .when(label.is_none(), |el| el.w(side))
            .when(label.is_some(), |el| el.px(px(size.spacing() * 0.75)))
            .rounded(r)
            .border_1()
            .text_size(px(size.font_size()
                * scale
                * if label.is_some() { 1.0 } else { 0.9 }))
            .child(div().size(px(size.icon_size())).child(if copied {
                IconName::Check
            } else {
                IconName::Copy
            }))
            .when_some(label, |el, label| el.child(label))
            .press(&press, &self.focus, window)
            .on_press(
                disabled,
                cx.listener(|this: &mut Self, _: &ClickEvent, window, cx| {
                    this.copy(window, cx);
                }),
            )
    }
}

impl Focusable for CopyButton {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::COPIED_MS;

    #[test]
    fn copied_state_lasts_two_seconds() {
        assert_eq!(COPIED_MS, 2000);
    }
}
