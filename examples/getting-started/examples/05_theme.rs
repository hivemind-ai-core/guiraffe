//! Step 5: theming, dark mode and the inherited style context.
//!
//! `cargo run -p getting-started-examples --example 05_theme --features gpui-fast`

use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription,
    Window, WindowOptions,
    component::{
        Sizable,
        button::{Button, ButtonVariants},
        switch::Switch,
    },
    div,
    prelude::*,
    px,
};
use guiraffe_components::{Card, SegmentedControl, SegmentedEvent, Text, Title};
use guiraffe_core::{
    ColorName, Density, GuiraffeTheme, Scheme, Size, StyleContext, StylePatch, apply, kit_size,
    provide, set_scheme,
};

const PRIMARIES: [(&str, ColorName); 4] = [
    ("Blue", ColorName::Blue),
    ("Teal", ColorName::Teal),
    ("Grape", ColorName::Grape),
    ("Orange", ColorName::Orange),
];
const DENSITIES: [(&str, Density, Size); 3] = [
    ("Compact", Density::Compact, Size::Xs),
    ("Comfortable", Density::Comfortable, Size::Sm),
    ("Spacious", Density::Spacious, Size::Md),
];

/// A widget of your own that follows the inherited context instead of taking size props:
/// it reads `StyleContext::current()` while it renders and sizes a Kit button to match.
#[derive(IntoElement)]
struct SaveButton;

impl RenderOnce for SaveButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let ctx = StyleContext::current();
        Button::new("save")
            .label(format!("Save ({:?})", ctx.control_size))
            .primary()
            .with_size(kit_size(ctx.control_size))
    }
}

/// The preview body. It reads the context in `render`, which runs while the enclosing
/// `provide` lays out its children. Reading `StyleContext::current()` in the parent's
/// `render` instead would see the parent's context, not the patched one.
#[derive(IntoElement)]
struct Preview {
    sample: Entity<SegmentedControl>,
}

impl RenderOnce for Preview {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let ctx = StyleContext::current();
        div()
            .flex()
            .flex_col()
            .gap(px(ctx.density.space(Size::Sm)))
            .child(Title::new("Preview").order(4))
            .child(Text::new(format!("Density {:?}", ctx.density)).dimmed(true))
            .child(self.sample)
            .child(div().flex().child(SaveButton))
    }
}

struct ThemeLab {
    primary: Entity<SegmentedControl>,
    density: Entity<SegmentedControl>,
    sample: Entity<SegmentedControl>,
    patch: StylePatch,
    _subscriptions: Vec<Subscription>,
}

impl ThemeLab {
    fn new(cx: &mut Context<Self>) -> Self {
        let primary = SegmentedControl::new(PRIMARIES.map(|(name, _)| name), cx);
        let density = SegmentedControl::new(DENSITIES.map(|(name, ..)| name), cx);
        density.update(cx, |c, cx| c.select(1, cx));
        let sample = SegmentedControl::new(["Day", "Week", "Month"], cx);
        let _subscriptions = vec![
            // Theme changes are app-wide: `apply` installs the theme global, updates GPUI
            // Controls' theme to match and refreshes every window.
            cx.subscribe(&primary, |_, _, event: &SegmentedEvent, cx| {
                let SegmentedEvent::Changed(index) = *event;
                if let Some((_, color)) = PRIMARIES.get(index) {
                    let theme = GuiraffeTheme {
                        primary: *color,
                        ..GuiraffeTheme::global(cx).clone()
                    };
                    apply(theme, cx);
                }
            }),
            // Density is local: it only changes the patch this view provides to its subtree.
            cx.subscribe(&density, |this, _, event: &SegmentedEvent, cx| {
                let SegmentedEvent::Changed(index) = *event;
                if let Some((_, density, size)) = DENSITIES.get(index) {
                    this.patch = StylePatch::new().density(*density).control_size(*size);
                    cx.notify();
                }
            }),
        ];
        Self {
            primary,
            density,
            sample,
            patch: StylePatch::new()
                .density(Density::Comfortable)
                .control_size(Size::Sm),
            _subscriptions,
        }
    }
}

impl Render for ThemeLab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let s = theme.surfaces();
        let dark = theme.scheme == Scheme::Dark;
        let swatch = |label: &'static str, color: guiraffe_core::Rgb| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .size(px(20.))
                        .rounded(px(Size::Sm.radius()))
                        .border_1()
                        .border_color(s.border.to_hsla())
                        .bg(color.to_hsla()),
                )
                .child(Text::new(label).size(Size::Sm))
        };

        // Everything inside `provide` sees the patched context: Card padding follows the
        // density, segmented controls and `SaveButton` follow the control size.
        let preview = provide(
            self.patch,
            Card::new().child(Preview {
                sample: self.sample.clone(),
            }),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .p(px(Size::Xl.spacing()))
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .child(Title::new("Theme lab").order(2))
            .child(
                Switch::new("dark")
                    .label("Dark mode")
                    .checked(dark)
                    .on_click(|checked: &bool, _, cx| {
                        set_scheme(
                            if *checked {
                                Scheme::Dark
                            } else {
                                Scheme::Light
                            },
                            cx,
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(Text::new("Primary colour").size(Size::Sm))
                    .child(self.primary.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(swatch("body", s.body))
                    .child(swatch("surface", s.surface))
                    .child(swatch("border", s.border))
                    .child(swatch("dimmed", s.dimmed))
                    .child(swatch("primary", theme.primary_color())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(Text::new("Density").size(Size::Sm))
                    .child(self.density.clone()),
            )
            .child(div().w(px(420.)).child(preview))
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(guiraffe_icons::GuiraffeAssets)
        .run(|cx: &mut App| {
            guiraffe_components::init(cx);
            // Start from your own theme instead of the default.
            apply(
                GuiraffeTheme {
                    primary: ColorName::Blue,
                    default_radius: Size::Md,
                    ..Default::default()
                },
                cx,
            );
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                let lab = cx.new(ThemeLab::new);
                cx.new(|cx| gpui_kit::component::Root::new(lab, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("could not open the theme window: {error}");
                cx.quit();
            }
        });
}
