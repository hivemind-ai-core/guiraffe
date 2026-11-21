//! The themed notification card. Usable on its own (in-app) and inside notification windows.
use std::rc::Rc;

use gpui_kit::{
    App, BoxShadow, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, StyleRefinement, Styled, Window,
    component::{
        Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        progress::Progress,
    },
    div, point,
    prelude::FluentBuilder,
    px,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Rgb, Size, Variant};

use super::{
    model::{CardIcon, NotificationKind, WindowNotification},
    stack::CardMetrics,
};

const PAD: Size = Size::Sm;
const TITLE: Size = Size::Sm;
const BODY: Size = Size::Xs;
/// Gap between title and body, and before the progress/actions rows.
const ROW_GAP: Size = Size::Xs;
const BODY_LINES: usize = 2;

fn line(size: Size) -> f32 {
    (size.font_size() * size.line_height()).ceil()
}
fn small_gap() -> f32 {
    (ROW_GAP.spacing() / 5.0).round()
}
fn row_gap() -> f32 {
    (ROW_GAP.spacing() * 0.8).round()
}
fn action_height() -> f32 {
    (Size::Xs.control_height() * 0.9).round()
}
const PROGRESS_HEIGHT: f32 = 6.0;

/// Card section heights from the size tokens. Every card renders exactly this tall.
pub fn metrics() -> CardMetrics {
    let pad = PAD.spacing();
    let body = line(BODY) * BODY_LINES as f32;
    CardMetrics {
        base: pad * 2.0 + line(TITLE) + small_gap() + body,
        no_body: -(small_gap() + body),
        actions: row_gap() + action_height(),
        progress: row_gap() + PROGRESS_HEIGHT,
    }
}

fn accent(kind: NotificationKind) -> Option<ColorName> {
    match kind {
        NotificationKind::Info => Some(ColorName::Blue),
        NotificationKind::Success => Some(ColorName::Green),
        NotificationKind::Warning => Some(ColorName::Orange),
        NotificationKind::Error => Some(ColorName::Red),
        NotificationKind::Neutral => None,
    }
}

fn kind_icon(kind: NotificationKind) -> Option<IconName> {
    match kind {
        NotificationKind::Info => Some(IconName::Info),
        NotificationKind::Success => Some(IconName::CircleCheck),
        NotificationKind::Warning => Some(IconName::TriangleAlert),
        NotificationKind::Error => Some(IconName::CircleX),
        NotificationKind::Neutral => None,
    }
}

fn kind_label(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::Info => "Information",
        NotificationKind::Success => "Success",
        NotificationKind::Warning => "Warning",
        NotificationKind::Error => "Error",
        NotificationKind::Neutral => "Notification",
    }
}

type ClickFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ActionFn = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// A notification card: accent stripe, icon, title, two-line body, progress bar, action buttons
/// and a close button. Fixed height ([`metrics`]), full width of its container.
///
/// ```
/// use guiraffe_windows::notify::{NotificationCard, WindowNotification, NotificationKind};
/// let card = NotificationCard::new(
///     "saved",
///     WindowNotification::new("Saved").body("All changes are on disk").kind(NotificationKind::Success),
/// )
/// .on_close(|_, _| {});
/// # let _ = card;
/// ```
#[derive(IntoElement)]
pub struct NotificationCard {
    id: ElementId,
    content: WindowNotification,
    on_click: Option<ClickFn>,
    on_action: Option<ActionFn>,
    on_close: Option<ClickFn>,
    test_id: Option<SharedString>,
    style: StyleRefinement,
}

impl NotificationCard {
    pub fn new(id: impl Into<ElementId>, content: WindowNotification) -> Self {
        Self {
            id: id.into(),
            content,
            on_click: None,
            on_action: None,
            on_close: None,
            test_id: None,
            style: StyleRefinement::default(),
        }
    }
    /// Body click (only when the content has `dismiss_on_click` or a handler is set).
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
    /// An action button was pressed; receives its key.
    pub fn on_action(mut self, f: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }
    /// The close button was pressed.
    pub fn on_close(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
    /// Debug selectors for tests: `<id>` (card), `<id>-close`, `<id>-action-<key>`.
    pub fn test_id(mut self, id: impl Into<SharedString>) -> Self {
        self.test_id = Some(id.into());
        self
    }
    /// The card's fixed height.
    pub fn height(&self) -> f32 {
        self.content.height(&metrics())
    }
}

impl Styled for NotificationCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn shadow(theme: &GuiraffeTheme) -> Vec<BoxShadow> {
    theme
        .shadow(Size::Md)
        .iter()
        .map(|l| BoxShadow {
            color: Rgb::BLACK.alpha(l.alpha).to_hsla(),
            offset: point(px(0.0), px(l.y)),
            blur_radius: px(l.blur),
            spread_radius: px(l.spread),
            inset: false,
        })
        .collect()
}

impl RenderOnce for NotificationCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let content = self.content;
        let height = content.height(&metrics());
        let color = accent(content.kind);
        let light = theme.variant(color, Variant::Light);
        let filled = theme.variant(color, Variant::Filled);
        let radius = px(Size::Md.radius());
        let pad = px(PAD.spacing());
        let icon_box = line(TITLE) + small_gap() + line(BODY);
        let icon = match &content.icon {
            CardIcon::None => None,
            CardIcon::Auto => kind_icon(content.kind).map(Icon::new),
            CardIcon::Svg(path) => Some(Icon::default().path(SharedString::from(path.clone()))),
        };
        let id = self.id;
        let test_id = self.test_id.unwrap_or_default();
        let clickable = content.dismiss_on_click && self.on_click.is_some();
        let on_click = self.on_click.filter(|_| clickable);
        let label: SharedString = if content.body.is_empty() {
            format!("{}: {}", kind_label(content.kind), content.title).into()
        } else {
            format!(
                "{}: {}. {}",
                kind_label(content.kind),
                content.title,
                content.body
            )
            .into()
        };

        let text = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(small_gap()))
            .child(
                div()
                    .h(px(line(TITLE)))
                    .text_size(px(TITLE.font_size()))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_color(surfaces.text.to_hsla())
                    .truncate()
                    .child(content.title.clone()),
            )
            .when(!content.body.is_empty(), |el| {
                el.child(
                    div()
                        .h(px(line(BODY) * BODY_LINES as f32))
                        .text_size(px(BODY.font_size()))
                        .line_height(px(line(BODY)))
                        .text_color(surfaces.dimmed.to_hsla())
                        .line_clamp(BODY_LINES)
                        .overflow_hidden()
                        .child(content.body.clone()),
                )
            });

        let header = div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(PAD.spacing() * 0.75))
            .when_some(icon, |el, icon| {
                el.child(
                    div()
                        .size(px(icon_box.min(line(TITLE) * 1.6)))
                        .flex_none()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(light.background.to_hsla())
                        .text_color(light.foreground.to_hsla())
                        .child(icon.with_size(px(Size::Sm.icon_size()))),
                )
            })
            .child(text)
            .when(content.closable, |el| {
                let on_close = self.on_close.clone();
                let selector = format!("{test_id}-close");
                el.child(
                    div().debug_selector(|| selector).child(
                        Button::new((id.clone(), "close"))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tab_stop(false)
                            .accessibility_label("Dismiss notification")
                            .on_click(move |_: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(f) = &on_close {
                                    f(window, cx);
                                }
                            }),
                    ),
                )
            });

        let progress = content.progress.map(|value| {
            div().mt(px(row_gap())).h(px(PROGRESS_HEIGHT)).child(
                Progress::new((id.clone(), "progress"))
                    .value(value * 100.0)
                    .color(filled.background.to_hsla())
                    .with_size(px(PROGRESS_HEIGHT))
                    .accessibility_label(SharedString::from(format!("{} progress", content.title))),
            )
        });

        let actions = (!content.actions.is_empty()).then(|| {
            let on_action = self.on_action.clone();
            div()
                .mt(px(row_gap()))
                .h(px(action_height()))
                .flex()
                .flex_row()
                .justify_end()
                .items_center()
                .gap(px(small_gap() * 3.0))
                .children(content.actions.iter().enumerate().map(|(i, action)| {
                    let key = action.key.clone();
                    let on_action = on_action.clone();
                    let button =
                        Button::new((id.clone(), SharedString::from(format!("action-{i}"))))
                            .small()
                            .label(action.label.clone())
                            .tab_stop(false)
                            .on_click(move |_: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(f) = &on_action {
                                    f(&key, window, cx);
                                }
                            });
                    let selector = format!("{test_id}-action-{}", action.key);
                    div().debug_selector(|| selector).child(if action.primary {
                        button.primary()
                    } else {
                        button.outline()
                    })
                }))
        });

        let card_selector = test_id.to_string();
        let mut card = div()
            .id(id.clone())
            .debug_selector(|| card_selector)
            .relative()
            .w_full()
            .h(px(height))
            .flex_none()
            .overflow_hidden()
            .rounded(radius)
            .bg(surfaces.surface.to_hsla())
            .border_1()
            .border_color(surfaces.border.to_hsla())
            .shadow(shadow(theme))
            .role(match content.kind {
                NotificationKind::Warning | NotificationKind::Error => gpui_kit::Role::Alert,
                _ => gpui_kit::Role::Status,
            })
            .aria_label(label)
            .when_some(color, |el, _| {
                // Accent stripe: a second cue besides the icon (never color alone, §12.4).
                // Inset with round ends: GPUI does not clip children to rounded corners.
                el.child(
                    div()
                        .absolute()
                        .top(px(PAD.spacing() * 0.75))
                        .bottom(px(PAD.spacing() * 0.75))
                        .left(px(3.0))
                        .w(px(3.0))
                        .rounded_full()
                        .bg(filled.background.to_hsla()),
                )
            })
            .child(
                div()
                    .size_full()
                    .p(pad)
                    .flex()
                    .flex_col()
                    .child(header)
                    .children(progress)
                    .children(actions),
            )
            .when_some(on_click, |el, f| {
                el.cursor_pointer()
                    .hover(|s| s.bg(surfaces.hover.to_hsla()))
                    .on_click(move |_, window, cx| f(window, cx))
            });
        gpui_kit::Refineable::refine(card.style(), &self.style);
        card
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_are_consistent_and_positive() {
        let m = metrics();
        assert!(m.base > 0.0);
        assert!(m.height(false, false, false) > 0.0);
        assert!(m.height(true, true, true) > m.height(true, false, false));
        // The header must fit the icon chip and the text column.
        assert!(m.height(false, false, false) >= PAD.spacing() * 2.0 + line(TITLE));
    }

    #[test]
    fn every_kind_but_neutral_has_an_accent_and_icon() {
        for kind in [
            NotificationKind::Info,
            NotificationKind::Success,
            NotificationKind::Warning,
            NotificationKind::Error,
        ] {
            assert!(accent(kind).is_some() && kind_icon(kind).is_some());
        }
        assert!(accent(NotificationKind::Neutral).is_none());
        assert!(kind_icon(NotificationKind::Neutral).is_none());
    }
}
