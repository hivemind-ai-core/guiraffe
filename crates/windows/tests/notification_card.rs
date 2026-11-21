//! Visual goldens for the notification card in both schemes.
use std::sync::Arc;

use gpui_kit::{
    AppContext, Context, HeadlessAppContext, IntoElement, ParentElement, Render, Styled, Window,
    div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, Size, apply};
use guiraffe_test::snapshot::{SnapshotConfig, assert_snapshot};
use guiraffe_windows::notify::{NotificationCard, NotificationKind, WindowNotification};

struct Scene;

impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surfaces = GuiraffeTheme::global(cx).surfaces();
        let cards = [
            WindowNotification::new("Changes saved")
                .body("All 12 files were written to disk.")
                .kind(NotificationKind::Success),
            WindowNotification::new("Downloading update")
                .body(
                    "guiraffe-0.2.0.tar.gz — 18 of 42 MB, about a minute left at the current speed",
                )
                .progress(0.43)
                .action("later", "Later")
                .primary_action("open", "Show"),
            WindowNotification::new("Sync failed: the server could not be reached")
                .kind(NotificationKind::Error)
                .action("retry", "Retry"),
            WindowNotification::new("Disk almost full")
                .body("2.1 GB left on /home")
                .kind(NotificationKind::Warning)
                .closable(false),
            WindowNotification::new("Plain message")
                .body("Neutral cards have no accent and no icon.")
                .kind(NotificationKind::Neutral),
        ];
        div()
            .size_full()
            .p(px(Size::Md.spacing()))
            .bg(surfaces.body.to_hsla())
            .flex()
            .flex_col()
            .gap(px(Size::Sm.spacing()))
            .children(cards.into_iter().enumerate().map(|(i, c)| {
                div()
                    .w(px(360.))
                    .child(NotificationCard::new(("card", i), c))
            }))
    }
}

#[test]
fn notification_cards_in_both_schemes() {
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            gpui_kit::component::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
        });
        let window = cx
            .open_window(size(px(392.), px(560.)), |_, cx| cx.new(|_| Scene))
            .expect("headless window");
        for _ in 0..4 {
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .expect("frame");
            cx.run_until_parked();
        }
        let image = cx.capture_screenshot(window.into()).expect("capture");
        let name = match scheme {
            Scheme::Light => "notification-card-light",
            Scheme::Dark => "notification-card-dark",
        };
        assert_snapshot(name, &image, &config).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
