//! CopyButton: copies a value to the clipboard and shows a "copied" state for two seconds.
//!
//! The copied state is a Control concern: it outlives a frame and resets on a timer. Copying is
//! the only side effect; the value is data (`.set_value`), never read from the environment.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::{
    App, AppContext, ClickEvent, ClipboardItem, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div, prelude::*, px,
};
use guiraffe_core::{ColorName, Size, Variant};

use crate::style::{hsla, metrics, radius, theme, variant};

/// How long "copied" is shown, in milliseconds (Mantine: 2000).
pub const COPIED_MS: u64 = 2000;

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
    label: Option<SharedString>,
    copied: bool,
    /// Incremented per copy so a stale timer cannot clear a newer copy.
    generation: u64,
    /// `None`: inherit the context's control size.
    size: Option<Size>,
    color: Option<ColorName>,
    disabled: bool,
    on_copy: Option<OnCopy>,
}

impl CopyButton {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            value: SharedString::default(),
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = guiraffe_core::StyleContext::current();
        let size = self.size.unwrap_or(ctx.control_size);
        let scale = theme(cx).scale;
        let copied = self.copied;
        let kind = if copied {
            Variant::Filled
        } else {
            Variant::Light
        };
        let v = variant(cx, self.color, kind);
        let id = format!(
            "gr-copy-button:{}",
            self.value.chars().take(24).collect::<String>()
        );
        let label = self.label.clone();
        let disabled = self.disabled;
        let side = px(size.control_height());
        let mut el = div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(metrics::gap(size)))
            .h(side)
            .when(label.is_none(), |el| el.w(side))
            .when(label.is_some(), |el| el.px(px(size.spacing() * 0.75)))
            .rounded(radius(None, cx))
            .bg(hsla(v.background))
            .border_1()
            .border_color(hsla(v.border))
            .text_color(hsla(v.foreground))
            .text_size(px(size.font_size()
                * scale
                * if label.is_some() { 1.0 } else { 0.9 }))
            .child(div().size(px(size.icon_size())).child(if copied {
                IconName::Check
            } else {
                IconName::Copy
            }))
            .when_some(label, |el, label| el.child(label));
        if disabled {
            el = el.opacity(0.5);
        } else {
            let hover = hsla(v.hover);
            el = el
                .cursor_pointer()
                .hover(move |el| el.bg(hover))
                .on_click(cx.listener(|this: &mut Self, _: &ClickEvent, window, cx| {
                    this.copy(window, cx);
                }));
        }
        el
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
