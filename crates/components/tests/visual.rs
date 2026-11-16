//! Visual snapshots of every component, in light and dark, on the headless renderer.
//! Accept intentional changes with `GUIRAFFE_UPDATE_SNAPSHOTS=1 cargo test -p guiraffe-components --test visual`.

use gpui_kit::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, component::IconName,
    div, px, size,
};
use guiraffe_components::*;
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, Size, Variant, apply};
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct KitchenSink {
    segmented: gpui_kit::Entity<SegmentedControl>,
}

impl Render for KitchenSink {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(Title::new("GUIraffe").order(2))
            .child(Text::new("Rich, fast desktop UI").dimmed(true))
            .child(self.segmented.clone())
            .child(Highlight::new("Highlight the important words").term("important"))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(ThemeIcon::new(IconName::Bell))
                    .child(
                        ThemeIcon::new(IconName::Star)
                            .color(ColorName::Teal)
                            .variant(Variant::Light),
                    )
                    .child(ActionIcon::new("a", IconName::Settings).variant(Variant::Outline))
                    .child(
                        Indicator::new(ThemeIcon::new(IconName::Inbox).color(ColorName::Grape))
                            .label("3"),
                    )
                    .child(Pill::new("p", "React").on_remove(|_, _, _| {}))
                    .child(
                        RingProgress::new()
                            .diameter(56.0)
                            .thickness(7.0)
                            .section(40.0, ColorName::Blue)
                            .section(25.0, ColorName::Orange),
                    ),
            )
            .child(
                Card::new()
                    .section(
                        CardSection::new().child(
                            div().h(px(36.)).bg(GuiraffeTheme::global(cx)
                                .variant(Some(ColorName::Violet), Variant::Light)
                                .background
                                .to_hsla()),
                        ),
                    )
                    .child(Title::new("Card title").order(4))
                    .child(
                        Text::new("Cards group related content on a raised surface.")
                            .size(Size::Sm)
                            .dimmed(true),
                    ),
            )
            .child(
                Blockquote::new()
                    .color(ColorName::Indigo)
                    .cite("Mantine docs")
                    .child("Life is like an npm install."),
            )
    }
}

fn render(scheme: Scheme) -> image::RgbaImage {
    let mut snap = Snapshotter::new()
        .unwrap_or_else(|e| panic!("{e}"))
        .with_setup(move |cx| {
            init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
    snap.render(size(px(560.), px(500.)), |_, cx| {
        let segmented = SegmentedControl::new(["Preview", "Code", "Export"], cx);
        segmented.update(cx, |s, cx| s.select(1, cx));
        cx.new(|_| KitchenSink { segmented })
    })
    .unwrap_or_else(|e| panic!("{e:#}"))
}

#[test]
fn kitchen_sink_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "kitchen-sink-light"),
        (Scheme::Dark, "kitchen-sink-dark"),
    ] {
        let image = render(scheme);
        let distinct: std::collections::HashSet<[u8; 4]> = image.pixels().map(|p| p.0).collect();
        assert!(
            distinct.len() > 50,
            "{name}: scene rendered with real content ({} colours)",
            distinct.len()
        );
        assert_snapshot(name, &image, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

#[test]
fn light_and_dark_differ() {
    let (l, d) = (render(Scheme::Light), render(Scheme::Dark));
    assert_ne!(
        l.get_pixel(2, 2),
        d.get_pixel(2, 2),
        "body background follows the scheme"
    );
}
